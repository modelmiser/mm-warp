use mm_warp_client::{QuicClient, H264Decoder, wayland_display::WaylandDisplay, InputEvent};
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "mm-warp-client", about = "mm-warp remote desktop client")]
struct Args {
    /// Server address to connect to
    #[arg(short, long, default_value = "127.0.0.1:4433")]
    server: String,

    /// Stream resolution (WxH)
    #[arg(short, long, default_value = "3840x2160")]
    resolution: String,
}

fn parse_resolution(s: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        anyhow::bail!("Resolution must be WxH (e.g., 3840x2160)");
    }
    Ok((parts[0].parse()?, parts[1].parse()?))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();
    println!("=== mm-warp Client (Wayland Display) ===\n");

    let (width, height) = parse_resolution(&args.resolution)?;

    let client = QuicClient::new()?;

    let server_addr = args.server.parse()
        .map_err(|e| anyhow::anyhow!("Invalid server address '{}': {}", args.server, e))?;
    println!("Connecting to {}...", server_addr);

    let connection = loop {
        match client.connect(server_addr).await {
            Ok(conn) => {
                println!("✅ Connected\n");
                break conn;
            }
            Err(e) => {
                eprintln!("⚠️  Connection failed: {} - retrying in 2s...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            }
        }
    };

    println!("Creating H.264 decoder ({}x{})...", width, height);
    let mut decoder = H264Decoder::new(width, height)?;
    println!("✅ Decoder ready\n");

    println!("Creating Wayland display window...");
    let mut display = WaylandDisplay::new(width, height)?;
    println!("✅ Display ready\n");

    println!("Receiving and displaying...");
    println!("🎹 Keyboard/mouse control active — focus the window and type.\n");

    let mut stats = mm_warp_common::stats::StreamStats::new();

    loop {
        let encoded = match QuicClient::receive_frame(&connection).await {
            Ok(e) => e,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("Broken pipe") || msg.contains("closed") || msg.contains("reset") {
                    println!("\n⚠️  Connection lost — server disconnected.");
                } else {
                    println!("\n⚠️  Connection error: {}", e);
                }
                println!("Restart client to reconnect.");
                return Ok(());
            }
        };
        let frame_size = encoded.len() as u64;

        let decoded = decoder.decode(&encoded)?;
        if !decoded.is_empty() {
            if let Err(e) = display.display_frame(&decoded) {
                let msg = e.to_string();
                if msg.contains("Broken pipe") || msg.contains("closed") {
                    println!("\n✅ Window closed — disconnecting gracefully");
                    return Ok(());
                }
                return Err(e);
            }

            stats.record_frame(frame_size);

            let input_events = display.poll_input_events();
            for event in input_events {
                let _ = InputEvent::send(&connection, event).await;
            }

            if let Some(report) = stats.report_if_due("CLIENT", None) {
                println!("{}", report);
            }
        }
    }
}
