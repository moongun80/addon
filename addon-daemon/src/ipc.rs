//! IPC server — Unix domain socket listener and handler.
//!
//! The daemon listens on a Unix socket for JSON messages from the
//! Tauri GUI client. Each message is newline-delimited (`\n`),
//! following the same protocol used by the GUI client.

use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::daemon::DaemonState;
use crate::get_config_path;
use addon_core::ipc::{IpcMessage, IpcRequest, IpcResponse};

// ---------------------------------------------------------------------------
// Socket path
// ---------------------------------------------------------------------------

/// Returns the platform-specific socket path.
pub fn get_socket_path() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("addon");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("daemon.sock")
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// The IPC server listens for incoming GUI client connections.
pub struct IpcServer {
    listener: UnixListener,
    address: std::path::PathBuf,
    daemon_state: Arc<Mutex<DaemonState>>,
}

impl IpcServer {
    /// Create and bind a new IPC server.
    pub fn new(daemon_state: Arc<Mutex<DaemonState>>) -> Result<Self, std::io::Error> {
        let address = get_socket_path();

        // Remove stale socket file if it exists.
        if address.exists() {
            std::fs::remove_file(&address)?;
        }

        let listener = UnixListener::bind(&address)?;

        // FIX-012: Restrict socket file permissions to owner-only (0o600)
        std::fs::set_permissions(
            &address,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .ok();

        tracing::info!("IPC server listening on {}", address.display());

        Ok(Self {
            listener,
            address,
            daemon_state,
        })
    }

    /// Return the address the server is bound to.
    pub fn address(&self) -> &Path {
        &self.address
    }

    /// Block until a client connects, then spawn a handler task.
    pub async fn accept(&self) -> Result<(), std::io::Error> {
        let (stream, _addr) = self.listener.accept().await?;
        let state = Arc::clone(&self.daemon_state);

        tokio::spawn(async move {
            if let Err(e) = handle_client(stream, state).await {
                tracing::error!("IPC client handler error: {e}");
            }
        });

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Client handler
// ---------------------------------------------------------------------------

/// Process messages from a single GUI client connection.
///
/// The protocol is newline-delimited JSON: each message ends with `\n`.
/// We use a `BufReader` with `read_line` to handle partial reads correctly.
async fn handle_client(
    stream: UnixStream,
    daemon_state: Arc<Mutex<DaemonState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = tokio::io::BufReader::new(reader);

    loop {
        let mut line = String::new();
        buf_reader.read_line(&mut line).await?;
        let line = line.trim_end_matches(['\n', '\r']);

        if line.is_empty() {
            break;
        }

        let msg: IpcMessage = serde_json::from_str(line)?;

        let resp = match &msg {
            IpcMessage::Request(req) => process_request(req, &daemon_state),
            IpcMessage::Response(_) => {
                tracing::warn!("Unexpected response from client, ignoring");
                continue;
            }
        };

        let json = serde_json::to_string(&resp)?;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
    }

    Ok(())
}

/// Process a single client request and produce a response.
fn process_request(req: &IpcRequest, state: &Arc<Mutex<DaemonState>>) -> IpcMessage {
    let request_id = req.request_id().map(str::to_string);

    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };

    let resp: IpcResponse = match req {
        IpcRequest::GetStatus { .. } => IpcResponse::DaemonStatus {
            running: guard.running,
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            request_id: None,
        },

        IpcRequest::StartDaemon { .. } => {
            if guard.running {
                IpcResponse::TestResult {
                    success: false,
                    message: "Daemon is already running".to_string(),
                    request_id: None,
                }
            } else {
                // FIX-009: Capture initialized flag before mutable borrow
                let was_initialized = guard.initialized;

                if let Some(ref mut adapter) = guard.adapter {
                    // FIX-009: Only init if not already initialized
                    if !was_initialized {
                        if let Err(e) = adapter.init() {
                            return IpcMessage::response(
                                IpcResponse::Error {
                                    code: "ADAPTER_INIT_ERROR".to_string(),
                                    details: e.to_string(),
                                    request_id: None,
                                }
                                .with_request_id(request_id),
                            );
                        }
                    }

                    // FIX-011: Set running = true ONLY after start() succeeds
                    if let Err(e) = adapter.start() {
                        return IpcMessage::response(
                            IpcResponse::Error {
                                code: "ADAPTER_START_ERROR".to_string(),
                                details: e.to_string(),
                                request_id: None,
                            }
                            .with_request_id(request_id),
                        );
                    }
                    // Both init and start succeeded — update flags now
                    guard.running = true;
                    guard.initialized = true;

                    IpcResponse::DaemonStatus {
                        running: true,
                        pid: std::process::id(),
                        version: env!("CARGO_PKG_VERSION").to_string(),
                        request_id: None,
                    }
                } else {
                    guard.running = true;
                    IpcResponse::TestResult {
                        success: true,
                        message: "Daemon started (no adapter available)".to_string(),
                        request_id: None,
                    }
                }
            }
        }

        IpcRequest::StopDaemon { .. } => {
            if let Some(ref mut adapter) = guard.adapter {
                // FIX-011: Set running = false ONLY after stop() succeeds
                if let Err(e) = adapter.stop() {
                    return IpcMessage::response(
                        IpcResponse::Error {
                            code: "ADAPTER_STOP_ERROR".to_string(),
                            details: e.to_string(),
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }
            }
            guard.running = false;
            guard.initialized = false;
            return IpcMessage::response(
                IpcResponse::TestResult {
                    success: true,
                    message: "Daemon stopping".to_string(),
                    request_id: None,
                }
                .with_request_id(request_id),
            );
        }

        IpcRequest::SetConfig { config: json, .. } => {
            // Parse JSON as Config directly
            match serde_json::from_value::<addon_core::config::Config>(json.clone()) {
                Ok(new_config) => {
                    // Detect conflicts before applying
                    let conflicts = addon_core::conflict::detect_conflicts(&new_config.keybindings);
                    if !conflicts.is_empty() {
                        tracing::warn!("{} conflict(s) in new config", conflicts.len());
                    }

                    // Update config
                    let old_running = guard.running;
                    guard.config = new_config.clone();

                    // Rebuild adapter if running
                    if old_running {
                        // FIX-010: Clone config before mutable borrow on adapter
                        let new_cfg = guard.config.clone();
                        let mut init_ok = false;
                        if let Some(ref mut adapter) = guard.adapter {
                            adapter.set_config(&new_cfg);
                            if adapter.stop().is_ok() {
                                init_ok = adapter.init().is_ok();
                                if init_ok {
                                    if let Err(e) = adapter.start() {
                                        tracing::warn!("Adapter restart during SetConfig: {e}");
                                    }
                                } else {
                                    tracing::warn!("Adapter reinit during SetConfig");
                                }
                            } else {
                                tracing::warn!("Adapter stop during SetConfig");
                            }
                        }
                        guard.initialized = init_ok;
                    }

                    let keys: Vec<addon_core::ipc::KeyBindingJson> = guard
                        .config
                        .keybindings
                        .iter()
                        .cloned()
                        .map(addon_core::ipc::KeyBindingJson::from)
                        .collect();
                    return IpcMessage::response(
                        IpcResponse::ConfigLoaded {
                            keys,
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }
                Err(e) => IpcResponse::Error {
                    code: "CONFIG_PARSE_ERROR".to_string(),
                    details: e.to_string(),
                    request_id: None,
                },
            }
        }

        IpcRequest::ReloadConfig { .. } => match reload_config_inner(&mut guard) {
            Ok(keys) => IpcResponse::ConfigLoaded {
                keys,
                request_id: None,
            },
            Err(e) => IpcResponse::Error {
                code: "RELOAD_ERROR".to_string(),
                details: e,
                request_id: None,
            },
        },

        IpcRequest::TestShortcut {
            keys, action: _, ..
        } => {
            let mut all_ok = true;
            let mut messages: Vec<String> = Vec::new();

            for key_str in keys {
                match addon_core::keymap::KeyStroke::parse(key_str) {
                    Ok(stroke) => {
                        messages.push(format!("{} → parsed OK", stroke.display()));
                    }
                    Err(e) => {
                        messages.push(format!("{} → ERROR: {}", key_str, e));
                        all_ok = false;
                    }
                }
            }

            let msg_text = if all_ok {
                format!("All {} key(s) parsed successfully.", keys.len())
            } else {
                messages.join("; ")
            };

            IpcResponse::TestResult {
                success: all_ok,
                message: msg_text,
                request_id: None,
            }
        }
    };

    IpcMessage::response(resp.with_request_id(request_id))
}

/// Reload configuration from disk (internal, requires mutable state).
fn reload_config_inner(
    guard: &mut DaemonState,
) -> Result<Vec<addon_core::ipc::KeyBindingJson>, String> {
    let path = get_config_path().map_err(|e| e.to_string())?;
    let new_config = addon_core::config::load(&path).map_err(|e| e.to_string())?;

    // Detect conflicts.
    let conflicts = addon_core::conflict::detect_conflicts(&new_config.keybindings);
    if !conflicts.is_empty() {
        for c in &conflicts {
            tracing::warn!(
                "Conflict: {} ↔ {} [platform: {:?}]",
                c.binding1,
                c.binding2,
                c.platform
            );
        }
    }

    // Update state.
    guard.config = new_config.clone();

    // Rebuild adapter keymap if running.
    let cfg_for_adapter = guard.config.clone();
    let mut init_ok = true;
    if let Some(ref mut adapter) = guard.adapter {
        // FIX-010: Inject new config into adapter before re-init
        adapter.set_config(&cfg_for_adapter);
        if adapter.stop().is_err() {
            init_ok = false;
        } else if adapter.init().is_err() {
            init_ok = false;
        } else if adapter.start().is_err() {
            init_ok = false;
        }
    }
    guard.initialized = init_ok;

    let keys: Vec<addon_core::ipc::KeyBindingJson> = guard
        .config
        .keybindings
        .iter()
        .cloned()
        .map(addon_core::ipc::KeyBindingJson::from)
        .collect();

    Ok(keys)
}
