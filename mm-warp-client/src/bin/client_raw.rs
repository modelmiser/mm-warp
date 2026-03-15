use mm_warp_client::QuicClient;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== mm-warp Client (RAW/Uncompressed) ===\n");

    // Create client
    println!("Creating QUIC client...");
    let client = QuicClient::new()?;
    println!("✅ Client ready\n");

    // Connect to server
    println!("Connecting to server at 127.0.0.1:4433...");
    let connection = client.connect("127.0.0.1:4433".parse().unwrap(), true).await?;
    println!("✅ Connected\n");

    // Receive raw frames
    let width = 320;
    let height = 240;
    let expected_size = width * height * 4;

    println!("Receiving raw frames ({}x{} RGBA)...", width, height);

    for i in 0..5 {
        let frame = QuicClient::receive_frame(&connection).await?;
        println!("  Frame {}: Received {} bytes", i + 1, frame.len());

        if frame.len() == expected_size {
            // Verify frame has color data (not all zeros)
            let sum: u32 = frame.iter().take(100).map(|&b| b as u32).sum();
            if sum > 0 {
                println!("  Frame {}: Valid ✅ (color sum: {})", i + 1, sum);
            } else {
                println!("  Frame {}: Warning - all zeros", i + 1);
            }
        } else {
            println!("  Frame {}: Size mismatch! Expected {}, got {}",
                     i + 1, expected_size, frame.len());
        }
    }

    println!("\n✅ All 5 frames received successfully");
    println!("Client complete!");

    Ok(())
}
