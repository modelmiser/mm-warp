# ext-image-copy-capture-v1 Implementation Plan

**Status**: Ready to implement - all dependencies confirmed!

## Discovery: The Staging Feature

**KEY FINDING**: ext-image-copy-capture-v1 modules ARE available in `wayland-protocols` 0.32, but require the `staging` feature flag!

```toml
wayland-protocols = { version = "0.32", features = ["client", "staging"] }
```

**Confirmed available types**:
- `ExtImageCopyCaptureManagerV1`
- `ExtImageCopyCaptureSessionV1`
- `ExtImageCopyCaptureFrameV1`
- `ExtOutputImageCaptureSourceManagerV1`
- `ExtImageCaptureSourceV1`

✅ **Test**: See `test-ext-available/` - compiles and runs successfully!

---

## Reference Implementation

**Source**: `wl-screenrec` (https://github.com/russelltg/wl-screenrec)
**File**: `/tmp/wl-screenrec/src/cap_ext_image_copy.rs`

This is a working, production implementation we can adapt from.

### Key Pattern (from wl-screenrec)

```rust
use wayland_protocols::ext::{
    image_capture_source::v1::client::{
        ext_image_capture_source_v1::ExtImageCaptureSourceV1,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_frame_v1::ExtImageCopyCaptureFrameV1,
        ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
        ext_image_copy_capture_session_v1::ExtImageCopyCaptureSessionV1,
    },
};

// 1. Bind managers
let source_manager: ExtOutputImageCaptureSourceManagerV1 = /* bind from registry */;
let copy_manager: ExtImageCopyCaptureManagerV1 = /* bind from registry */;

// 2. Create source from output
let source = source_manager.create_source(&output, &qh, ());

// 3. Create capture session
let session = copy_manager.create_session(&source, OPTIONS, &qh, ());

// 4. Handle session events (BufferSize, DmabufFormat, Done)
// 5. Create frame when ready
let frame = session.create_frame(&qh, ());

// 6. Attach buffer and capture
frame.attach_buffer(&buffer);
frame.damage_buffer(x, y, width, height);
frame.capture();

// 7. Wait for Ready event
```

---

## Implementation Steps

### Step 1: Create New Module (30-60 min)

**File**: `mm-warp-server/src/ext_capture.rs`

Based on wl-screenrec's implementation:
- Define `ExtCapture` struct
- Implement Wayland event dispatchers
- Handle session lifecycle
- Capture single frames (for mm-warp use case)

**Simpler than wl-screenrec**: We don't need dmabuf/GPU encoding - just copy to RAM like our existing implementation.

### Step 2: Add to lib.rs (10 min)

```rust
pub mod ext_capture;

pub use ext_capture::ExtCapture;
```

### Step 3: Test on COSMIC (10 min)

```bash
cargo run --bin test_ext_capture
```

Should show the COSMIC screen!

### Step 4: Integration (30 min)

Update `WaylandConnection` or create new abstraction:

```rust
pub enum ScreenCapture {
    ExtImageCopy(ExtCapture),     // COSMIC, newer compositors
    WlrScreencopy(WlrCapture),    // Sway, Hyprland
}

impl ScreenCapture {
    pub fn new() -> Result<Self> {
        // Try ext-image-copy-capture first (newer)
        if let Ok(ext) = ExtCapture::new() {
            return Ok(Self::ExtImageCopy(ext));
        }

        // Fall back to wlr-screencopy
        Ok(Self::WlrScreencopy(WlrCapture::new()?))
    }

    pub fn capture_frame(&mut self) -> Result<Vec<u8>> {
        match self {
            Self::ExtImageCopy(c) => c.capture(),
            Self::WlrScreencopy(c) => c.capture(),
        }
    }
}
```

---

## Current State of lib.rs

**Problem**: lib.rs has the old WlrScreencopy implementation partially removed, causing 69 compilation errors.

**Options**:

### Option A: Fix and Keep Both (Recommended)
1. Re-add missing imports (memmap2, nix, wayland-protocols-wlr)
2. Keep WlrScreencopy as is
3. Add ExtCapture as new module
4. Create ScreenCapture enum with fallback logic

**Time**: 1-2 hours
**Result**: Works on ALL compositors (wlroots + COSMIC + future)

### Option B: Replace with Ext Only
1. Remove all WlrScreencopy code
2. Implement only ExtCapture
3. Works on COSMIC

**Time**: 1 hour
**Result**: Breaks Sway/Hyprland support (bad trade-off)

---

## Dependencies Needed

### Already Have
- ✅ wayland-client 0.31
- ✅ wayland-protocols 0.32 (with staging feature)

### Need to Add Back (for WlrScreencopy)
```toml
wayland-protocols-wlr = { version = "0.3", features = ["client"] }
memmap2 = "0.9"
nix = { version = "0.29", features = ["fs", "mman"] }
```

### For ExtCapture (simpler - no DRM/GPU needed for our use case)
- Just wayland-protocols with staging ✅

---

## Recommended Next Session Plan

**Goal**: Get COSMIC support working while keeping Sway/Hyprland support

**Tasks** (2-3 hours total):

1. **Restore dependencies** (5 min)
   - Add back wayland-protocols-wlr, memmap2, nix
   - Verify lib.rs compiles

2. **Create ext_capture.rs** (60-90 min)
   - Copy pattern from wl-screenrec
   - Simplify for our use case (no GPU encoding)
   - Just get RGB frames to Vec<u8>

3. **Add ScreenCapture enum** (30 min)
   - Abstraction over both implementations
   - Fallback logic (try ext first, then wlr)

4. **Test on COSMIC** (10 min)
   - cargo run --bin test_screencopy
   - Should work!

5. **Update docs** (15 min)
   - README: Update compositor support
   - Commit with clear message

---

## Why This Will Work

1. **ext modules confirmed available** (we tested!)
2. **Reference implementation exists** (wl-screenrec)
3. **Pattern is familiar** (similar to what we already did for wlr)
4. **No complex dependencies** (for our simple use case)
5. **Backwards compatible** (keeps existing Sway/Hyprland support)

---

## Session Notes

**What went well**:
- ✅ Found the staging feature requirement
- ✅ Confirmed ext modules compile
- ✅ Located working reference implementation
- ✅ Documented clear path forward

**What was challenging**:
- Session instability (stream crashes)
- Tool permission failures
- Large existing codebase to navigate

**Key insight**: Don't need portal+PipeWire complexity. The ext protocol is just another Wayland protocol, similar to wlr-screencopy we already implemented!

---

🦬☀️ **The path is clear. The tools exist. Time to build.**
