use mm_warp_server::H264Encoder;
use mm_warp_client::H264Decoder;
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

#[test]
fn test_encode_decode_round_trip() -> Result<()> {
    let width = 320u32;
    let height = 240u32;

    let mut encoder = H264Encoder::new(width, height)?;
    let mut decoder = H264Decoder::new(width, height)?;

    // Create a solid red RGBA frame
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for pixel in rgba.chunks_mut(4) {
        pixel[0] = 255; // R
        pixel[1] = 0;   // G
        pixel[2] = 0;   // B
        pixel[3] = 255; // A
    }

    // First frame may be buffered by the decoder; send a few to flush pipeline
    let mut decoded = Vec::new();
    for _ in 0..5 {
        let encoded = encoder.encode(&rgba)?;
        assert!(!encoded.is_empty(), "Encoder should produce output every frame");
        let frame = decoder.decode(&encoded)?;
        if !frame.is_empty() {
            decoded = frame;
        }
    }

    assert!(!decoded.is_empty(), "Decoder should produce at least one frame after 5 packets");

    // Decoded frame should match the expected size (width * height * 4 RGBA).
    // Note: ffmpeg RGBA frames may have padding (linesize > width*4), so the
    // decoded slice can be larger. Just verify it is at least the expected size.
    let expected_min = (width * height * 4) as usize;
    assert!(
        decoded.len() >= expected_min,
        "Decoded frame too small: {} < {}", decoded.len(), expected_min,
    );

    Ok(())
}
