use mm_warp_server::{QuicServer, FrameBuffer};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Server (RAW/Uncompressed) ===\n");

    // Start QUIC server
    println!("Starting QUIC server on 127.0.0.1:4433...");
    let mut server = QuicServer::new("127.0.0.1:4433".parse().unwrap()).await?;
    println!("✅ Server listening\n");

    // Wait for client
    println!("Waiting for client connection...");
    let connection = server.accept().await?;
    println!("✅ Client connected from {}\n", connection.remote_address());

    // Send raw uncompressed frames (small size for testing)
    let width = 320;
    let height = 240;
    let frame_size = width * height * 4; // RGBA

    println!("Sending 5 raw frames ({}x{} RGBA)...", width, height);

    for i in 0..5 {
        // Create test frame with different colors
        let mut frame = vec![0u8; frame_size];
        let brightness = ((i + 1) * 50) as u8;

        for pixel in frame.chunks_mut(4) {
            pixel[0] = brightness;           // R
            pixel[1] = (255 - brightness);   // G
            pixel[2] = ((i * 40) % 255) as u8; // B
            pixel[3] = 255;                  // A
        }

        println!("  Frame {}: Sending {} bytes", i + 1, frame.len());
        QuicServer::send_frame(&connection, &frame).await?;
        println!("  Frame {}: Sent ✅", i + 1);

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\n✅ All 5 frames sent successfully");
    println!("Server complete.");

    // Keep alive briefly
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    Ok(())
}
