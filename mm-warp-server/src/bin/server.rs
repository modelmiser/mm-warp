use mm_warp_server::{QuicServer, H264Encoder, capture::FrameSource, ext_capture::ExtCapture, WaylandConnection, InputEvent, InputInjector};
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "mm-warp-server", about = "mm-warp remote desktop server")]
struct Args {
    /// Listen address
    #[arg(short, long, default_value = "127.0.0.1:4433")]
    listen: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();
    println!("=== mm-warp Server (H.264 over QUIC) ===\n");

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
    let listen_addr = args.listen.parse()
        .map_err(|e| anyhow::anyhow!("Invalid listen address '{}': {}", args.listen, e))?;
    println!("Starting QUIC server on {}...", listen_addr);
    let mut server = QuicServer::new(listen_addr).await?;
    println!("✅ Server listening\n");

    // Create encoder matching capture resolution
    println!("Creating H.264 encoder ({})...", res);
    let mut encoder = H264Encoder::new(res.width, res.height)?;
    println!("✅ Encoder ready\n");

    // Accept clients in a loop (allows reconnection)
    println!("Waiting for client connections... (Ctrl+C to stop)\n");

    loop {
        let connection = match server.accept().await {
            Ok(conn) => {
                println!("✅ Client connected from {}", conn.remote_address());
                encoder.force_keyframe();
                println!("   Forcing keyframe for new client\n");
                conn
            }
            Err(e) => {
                eprintln!("⚠️  Failed to accept client: {}", e);
                continue;
            }
        };

        // Spawn input event receiver task
        let connection_clone = connection.clone();
        tokio::spawn(async move {
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

            loop {
                match connection_clone.read_datagram().await {
                    Ok(bytes) => {
                        match InputEvent::from_bytes(&bytes) {
                            Ok(event) => {
                                match event {
                                    InputEvent::KeyPress { key } => {
                                        if let Err(e) = injector.inject_key(key, true) {
                                            tracing::warn!("inject_key failed: {}", e);
                                        }
                                    }
                                    InputEvent::KeyRelease { key } => {
                                        if let Err(e) = injector.inject_key(key, false) {
                                            tracing::warn!("inject_key failed: {}", e);
                                        }
                                    }
                                    InputEvent::MouseMove { x, y } => {
                                        let _ = injector.inject_mouse_move(x, y);
                                    }
                                    InputEvent::MouseButton { button, pressed } => {
                                        let _ = injector.inject_mouse_button(button, pressed);
                                    }
                                }
                            }
                            Err(e) => tracing::warn!("Bad datagram: {}", e),
                        }
                    }
                    Err(_) => break,
                }
            }
            println!("Input receiver ended");
        });

        // Adaptive FPS
        let max_fps = monitor_fps.min(60);
        let min_fps = 5;
        let mut current_fps = max_fps;
        let idle_threshold_kb = 25u64;

        println!("Streaming with adaptive FPS ({}-{})...\n", min_fps, max_fps);

        let mut stats = mm_warp_common::stats::StreamStats::new();

        let stream_result: Result<()> = loop {
            let start = tokio::time::Instant::now();

            let frame = match capture.capture_frame() {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("⚠️  Capture error: {}", e);
                    break Err(e);
                }
            };

            let encoded = match encoder.encode(&frame) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("⚠️  Encoding error: {}", e);
                    break Err(e);
                }
            };

            if !encoded.is_empty() {
                if let Err(_) = QuicServer::send_frame(&connection, &encoded).await {
                    break Err(mm_warp_common::WarpError::ClientDisconnected.into());
                }

                let frame_size = encoded.len() as u64;
                stats.record_frame(frame_size);

                current_fps = if frame_size / 1024 < idle_threshold_kb { min_fps } else { max_fps };

                if let Some(report) = stats.report_if_due("SERVER", Some(current_fps)) {
                    println!("{}", report);
                }
            }

            let frame_duration = tokio::time::Duration::from_millis(1000 / current_fps as u64);
            let elapsed = start.elapsed();
            if elapsed < frame_duration {
                tokio::time::sleep(frame_duration - elapsed).await;
            }
        };

        match stream_result {
            Ok(()) => println!("\n✅ Session ended cleanly\n"),
            Err(e) => println!("\n⚠️  Session ended: {}\n", e),
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
}
