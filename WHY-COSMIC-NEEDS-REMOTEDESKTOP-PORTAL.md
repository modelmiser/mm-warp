# Why COSMIC Needs RemoteDesktop Portal Support

**Date:** October 22, 2025
**Context:** Issue #23 in xdg-desktop-portal-cosmic
**Reference:** mm-warp development experience

---

## TL;DR

**The standard exists** (RemoteDesktop portal in xdg-desktop-portal 1.18).
**COSMIC hasn't implemented it yet** (tracked in issue #23).
**Here's why it matters** (real-world use case: mm-warp).

---

## What We Built

**mm-warp:** Native Wayland remote desktop for COSMIC
https://github.com/modelmiser/mm-warp

**Working:**
- ✅ 4K screen capture via ext-image-copy-capture-v1
- ✅ H.264 streaming over QUIC (18-20 FPS)
- ✅ Keyboard/mouse capture from Wayland window
- ✅ Works great on COSMIC

**The Problem:**
- ❌ Input injection uses **ydotool** (kernel-level hack)
- ❌ Input is **global** (not session-bound to captured output)
- ❌ Requires root access and external daemon

**What We Need:**
- ✅ Session-bound input (inject to captured desktop, not global)
- ✅ Proper security model (portal permissions)
- ✅ No kernel hacks

---

## The Standard That Already Exists

**XDG RemoteDesktop Portal** (xdg-desktop-portal 1.18+):

**Screen capture:** Already supported by COSMIC ✅

**Input injection:** Not yet implemented ❌
- `SelectDevices()` - Choose which devices to control
- `ConnectToEIS()` - Connect to input capture system
- Session-bound (input goes to captured session)
- Secure (portal permission model)

**Reference:** https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html

---

## Why This Matters for COSMIC

### 1. Enterprise Use Cases

**System76 sells Linux workstations.** Customers need remote access:
- Remote administration (sysadmin controlling servers)
- IT support (helping users troubleshoot)
- Remote work (accessing office workstation from home)
- Development (accessing build servers)

**Current answer:** "Install ydotool and run a daemon as root"
**Competitor answer (Windows/Mac):** RDP/VNC just works

### 2. Competitive Positioning

**COSMIC is marketing as modern desktop environment.**

**Modern means:**
- Built-in remote desktop (not hacky third-party tools)
- Secure by default (portal permissions, not root daemons)
- Works out of the box (no ydotool installation)

**Right now:**
- Screen sharing works (portal implemented) ✅
- Remote control doesn't (portal not implemented) ❌

**Half-solved problem is worse than no solution** - users expect remote desktop to include input.

### 3. Wayland Ecosystem Leadership

**COSMIC is uniquely positioned:**
- Modern compositor (no X11 legacy)
- Rust stack (easier to implement than C compositors)
- Growing adoption (Pop!_OS users expect features)
- System76 backing (engineering resources available)

**Be the reference implementation:**
- Implement RemoteDesktop portal properly
- Show other compositors how it's done
- "COSMIC has best Wayland remote desktop support"

---

## What mm-warp Experience Taught Us

### Current Workaround (ydotool)

**Implementation:**
```rust
pub fn inject_mouse_move(&mut self, x: i32, y: i32) -> Result<()> {
    std::process::Command::new("ydotool")
        .args(&["mousemove", "--absolute", &x.to_string(), &y.to_string()])
        .output()
        .context("ydotool not found")?;
    Ok(())
}
```

**Problems encountered:**
1. **External dependency** - Users must install ydotool separately
2. **Daemon requirement** - ydotoold must run as root
3. **Global injection** - Goes to focused window, not captured session
4. **Local testing confusion** - Can't test on same machine properly
5. **Security theater** - Requires root for "secure" Wayland

**README disclaimer:**
> "mm-warp uses ydotool for mouse injection. This is duct tape, and we're transparent about it."

### What RemoteDesktop Portal Would Fix

**With proper portal:**
```rust
// Hypothetical code when COSMIC implements portal
pub fn inject_mouse_move(&mut self, x: i32, y: i32) -> Result<()> {
    self.portal_session.inject_pointer_motion(x as f64, y as f64)?;
    Ok(())
}
```

**Benefits:**
1. ✅ **No external dependencies** - Portal is built-in
2. ✅ **No root required** - User grants permission via dialog
3. ✅ **Session-bound** - Input goes to captured output
4. ✅ **Proper security** - Revocable, auditable, token-based
5. ✅ **Local testing works** - Input bound to session, not global focus

---

## The Reality Check

**We're NOT proposing something new.**
**We're asking COSMIC to implement an existing standard.**

**The standard is good:**
- Designed by freedesktop.org (not one vendor)
- Already implemented by GNOME, KDE (partially)
- Proven architecture (screen capture works this way)
- Secure by design (portal permission model)

**COSMIC just needs to implement it.**

---

## What We Can Offer

### 1. Real Use Case

**mm-warp proves people want this:**
- Built a working remote desktop
- Hit the input injection wall immediately
- Had to use kernel hacks (ydotool)
- Ready to switch to portal when available

### 2. Testing & Feedback

**We'll test your implementation:**
- Port mm-warp to use RemoteDesktop portal
- Report bugs, edge cases, performance
- Provide feedback on DX (developer experience)
- Help document "how to use this portal"

### 3. Reference Client

**mm-warp can be the showcase:**
- "Here's a remote desktop built on COSMIC portals"
- Clean Rust code (similar to COSMIC's stack)
- Demonstrates proper portal usage
- Marketing: "See COSMIC's remote desktop in action"

### 4. Implementation Help (If Wanted)

**If System76 is open to PRs:**
- We can help implement the portal
- Already familiar with Wayland/Pipewire protocols
- Rust experience (mm-warp is all Rust)
- Motivated (we need this for mm-warp!)

**Or we can just wait** - no pressure, happy to test when ready.

---

## Timeline & Priority

**Where this ranks:**

**Must-have for v1.0?** No - COSMIC is usable without it
**Nice-to-have for enterprise?** Yes - makes Pop!_OS workstations more sellable
**Ecosystem leadership?** Yes - be first compositor with proper implementation

**Effort estimate:**
- Portal implementation: 2-4 weeks (based on existing screen capture portal)
- Testing & polish: 1-2 weeks
- **Total:** 1-2 months for solid implementation

**Our timeline:**
- mm-warp works with ydotool today (shipped)
- We can wait 1-2 months for proper solution
- Happy to help if it speeds things up

---

## Comparison: Current vs With Portal

### User Experience

**Today (ydotool):**
```bash
# User setup:
sudo apt install ydotool
sudo ydotoold &

# Then run mm-warp
./server  # Connects to Wayland, creates uinput device
./client  # Input injection works (but global)
```

**With RemoteDesktop portal:**
```bash
# User setup:
# (nothing - portal is built-in)

# Run mm-warp
./server  # Connects to Wayland, requests RemoteDesktop permission
# Dialog: "Allow mm-warp to view and control Desktop 1?" [Allow]
./client  # Input injection works (session-bound!)
```

**Cleaner, more secure, more usable.**

### Developer Experience

**Today (ydotool):**
```rust
// External process spawning (1ms overhead per event)
std::process::Command::new("ydotool")
    .args(&["mousemove", &x.to_string(), &y.to_string()])
    .output()?;
```

**With RemoteDesktop portal:**
```rust
// Clean API (microseconds, session-bound)
portal.inject_pointer_motion(session_id, x, y).await?;
```

**Simpler, faster, proper abstraction.**

---

## Questions We Can Answer

**Q: Is there actual demand for this?**
**A:** Yes - mm-warp exists because we need it. Remote desktop is table-stakes for enterprise.

**Q: Will you actually use it if we implement it?**
**A:** Absolutely - we'll port mm-warp immediately and help document the migration path.

**Q: Can you help implement?**
**A:** If you're open to PRs, yes. If not, we'll wait and test.

**Q: What about performance?**
**A:** Current setup works at 18-20 FPS, ~5-10% CPU. Portal won't hurt performance (removes ydotool process spawning overhead if anything).

**Q: Will this work for X use case?**
**A:** Happy to test whatever scenarios you need validated.

---

## How We Can Help (Concretely)

### Option 1: You Implement, We Test
- You write the portal code
- We port mm-warp to use it
- We report bugs, test edge cases
- We document usage for other developers

### Option 2: We Contribute PR
- We implement RemoteDesktop portal in xdg-desktop-portal-cosmic
- You review and guide
- We iterate until it meets your standards
- You merge when ready

### Option 3: Just Wait
- You implement when priority allows
- We use ydotool until then
- No pressure, we're patient

**Your call!** We're flexible and happy to contribute however helps most.

---

## The Bottom Line

**You don't need a proposal from us.**
**You already have issue #23 tracking this.**

**What you need:**
- Someone who cares enough to implement it (us?)
- Real use case to validate against (mm-warp ✅)
- Testing and feedback (we'll provide)

**What we need:**
- RemoteDesktop portal implemented in COSMIC
- Session-bound input injection
- No more ydotool kernel hacks

**Let's work together on this.** 🦬☀️

---

## References

- **COSMIC Issue:** https://github.com/pop-os/xdg-desktop-portal-cosmic/issues/23
- **RemoteDesktop Portal Spec:** https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html
- **mm-warp:** https://github.com/modelmiser/mm-warp
- **Input Capture Portal:** https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.InputCapture.html

---

**No grand proposals needed. Just: "We need this, can we help implement it?"**

Simple. Honest. Tasteful. 🦬☀️
