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

    // Create decoder
    println!("Creating H.264 decoder...");
    let mut decoder = H264Decoder::new(1920, 1080)?;
    println!("✅ Decoder ready\n");

    // Receive and decode frames
    println!("Receiving frames...");
    for i in 0..3 {
        let encoded = QuicClient::receive_frame(&connection).await?;
        println!("  Frame {}: Received {} bytes", i + 1, encoded.len());

        let decoded = decoder.decode(&encoded)?;
        println!("  Frame {}: Decoded to {} bytes", i + 1, decoded.len());
    }

    println!("\n✅ All frames received and decoded");
    println!("Client complete!");

    Ok(())
}
