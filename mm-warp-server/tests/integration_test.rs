use mm_warp_server::{H264Encoder, FrameBuffer};
use anyhow::Result;

#[test]
fn test_encoder_creates_valid_output() -> Result<()> {
    // Create encoder
    let mut encoder = H264Encoder::new(1920, 1080)?;

    // Create test frame (red screen)
    let mut frame = vec![0u8; 1920 * 1080 * 4];
    for pixel in frame.chunks_mut(4) {
        pixel[0] = 255; // R
        pixel[1] = 0;   // G
        pixel[2] = 0;   // B
        pixel[3] = 255; // A
    }

    // Encode
    let encoded = encoder.encode(&frame)?;

    // Verify we got output
    assert!(!encoded.is_empty(), "Encoder should produce output");
    assert!(encoded.len() > 0, "Encoded size should be > 0");

    println!("✅ Encoder produced {} bytes", encoded.len());

    Ok(())
}

#[test]
fn test_frame_buffer_ring() {
    let mut buffer = FrameBuffer::new(3);

    assert_eq!(buffer.len(), 0);
    assert!(buffer.latest().is_none());

    // Add frames
    buffer.push(vec![1, 2, 3]);
    buffer.push(vec![4, 5, 6]);

    assert_eq!(buffer.len(), 2);
    assert_eq!(buffer.latest(), Some(&[4, 5, 6][..]));

    // Fill to capacity
    buffer.push(vec![7, 8, 9]);
    assert_eq!(buffer.len(), 3);

    // Ring buffer wraps (oldest frame replaced)
    buffer.push(vec![10, 11, 12]);
    assert_eq!(buffer.len(), 3);
    assert_eq!(buffer.latest(), Some(&[10, 11, 12][..]));

    println!("✅ Frame buffer ring works correctly");
}

#[test]
fn test_encoder_with_different_content() -> Result<()> {
    let mut encoder = H264Encoder::new(1920, 1080)?;

    // Blue frame
    let mut blue_frame = vec![0u8; 1920 * 1080 * 4];
    for pixel in blue_frame.chunks_mut(4) {
        pixel[0] = 0;   // R
        pixel[1] = 0;   // G
        pixel[2] = 255; // B
        pixel[3] = 255; // A
    }

    let encoded_blue = encoder.encode(&blue_frame)?;
    assert!(!encoded_blue.is_empty());

    // Black frame
    let black_frame = vec![0u8; 1920 * 1080 * 4];
    let encoded_black = encoder.encode(&black_frame)?;
    assert!(!encoded_black.is_empty());

    println!("✅ Encoder handles different frame content");

    Ok(())
}
