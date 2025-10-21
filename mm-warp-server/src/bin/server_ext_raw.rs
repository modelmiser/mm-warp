// Server using ext-image-copy-capture (COSMIC) with UNCOMPRESSED streaming
// Tests the full pipeline: ext capture -> QUIC -> client

use mm_warp_server::{QuicServer, ext_capture::ExtCapture};
use anyhow::Result;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    // Enable logging
    tracing_subscriber::fmt::init();

    println!("=== mm-warp Server (ext-image-copy-capture UNCOMPRESSED) ===\n");

    // Create ext capture
    println!("Initializing ext-image-copy-capture...");
    let mut capture = ExtCapture::new()?;
    println!("✅ ExtCapture initialized\n");

    // Create QUIC server
    let addr: SocketAddr = "127.0.0.1:4433".parse()?;
    let mut server = QuicServer::new(addr).await?;
    println!("✅ Server listening on {}\n", addr);

    // Wait for client connection
    println!("Waiting for client connection...");
    let connection = server.accept().await?;
    println!("✅ Client connected from {}\n", connection.remote_address());

    // Capture and stream frames
    println!("Streaming frames (uncompressed)...");
    for i in 1..=10 {
        print!("Frame {}/10: Capturing... ", i);
        let frame = capture.capture_frame()?;
        println!("{}MB, sending... ", frame.len() / 1024 / 1024);

        QuicServer::send_frame(&connection, &frame).await?;
        println!("  ✅ Sent");

        // Small delay between frames
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!("\n✅ All 10 frames sent successfully!");
    println!("Total data: ~{}MB", (10 * 31)); // Approx 31MB per 4K frame

    Ok(())
}
