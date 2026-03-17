use mm_warp_client::{QuicClient, H264Decoder, wayland_display::WaylandDisplay};
use anyhow::Result;
use clap::Parser;

#[derive(Parser)]
#[command(name = "mm-warp-client", version, about = "mm-warp remote desktop client")]
struct Args {
    /// Server address to connect to
    #[arg(short, long, default_value = "127.0.0.1:4433")]
    server: String,

    /// Skip TLS certificate verification (INSECURE — allows MITM attacks)
    #[arg(long)]
    insecure: bool,

    /// PIN for server authentication (must match server's --pin)
    #[arg(long)]
    pin: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    tracing_subscriber::fmt::init();
    println!("=== mm-warp Client (Wayland Display) ===\n");

    let server_addr: std::net::SocketAddr = args.server.parse()
        .map_err(|e| anyhow::anyhow!("Invalid server address '{}': {}", args.server, e))?;
    let client = QuicClient::new(server_addr)?;
    println!("Connecting to {}...", server_addr);

    // Reconnect loop — wraps the entire session
    loop {
        let connection = loop {
            match client.connect(server_addr, args.insecure).await {
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

        // PIN authentication (if server requires it)
        if let Some(ref pin) = args.pin {
            println!("Sending PIN...");
            let pin_result = tokio::time::timeout(std::time::Duration::from_secs(10), async {
                let (mut send, mut recv) = connection.open_bi().await
                    .map_err(|e| anyhow::anyhow!("PIN: failed to open bidi stream: {}", e))?;
                send.write_all(pin.as_bytes()).await?;
                send.finish()?;
                let resp = recv.read_to_end(64).await?;
                if resp != b"OK" {
                    anyhow::bail!("Server rejected PIN — check your --pin value");
                }
                Ok::<(), anyhow::Error>(())
            }).await;
            match pin_result {
                Ok(Ok(())) => println!("✅ PIN accepted"),
                Ok(Err(e)) => return Err(e),
                Err(_) => anyhow::bail!("PIN exchange timed out (10s) — server may not require --pin"),
            }
        }

        // Receive stream metadata from server (resolution, fps)
        println!("Waiting for stream metadata...");
        let meta = match QuicClient::receive_metadata(&connection).await {
            Ok(m) => m,
            Err(e) => {
                eprintln!("⚠️  Failed to receive metadata: {} — reconnecting...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
        };
        let (width, height) = (meta.width, meta.height);

        // Validate resolution bounds (max 16384x16384 = ~1GB buffer)
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            eprintln!("⚠️  Server sent invalid resolution {}x{} — reconnecting...", width, height);
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            continue;
        }
        println!("✅ Stream: {}x{} @ {} FPS\n", width, height, meta.fps);

        println!("Creating H.264 decoder ({}x{})...", width, height);
        let mut decoder = match H264Decoder::new(width, height) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("⚠️  Failed to create decoder: {}", e);
                return Err(e);
            }
        };
        println!("✅ Decoder ready\n");

        println!("Creating Wayland display window...");
        let mut display = WaylandDisplay::new(width, height)?;
        println!("✅ Display ready\n");

        println!("Receiving and displaying...");
        println!("🎹 Keyboard/mouse control active — focus the window and type.\n");

        let mut stats = mm_warp_common::stats::StreamStats::new();

        let session_result: Result<()> = async {
            loop {
                let encoded = match QuicClient::receive_frame(&connection).await {
                    Ok(e) => e,
                    Err(e) => {
                        // Use typed error matching where possible
                        let msg = e.to_string();
                        if msg.contains("closed") || msg.contains("reset") || msg.contains("timed out") {
                            println!("\n⚠️  Connection lost — server disconnected.");
                        } else {
                            println!("\n⚠️  Connection error: {}", e);
                        }
                        return Ok(()); // break to reconnect
                    }
                };
                let frame_size = encoded.len() as u64;

                let decoded = decoder.decode(&encoded)?;
                if !decoded.is_empty() {
                    if let Err(e) = display.display_frame(&decoded) {
                        let msg = e.to_string();
                        if msg.contains("Broken pipe") || msg.contains("closed") {
                            println!("\n✅ Window closed — disconnecting gracefully");
                            return Err(anyhow::anyhow!("window closed"));
                        }
                        return Err(e);
                    }

                    stats.record_frame(frame_size);

                    let input_events = display.poll_input_events();
                    for event in input_events {
                        // Inline send — datagram is fire-and-forget
                        let _ = connection.send_datagram(event.to_bytes().into());
                    }

                    if let Some(report) = stats.report_if_due("CLIENT", None) {
                        println!("{}", report);
                    }
                }
            }
        }.await;

        match session_result {
            Ok(()) => {
                // Connection lost — reconnect
                println!("Reconnecting in 2s...\n");
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("window closed") {
                    return Ok(());
                }
                return Err(e);
            }
        }
    }
}
