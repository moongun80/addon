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

    // Check XDG config directory (~/.config/addon/config.yaml) to match README
    if let Some(config_dir) = dirs::config_dir() {
        let xdg_config = config_dir.join("addon").join("config.yaml");
        if xdg_config.exists() {
            return Ok(xdg_config);
        }
    }

    // Check home directory (~/.addon/config.yaml) as fallback
    if let Some(home) = dirs::home_dir() {
        let home_config = home.join(".addon").join("config.yaml");
        if home_config.exists() {
            return Ok(home_config);
        }
    }

    // Check current directory (./config.yaml) as fallback
    let local_config = PathBuf::from("config.yaml");
    if local_config.exists() {
        return Ok(local_config);
    }

    Err(anyhow::anyhow!(
        "Config not found. Searched: ~/.config/addon/config.yaml, ~/.addon/config.yaml, ./config.yaml"
    ))
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
        let mapper = _config.build_keymapper(addon_core::os::OsPlatform::Linux);
        return Ok(Box::new(addon_linux::LinuxX11Adapter::new(
            _config, mapper,
        )));
    }

    #[cfg(feature = "macos")]
    {
        let mapper = _config.build_keymapper(addon_core::os::OsPlatform::Macos);
        return Ok(Box::new(addon_macos::MacOsAdapter::new(_config, mapper)));
    }

    #[cfg(feature = "windows")]
    {
        let mapper = _config.build_keymapper(addon_core::os::OsPlatform::Windows);
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
