// Input injection using uinput (Linux virtual input device)
use anyhow::{Context, Result};
use evdev::{uinput::VirtualDeviceBuilder, AttributeSet, InputEvent as EvInputEvent, EventType, Key};

pub struct InputInjector {
    device: evdev::uinput::VirtualDevice,
}

impl InputInjector {
    pub fn new() -> Result<Self> {
        let mut keys = AttributeSet::<Key>::new();
        for key_code in 0..=255 {
            keys.insert(Key::new(key_code));
        }

        let device = VirtualDeviceBuilder::new()?
            .name("mm-warp Remote Input")
            .with_keys(&keys)?
            .build()
            .context("Failed to build virtual device")?;

        Ok(Self { device })
    }

    pub fn inject_key(&mut self, key: u32, pressed: bool) -> Result<()> {
        use evdev::InputEvent;
        let key_obj = Key::new(key as u16);
        let value = if pressed { 1 } else { 0 };
        self.device.emit(&[
            InputEvent::new(EventType::KEY, key_obj.code(), value),
            InputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])?;
        Ok(())
    }

    pub fn inject_mouse_move(&mut self, x: i32, y: i32) -> Result<()> {
        std::process::Command::new("ydotool")
            .args(&["mousemove", "--absolute", &x.to_string(), &y.to_string()])
            .output()
            .context("ydotool not found - install: sudo apt install ydotool")?;
        Ok(())
    }

    pub fn inject_mouse_button(&mut self, button: u32, pressed: bool) -> Result<()> {
        // Only handle button press (ydotool click does press+release)
        if !pressed {
            return Ok(());
        }

        // Wayland button codes: 272=left, 273=right, 274=middle
        // ydotool codes: 0x40=left, 0x41=right, 0x42=middle
        let ydotool_button = match button {
            272 => "0x40", // BTN_LEFT
            273 => "0x41", // BTN_RIGHT
            274 => "0x42", // BTN_MIDDLE
            _ => return Ok(()), // Ignore unknown buttons
        };

        std::process::Command::new("ydotool")
            .args(&["click", ydotool_button])
            .output()
            .context("ydotool failed")?;
        Ok(())
    }
}
