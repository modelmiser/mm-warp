use mm_warp_server::{QuicServer, H264Encoder, capture::FrameSource, ext_capture::ExtCapture, WaylandConnection, InputEvent, InputInjector};
use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

#[derive(Parser)]
#[command(name = "mm-warp-server", version, about = "mm-warp remote desktop server")]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "127.0.0.1:4433")]
    listen: String,

    /// Require clients to provide this PIN before connecting.
    /// Strongly recommended when listening on non-loopback addresses.
    #[arg(long)]
    pin: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();
    println!("=== mm-warp Server (H.264 over QUIC) ===\n");

    // Warn if binding to non-loopback address
    let listen_addr: std::net::SocketAddr = args.listen.parse()
        .map_err(|e| anyhow::anyhow!("Invalid listen address '{}': {}", args.listen, e))?;
    if !listen_addr.ip().is_loopback() {
        if args.pin.is_none() {
            eprintln!("⚠️  WARNING: Binding to non-loopback address {} WITHOUT --pin.", listen_addr);
            eprintln!("   Anyone on the network can view your screen and control your input.");
            eprintln!("   Use --pin <SECRET> to require authentication.\n");
        } else {
            println!("🔑 PIN authentication enabled for non-loopback address {}.", listen_addr);
        }
    }

    // Create screen capture — try ext-image-copy-capture first, fall back to wlr-screencopy
    let (mut capture, monitor_fps): (Box<dyn FrameSource>, u32) = if ExtCapture::is_available() {
        println!("Initializing ext-image-copy-capture...");
        let cap = ExtCapture::new()?;
        let fps = cap.refresh_rate();
        println!("✅ Screen capture ready (ext-image-copy-capture)");
        println!("   Monitor refresh rate: {} Hz", fps);
        (Box::new(cap), fps)
    } else {
        println!("ext-image-copy-capture not available, falling back to wlr-screencopy...");
        let cap = WaylandConnection::new()?;
        println!("✅ Screen capture ready (wlr-screencopy)");
        (Box::new(cap), 60)
    };

    // Propagate resolution from capture backend
    let res = capture.resolution();
    println!("   Capture resolution: {}\n", res);

    // Start QUIC server
    println!("Starting QUIC server on {}...", listen_addr);
    let mut server = QuicServer::new(listen_addr).await?;
    println!("✅ Server listening\n");

    // Create encoder matching capture resolution
    println!("Creating H.264 encoder ({})...", res);
    let mut encoder = H264Encoder::new(res.width, res.height)?;
    println!("✅ Encoder ready\n");

    // Accept clients in a loop (allows reconnection)
    println!("Waiting for client connections... (Ctrl+C to stop)\n");

    // Shared keyframe flag: set by accept loop, consumed by encode task
    let keyframe_flag = Arc::new(AtomicBool::new(false));

    // Track input receiver task for cancellation on new client
    let mut input_task: Option<JoinHandle<()>> = None;

    loop {
        // Select between accepting a new client and Ctrl+C
        let connection = tokio::select! {
            result = server.accept() => {
                match result {
                    Ok(conn) => conn,
                    Err(e) => {
                        eprintln!("⚠️  Failed to accept client: {}", e);
                        continue;
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n✅ Shutting down...");
                // Abort input task to release virtual devices cleanly
                if let Some(handle) = input_task.take() {
                    handle.abort();
                }
                return Ok(());
            }
        };

        println!("✅ Client connected from {}", connection.remote_address());
        keyframe_flag.store(true, Ordering::Release);
        println!("   Forcing keyframe for new client");

        // Cancel previous input receiver task if still running
        if let Some(handle) = input_task.take() {
            handle.abort();
        }

        // PIN authentication (if enabled)
        if let Some(ref pin) = args.pin {
            use tokio::time::timeout;
            let pin_result = timeout(std::time::Duration::from_secs(10), async {
                let (mut send, mut recv) = connection.accept_bi().await
                    .map_err(|e| anyhow::anyhow!("PIN: no bidi stream from client: {}", e))?;
                let mut buf = vec![0u8; 256];
                let n = recv.read(&mut buf).await
                    .map_err(|e| anyhow::anyhow!("PIN: read failed: {}", e))?
                    .unwrap_or(0);
                let client_pin = std::str::from_utf8(&buf[..n]).unwrap_or("");
                if client_pin == pin.as_str() {
                    send.write_all(b"OK").await?;
                    send.finish()?;
                    Ok::<bool, anyhow::Error>(true)
                } else {
                    send.write_all(b"REJECT").await?;
                    send.finish()?;
                    Ok(false)
                }
            }).await;

            match pin_result {
                Ok(Ok(true)) => println!("   PIN verified"),
                Ok(Ok(false)) => {
                    eprintln!("⚠️  Wrong PIN from {} — disconnecting", connection.remote_address());
                    continue;
                }
                Ok(Err(e)) => {
                    eprintln!("⚠️  PIN exchange failed: {} — disconnecting", e);
                    continue;
                }
                Err(_) => {
                    eprintln!("⚠️  PIN timeout (10s) from {} — disconnecting", connection.remote_address());
                    continue;
                }
            }
        }

        // Send stream metadata to client
        let max_fps = monitor_fps.min(60);
        let meta = mm_warp_common::StreamMetadata::new(res.width, res.height, max_fps);
        if let Err(e) = QuicServer::send_metadata(&connection, &meta).await {
            eprintln!("⚠️  Failed to send metadata: {} — dropping client", e);
            continue;
        }
        println!("   Sent stream metadata: {}x{} @ {} FPS\n", res.width, res.height, max_fps);

        // Spawn input event receiver task
        let connection_clone = connection.clone();
        let capture_width = res.width;
        let capture_height = res.height;
        input_task = Some(tokio::spawn(async move {
            let mut injector = match InputInjector::new() {
                Ok(inj) => {
                    println!("✅ Input injector ready\n");
                    inj
                }
                Err(e) => {
                    eprintln!("⚠️  Input injector failed: {}", e);
                    eprintln!("    Run with sudo or setup-uinput.sh to enable input\n");
                    return;
                }
            };

            // Rate limiting: max 1000 events/sec
            let mut event_count = 0u32;
            let mut rate_window = tokio::time::Instant::now();

            loop {
                match connection_clone.read_datagram().await {
                    Ok(bytes) => {
                        // Rate limiting
                        event_count += 1;
                        if rate_window.elapsed() >= tokio::time::Duration::from_secs(1) {
                            event_count = 0;
                            rate_window = tokio::time::Instant::now();
                        }
                        if event_count > 1000 {
                            continue; // drop excess events
                        }

                        match InputEvent::from_bytes(&bytes) {
                            Ok(event) => {
                                if let Err(e) = injector.inject(&event, capture_width, capture_height) {
                                    tracing::warn!("Input injection failed: {}", e);
                                }
                            }
                            Err(e) => tracing::warn!("Bad datagram: {}", e),
                        }
                    }
                    Err(_) => break,
                }
            }
            println!("Input receiver ended");
        }));

        // --- Pipelined streaming: capture → encode → send ---

        // Adaptive FPS
        let min_fps = 5u32;

        println!("Streaming with adaptive FPS ({}-{})...\n", min_fps, max_fps);

        // Channels connecting the three stages (capacity 2 to bound latency)
        let (cap_tx, cap_rx) = mpsc::channel::<Vec<u8>>(2);
        let (enc_tx, enc_rx) = mpsc::channel::<Vec<u8>>(2);

        // Adaptive FPS feedback: send task → capture task
        let (fps_tx, fps_rx) = watch::channel(max_fps);

        // --- Encode task (blocking thread — H264Encoder::encode is CPU-bound) ---
        let keyframe_flag_enc = keyframe_flag.clone();
        let encode_handle = tokio::task::spawn_blocking(move || {
            run_encode_task(encoder, cap_rx, enc_tx, keyframe_flag_enc)
        });

        // --- Send task (async, spawned) ---
        let send_handle = tokio::spawn(async move {
            run_send_task(connection, enc_rx, fps_tx, max_fps, min_fps).await
        });

        // --- Capture loop (main thread — FrameSource is !Send due to Wayland) ---
        let capture_result = run_capture_loop(&mut capture, &cap_tx, &fps_rx).await;

        // Drop the capture sender to signal encode task to finish
        drop(cap_tx);

        // Wait for pipeline to drain
        let encode_result = encode_handle.await;
        let send_result = send_handle.await;

        // Get encoder back from the blocking task
        match encode_result {
            Ok((returned_encoder, encode_res)) => {
                encoder = returned_encoder;
                if let Err(e) = encode_res {
                    tracing::warn!("Encode task ended with error: {}", e);
                }
            }
            Err(e) => {
                eprintln!("⚠️  Encode task panicked: {}", e);
                encoder = H264Encoder::new(res.width, res.height)?;
            }
        }

        match send_result {
            Ok(Ok(())) => println!("\n✅ Session ended cleanly\n"),
            Ok(Err(e)) => println!("\n⚠️  Session ended: {}\n", e),
            Err(e) => println!("\n⚠️  Send task panicked: {}\n", e),
        }

        if let Err(e) = capture_result {
            eprintln!("⚠️  Capture ended with error: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}

/// Capture loop — runs on the main thread because FrameSource is !Send (Wayland).
async fn run_capture_loop(
    capture: &mut Box<dyn FrameSource>,
    cap_tx: &mpsc::Sender<Vec<u8>>,
    fps_rx: &watch::Receiver<u32>,
) -> Result<()> {
    let mut dropped = 0u64;

    loop {
        let current_fps = (*fps_rx.borrow()).max(1);
        let frame_duration = tokio::time::Duration::from_secs_f64(1.0 / current_fps as f64);
        let start = tokio::time::Instant::now();

        let frame = match capture.capture_frame() {
            Ok(f) => f,
            Err(e) => {
                eprintln!("⚠️  Capture error: {}", e);
                return Err(e);
            }
        };

        match cap_tx.try_send(frame) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                dropped += 1;
                if dropped % 100 == 1 {
                    tracing::warn!("Capture dropping frame (encode backpressure, total dropped: {})", dropped);
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::info!("Capture: encode channel closed, stopping");
                return Ok(());
            }
        }

        let elapsed = start.elapsed();
        if elapsed < frame_duration {
            tokio::time::sleep(frame_duration - elapsed).await;
        }
    }
}

/// Encode task — runs on a blocking thread because H264Encoder::encode() is CPU-bound.
fn run_encode_task(
    mut encoder: H264Encoder,
    mut cap_rx: mpsc::Receiver<Vec<u8>>,
    enc_tx: mpsc::Sender<Vec<u8>>,
    keyframe_flag: Arc<AtomicBool>,
) -> (H264Encoder, Result<()>) {
    let mut dropped = 0u64;

    while let Some(frame) = cap_rx.blocking_recv() {
        if keyframe_flag.swap(false, Ordering::AcqRel) {
            encoder.force_keyframe();
        }

        let encoded = match encoder.encode(&frame) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("⚠️  Encoding error: {}", e);
                return (encoder, Err(e));
            }
        };

        if encoded.is_empty() {
            continue;
        }

        match enc_tx.try_send(encoded) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                dropped += 1;
                if dropped % 100 == 1 {
                    tracing::warn!("Encode dropping frame (send backpressure, total dropped: {})", dropped);
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::info!("Encode: send channel closed, stopping");
                return (encoder, Ok(()));
            }
        }
    }

    tracing::info!("Encode: capture channel closed, stopping");
    (encoder, Ok(()))
}

/// Send task — async, owns the QUIC connection.
async fn run_send_task(
    connection: quinn::Connection,
    mut enc_rx: mpsc::Receiver<Vec<u8>>,
    fps_tx: watch::Sender<u32>,
    max_fps: u32,
    min_fps: u32,
) -> Result<()> {
    let idle_threshold_kb = 25u64;
    let mut stats = mm_warp_common::stats::StreamStats::new();

    while let Some(encoded) = enc_rx.recv().await {
        if let Err(_) = QuicServer::send_frame(&connection, &encoded).await {
            return Err(anyhow::anyhow!("Client disconnected"));
        }

        let frame_size = encoded.len() as u64;
        stats.record_frame(frame_size);

        let current_fps = if frame_size / 1024 < idle_threshold_kb { min_fps } else { max_fps };
        let _ = fps_tx.send(current_fps);

        if let Some(report) = stats.report_if_due("SERVER", Some(current_fps)) {
            println!("{}", report);
        }
    }

    Ok(())
}
