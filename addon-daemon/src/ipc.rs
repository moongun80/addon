//! IPC server — Unix domain socket listener and handler.
//!
//! The daemon listens on a Unix socket for JSON messages from the
//! Tauri GUI client. Each message is newline-delimited (`\n`).

use std::path::Path;
use std::sync::{Arc, Mutex};

use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};

use addon_core::ipc::{IpcMessage, IpcRequest, IpcResponse};
use crate::daemon::DaemonState;

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
async fn handle_client(
    stream: UnixStream,
    daemon_state: Arc<Mutex<DaemonState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = tokio::io::BufReader::new(reader);

    loop {
        let mut line = String::new();
        buf_reader.read_line(&mut line).await?;
        let line = line.trim_end_matches(|c| c == '\n' || c == '\r');

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
fn process_request(
    req: &IpcRequest,
    state: &Arc<Mutex<DaemonState>>,
) -> IpcMessage {
    let mut guard = state.lock().unwrap();

    match req {
        IpcRequest::GetStatus => {
            IpcMessage::response(IpcResponse::DaemonStatus {
                running: guard.running,
                pid: std::process::id(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            })
        }

        IpcRequest::StartDaemon => {
            if guard.running {
                IpcMessage::response(IpcResponse::TestResult {
                    success: false,
                    message: "Daemon is already running".to_string(),
                })
            } else {
                guard.running = true;
                if let Some(ref mut adapter) = guard.adapter {
                    if let Err(e) = adapter.init() {
                        IpcMessage::response(IpcResponse::Error {
                            code: "ADAPTER_INIT_ERROR".to_string(),
                            details: e.to_string(),
                        })
                    } else if let Err(e) = adapter.start() {
                        IpcMessage::response(IpcResponse::Error {
                            code: "ADAPTER_START_ERROR".to_string(),
                            details: e.to_string(),
                        })
                    } else {
                        IpcMessage::response(IpcResponse::DaemonStatus {
                            running: true,
                            pid: std::process::id(),
                            version: env!("CARGO_PKG_VERSION").to_string(),
                        })
                    }
                } else {
                    IpcMessage::response(IpcResponse::TestResult {
                        success: true,
                        message: "Daemon started (no adapter available)".to_string(),
                    })
                }
            }
        }

        IpcRequest::StopDaemon => {
            if let Some(ref mut adapter) = guard.adapter {
                if let Err(e) = adapter.stop() {
                    return IpcMessage::response(IpcResponse::Error {
                        code: "ADAPTER_STOP_ERROR".to_string(),
                        details: e.to_string(),
                    });
                }
            }
            guard.running = false;
            IpcMessage::response(IpcResponse::TestResult {
                success: true,
                message: "Daemon stopping".to_string(),
            })
        }

        IpcRequest::SetConfig { config: json } => {
            match serde_json::from_value::<DaemonConfig>(json.clone()) {
                Ok(_cfg) => {
                    let keys = cfg_keys_from_json(json);
                    IpcMessage::response(IpcResponse::ConfigLoaded { keys })
                }
                Err(e) => IpcMessage::response(IpcResponse::Error {
                    code: "CONFIG_PARSE_ERROR".to_string(),
                    details: e.to_string(),
                }),
            }
        }

        IpcRequest::ReloadConfig => {
            match reload_config_inner(&mut guard) {
                Ok(keys) => IpcMessage::response(IpcResponse::ConfigLoaded { keys }),
                Err(e) => IpcMessage::response(IpcResponse::Error {
                    code: "RELOAD_ERROR".to_string(),
                    details: e,
                }),
            }
        }

        IpcRequest::TestShortcut { keys, action: _ } => {
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

            IpcMessage::response(IpcResponse::TestResult {
                success: all_ok,
                message: msg_text,
            })
        }
    }
}

/// Reload configuration from disk (internal, requires mutable state).
fn reload_config_inner(guard: &mut DaemonState) -> Result<Vec<addon_core::ipc::KeyBindingJson>, String> {
    let path = get_config_path().map_err(|e| e.to_string())?;
    let new_config = addon_core::config::load(&path).map_err(|e| e.to_string())?;

    // Detect conflicts.
    let conflicts = addon_core::conflict::detect_conflicts(&new_config.keybindings);
    if !conflicts.is_empty() {
        for c in &conflicts {
            tracing::warn!(
                "Conflict: {} ↔ {} [platform: {:?}]",
                c.binding1, c.binding2, c.platform
            );
        }
    }

    // Update state.
    guard.config = new_config.clone();

    // Rebuild adapter keymap if running.
    if let Some(ref mut adapter) = guard.adapter {
        adapter.stop().map_err(|e| e.to_string())?;
        adapter.init().map_err(|e| e.to_string())?;
        adapter.start().map_err(|e| e.to_string())?;
    }

    let keys: Vec<addon_core::ipc::KeyBindingJson> = guard
        .config
        .keybindings
        .clone()
        .into_iter()
        .map(addon_core::ipc::KeyBindingJson::from)
        .collect();

    Ok(keys)
}

/// Extract key bindings from a JSON config value (for SetConfig requests).
fn cfg_keys_from_json(json: &serde_json::Value) -> Vec<addon_core::ipc::KeyBindingJson> {
    if let Some(obj) = json.as_object() {
        if let Some(bindings) = obj.get("keybindings").and_then(|v| v.as_array()) {
            return bindings
                .iter()
                .filter_map(|b| {
                    let id = b.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let keys = b.get("keys")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let action_type = b.get("type")
                        .and_then(|v| v.as_str())
                        .or_else(|| b.get("action").and_then(|a| a.get("type")).and_then(|v| v.as_str()))
                        .unwrap_or("unknown")
                        .to_string();
                    Some(addon_core::ipc::KeyBindingJson { id, keys, action_type })
                })
                .collect();
        }
    }
    Vec::new()
}

/// Returns the path to the configuration file.
fn get_config_path() -> Result<std::path::PathBuf, String> {
    if let Ok(path) = std::env::var("ADDON_CONFIG") {
        return Ok(std::path::PathBuf::from(path));
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".addon").join("config.yaml");
        if p.exists() {
            return Ok(p);
        }
    }
    if std::path::PathBuf::from("config.yaml").exists() {
        return Ok(std::path::PathBuf::from("config.yaml"));
    }
    if let Some(home) = dirs::home_dir() {
        Ok(home.join(".addon").join("config.yaml"))
    } else {
        Err("cannot determine config path".to_string())
    }
}

/// A lightweight config struct used for IPC message validation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DaemonConfig {
    version: String,
    #[serde(default)]
    keybindings: Vec<DaemonKeyBinding>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct DaemonKeyBinding {
    id: String,
    keys: Vec<String>,
    #[serde(rename = "type")]
    action_type: String,
}
