use mm_warp_server::WaylandConnection;
use anyhow::Result;

fn main() -> Result<()> {
    // Enable logging
    tracing_subscriber::fmt::init();

    println!("=== mm-warp Real Wayland Screencopy Test ===\n");

    // Connect to Wayland
    println!("Connecting to Wayland compositor...");
    let mut conn = WaylandConnection::new()?;
    println!("✅ Connected to Wayland\n");

    // List displays
    println!("Enumerating displays...");
    let displays = conn.list_displays()?;
    println!("✅ Found {} display(s)\n", displays.len());

    // Capture a frame
    println!("Capturing frame from first display...");
    println!("(This will capture your actual screen!)\n");

    let frame = conn.capture_frame()?;

    println!("✅ Frame captured!");
    println!("   Size: {} bytes ({} MB)", frame.len(), frame.len() / 1024 / 1024);
    println!("   Expected: {} bytes (1920x1080 RGBA)", 1920 * 1080 * 4);

    // Validate frame has actual data (not all zeros)
    let sum: u64 = frame.iter().take(10000).map(|&b| b as u64).sum();
    println!("   Sample checksum: {} (should be > 0 if real capture)", sum);

    if sum > 0 {
        println!("\n🎉 SUCCESS! Real screen capture working!");
        println!("Your desktop was captured to memory.");
    } else {
        println!("\n⚠️  Warning: Frame appears to be all zeros");
    }

    Ok(())
}
