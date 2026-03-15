/// Manual pixel format verification test for audit item #4 (PixelFormatIdentity).
///
/// Captures ONE frame via ext-image-copy-capture with Abgr8888 format, then
/// prints raw byte values for the first non-black pixels so a human can verify
/// the channel ordering is actually RGBA without conversion.
///
/// Run: cargo run --bin test_pixel_format
/// Requires: running Wayland compositor with ext-image-copy-capture-v1 support

use mm_warp_server::ext_capture::ExtCapture;
use mm_warp_server::capture::FrameSource;
use anyhow::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    println!("=== Pixel Format Verification (Audit #4: PixelFormatIdentity) ===\n");

    // --- Theory ---
    println!("THEORY (from wayland.xml spec):");
    println!("  abgr8888: \"[31:0] A:B:G:R 8:8:8:8 little endian\"");
    println!("  As 32-bit word: 0xAABBGGRR");
    println!("  Little-endian memory bytes: [RR, GG, BB, AA] = [R, G, B, A] = RGBA");
    println!("  Therefore: Abgr8888 shm buffer IS RGBA byte order. No conversion needed.");
    println!();

    // For comparison:
    println!("COMPARISON (argb8888 for reference):");
    println!("  argb8888: \"[31:0] A:R:G:B 8:8:8:8 little endian\"");
    println!("  As 32-bit word: 0xAARRGGBB");
    println!("  Little-endian memory bytes: [BB, GG, RR, AA] = [B, G, R, A] = BGRA");
    println!("  Therefore: Argb8888 shm buffer IS BGRA byte order. Needs swizzle to get RGBA.");
    println!();

    // --- Check availability ---
    if !ExtCapture::is_available() {
        println!("ext-image-copy-capture-v1 not available on this compositor.");
        println!("Cannot run live capture test. Theory analysis above still holds.");
        return Ok(());
    }

    println!("ext-image-copy-capture-v1 is available. Capturing one frame...\n");

    let mut capture = ExtCapture::new()?;
    let res = capture.resolution();
    println!("Resolution: {}x{}", res.width, res.height);
    println!("Requested format: wl_shm::Format::Abgr8888");
    println!();

    let frame = capture.capture_frame()?;
    println!("Captured {} bytes ({} pixels)", frame.len(), frame.len() / 4);
    println!();

    // --- Find first non-black pixels ---
    println!("LIVE PIXEL DATA (first 10 non-black pixels):");
    println!("  If format is correctly RGBA, colored pixels should show:");
    println!("  - Red areas:   R>100, G<50, B<50, A=255");
    println!("  - Green areas: R<50, G>100, B<50, A=255");
    println!("  - Blue areas:  R<50, G<50, B>100, A=255");
    println!("  - White areas: R=255, G=255, B=255, A=255");
    println!();

    let mut found = 0;
    let mut first_pixel_offset = None;
    for i in 0..(frame.len() / 4) {
        let idx = i * 4;
        let (b0, b1, b2, b3) = (frame[idx], frame[idx + 1], frame[idx + 2], frame[idx + 3]);

        // Skip fully black/transparent pixels
        if b0 == 0 && b1 == 0 && b2 == 0 {
            continue;
        }
        // Skip near-black pixels
        if b0 < 10 && b1 < 10 && b2 < 10 {
            continue;
        }

        if first_pixel_offset.is_none() {
            first_pixel_offset = Some(i);
        }

        let x = i % res.width as usize;
        let y = i / res.width as usize;
        println!(
            "  pixel[{:>6}] ({:>4},{:>4}): byte0={:>3} byte1={:>3} byte2={:>3} byte3={:>3}  (if RGBA: R={:>3} G={:>3} B={:>3} A={:>3})",
            i, x, y, b0, b1, b2, b3, b0, b1, b2, b3
        );

        found += 1;
        if found >= 10 {
            break;
        }
    }

    if found == 0 {
        println!("  (no non-black pixels found -- is the screen blank?)");
    }

    println!();

    // --- Also dump a region from center of screen for sampling ---
    let cx = res.width as usize / 2;
    let cy = res.height as usize / 2;
    println!("CENTER PIXEL SAMPLE (5x1 strip at ({}, {})):", cx, cy);
    for dx in 0..5 {
        let i = cy * res.width as usize + cx + dx;
        let idx = i * 4;
        if idx + 3 < frame.len() {
            let (b0, b1, b2, b3) = (frame[idx], frame[idx + 1], frame[idx + 2], frame[idx + 3]);
            println!(
                "  ({:>4},{:>4}): [{:>3}, {:>3}, {:>3}, {:>3}]  (if RGBA: R={} G={} B={} A={})",
                cx + dx, cy, b0, b1, b2, b3, b0, b1, b2, b3
            );
        }
    }

    println!();
    println!("VERDICT:");
    println!("  If the R/G/B values above match what you SEE on screen at those");
    println!("  coordinates, then Abgr8888 -> RGBA identity mapping is CORRECT.");
    println!("  If colors appear swapped (e.g., blue things show high R), then");
    println!("  the identity assumption is WRONG and a conversion is needed.");

    Ok(())
}
