use mm_warp_server::{QuicServer, H264Encoder, ext_capture::ExtCapture, InputEvent, InputInjector};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Server (COSMIC ext-image-copy-capture + H.264) ===\n");

    // Create screen capture
    println!("Initializing ext-image-copy-capture...");
    let mut capture = ExtCapture::new()?;
    let monitor_fps = capture.refresh_rate();
    println!("✅ Screen capture ready");
    println!("   Monitor refresh rate: {} Hz\n", monitor_fps);

    // Start QUIC server
    println!("Starting QUIC server on 127.0.0.1:4433...");
    let mut server = QuicServer::new("127.0.0.1:4433".parse().unwrap()).await?;
    println!("✅ Server listening\n");

    // Create encoder (4K resolution for COSMIC at monitor refresh rate)
    println!("Creating H.264 encoder (3840x2160 @ {} FPS)...", monitor_fps);
    let mut encoder = H264Encoder::new(3840, 2160)?;
    println!("✅ Encoder ready\n");

    // Accept clients in a loop (allows reconnection)
    println!("Waiting for client connections... (Ctrl+C to stop)\n");

    loop {
        // Wait for next client
        let connection = match server.accept().await {
            Ok(conn) => {
                println!("✅ Client connected from {}", conn.remote_address());
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
        // Note: This requires sudo to create uinput device
        let mut injector = match InputInjector::new() {
            Ok(inj) => {
                println!("✅ Input injector ready (keyboard events will be injected)\n");
                inj
            }
            Err(e) => {
                eprintln!("⚠️  Input injector failed: {}", e);
                eprintln!("    Run with sudo to enable keyboard control\n");
                return;
            }
        };

        loop {
            match connection_clone.read_datagram().await {
                Ok(bytes) => {
                    if let Ok(event) = InputEvent::from_bytes(&bytes) {
                        match event {
                            InputEvent::KeyPress { key } => {
                                let _ = injector.inject_key(key, true);
                            }
                            InputEvent::KeyRelease { key } => {
                                let _ = injector.inject_key(key, false);
                            }
                            InputEvent::MouseMove { x, y } => {
                                let _ = injector.inject_mouse_move(x, y);
                            }
                            InputEvent::MouseButton { button, pressed } => {
                                let _ = injector.inject_mouse_button(button, pressed);
                            }
                        }
                    }
                }
                Err(_) => break, // Connection closed
            }
        }
        println!("Input receiver task ended");
    });

        // Adaptive streaming with stats
        println!("Streaming with adaptive FPS (5-20 based on motion)...\n");
        let mut frame_count = 0;

        // Adaptive FPS settings
        let max_fps = 20;  // Cap at achieved FPS (not monitor rate)
        let min_fps = 5;   // Drop to 5 when idle
        let mut current_fps = max_fps;

        // Motion detection threshold (small frames = no motion)
        let idle_threshold_kb = 25; // Frames < 25KB are probably idle

        // Stats tracking
        let mut stats_start = tokio::time::Instant::now();
        let mut interval_bytes = 0u64;
        let mut interval_frames = 0u64;

        let stream_result: Result<()> = loop {
            let start = tokio::time::Instant::now();

            // Capture real frame from COSMIC desktop
            let frame = match capture.capture_frame() {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("⚠️  Capture error: {}", e);
                    break Err(e);
                }
            };

            // Encode to H.264
            let encoded = match encoder.encode(&frame) {
                Ok(e) => e,
                Err(e) => {
                    eprintln!("⚠️  Encoding error: {}", e);
                    break Err(e);
                }
            };

            if !encoded.is_empty() {
                // Send - break on error (client disconnected)
                if let Err(e) = QuicServer::send_frame(&connection, &encoded).await {
                    println!("Client disconnected: {}", e);
                    break Ok(());
                }
                frame_count += 1;

                let frame_size = encoded.len() as u64;
                let frame_kb = frame_size / 1024;
                interval_bytes += frame_size;
                interval_frames += 1;

                // Adaptive FPS: small frames = no motion, drop FPS
                if frame_kb < idle_threshold_kb {
                    current_fps = min_fps; // Idle - drop to 5 FPS
                } else {
                    current_fps = max_fps; // Motion detected - max FPS
                }

                // Print stats every second
                let elapsed = stats_start.elapsed();
                if elapsed.as_secs() >= 1 {
                    let fps = interval_frames as f64 / elapsed.as_secs_f64();
                    let mbps = (interval_bytes as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
                    let avg_kb = interval_bytes / interval_frames / 1024;

                    println!("FPS: {:.1} (target: {}) | Bitrate: {:.2} Mbps | Avg: {}KB/frame | Total: {}",
                        fps, current_fps, mbps, avg_kb, frame_count);

                    stats_start = tokio::time::Instant::now();
                    interval_bytes = 0;
                    interval_frames = 0;
                }
            }

            // Maintain adaptive FPS
            let frame_duration = tokio::time::Duration::from_millis(1000 / current_fps as u64);
            let elapsed = start.elapsed();
            if elapsed < frame_duration {
                tokio::time::sleep(frame_duration - elapsed).await;
            }
        };

        match stream_result {
            Ok(()) => println!("\n✅ Client session ended cleanly\n"),
            Err(e) => println!("\n⚠️  Session ended with error: {}\n", e),
        }

        // Brief pause before accepting next client
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    } // Loop back to accept next client
}
