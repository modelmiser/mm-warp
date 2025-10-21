use mm_warp_server::{QuicServer, H264Encoder, ext_capture::ExtCapture};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Server (COSMIC ext-image-copy-capture + H.264) ===\n");

    // Create screen capture
    println!("Initializing ext-image-copy-capture...");
    let mut capture = ExtCapture::new()?;
    println!("✅ Screen capture ready\n");

    // Start QUIC server
    println!("Starting QUIC server on 127.0.0.1:4433...");
    let mut server = QuicServer::new("127.0.0.1:4433".parse().unwrap()).await?;
    println!("✅ Server listening\n");

    // Create encoder (4K resolution for COSMIC)
    println!("Creating H.264 encoder (3840x2160)...");
    let mut encoder = H264Encoder::new(3840, 2160)?;
    println!("✅ Encoder ready\n");

    // Wait for client
    println!("Waiting for client connection...");
    let connection = server.accept().await?;
    println!("✅ Client connected from {}\n", connection.remote_address());

    // Capture, encode and send frames
    println!("Capturing and streaming frames...");
    let mut frames_sent = 0;
    for i in 0..10 {
        // Capture real frame from COSMIC desktop
        print!("Frame {}/10: Capturing... ", i + 1);
        let frame = capture.capture_frame()?;
        print!("{}MB, encoding... ", frame.len() / 1024 / 1024);

        // Encode to H.264
        let encoded = encoder.encode(&frame)?;

        if encoded.is_empty() {
            println!("buffered");
            continue;
        }

        print!("{}KB, sending... ", encoded.len() / 1024);

        // Send
        QuicServer::send_frame(&connection, &encoded).await?;
        println!("✅ sent");
        frames_sent += 1;

        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\n✅ {} frames sent successfully", frames_sent);
    println!("Server complete.");

    // Keep alive briefly
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    Ok(())
}
