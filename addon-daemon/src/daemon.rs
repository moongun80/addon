//! Daemon core logic — manages configuration, adapter lifecycle,
//! and key binding operations.

use addon_core::config::Config;
use addon_core::os::OsAdapter;
use rand::RngCore;
use std::sync::Arc;
use std::sync::RwLock;

/// The mutable state shared between the daemon loop and IPC handler.
pub struct DaemonState {
    /// Whether the daemon is currently running.
    pub running: bool,
    /// Whether the adapter has been initialized (tracks init state to avoid double-init).
    pub initialized: bool,
    /// Current configuration.
    pub config: Config,
    /// Platform adapter (optional — not yet started).
    pub adapter: Option<Box<dyn OsAdapter + Send + Sync>>,
    /// HMAC secret for IPC authentication (generated at startup).
    pub auth_secret: String,
}

/// Shared state handle — cloneable reference to the daemon state.
/// FIX-200: Use RwLock instead of Mutex to allow concurrent reads.
pub type DaemonStateHandle = Arc<RwLock<DaemonState>>;

/// Create the initial daemon state from a config and OS adapter.
/// IMP-001: Generates a random HMAC secret for IPC authentication.
pub fn create_daemon_state(config: Config, adapter: Box<dyn OsAdapter>) -> DaemonStateHandle {
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let auth_secret = hex::encode(secret);
    
    Arc::new(RwLock::new(DaemonState {
        running: false,
        initialized: false,
        config,
        adapter: Some(adapter),
        auth_secret,
    }))
}
