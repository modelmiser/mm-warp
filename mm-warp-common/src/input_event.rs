// Input event types for network transmission
use anyhow::{Context, Result};
use quinn::Connection;

#[derive(Debug, Clone)]
pub enum InputEvent {
    KeyPress { key: u32 },
    KeyRelease { key: u32 },
    MouseMove { x: i32, y: i32 },
    MouseButton { button: u32, pressed: bool },
}

impl InputEvent {
    /// Deserialize event from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.is_empty() {
            anyhow::bail!("Empty input event");
        }

        match bytes[0] {
            1 => { // KeyPress
                if bytes.len() < 5 {
                    anyhow::bail!("KeyPress too short: {} bytes", bytes.len());
                }
                let key = u32::from_be_bytes(bytes[1..5].try_into()?);
                Ok(InputEvent::KeyPress { key })
            }
            2 => { // KeyRelease
                if bytes.len() < 5 {
                    anyhow::bail!("KeyRelease too short: {} bytes", bytes.len());
                }
                let key = u32::from_be_bytes(bytes[1..5].try_into()?);
                Ok(InputEvent::KeyRelease { key })
            }
            3 => { // MouseMove
                if bytes.len() < 9 {
                    anyhow::bail!("MouseMove too short: {} bytes", bytes.len());
                }
                let x = i32::from_be_bytes(bytes[1..5].try_into()?);
                let y = i32::from_be_bytes(bytes[5..9].try_into()?);
                Ok(InputEvent::MouseMove { x, y })
            }
            4 => { // MouseButton
                if bytes.len() < 6 {
                    anyhow::bail!("MouseButton too short: {} bytes", bytes.len());
                }
                let button = u32::from_be_bytes(bytes[1..5].try_into()?);
                let pressed = bytes[5] == 1;
                Ok(InputEvent::MouseButton { button, pressed })
            }
            _ => anyhow::bail!("Unknown input event type: {}", bytes[0]),
        }
    }

    /// Serialize event to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            InputEvent::KeyPress { key } => {
                let mut bytes = vec![1]; // Type = 1
                bytes.extend_from_slice(&key.to_be_bytes());
                bytes
            }
            InputEvent::KeyRelease { key } => {
                let mut bytes = vec![2]; // Type = 2
                bytes.extend_from_slice(&key.to_be_bytes());
                bytes
            }
            InputEvent::MouseMove { x, y } => {
                let mut bytes = vec![3]; // Type = 3
                bytes.extend_from_slice(&x.to_be_bytes());
                bytes.extend_from_slice(&y.to_be_bytes());
                bytes
            }
            InputEvent::MouseButton { button, pressed } => {
                let mut bytes = vec![4]; // Type = 4
                bytes.extend_from_slice(&button.to_be_bytes());
                bytes.push(if *pressed { 1 } else { 0 });
                bytes
            }
        }
    }

    /// Send event over QUIC connection (datagram - fast, unreliable OK)
    pub async fn send(connection: &Connection, event: Self) -> Result<()> {
        let bytes = event.to_bytes();
        connection.send_datagram(bytes.into())
            .context("Failed to send input event")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Round-trip tests for all 4 event types ---

    #[test]
    fn round_trip_key_press() {
        let event = InputEvent::KeyPress { key: 42 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::KeyPress { key } => assert_eq!(key, 42),
            other => panic!("Expected KeyPress, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_key_release() {
        let event = InputEvent::KeyRelease { key: 100 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::KeyRelease { key } => assert_eq!(key, 100),
            other => panic!("Expected KeyRelease, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_mouse_move() {
        let event = InputEvent::MouseMove { x: 1920, y: 1080 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::MouseMove { x, y } => {
                assert_eq!(x, 1920);
                assert_eq!(y, 1080);
            }
            other => panic!("Expected MouseMove, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_mouse_button() {
        let event = InputEvent::MouseButton { button: 272, pressed: true };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::MouseButton { button, pressed } => {
                assert_eq!(button, 272);
                assert!(pressed);
            }
            other => panic!("Expected MouseButton, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_mouse_button_released() {
        let event = InputEvent::MouseButton { button: 273, pressed: false };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::MouseButton { button, pressed } => {
                assert_eq!(button, 273);
                assert!(!pressed);
            }
            other => panic!("Expected MouseButton, got {:?}", other),
        }
    }

    // --- Malformed input tests ---

    #[test]
    fn from_bytes_empty_input() {
        let result = InputEvent::from_bytes(&[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty"));
    }

    #[test]
    fn from_bytes_invalid_type_code() {
        let result = InputEvent::from_bytes(&[0]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown"));
    }

    #[test]
    fn from_bytes_invalid_type_code_high() {
        let result = InputEvent::from_bytes(&[255]);
        assert!(result.is_err());
    }

    #[test]
    fn from_bytes_truncated_key_press() {
        // Type 1 (KeyPress) needs 5 bytes total, give it 3
        let result = InputEvent::from_bytes(&[1, 0, 0]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn from_bytes_truncated_key_release() {
        let result = InputEvent::from_bytes(&[2, 0]);
        assert!(result.is_err());
    }

    #[test]
    fn from_bytes_truncated_mouse_move() {
        // Type 3 (MouseMove) needs 9 bytes total, give it 5
        let result = InputEvent::from_bytes(&[3, 0, 0, 0, 1]);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("too short"));
    }

    #[test]
    fn from_bytes_truncated_mouse_button() {
        // Type 4 (MouseButton) needs 6 bytes total, give it 4
        let result = InputEvent::from_bytes(&[4, 0, 0, 0]);
        assert!(result.is_err());
    }

    // --- Boundary value tests ---

    #[test]
    fn round_trip_key_zero() {
        let event = InputEvent::KeyPress { key: 0 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::KeyPress { key } => assert_eq!(key, 0),
            other => panic!("Expected KeyPress, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_key_max() {
        let event = InputEvent::KeyPress { key: u32::MAX };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::KeyPress { key } => assert_eq!(key, u32::MAX),
            other => panic!("Expected KeyPress, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_mouse_move_origin() {
        let event = InputEvent::MouseMove { x: 0, y: 0 };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::MouseMove { x, y } => {
                assert_eq!(x, 0);
                assert_eq!(y, 0);
            }
            other => panic!("Expected MouseMove, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_mouse_move_negative() {
        let event = InputEvent::MouseMove { x: i32::MIN, y: i32::MAX };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::MouseMove { x, y } => {
                assert_eq!(x, i32::MIN);
                assert_eq!(y, i32::MAX);
            }
            other => panic!("Expected MouseMove, got {:?}", other),
        }
    }

    #[test]
    fn round_trip_button_max() {
        let event = InputEvent::MouseButton { button: u32::MAX, pressed: true };
        let bytes = event.to_bytes();
        let decoded = InputEvent::from_bytes(&bytes).unwrap();
        match decoded {
            InputEvent::MouseButton { button, pressed } => {
                assert_eq!(button, u32::MAX);
                assert!(pressed);
            }
            other => panic!("Expected MouseButton, got {:?}", other),
        }
    }

    // --- Byte-level size assertions ---

    #[test]
    fn to_bytes_sizes() {
        assert_eq!(InputEvent::KeyPress { key: 0 }.to_bytes().len(), 5);
        assert_eq!(InputEvent::KeyRelease { key: 0 }.to_bytes().len(), 5);
        assert_eq!(InputEvent::MouseMove { x: 0, y: 0 }.to_bytes().len(), 9);
        assert_eq!(InputEvent::MouseButton { button: 0, pressed: false }.to_bytes().len(), 6);
    }
}
