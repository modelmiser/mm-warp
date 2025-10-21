use mm_warp_client::{QuicClient, H264Decoder, wayland_display::WaylandDisplay};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Client (Wayland Display) ===\n");

    // Create client
    println!("Creating QUIC client...");
    let client = QuicClient::new()?;
    println!("✅ Client ready\n");

    // Connect to server
    println!("Connecting to server at 127.0.0.1:4433...");
    let connection = client.connect("127.0.0.1:4433".parse().unwrap()).await?;
    println!("✅ Connected\n");

    // Create decoder (4K for COSMIC)
    println!("Creating H.264 decoder (3840x2160)...");
    let mut decoder = H264Decoder::new(3840, 2160)?;
    println!("✅ Decoder ready\n");

    // Create Wayland display window
    // Start with 1920x1080 window size (will display 4K buffer scaled down)
    println!("Creating Wayland display window (1920x1080 initial size)...");
    let mut display = WaylandDisplay::new(3840, 2160)?;
    println!("✅ Display ready\n");

    // Receive, decode and display frames continuously with stats
    println!("Receiving and displaying... (Ctrl+C to stop)\n");
    let mut frame_count = 0;

    // Stats tracking
    let mut stats_start = tokio::time::Instant::now();
    let mut interval_frames = 0u64;
    let mut interval_bytes = 0u64;

    loop {
        let encoded = QuicClient::receive_frame(&connection).await?;
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

    Ok(())
}
