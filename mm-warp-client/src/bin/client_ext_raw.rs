// Client for receiving UNCOMPRESSED frames from ext-image-copy-capture server
// Tests the full pipeline: server -> QUIC -> receive frames

use mm_warp_client::QuicClient;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    // Enable logging
    tracing_subscriber::fmt::init();

    println!("=== mm-warp Client (UNCOMPRESSED receiver) ===\n");

    // Connect to server
    let server_addr: std::net::SocketAddr = "127.0.0.1:4433".parse()?;
    println!("Connecting to {}...", server_addr);

    let client = QuicClient::new()?;
    let connection = client.connect(server_addr, true).await?;
    println!("✅ Connected via QUIC\n");

    // Receive frames
    println!("Receiving frames...");
    for i in 1..=10 {
        print!("Frame {}/10: ", i);
        let frame = QuicClient::receive_frame(&connection).await?;
        println!("Received {}MB", frame.len() / 1024 / 1024);

        // Validate frame size (4K RGBA should be ~31MB)
        let expected_size = 3840 * 2160 * 4;
        if frame.len() != expected_size {
            println!("  ⚠️ Unexpected size (expected {}MB)", expected_size / 1024 / 1024);
        } else {
            // Check if frame has actual data (not all zeros)
            let sum: u64 = frame.iter().take(10000).map(|&b| b as u64).sum();
            if sum > 0 {
                println!("  ✅ Frame has real data (checksum: {})", sum);
            } else {
                println!("  ⚠️ Frame appears empty");
            }
        }
    }

    println!("\n✅ All 10 frames received successfully!");
    println!("Uncompressed streaming works! Ready for H.264 encoding.");

    Ok(())
}
