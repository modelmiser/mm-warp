use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;
use anyhow::Result;

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {}
}

fn main() -> Result<()> {
    println!("=== Wayland Compositor Info ===\n");

    // Connect
    let connection = Connection::connect_to_env()?;
    println!("✅ Connected to Wayland\n");

    // Get globals
    let (globals, _) = registry_queue_init::<State>(&connection)?;

    println!("Available Wayland Globals:\n");

    globals.contents().with_list(|list| {
        for (i, global) in list.iter().enumerate() {
            println!("{:3}. {} (v{})",
                     i + 1,
                     global.interface,
                     global.version);
        }
    });

    println!("\n=== Looking for screencopy support ===\n");

    let has_screencopy = globals.contents().with_list(|list| {
        list.iter().any(|g| g.interface.contains("screencopy"))
    });

    if has_screencopy {
        println!("✅ Screencopy protocol IS available!");
    } else {
        println!("❌ Screencopy protocol NOT available");
        println!("\nThis compositor doesn't support wlr-screencopy.");
        println!("Alternative: Use portal.desktop screenshot API (more compatible)");
    }

    Ok(())
}
