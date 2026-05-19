//! Logging initialization and helpers for the daemon.
//!
//! Provides [`init`] which sets up a `tracing` subscriber with
//! reasonable defaults.

use anyhow::Result;

/// Initialize the tracing subscriber for the daemon process.
///
/// Reads the `RUST_LOG` environment variable for log level control.
/// Falls back to `"info"` if unset or invalid.
///
/// # Errors
///
/// Returns an error if the tracing subscriber cannot be installed
/// (e.g. another subscriber is already active).
pub fn init() -> Result<()> {
    addon_core::log::init().map_err(|e| anyhow::anyhow!("failed to init logging: {}", e))?;
    tracing::info!(
        "Logging initialized (RUST_LOG: {})",
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into())
    );
    Ok(())
}

/// Returns the path to the daemon log directory.
#[allow(dead_code)]
pub fn log_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("addon").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
