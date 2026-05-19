//! # addon daemon
//!
//! Daemon binary for the addon — background service that loads configuration,
//! detects key binding conflicts, creates the platform-specific OS adapter,
//! starts an IPC server for the Tauri GUI, and runs the event loop until
//! termination (Ctrl+C).
//!
//! ## Process
//!
//! 1. Initialize logging via `tracing-subscriber`.
//! 2. Load configuration from the default config path.
//! 3. Detect key binding conflicts.
//! 4. Create the platform-specific OS adapter.
//! 5. Start the IPC server on a Unix domain socket.
//! 6. Enter the main event loop accepting IPC connections.
//! 7. On Ctrl+C, stop the adapter and exit cleanly.

mod daemon;
mod ipc;
mod log;

use std::path::PathBuf;

use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{info, warn};

/// Main entry point for the daemon.
#[tokio::main]
async fn main() -> Result<()> {
    // ------------------------------------------------------------------
    // 1. Initialize logging.
    // ------------------------------------------------------------------
    log::init().context("failed to initialize logging")?;
    info!(
        "addon daemon starting — version {}",
        env!("CARGO_PKG_VERSION")
    );

    // ------------------------------------------------------------------
    // 2. Load configuration.
    // ------------------------------------------------------------------
    let config_path = get_config_path().context("cannot determine config path")?;
    info!("Loading config from: {:?}", config_path);
    let config = addon_core::config::load(&config_path).context("failed to load configuration")?;

    info!(
        "Loaded {} key binding(s) (version {})",
        config.keybindings.len(),
        config.version
    );

    // ------------------------------------------------------------------
    // 3. Detect conflicts.
    // ------------------------------------------------------------------
    let conflicts = addon_core::conflict::detect_conflicts(&config.keybindings);
    if !conflicts.is_empty() {
        warn!("{} key binding conflict(s) detected", conflicts.len());
        for c in &conflicts {
            warn!(
                "  - {} ↔ {} [platform: {:?}]",
                c.binding1, c.binding2, c.platform
            );
        }
    } else {
        info!("No key binding conflicts detected.");
    }

    // ------------------------------------------------------------------
    // 4. Create the platform-specific adapter.
    // ------------------------------------------------------------------
    let mut adapter = create_adapter(config.clone()).context("failed to create OS adapter")?;
    adapter.init().context("adapter init failed")?;

    // ------------------------------------------------------------------
    // 5. Create daemon state.
    // ------------------------------------------------------------------
    let state = daemon::create_daemon_state(config, adapter);

    // ------------------------------------------------------------------
    // 6. Start IPC server.
    // ------------------------------------------------------------------
    let server = ipc::IpcServer::new(state.clone()).context("failed to create IPC server")?;
    info!("IPC server started on {}", server.address().display());

    // ------------------------------------------------------------------
    // 7. Main event loop.
    // ------------------------------------------------------------------
    info!("Daemon is ready — accepting IPC connections (Ctrl+C to stop)");

    loop {
        tokio::select! {
            // Accept a new IPC client connection.
            result = server.accept() => {
                match result {
                    Ok(()) => {}
                    Err(e) => {
                        warn!("IPC accept error: {}", e);
                    }
                }
            }

            // Stop on Ctrl+C.
            _ = tokio::signal::ctrl_c() => {
                info!("Shutting down daemon...");

                // Stop adapter.
                if let Ok(mut guard) = state.lock() {
                    if let Some(ref mut adapter) = guard.adapter {
                        if let Err(e) = adapter.stop() {
                            warn!("Adapter stop error: {}", e);
                        }
                    }
                }

                // Remove the socket file.
                let socket_path = ipc::get_socket_path();
                std::fs::remove_file(&socket_path).ok();

                info!("Daemon stopped cleanly.");
                break;
            }
        }
    }

    Ok(())
}

/// Returns the path to the configuration file.
///
/// Looks in the following locations (first found wins):
/// 1. `$ADDON_CONFIG` environment variable
/// 2. `~/.addon/config.yaml`
/// 3. `./config.yaml` (current directory)
fn get_config_path() -> Result<PathBuf> {
    // Check environment variable first.
    if let Ok(path) = std::env::var("ADDON_CONFIG") {
        return Ok(PathBuf::from(path));
    }

    // Check home directory.
    if let Some(home) = dirs::home_dir() {
        let home_config = home.join(".addon").join("config.yaml");
        if home_config.exists() {
            return Ok(home_config);
        }
    }

    // Check current directory.
    let local_config = PathBuf::from("config.yaml");
    if local_config.exists() {
        return Ok(local_config);
    }

    // Default fallback.
    if let Some(home) = dirs::home_dir() {
        Ok(home.join(".addon").join("config.yaml"))
    } else {
        Err(anyhow::anyhow!("cannot determine config path"))
    }
}

/// Creates the platform-specific OS adapter based on the target OS feature.
///
/// The appropriate OS adapter is selected at compile time via Cargo features:
/// - `--features linux`  → Linux X11 adapter
/// - `--features macos`  → macOS Carbon adapter
/// - `--features windows` → Windows hook adapter
fn create_adapter(_config: addon_core::config::Config) -> Result<Box<dyn addon_core::OsAdapter>> {
    #[cfg(feature = "linux")]
    {
        let mapper = build_keymapper(&_config);
        return Ok(Box::new(addon_linux::LinuxX11Adapter::new(_config, mapper)));
    }

    #[cfg(feature = "macos")]
    {
        let mapper = build_keymapper(&_config);
        return Ok(Box::new(addon_macos::MacOsAdapter::new(_config, mapper)));
    }

    #[cfg(feature = "windows")]
    {
        let mapper = build_keymapper(&_config);
        return Ok(Box::new(addon_windows::WindowsAdapter::new(
            _config, mapper,
        )));
    }

    #[cfg(not(any(feature = "linux", feature = "macos", feature = "windows")))]
    {
        Err(anyhow::anyhow!(
            "No OS adapter enabled. Enable one of: linux, macos, windows"
        ))
    }
}

/// Builds a key mapper from the configuration.
#[allow(dead_code)]
fn build_keymapper(config: &addon_core::config::Config) -> Box<dyn addon_core::mapper::KeyMapper> {
    let mut map: HashMap<addon_core::keymap::KeyStroke, addon_core::actions::Action> =
        HashMap::new();

    for binding in &config.keybindings {
        for key_str in &binding.keys {
            if let Ok(stroke) = addon_core::keymap::KeyStroke::parse(key_str) {
                map.insert(stroke, binding.action.clone());
            }
        }
        // Also include overrides.
        if let Some(ref overrides) = binding.overrides {
            for key_str in overrides.macos.iter().flatten() {
                if let Ok(stroke) = addon_core::keymap::KeyStroke::parse(key_str) {
                    map.insert(stroke, binding.action.clone());
                }
            }
            for key_str in overrides.windows.iter().flatten() {
                if let Ok(stroke) = addon_core::keymap::KeyStroke::parse(key_str) {
                    map.insert(stroke, binding.action.clone());
                }
            }
        }
    }

    Box::new(DaemonKeyMapper { map })
}

/// A simple key mapper backed by a HashMap.
#[allow(dead_code)]
struct DaemonKeyMapper {
    map: HashMap<addon_core::keymap::KeyStroke, addon_core::actions::Action>,
}

impl addon_core::mapper::KeyMapper for DaemonKeyMapper {
    fn lookup(
        &self,
        stroke: &addon_core::keymap::KeyStroke,
    ) -> Option<&addon_core::actions::Action> {
        self.map.get(stroke)
    }
}
