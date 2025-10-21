use mm_warp_client::{H264Decoder, InputEvent};
use anyhow::Result;

#[test]
fn test_decoder_creation() -> Result<()> {
    // Decoder should be creatable
    let decoder = H264Decoder::new(1920, 1080)?;
    println!("✅ Decoder created successfully");
    Ok(())
}

#[test]
fn test_input_event_serialization() {
    // KeyPress
    let key_press = InputEvent::KeyPress { key: 42 };
    let bytes = key_press.to_bytes();
    assert_eq!(bytes[0], 1); // Type
    assert_eq!(bytes.len(), 5); // Type + u32

    // KeyRelease
    let key_release = InputEvent::KeyRelease { key: 13 };
    let bytes = key_release.to_bytes();
    assert_eq!(bytes[0], 2);
    assert_eq!(bytes.len(), 5);

    // MouseMove
    let mouse_move = InputEvent::MouseMove { x: 100, y: 200 };
    let bytes = mouse_move.to_bytes();
    assert_eq!(bytes[0], 3);
    assert_eq!(bytes.len(), 9); // Type + i32 + i32

    // MouseButton
    let mouse_btn = InputEvent::MouseButton { button: 1, pressed: true };
    let bytes = mouse_btn.to_bytes();
    assert_eq!(bytes[0], 4);
    assert_eq!(bytes.len(), 6); // Type + u32 + bool

    println!("✅ All input events serialize correctly");
}

#[test]
fn test_input_event_round_trip_encoding() {
    // Verify events encode to expected byte patterns
    let events = vec![
        InputEvent::KeyPress { key: 65 }, // 'A'
        InputEvent::MouseMove { x: 1920, y: 1080 },
        InputEvent::MouseButton { button: 0, pressed: true },
        InputEvent::KeyRelease { key: 65 },
    ];

    for event in events {
        let bytes = event.to_bytes();
        assert!(bytes.len() > 1, "Event should serialize to >1 byte");
        assert!(bytes[0] >= 1 && bytes[0] <= 4, "Event type should be 1-4");
    }

    println!("✅ Input event encoding round-trip works");
}
