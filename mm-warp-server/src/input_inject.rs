// Input injection using uinput (Linux virtual input device)
// Keyboard AND mouse via evdev — no external tools (ydotool) needed.
use anyhow::{Context, Result};
use evdev::{uinput::VirtualDeviceBuilder, AbsInfo, AbsoluteAxisType, AttributeSet, InputEvent as EvInputEvent, EventType, Key, RelativeAxisType, UinputAbsSetup};

pub struct InputInjector {
    keyboard: evdev::uinput::VirtualDevice,
    mouse: evdev::uinput::VirtualDevice,
}

impl InputInjector {
    /// Linux KEY_MAX — keycodes above this are not valid evdev keys.
    const KEY_MAX: u16 = 767;

    /// Safe key code ranges for remote injection (allowlist approach).
    /// Only standard keyboard keys — no system power, RF kill, or device toggles.
    fn is_safe_key(key: u32) -> bool {
        matches!(key,
            // Standard keys: ESC through backslash (1-43)
            1..=43 |
            // Enter through grave (28, 44-53)
            44..=53 |
            // Shift, ctrl, alt, space, caps, F1-F10 (54-68)
            54..=68 |
            // Numlock through keypad-dot (69-83)
            69..=83 |
            // F11-F12 (87-88)
            87..=88 |
            // Keypad enter, right ctrl (96-97)
            96..=97 |
            // Right alt, home/up/pgup/left/right/end/down/pgdn/ins/del (100-111)
            100..=111 |
            // Pause (119)
            119 |
            // Left/right meta (125-126)
            125..=126 |
            // F13-F24 (183-194) — extended function keys
            183..=194
        )
    }

    pub fn new() -> Result<Self> {
        // Keyboard: register all valid keys (0..=KEY_MAX)
        let mut keys = AttributeSet::<Key>::new();
        for key_code in 0..=Self::KEY_MAX {
            keys.insert(Key::new(key_code));
        }

        let keyboard = VirtualDeviceBuilder::new()?
            .name("mm-warp Keyboard")
            .with_keys(&keys)?
            .build()
            .context("Failed to build virtual keyboard")?;

        // Mouse: absolute positioning + buttons + scroll wheel
        let mut mouse_keys = AttributeSet::<Key>::new();
        mouse_keys.insert(Key::BTN_LEFT);
        mouse_keys.insert(Key::BTN_RIGHT);
        mouse_keys.insert(Key::BTN_MIDDLE);
        mouse_keys.insert(Key::BTN_SIDE);
        mouse_keys.insert(Key::BTN_EXTRA);

        // Screen dimensions for absolute positioning (compositor scales)
        let abs_info = AbsInfo::new(0, 0, 32767, 0, 0, 1);

        // Relative axes for scroll wheel
        let mut rel_axes = AttributeSet::<RelativeAxisType>::new();
        rel_axes.insert(RelativeAxisType::REL_WHEEL);
        rel_axes.insert(RelativeAxisType::REL_HWHEEL);

        let mouse = VirtualDeviceBuilder::new()?
            .name("mm-warp Mouse")
            .with_keys(&mouse_keys)?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_X, abs_info))?
            .with_absolute_axis(&UinputAbsSetup::new(AbsoluteAxisType::ABS_Y, abs_info))?
            .with_relative_axes(&rel_axes)?
            .build()
            .context("Failed to build virtual mouse")?;

        Ok(Self { keyboard, mouse })
    }

    /// Dispatch any InputEvent to the appropriate virtual device.
    /// `capture_width` and `capture_height` are used to normalize mouse
    /// coordinates from buffer-space pixels to the 0-32767 absolute range.
    pub fn inject(&mut self, event: &mm_warp_common::InputEvent, capture_width: u32, capture_height: u32) -> Result<()> {
        match event {
            mm_warp_common::InputEvent::KeyPress { key } => self.inject_key(*key, true),
            mm_warp_common::InputEvent::KeyRelease { key } => self.inject_key(*key, false),
            mm_warp_common::InputEvent::MouseMove { x, y } => self.inject_mouse_move(*x, *y, capture_width, capture_height),
            mm_warp_common::InputEvent::MouseButton { button, pressed } => self.inject_mouse_button(*button, *pressed),
            mm_warp_common::InputEvent::MouseScroll { axis, value } => self.inject_mouse_scroll(*axis, *value),
        }
    }

    pub fn inject_key(&mut self, key: u32, pressed: bool) -> Result<()> {
        if key > Self::KEY_MAX as u32 {
            anyhow::bail!("Key code {} out of range (max {})", key, Self::KEY_MAX);
        }
        // Allowlist: only standard keyboard keys, no system power/RF/device toggles
        if !Self::is_safe_key(key) {
            tracing::debug!("Blocked non-keyboard key code {} from remote client", key);
            return Ok(());
        }
        let key_obj = Key::new(key as u16);
        let value = if pressed { 1 } else { 0 };
        self.keyboard.emit(&[
            EvInputEvent::new(EventType::KEY, key_obj.code(), value),
            EvInputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])?;
        Ok(())
    }

    /// Inject mouse move with coordinate normalization.
    /// `x` and `y` are buffer-space pixel coordinates from the client.
    /// These are mapped to the 0-32767 absolute axis range using the
    /// capture resolution, so the cursor reaches all edges of the screen.
    pub fn inject_mouse_move(&mut self, x: i32, y: i32, capture_width: u32, capture_height: u32) -> Result<()> {
        // Map [0, width-1] → [0, 32767] so the cursor reaches all screen edges.
        // Divide by (dimension - 1) to ensure the last pixel maps to 32767.
        let abs_x = if capture_width > 1 {
            (((x.max(0) as u64) * 32767) / (capture_width as u64 - 1)).min(32767) as i32
        } else {
            x.max(0).min(32767)
        };
        let abs_y = if capture_height > 1 {
            (((y.max(0) as u64) * 32767) / (capture_height as u64 - 1)).min(32767) as i32
        } else {
            y.max(0).min(32767)
        };

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
            275 => Key::BTN_SIDE,
            276 => Key::BTN_EXTRA,
            _ => {
                tracing::debug!("Ignoring unrecognized mouse button code {}", button);
                return Ok(());
            }
        };

        let value = if pressed { 1 } else { 0 };
        self.mouse.emit(&[
            EvInputEvent::new(EventType::KEY, key.code(), value),
            EvInputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])?;
        Ok(())
    }

    pub fn inject_mouse_scroll(&mut self, axis: u32, value: i32) -> Result<()> {
        // axis: 0 = vertical (REL_WHEEL), 1 = horizontal (REL_HWHEEL)
        let rel_axis = match axis {
            0 => RelativeAxisType::REL_WHEEL,
            1 => RelativeAxisType::REL_HWHEEL,
            _ => {
                tracing::debug!("Ignoring unrecognized scroll axis {}", axis);
                return Ok(());
            }
        };

        self.mouse.emit(&[
            EvInputEvent::new(EventType::RELATIVE, rel_axis.0, value),
            EvInputEvent::new(EventType::SYNCHRONIZATION, 0, 0),
        ])?;
        Ok(())
    }
}
