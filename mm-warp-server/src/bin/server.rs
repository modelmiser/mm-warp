use mm_warp_server::{QuicServer, H264Encoder};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Server ===\n");

    // Start QUIC server
    println!("Starting QUIC server on 127.0.0.1:4433...");
    let mut server = QuicServer::new("127.0.0.1:4433".parse().unwrap()).await?;
    println!("✅ Server listening\n");

    // Create encoder
    println!("Creating H.264 encoder...");
    let mut encoder = H264Encoder::new(1920, 1080)?;
    println!("✅ Encoder ready\n");

    // Wait for client
    println!("Waiting for client connection...");
    let connection = server.accept().await?;
    println!("✅ Client connected from {}\n", connection.remote_address());

    // Encode and send frames
    println!("Sending 3 test frames...");
    for i in 0..3 {
        // Create test frame (grayscale gradient)
        let mut frame = vec![0u8; 1920 * 1080 * 4];
        let brightness = ((i + 1) * 80) as u8;
        for pixel in frame.chunks_mut(4) {
            pixel[0] = brightness;
            pixel[1] = brightness;
            pixel[2] = brightness;
            pixel[3] = 255;
        }

        // Encode
        let encoded = encoder.encode(&frame)?;
        println!("  Frame {}: Encoded to {} bytes", i + 1, encoded.len());

        // Send
        QuicServer::send_frame(&connection, &encoded).await?;
        println!("  Frame {}: Sent", i + 1);

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\n✅ All frames sent");
    println!("Server complete. Client should have received 3 frames.");

    // Keep alive briefly
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    Ok(())
}
