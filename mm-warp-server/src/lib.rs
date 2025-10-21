use anyhow::{Context, Result};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::wl_registry;

/// Represents a detected display output
#[derive(Debug, Clone)]
pub struct Display {
    pub name: String,
    pub width: i32,
    pub height: i32,
}

/// State for Wayland event handling
struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: <wl_registry::WlRegistry as wayland_client::Proxy>::Event,
        _data: &GlobalListContents,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        // Minimal handler - just need this to compile
    }
}

/// WaylandConnection manages connection to Wayland compositor
pub struct WaylandConnection {
    connection: Connection,
    displays: Vec<Display>,
}

impl WaylandConnection {
    /// Connect to Wayland compositor
    pub fn new() -> Result<Self> {
        let connection = Connection::connect_to_env()
            .context("Failed to connect to Wayland compositor. Is WAYLAND_DISPLAY set?")?;

        Ok(Self {
            connection,
            displays: Vec::new(),
        })
    }

    /// Enumerate available displays
    pub fn list_displays(&mut self) -> Result<&[Display]> {
        // Initialize registry and event queue
        let (globals, _event_queue) = registry_queue_init::<State>(&self.connection)
            .context("Failed to initialize Wayland registry")?;

        // Count output globals
        let output_count = globals.contents().with_list(|list| {
            list.iter()
                .filter(|global| global.interface == "wl_output")
                .count()
        });

        tracing::info!("Found {} display outputs", output_count);

        // For now, just record how many we found
        // Full output enumeration with geometry would require more protocol handling
        self.displays = (0..output_count).map(|i| Display {
            name: format!("Display {}", i),
            width: 0,  // Would get from wl_output.geometry event
            height: 0, // Would get from wl_output.geometry event
        }).collect();

        Ok(&self.displays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wayland_connect() {
        // This will fail in CI without Wayland, that's OK
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            let result = WaylandConnection::new();
            assert!(result.is_ok(), "Should connect to Wayland when WAYLAND_DISPLAY is set");
        }
    }
}
