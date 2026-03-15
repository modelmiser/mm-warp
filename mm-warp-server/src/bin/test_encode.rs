use mm_warp_server::H264Encoder;
use anyhow::Result;

fn main() -> Result<()> {
    println!("=== mm-warp Encoding Test ===\n");

    println!("Creating H.264 encoder (1920x1080)...");
    let mut encoder = H264Encoder::new(1920, 1080)?;
    println!("✅ Encoder created\n");

    // Create fake frame (red screen)
    let mut frame = vec![0u8; 1920 * 1080 * 4];
    for pixel in frame.chunks_mut(4) {
        pixel[0] = 255; // R
        pixel[1] = 0;   // G
        pixel[2] = 0;   // B
        pixel[3] = 255; // A
    }

    println!("Encoding frame to H.264...");
    let encoded = encoder.encode(&frame)?;
    println!("✅ Encoded to {} bytes", encoded.len());

    Ok(())
}
