use mm_warp_client::{QuicClient, H264Decoder};
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Client ===\n");

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

    // Receive and decode frames (receive up to 10)
    println!("Receiving frames...");
    let mut frames_decoded = 0;
    for i in 0..10 {
        let encoded = QuicClient::receive_frame(&connection).await?;
        println!("  Frame {}: Received {} bytes", i + 1, encoded.len());

        let decoded = decoder.decode(&encoded)?;
        if decoded.is_empty() {
            println!("  Frame {}: Buffered/empty", i + 1);
        } else {
            println!("  Frame {}: Decoded to {} bytes", i + 1, decoded.len());
            frames_decoded += 1;
        }
    }

    println!("\n✅ {} frames successfully decoded", frames_decoded);
    println!("Client complete!");

    Ok(())
}
