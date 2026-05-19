//! Daemon core logic — manages configuration, adapter lifecycle,
//! and key binding operations.

use addon_core::config::Config;
use addon_core::os::OsAdapter;
use std::sync::Arc;
use std::sync::Mutex;

/// The mutable state shared between the daemon loop and IPC handler.
pub struct DaemonState {
    /// Whether the daemon is currently running.
    pub running: bool,
    /// Current configuration.
    pub config: Config,
    /// Platform adapter (optional — not yet started).
    pub adapter: Option<Box<dyn OsAdapter + Send + Sync>>,
}

/// Shared state handle — cloneable reference to the daemon state.
pub type DaemonStateHandle = Arc<Mutex<DaemonState>>;

/// Create the initial daemon state from a config and OS adapter.
pub fn create_daemon_state(
    config: Config,
    adapter: Box<dyn OsAdapter>,
) -> DaemonStateHandle {
    Arc::new(Mutex::new(DaemonState {
        running: false,
        config,
        adapter: Some(adapter),
    }))
}
