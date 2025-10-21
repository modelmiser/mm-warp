use mm_warp_server::ext_capture::ExtCapture;
use anyhow::Result;

fn main() -> Result<()> {
    // Enable logging
    tracing_subscriber::fmt::init();

    println!("=== mm-warp ext-image-copy-capture Test ===\n");

    // Check if protocol is available
    println!("Checking if ext-image-copy-capture-v1 is available...");
    let is_available = ExtCapture::is_available();

    if is_available {
        println!("✅ ext-image-copy-capture-v1 IS available!");
        println!("   This compositor supports the newer protocol.");
        println!("   (COSMIC, newer GNOME/KDE, etc.)\n");

        println!("Creating ExtCapture instance...");
        let mut capture = ExtCapture::new()?;
        println!("✅ ExtCapture created successfully\n");

        println!("Attempting to capture frame (stub)...");
        let frame = capture.capture_frame()?;
        println!("✅ Stub capture succeeded");
        println!("   Frame size: {} bytes ({} MB)", frame.len(), frame.len() / 1024 / 1024);

        println!("\n🎉 SUCCESS! Module works on this compositor!");
        println!("   Next: Implement actual capture logic");

    } else {
        println!("❌ ext-image-copy-capture-v1 NOT available");
        println!("   This compositor doesn't support the newer protocol.");
        println!("   (Try wlr-screencopy instead for Sway/Hyprland)");
    }

    Ok(())
}
