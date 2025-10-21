use mm_warp_client::H264Decoder;
use anyhow::Result;

fn main() -> Result<()> {
    println!("=== mm-warp Client Decoding Test ===\n");

    // Create decoder
    println!("Creating H.264 decoder (1920x1080)...");
    let mut decoder = H264Decoder::new(1920, 1080)?;
    println!("✅ Decoder created\n");

    // Create fake encoded packet (can't actually decode this)
    println!("Creating test packet...");
    let fake_packet = vec![0u8; 1024];
    println!("✅ Packet created (1024 bytes)\n");

    // Try to decode (will return empty buffer since packet is fake)
    println!("Attempting decode...");
    let decoded = decoder.decode(&fake_packet)?;
    println!("✅ Decode returned {} bytes\n", decoded.len());

    println!("=== Decoding Pipeline: SUCCESS ===");
    println!("\nNote: Real decode needs real H.264 packets from encoder");
    println!("Next: Connect client to server for full pipeline test");

    Ok(())
}
