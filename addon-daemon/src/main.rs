//! # addon-daemon
//!
//! Daemon binary for the addon — background service that loads configuration,
//! detects key binding conflicts, creates the platform-specific OS adapter,
//! and runs the event loop until termination (Ctrl+C).

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Initialize logging.
    addon_core::log::init().context("failed to initialize logging")?;

    info!("addon-daemon starting");

    // 2. Load configuration.
    let config_path = get_config_path()?;
    info!("Loading config from: {:?}", config_path);
    let config = addon_core::config::load(&config_path)
        .context("failed to load configuration")?;

    info!(
        "Loaded {} key binding(s) (version {})",
        config.keybindings.len(),
        config.version
    );

    // 3. Detect conflicts.
    let conflicts = addon_core::conflict::detect_conflicts(&config.keybindings);
    if !conflicts.is_empty() {
        warn!(
            "{} key binding conflict(s) detected",
            conflicts.len()
        );
        for c in &conflicts {
            warn!(
                "  - {} ↔ {} [platform: {:?}]",
                c.binding1, c.binding2, c.platform
            );
        }
    }

    // 4. Create the platform-specific adapter and start.
    let mut adapter = create_adapter(config)?;
    adapter.init().context("adapter init failed")?;
    adapter.start().context("adapter start failed")?;

    info!("addon-daemon running — press Ctrl+C to stop");

    // 5. Wait for termination signal.
    tokio::signal::ctrl_c().await?;

    // 6. Stop the adapter and clean up.
    info!("Stopping addon-daemon...");
    adapter.stop().context("adapter stop failed")?;
    info!("addon-daemon stopped cleanly");

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
fn create_adapter(config: addon_core::config::Config) -> Result<Box<dyn addon_core::OsAdapter>> {
    #[cfg(feature = "linux")]
    {
        let mapper = build_keymapper(&config);
        return Ok(Box::new(addon_linux::LinuxX11Adapter::new(config, mapper)));
    }

    #[cfg(feature = "macos")]
    {
        let mapper = build_keymapper(&config);
        return Ok(Box::new(addon_macos::MacOsAdapter::new(config, mapper)));
    }

    #[cfg(feature = "windows")]
    {
        let mapper = build_keymapper(&config);
        return Ok(Box::new(addon_windows::WindowsAdapter::new(config, mapper)));
    }

    #[cfg(not(any(feature = "linux", feature = "macos", feature = "windows")))]
    {
        Err(anyhow::anyhow!(
            "No OS adapter enabled. Enable one of: linux, macos, windows"
        ))
    }
}

/// Builds a key mapper from the configuration.
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
struct DaemonKeyMapper {
    map: HashMap<addon_core::keymap::KeyStroke, addon_core::actions::Action>,
}

impl addon_core::mapper::KeyMapper for DaemonKeyMapper {
    fn lookup(&self, stroke: &addon_core::keymap::KeyStroke) -> Option<&addon_core::actions::Action> {
        self.map.get(stroke)
    }
}
