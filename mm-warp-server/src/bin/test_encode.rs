use mm_warp_server::{H264Encoder, FrameBuffer};
use anyhow::Result;

fn main() -> Result<()> {
    println!("=== mm-warp Server Encoding Test ===\n");

    // Create encoder
    println!("Creating H.264 encoder (1920x1080)...");
    let mut encoder = H264Encoder::new(1920, 1080)?;
    println!("✅ Encoder created\n");

    // Create fake frame (red screen)
    println!("Generating test frame (red screen)...");
    let mut frame = vec![0u8; 1920 * 1080 * 4];
    for pixel in frame.chunks_mut(4) {
        pixel[0] = 255; // R
        pixel[1] = 0;   // G
        pixel[2] = 0;   // B
        pixel[3] = 255; // A
    }
    println!("✅ Frame generated (1920x1080 RGBA, red)\n");

    // Encode
    println!("Encoding frame to H.264...");
    let encoded = encoder.encode(&frame)?;
    println!("✅ Encoded to {} bytes\n", encoded.len());

    // Test frame buffer
    println!("Testing frame buffer...");
    let mut buffer = FrameBuffer::new(5);
    buffer.push(encoded.clone());
    buffer.push(encoded.clone());
    println!("✅ Frame buffer has {} frames\n", buffer.len());

    println!("=== Encoding Pipeline: SUCCESS ===");
    println!("\nNext: Run server to stream over QUIC");

    Ok(())
}
