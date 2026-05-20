//! IPC server — Unix domain socket listener and handler.
//!
//! The daemon listens on a Unix socket for JSON messages from the
//! Tauri GUI client. Each message is newline-delimited (`\n`),
//! following the same protocol used by the GUI client.

use std::path::Path;
use std::sync::Arc;

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
    daemon_state: Arc<std::sync::RwLock<DaemonState>>,
}

impl IpcServer {
    /// Create and bind a new IPC server.
    pub fn new(daemon_state: Arc<std::sync::RwLock<DaemonState>>) -> Result<Self, std::io::Error> {
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
        .map_err(|e| tracing::warn!("Failed to restrict socket permissions: {}", e))
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
    daemon_state: Arc<std::sync::RwLock<DaemonState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = tokio::io::BufReader::new(reader);

    loop {
        let mut line = String::new();
        buf_reader.read_line(&mut line).await?;
        // FIX-017: Reject oversized messages to prevent memory exhaustion.
        if line.len() > 1_048_576 {
            tracing::warn!("IPC message exceeded 1MB size limit, dropping connection");
            break;
        }

        let line = line.trim_end_matches(['\n', '\r']);

        if line.is_empty() {
            break;
        }

        let msg: IpcMessage = match serde_json::from_str(line) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!("Failed to parse IPC message: {}", e);
                let err_resp = IpcMessage::response(IpcResponse::Error {
                    code: "PARSE_ERROR".to_string(),
                    details: format!("Failed to parse message: {}", e),
                    request_id: None,
                });
                if let Ok(json) = serde_json::to_string(&err_resp) {
                    let _ = writer.write_all(json.as_bytes()).await;
                    let _ = writer.write_all(b"\n").await;
                    let _ = writer.flush().await;
                }
                continue;
            }
        };

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
/// FIX-200: Minimize lock hold time — use read locks for GET operations
/// and write locks only when state mutation is needed.
fn process_request(req: &IpcRequest, state: &Arc<std::sync::RwLock<DaemonState>>) -> IpcMessage {
    let request_id = req.request_id().map(str::to_string);

    // TestShortcut needs no lock at all — pure parsing
    if let IpcRequest::TestShortcut {
        keys, action: _, ..
    } = req
    {
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

        return IpcMessage::response(
            IpcResponse::TestResult {
                success: all_ok,
                message: msg_text,
                request_id: None,
            }
            .with_request_id(request_id),
        );
    }

    // GetStatus — read lock only
    if let IpcRequest::GetStatus { .. } = req {
        let guard = match state.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let resp = IpcResponse::DaemonStatus {
            running: guard.running,
            pid: std::process::id(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            request_id: None,
        };
        drop(guard); // Explicitly release read lock
        return IpcMessage::response(resp.with_request_id(request_id));
    }

    // All remaining requests need write lock
    let resp: IpcResponse = {
        let mut guard = match state.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        match req {
            IpcRequest::StartDaemon { .. } => {
                if guard.running {
                    drop(guard);
                    return IpcMessage::response(
                        IpcResponse::TestResult {
                            success: false,
                            message: "Daemon is already running".to_string(),
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }

                let was_initialized = guard.initialized;
                let has_adapter = guard.adapter.is_some();

                if has_adapter {
                    // Extract adapter reference, but we need to be careful about borrows
                    if !was_initialized {
                        if let Some(ref mut adapter) = guard.adapter {
                            if let Err(e) = adapter.init() {
                                drop(guard);
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
                    }

                    if let Some(ref mut adapter) = guard.adapter {
                        if let Err(e) = adapter.start() {
                            drop(guard);
                            return IpcMessage::response(
                                IpcResponse::Error {
                                    code: "ADAPTER_START_ERROR".to_string(),
                                    details: e.to_string(),
                                    request_id: None,
                                }
                                .with_request_id(request_id),
                            );
                        }
                    }
                    guard.running = true;
                    guard.initialized = true;
                } else {
                    guard.running = true;
                    drop(guard);
                    return IpcMessage::response(
                        IpcResponse::TestResult {
                            success: true,
                            message: "Daemon started (no adapter available)".to_string(),
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }

                IpcResponse::DaemonStatus {
                    running: true,
                    pid: std::process::id(),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    request_id: None,
                }
            }

            IpcRequest::StopDaemon { .. } => {
                // Stop adapter
                if let Some(ref mut adapter) = guard.adapter {
                    if let Err(e) = adapter.stop() {
                        drop(guard);
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
                drop(guard);
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
                // Parse and validate WITHOUT holding the lock
                let new_config =
                    match serde_json::from_value::<addon_core::config::Config>(json.clone()) {
                        Ok(c) => c,
                        Err(e) => {
                            drop(guard);
                            return IpcMessage::response(
                                IpcResponse::Error {
                                    code: "CONFIG_PARSE_ERROR".to_string(),
                                    details: e.to_string(),
                                    request_id: None,
                                }
                                .with_request_id(request_id),
                            );
                        }
                    };

                // FIX-201: Validate system commands to prevent shell injection
                for binding in &new_config.keybindings {
                    if let addon_core::actions::Action::SystemCommand { command } = &binding.action
                    {
                        if let Err(e) = addon_core::actions::validate_system_command(command) {
                            drop(guard);
                            return IpcMessage::response(
                                IpcResponse::Error {
                                    code: "COMMAND_VALIDATION_ERROR".to_string(),
                                    details: format!("Binding '{}': {}", binding.id, e),
                                    request_id: None,
                                }
                                .with_request_id(request_id),
                            );
                        }
                    }
                }

                // Detect conflicts before applying
                let conflicts = addon_core::conflict::detect_conflicts(&new_config.keybindings);
                if !conflicts.is_empty() {
                    tracing::warn!("{} conflict(s) in new config", conflicts.len());
                }

                // FIX-011: Minimize critical section — update config under lock,
                // then drop lock before expensive adapter lifecycle methods.
                let old_running = guard.running;
                guard.config = new_config.clone();

                // Collect keys for response while still under lock
                let keys: Vec<addon_core::ipc::KeyBindingJson> = guard
                    .config
                    .keybindings
                    .iter()
                    .cloned()
                    .map(addon_core::ipc::KeyBindingJson::from)
                    .collect();

                // Extract adapter and config for lifecycle outside the lock
                let adapter_for_reinit = if old_running {
                    guard.adapter.take()
                } else {
                    None
                };
                let new_cfg = guard.config.clone();
                drop(guard);

                // Adapter lifecycle OUTSIDE the lock
                if old_running {
                    if let Some(mut adapter) = adapter_for_reinit {
                        adapter.set_config(&new_cfg);
                        let stop_ok = adapter.stop().is_ok();
                        let init_ok_val = if stop_ok {
                            adapter.init().is_ok()
                        } else {
                            false
                        };
                        let start_ok = if init_ok_val {
                            adapter.start().is_ok()
                        } else {
                            true
                        };
                        if !stop_ok {
                            tracing::warn!("Adapter stop during SetConfig");
                        } else if !init_ok_val {
                            tracing::warn!("Adapter reinit during SetConfig");
                        } else if !start_ok {
                            tracing::warn!("Adapter start during SetConfig");
                        }
                        // Put adapter back under a fresh lock
                        {
                            let mut g = state.write().unwrap_or_else(|e| e.into_inner());
                            g.adapter = Some(adapter);
                            g.initialized = init_ok_val && start_ok;
                        }
                    }
                }

                return IpcMessage::response(
                    IpcResponse::ConfigLoaded {
                        keys,
                        request_id: None,
                    }
                    .with_request_id(request_id),
                );
            }

            IpcRequest::ReloadConfig { .. } => {
                // Reload config from disk
                let path = match get_config_path() {
                    Ok(p) => p,
                    Err(e) => {
                        drop(guard);
                        return IpcMessage::response(
                            IpcResponse::Error {
                                code: "RELOAD_ERROR".to_string(),
                                details: e.to_string(),
                                request_id: None,
                            }
                            .with_request_id(request_id),
                        );
                    }
                };

                let new_config = match addon_core::config::load(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        drop(guard);
                        return IpcMessage::response(
                            IpcResponse::Error {
                                code: "RELOAD_ERROR".to_string(),
                                details: e.to_string(),
                                request_id: None,
                            }
                            .with_request_id(request_id),
                        );
                    }
                };

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

                // FIX-011: Extract adapter and config, drop lock before lifecycle.
                let cfg_for_adapter = guard.config.clone();
                let adapter_for_reinit = guard.adapter.take();

                let keys: Vec<addon_core::ipc::KeyBindingJson> = guard
                    .config
                    .keybindings
                    .iter()
                    .cloned()
                    .map(addon_core::ipc::KeyBindingJson::from)
                    .collect();

                drop(guard);

                // Adapter lifecycle OUTSIDE the lock
                if let Some(mut adapter) = adapter_for_reinit {
                    adapter.set_config(&cfg_for_adapter);
                    let stop_ok = adapter.stop().is_ok();
                    let init_ok_val = if stop_ok {
                        adapter.init().is_ok()
                    } else {
                        false
                    };
                    let start_ok = if init_ok_val {
                        adapter.start().is_ok()
                    } else {
                        true
                    };
                    if !stop_ok {
                        tracing::warn!("Adapter stop during ReloadConfig");
                    } else if !init_ok_val {
                        tracing::warn!("Adapter reinit during ReloadConfig");
                    } else if !start_ok {
                        tracing::warn!("Adapter start during ReloadConfig");
                    }
                    {
                        let mut g = state.write().unwrap_or_else(|e| e.into_inner());
                        g.adapter = Some(adapter);
                        g.initialized = stop_ok && init_ok_val && start_ok;
                    }
                }

                return IpcMessage::response(
                    IpcResponse::ConfigLoaded {
                        keys,
                        request_id: None,
                    }
                    .with_request_id(request_id),
                );
            }

            // Already handled above
            IpcRequest::GetStatus { .. } | IpcRequest::TestShortcut { .. } => unreachable!(),
        }
    };

    IpcMessage::response(resp.with_request_id(request_id))
}
