# Pipewire Remote Desktop Input Injection - Proposal

**Date:** October 22, 2025
**Author:** ModelMiser (via mm-warp development)
**Target:** Pipewire project, System76/COSMIC, Wayland ecosystem

---

## Executive Summary

**Problem:** Wayland has no standard for remote desktop input injection, forcing developers to use kernel-level hacks (ydotool, evemu) that work globally instead of being session-bound.

**Solution:** Extend Pipewire's existing remote desktop portal to include input injection, bound to the same session as screen capture.

**Benefits:**
- Universal (works on all compositors)
- Secure (session-bound, token-based)
- Proper abstraction (remote desktop = capture + input)
- Eliminates kernel hacks

**Status:** Proven need via mm-warp implementation (working remote desktop for COSMIC that needs this)

---

## The Problem

### Current State (2025)

**Wayland has been around for 17 years.** Remote desktop input injection is still unsolved:

**No Standard Protocol:**
- X11 had XTest (insecure, but worked)
- RDP has built-in input (Microsoft, 25+ years old)
- VNC has built-in input (universal standard)
- **Wayland:** Nothing

**Fragmented Solutions:**

| Solution | Compositors | Architecture | Issues |
|----------|------------|--------------|--------|
| `wlr-virtual-pointer-v1` | Sway, Hyprland | Compositor protocol | wlroots only |
| ydotool | All | Kernel uinput | Global, not session-bound |
| evemu | All | Kernel evdev | Global, not session-bound |
| Nothing | COSMIC, GNOME, KDE | N/A | No solution |

**Result:** Developers use kernel hacks that work globally instead of per-session.

### Why This Matters

**Enterprise Use Cases:**
- Remote administration (sysadmin controlling servers)
- Support (IT helping users)
- Remote work (accessing office workstation from home)
- Automation (testing, CI/CD)

**All require input injection.** Current solutions are:
- ❌ Fragmented (different per compositor)
- ❌ Insecure (global input, not session-bound)
- ❌ Hacky (kernel workarounds)

**This hurts Linux adoption in enterprise environments.**

---

## Why Pipewire is the Right Layer

**Pipewire already owns "remote desktop" as a concept:**

✅ **Screen capture** - `org.freedesktop.portal.ScreenCast`
✅ **Audio capture** - Part of the same session
❌ **Input injection** - Missing piece!

**Architectural fit:**
- Remote desktop is ONE concern (video + audio + input)
- All three should be **session-bound** (same security model)
- Pipewire already does 2/3 - adding input completes it

**Benefits of Pipewire layer:**
- **Universal:** Works on any compositor that supports Pipewire (already all of them)
- **Secure:** Reuses existing portal permission model
- **Session-bound:** Input goes to the session being captured (not global!)
- **Token-based:** Revocable, time-limited, auditable

---

## Proposed Architecture

### High-Level Flow

```
┌─────────────────┐
│  Client App     │ (mm-warp, RustDesk, etc.)
│  (Remote)       │
└────────┬────────┘
         │
    ╔════▼════════════════════════════════════════╗
    ║         Pipewire Remote Desktop            ║
    ║  ┌──────────────┐  ┌─────────────────┐   ║
    ║  │Screen Capture│  │ Input Injection │   ║
    ║  │   Stream     │  │    Channel      │   ║
    ║  └──────┬───────┘  └────────┬────────┘   ║
    ╚═════════╬══════════════════╬══════════════╝
              ║                  ║
         ╔════▼══════════════════▼═══════╗
         ║      Wayland Compositor       ║
         ║  ┌─────────┐   ┌───────────┐ ║
         ║  │ Capture │   │  Inject   │ ║
         ║  │ Output  │   │  Input    │ ║
         ║  └─────────┘   └───────────┘ ║
         ╚═══════════════════════════════╝
```

### Protocol Extension

**New Pipewire Protocol:** `org.freedesktop.portal.RemoteDesktop` (extend existing)

**Add input injection methods:**
```c
// Inject keyboard event into remote desktop session
void InjectKeyboard(
    IN handle: ObjectPath,      // Session handle
    IN key: uint32,              // Evdev keycode
    IN state: uint32             // 0=release, 1=press
);

// Inject pointer motion into remote desktop session
void InjectPointerMotion(
    IN handle: ObjectPath,       // Session handle
    IN dx: double,               // Relative X movement
    IN dy: double                // Relative Y movement
);

// Inject pointer button into remote desktop session
void InjectPointerButton(
    IN handle: ObjectPath,       // Session handle
    IN button: uint32,           // Button code
    IN state: uint32             // 0=release, 1=press
);
```

### Security Model

**Permission Flow:**
1. Client requests remote desktop session (existing)
2. **Portal asks user:** "Allow [App] to view and control this desktop?" (NEW)
3. User approves → Session token granted
4. Token allows BOTH capture and input
5. Token is revocable, auditable, time-limited (optional)

**For unattended access:**
- Token can be stored (like SSH keys)
- Configurable policies: always allow from IP X, time-limited, etc.
- System settings: "Allowed remote desktop clients"

**Compositor receives:**
- Session ID from Pipewire
- Input events tagged with session ID
- Compositor injects into the output/workspace being captured
- **Session-bound, not global!**

---

## Implementation Roadmap

### Phase 1: Pipewire API (3-6 months)
- Extend `org.freedesktop.portal.RemoteDesktop` portal
- Add `InjectKeyboard`, `InjectPointerMotion`, `InjectPointerButton` methods
- Define session token security model
- Update portal UI for "view and control" permission

### Phase 2: Compositor Protocol (each compositor)
- Define `zwp-pipewire-input-injection-v1` protocol
- Compositor implements protocol
- Receives input from Pipewire, injects to session

**Per-compositor timeline:**
- **COSMIC** (System76): 1-2 months (Rust, modern, motivated)
- **Sway/wlroots**: 2-3 months (C, already have wlr-virtual-pointer)
- **GNOME**: 3-6 months (large project, slow-moving)
- **KDE**: 2-4 months (active development)

### Phase 3: Client Adoption (1-2 months each)
- Update mm-warp, RustDesk, etc. to use Pipewire input
- Deprecate ydotool/kernel hacks
- Feature parity with RDP/VNC

**Total timeline:** 12-18 months from approval to widespread adoption

---

## Why System76 Should Lead This

**You're uniquely positioned:**

✅ **COSMIC is new** - No legacy protocol baggage, can implement cleanly
✅ **Rust ecosystem** - Pipewire bindings, Smithay integration, modern stack
✅ **Enterprise focus** - Selling Linux workstations requires remote desktop
✅ **Community respect** - Leading Pop!_OS, COSMIC adoption growing
✅ **Engineering capacity** - Paid team, can dedicate resources

**Competitive advantage:**
- **First** compositor with proper Pipewire remote desktop
- **Marketing:** "COSMIC has enterprise-grade remote desktop built-in"
- **Ecosystem leadership:** Like cosmic-epoch, cosmic-settings
- **While others catch up:** 6-12 month head start

**ROI for System76:**
- **Enterprise sales** - "Can we remote into Pop!_OS workstations?" → YES
- **Ecosystem fix** - Rising tide lifts all boats (Wayland looks better)
- **Technical prestige** - "System76 solved Wayland remote desktop"

---

## Reference Implementation

**mm-warp** (this project) proves the need:

**What works:**
- ✅ 4K screen capture via `ext-image-copy-capture-v1` (COSMIC)
- ✅ H.264 streaming (efficient, real-time)
- ✅ Keyboard/mouse capture (Wayland seat binding)
- ✅ Network protocol (QUIC, encrypted)

**What's hacky:**
- ⚠️ Input injection via ydotool (kernel hack)
- ⚠️ Global injection (not session-bound)
- ⚠️ External dependency (user must install ydotool)

**With Pipewire input:**
- ✅ Session-bound injection
- ✅ No kernel hacks
- ✅ Proper security model
- ✅ Universal (all compositors)

**mm-warp can be the reference client** when Pipewire adds input injection.

---

## Comparison to Existing Solutions

### X11 (XTest)
- ✅ Universal, simple API
- ❌ Completely insecure (any app can inject)
- ❌ Global, not session-aware

### RDP (Microsoft)
- ✅ Input integrated with capture
- ✅ Session-bound
- ✅ 25+ years of refinement
- ❌ Proprietary, Windows-centric

### VNC
- ✅ Input integrated with capture
- ✅ Universal standard
- ✅ Simple protocol
- ❌ Less secure (varies by implementation)

### Pipewire (Proposed)
- ✅ Session-bound (secure)
- ✅ Universal (all compositors)
- ✅ Integrated (capture + input together)
- ✅ Modern security (tokens, revocation)
- ✅ Open standard

**Pipewire would be BETTER than RDP/VNC** for Linux remote desktop.

---

## Technical Details

### Session Binding

**The key innovation:** Input is bound to the **session**, not global.

**Example:**
```
Session A (ID: sess_001):
- Capturing: Output "DP-1" (external monitor)
- Input goes to: Applications on DP-1
- NOT to: Other outputs, other sessions

Session B (ID: sess_002):
- Capturing: Output "eDP-1" (laptop screen)
- Input goes to: Applications on eDP-1
- NOT to: Session A's output
```

**Implementation:**
- Pipewire tracks session → output mapping
- Compositor receives: (session_id, input_event)
- Compositor routes input to output associated with session_id

### Security Model

**Permission Grant:**
```
User grants: "Allow [mm-warp] to view and control Desktop 1"
Token generated: sess_abc123 (cryptographic, revocable)
Client uses: All capture/input requests include token
Compositor validates: Token is valid, not expired, matches session
```

**Revocation:**
- User can kill session anytime (portal UI: "Active Remote Desktop Sessions")
- Token expires (configurable: 1 hour, 1 day, permanent)
- Network disconnect → session ends → token invalidated

**Audit:**
- System logs: "Session sess_abc123 injected 1,247 keyboard events"
- User can review: "Who accessed my desktop and when?"

---

## Alternatives Considered

### Alternative 1: Compositor-Specific Protocols

**Status:** Already happening (wlr-virtual-pointer for wlroots)

**Problems:**
- Fragmentation (different protocol per compositor)
- Maintenance burden (clients need N implementations)
- Still need fallback for unsupported compositors

**Verdict:** Solves technical problem, creates ecosystem problem.

### Alternative 2: Wayland Core Protocol

**Status:** Unlikely (17 years, no progress)

**Problems:**
- Wayland philosophy: "Compositors decide" (no mandatory protocols)
- Security concerns paralyze consensus
- No governance forcing adoption

**Verdict:** Theoretically ideal, practically impossible.

### Alternative 3: Kernel-Level (ydotool, status quo)

**Status:** Current reality

**Problems:**
- Global injection (not session-bound)
- Requires root/uinput access
- Bypasses compositor security model
- Not elegant

**Verdict:** Works, but we can do better.

### Alternative 4: Pipewire Input (PROPOSED)

**Combines best of all:**
- ✅ Universal (like kernel hacks)
- ✅ Secure (like compositor protocols)
- ✅ Session-bound (like RDP/VNC)
- ✅ Standard (one implementation)

**Verdict:** Best architectural solution.

---

## Call to Action

### For System76:

**Lead this initiative:**
1. Implement in COSMIC first (prove it works)
2. Propose to Pipewire project (with working code)
3. Help other compositors adopt (Sway, GNOME, KDE)
4. Own "System76 fixed Wayland remote desktop"

**Benefits:**
- Competitive advantage (COSMIC has it first)
- Enterprise credibility (proper remote desktop)
- Ecosystem leadership (rising tide lifts all boats)
- Marketing win ("Pop!_OS workstations: remote access that just works")

### For Pipewire Project:

**This is your domain:**
- You already own remote desktop capture
- Input is the natural extension
- Architecture already exists (portals, sessions, tokens)
- Missing piece preventing Wayland from competing with RDP/VNC

### For Other Compositor Developers:

**Implement the protocol when it exists:**
- System76 will prove it works (COSMIC first)
- Reduces your engineering effort (standard to follow)
- Benefits your users (proper remote desktop)

---

## Reference: mm-warp Experience

**What we built:**
- Native Wayland remote desktop for COSMIC
- 4K H.264 streaming, keyboard/mouse capture
- **Currently uses ydotool** (the kernel hack)

**What we learned:**
- Input injection is **the** blocker for proper remote desktop
- ydotool works but is global (not session-bound)
- Local testing is confusing (input goes to wrong session)
- Remote access works but only by luck (single session)

**What we need:**
- Session-bound input (inject to captured output)
- Security model (authentication, revocation)
- Universal protocol (not per-compositor)

**We'll implement Pipewire input in mm-warp** as soon as the protocol exists.

---

## Technical Specification (Draft)

### Pipewire API Extension

**New object:** `PipeWireRemoteDesktopInput`

**Methods:**
```
InjectKeyboard(session_handle, keycode, state)
InjectPointerMotion(session_handle, dx, dy)
InjectPointerButton(session_handle, button, state)
InjectPointerAxis(session_handle, axis, value)
InjectTouch(session_handle, id, x, y, state)
```

**Session handle:** Links input to capture session (security boundary)

### Compositor Protocol (New)

**Protocol:** `zwp-pipewire-input-injection-v1`

**Flow:**
1. Compositor implements protocol
2. Pipewire connects to compositor
3. When input event received from portal:
   - Pipewire validates session token
   - Sends to compositor with session_id
   - Compositor injects to output associated with session_id

**Interface (Wayland XML):**
```xml
<interface name="zwp_pipewire_input_injection_v1" version="1">
  <request name="inject_keyboard">
    <arg name="session_id" type="string"/>
    <arg name="keycode" type="uint"/>
    <arg name="state" type="uint"/>
  </request>

  <request name="inject_pointer_motion">
    <arg name="session_id" type="string"/>
    <arg name="dx" type="fixed"/>
    <arg name="dy" type="fixed"/>
  </request>

  <!-- Additional methods... -->
</interface>
```

---

## Migration Path

**For existing tools:**

**Phase 1:** Pipewire input available on COSMIC
- mm-warp implements Pipewire input (feature flag)
- Falls back to ydotool on other compositors
- COSMIC users get proper session-bound input

**Phase 2:** Other compositors adopt
- Sway, GNOME, KDE implement protocol
- mm-warp automatically uses it (feature detection)
- ydotool becomes rare fallback

**Phase 3:** Deprecate kernel hacks
- All major compositors support Pipewire input
- ydotool only needed for legacy systems
- Wayland finally has proper remote desktop

**Timeline:** 12-18 months from starting implementation to broad adoption

---

## Security Considerations

### Threat Model

**Threats to mitigate:**
- Malicious app requesting input control
- Stolen session tokens
- Privilege escalation
- Unintended input to wrong session

**Mitigations:**

**1. Permission Dialog (First Access):**
```
┌────────────────────────────────────────┐
│  Remote Desktop Permission Request    │
├────────────────────────────────────────┤
│  Application: mm-warp                  │
│  IP Address: 192.168.1.100            │
│                                        │
│  Requesting permission to:             │
│  ✓ View your screen                   │
│  ✓ Control keyboard and mouse         │
│                                        │
│  [ ] Remember this decision            │
│                                        │
│     [Deny]              [Allow]        │
└────────────────────────────────────────┘
```

**2. Active Session Indicator:**
- Tray icon: "Remote desktop active"
- Click to see: Connected clients, session duration
- Quick revoke: "Disconnect all"

**3. Token Security:**
- Cryptographically random (no prediction)
- Short-lived by default (1 hour)
- Optional: Persistent tokens (stored in keyring)
- IP-bound (optional): Token only valid from specific IP

**4. Audit Trail:**
- System logs all input injection events
- User can review: "What was done during session X?"
- Forensics: "Who pressed Delete on my files?"

### Comparison to Current Solutions

**ydotool security:**
- ❌ Requires root (full system access)
- ❌ Global (affects all sessions)
- ❌ No audit trail
- ❌ No revocation (kill process = only way to stop)

**Pipewire security:**
- ✅ User grants permission (explicit)
- ✅ Session-bound (isolated)
- ✅ Audit trail (system logs)
- ✅ Revocable (portal UI)

**Result:** MUCH more secure than status quo.

---

## FAQ

### Q: Why not just fix ydotool?

**A:** ydotool is a kernel-level hack. You can't make it session-bound without compositor cooperation. If you have compositor cooperation, you should use proper protocols, not kernel hacks.

### Q: What about wlr-virtual-pointer?

**A:** Great for wlroots compositors! But:
- Doesn't work on COSMIC, GNOME, KDE
- Still compositor-specific (not universal)
- Missing session-binding to capture
- Pipewire layer would use wlr-virtual-pointer under the hood on wlroots

### Q: Won't this take years?

**A:** It could, or it could take months. Depends on who drives it:
- **With System76 leading:** 6-12 months (COSMIC first, then standardize)
- **Without champion:** 3-5 years or never

### Q: Why hasn't Pipewire added this already?

**A:** Nobody asked with a concrete proposal + working prototype. Most remote desktop developers gave up and used ydotool. **We're asking now.**

### Q: What if compositors don't implement it?

**A:** Fall back to ydotool (what we do now). But:
- Early adopters (COSMIC) get better UX
- Pressure on others to adopt (competitive)
- Eventually becomes expected feature

---

## Success Criteria

**This proposal succeeds if:**

✅ **Pipewire adds input injection API** (portal extended)
✅ **At least one compositor implements it** (COSMIC first?)
✅ **At least one client uses it** (mm-warp reference implementation)
✅ **Path to standardization exists** (freedesktop.org proposal)

**Within 2 years:**
✅ **Major compositors support it** (COSMIC, Sway, GNOME, KDE)
✅ **Remote desktop tools adopt it** (RustDesk, mm-warp, etc.)
✅ **ydotool becomes legacy fallback** (not primary solution)

---

## Appendix: Real-World Testing

**mm-warp development revealed:**

**Problem 1: Global vs Session-Bound**
- Tested on same machine (server + client local)
- Input went to globally focused app, not captured desktop
- Confusing, fragile, wrong

**Problem 2: No Standard**
- Had to choose: wlr-virtual-pointer (wlroots only) or ydotool (kernel hack)
- Neither is proper solution
- Wasted hours researching non-existent standards

**Problem 3: Security Theater**
- ydotool requires root (full system access)
- But "secure" Wayland has no alternative
- Forced into insecure solution by secure design

**These problems are SOLVABLE.** Pipewire is the answer.

---

## Contact

**Proposal Author:** ModelMiser
**Reference Implementation:** mm-warp (https://github.com/[...]/mm-warp)
**Target Steward:** System76/COSMIC team
**Upstream Target:** Pipewire project, freedesktop.org

**Willing to:**
- Collaborate on protocol design
- Implement in mm-warp as reference
- Help with COSMIC integration
- Advocate to Pipewire project

**Goal:** Fix Wayland remote desktop input properly. Together. 🦬☀️

---

## The Bottom Line

**Wayland has been around for 17 years. Remote desktop input is still unsolved.**

**Not because it's technically hard** - the architecture is clear, the security model exists, compositors are capable.

**Because nobody owned the problem.**

**Pipewire should own it.** It's the right layer, the right abstraction, the right steward.

**System76 should lead it.** You're positioned, motivated, and capable.

**Let's fix this.** Not in 5 years. In 12 months.

The bison stands before the sun. Main Street tech. Built to last. 🦬☀️
