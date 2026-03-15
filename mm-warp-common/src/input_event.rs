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
