// Input injection using uinput (Linux virtual input device)
// Keyboard AND mouse via evdev — no external tools (ydotool) needed.
use anyhow::{Context, Result};
use evdev::{uinput::VirtualDeviceBuilder, AbsInfo, AbsoluteAxisType, AttributeSet, InputEvent as EvInputEvent, EventType, Key, UinputAbsSetup};

pub struct InputInjector {
    keyboard: evdev::uinput::VirtualDevice,
    mouse: evdev::uinput::VirtualDevice,
}

impl InputInjector {
    pub fn new() -> Result<Self> {
        // Keyboard: all standard keys
        let mut keys = AttributeSet::<Key>::new();
        for key_code in 0..=255 {
            keys.insert(Key::new(key_code));
        }

        let keyboard = VirtualDeviceBuilder::new()?
            .name("mm-warp Keyboard")
            .with_keys(&keys)?
            .build()
            .context("Failed to build virtual keyboard")?;

        // Mouse: absolute positioning + buttons
        let mut mouse_keys = AttributeSet::<Key>::new();
        mouse_keys.insert(Key::BTN_LEFT);
        mouse_keys.insert(Key::BTN_RIGHT);
        mouse_keys.insert(Key::BTN_MIDDLE);

        // Screen dimensions for absolute positioning (large range, compositor scales)
        let abs_info = AbsInfo::new(0, 0, 32767, 0, 0, 1);

        let mouse = VirtualDeviceBuilder::new()?
            .name("mm-warp Mouse")
            .with_keys(&mouse_keys)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_X, abs_info))?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_Y, abs_info))?
            .build()
            .context("Failed to build virtual mouse")?;

        Ok(Self { keyboard, mouse })
    }

    pub fn inject_key(&mut self, key: u32, pressed: bool) -> Result<()> {
        if key > u16::MAX as u32 {
            anyhow::bail!("Key code {} out of range (max {})", key, u16::MAX);
        }
        let key_obj = Key::new(key as u16);
        let value = if pressed { 1 } else { 0 };
        self.keyboard.emit(&[
            EvInputEvent::new(EventType::KEY, key_obj.code(), value),
            EvInputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])?;
        Ok(())
    }

    pub fn inject_mouse_move(&mut self, x: i32, y: i32) -> Result<()> {
        // Map screen coordinates to 0..32767 absolute range.
        // Compositor handles final mapping. Clamp to valid range.
        let abs_x = x.max(0).min(32767);
        let abs_y = y.max(0).min(32767);

        self.mouse.emit(&[
            EvInputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_X.0, abs_x),
            EvInputEvent::new(EventType::ABSOLUTE, AbsoluteAxisType::ABS_Y.0, abs_y),
            EvInputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])?;
        Ok(())
    }

    pub fn inject_mouse_button(&mut self, button: u32, pressed: bool) -> Result<()> {
        // Wayland button codes: 272=BTN_LEFT, 273=BTN_RIGHT, 274=BTN_MIDDLE
        let key = match button {
            272 => Key::BTN_LEFT,
            273 => Key::BTN_RIGHT,
            274 => Key::BTN_MIDDLE,
            _ => return Ok(()), // Ignore unknown buttons
        };

        let value = if pressed { 1 } else { 0 };
        self.mouse.emit(&[
            EvInputEvent::new(EventType::KEY, key.code(), value),
            EvInputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])?;
        Ok(())
    }
}
