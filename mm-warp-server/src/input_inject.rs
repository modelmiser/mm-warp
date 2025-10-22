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
}
