use mm_warp_server::H264Encoder;
use anyhow::Result;

#[test]
fn test_encoder_creates_valid_output() -> Result<()> {
    let mut encoder = H264Encoder::new(1920, 1080)?;

    let mut frame = vec![0u8; 1920 * 1080 * 4];
    for pixel in frame.chunks_mut(4) {
        pixel[0] = 255; // R
        pixel[1] = 0;   // G
        pixel[2] = 0;   // B
        pixel[3] = 255; // A
    }

    let encoded = encoder.encode(&frame)?;
    assert!(!encoded.is_empty(), "Encoder should produce output");

    Ok(())
}

#[test]
fn test_encoder_with_different_content() -> Result<()> {
    let mut encoder = H264Encoder::new(1920, 1080)?;

    // Blue frame
    let mut blue_frame = vec![0u8; 1920 * 1080 * 4];
    for pixel in blue_frame.chunks_mut(4) {
        pixel[2] = 255; // B
        pixel[3] = 255; // A
    }
    let encoded_blue = encoder.encode(&blue_frame)?;
    assert!(!encoded_blue.is_empty());

    // Black frame
    let black_frame = vec![0u8; 1920 * 1080 * 4];
    let encoded_black = encoder.encode(&black_frame)?;
    assert!(!encoded_black.is_empty());

    Ok(())
}
