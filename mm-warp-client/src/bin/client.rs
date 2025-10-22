use mm_warp_client::{QuicClient, H264Decoder, wayland_display::WaylandDisplay, InputEvent};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Client (Wayland Display) ===\n");

    // Create client
    println!("Creating QUIC client...");
    let client = QuicClient::new()?;
    println!("✅ Client ready\n");

    // Connect to server with retries
    let server_addr = "127.0.0.1:4433".parse().unwrap();
    println!("Connecting to server at {}...", server_addr);

    let connection = loop {
        match client.connect(server_addr).await {
            Ok(conn) => {
                println!("✅ Connected\n");
                break conn;
            }
            Err(e) => {
                eprintln!("⚠️  Connection failed: {} - retrying in 2s...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    };

    // Create decoder (4K for COSMIC)
    println!("Creating H.264 decoder (3840x2160)...");
    let mut decoder = H264Decoder::new(3840, 2160)?;
    println!("✅ Decoder ready\n");

    // Create Wayland display window
    // Start with 1920x1080 window size (will display 4K buffer scaled down)
    println!("Creating Wayland display window (1920x1080 initial size)...");
    let mut display = WaylandDisplay::new(3840, 2160)?;
    println!("✅ Display ready\n");

    // Spawn keyboard test sender (sends 'a' key every 2 seconds)
    let connection_clone = connection.clone();
    tokio::spawn(async move {
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await; // Initial delay
        println!("🎹 Keyboard test active: typing 'a' every 2 seconds (focus text editor on server!)\n");

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

            // Send 'a' key press (evdev keycode 30)
            let _ = InputEvent::send(&connection_clone, InputEvent::KeyPress { key: 30 }).await;
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            let _ = InputEvent::send(&connection_clone, InputEvent::KeyRelease { key: 30 }).await;
        }
    });

    // Receive, decode and display frames continuously with stats
    println!("Receiving and displaying... (Ctrl+C to stop)\n");
    let mut frame_count = 0;

    // Stats tracking
    let mut stats_start = tokio::time::Instant::now();
    let mut interval_frames = 0u64;
    let mut interval_bytes = 0u64;

    loop {
        let encoded = match QuicClient::receive_frame(&connection).await {
            Ok(e) => e,
            Err(e) => {
                println!("\n⚠️  Connection lost: {}", e);
                println!("Restart client to reconnect.");
                return Ok(());
            }
        };
        let frame_size = encoded.len() as u64;

        let decoded = decoder.decode(&encoded)?;
        if !decoded.is_empty() {
            display.display_frame(&decoded)?;
            frame_count += 1;
            interval_frames += 1;
            interval_bytes += frame_size;

            // Print stats every second
            let elapsed = stats_start.elapsed();
            if elapsed.as_secs() >= 1 {
                let fps = interval_frames as f64 / elapsed.as_secs_f64();
                let mbps = (interval_bytes as f64 * 8.0) / (elapsed.as_secs_f64() * 1_000_000.0);
                let avg_kb = if interval_frames > 0 { interval_bytes / interval_frames / 1024 } else { 0 };

                println!("FPS: {:.1} | Bitrate: {:.2} Mbps | Avg: {}KB/frame | Total: {} frames",
                    fps, mbps, avg_kb, frame_count);

                stats_start = tokio::time::Instant::now();
                interval_frames = 0;
                interval_bytes = 0;
            }
        }
    }
}
