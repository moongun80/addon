//! Tauri command handlers for the addon GUI.
//!
//! All IPC commands communicate with the daemon via Unix domain sockets
//! using a newline-delimited JSON protocol. Commands are async because
//! socket I/O must not block the Tauri runtime.
//!
//! IMP-001: All connections authenticate via HMAC-SHA256 challenge-response
//! before sending any requests.

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use addon_core::ipc::{IpcMessage, IpcRequest, IpcResponse, KeyBindingJson, AuthChallenge, AuthToken};

use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the platform-specific socket path.
fn get_socket_path() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("addon");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("daemon.sock")
}

/// Returns the path to the daemon auth token file.
fn get_token_path() -> std::path::PathBuf {
    get_socket_path().parent()
        .map(|p| p.join(".daemon_token"))
        .expect("socket path has a parent")
}

/// Load the shared secret from the token file.
fn load_secret() -> Option<String> {
    std::fs::read_to_string(get_token_path()).ok()
}

/// Global mutex to serialize authentication (only one auth handshake at a time).
static AUTH_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Authenticate with the daemon using HMAC-SHA256 challenge-response.
/// IMP-001: This must be called before any IPC request.
async fn authenticate() -> Result<(), anyhow::Error> {
    // Serialize auth handshakes — only one at a time
    let _lock = AUTH_MUTEX.lock().unwrap();
    
    let secret = load_secret().ok_or_else(|| anyhow::anyhow!("Auth token file not found"))?;
    
    // Connect to daemon
    let socket = get_socket_path();
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        UnixStream::connect(&socket),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Daemon not responding (connection timeout)"))??;
    
    // Send auth request
    let auth_req = IpcMessage::request(IpcRequest::Auth {
        token: AuthToken {
            client_pid: std::process::id(),
            nonce: String::new(), // Will be filled after challenge
            signature: String::new(),
        },
        request_id: None,
    });
    
    let json = serde_json::to_string(&auth_req)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;
    
    // Read challenge response
    let mut buf_reader = BufReader::new(stream);
    let mut line = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        buf_reader.read_line(&mut line),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Auth challenge read timeout"))?
    .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;
    
    let msg: IpcMessage = serde_json::from_str(line.trim())?;
    
    // Extract challenge
    let (nonce, daemon_pid, timestamp) = match &msg {
        IpcMessage::Response { inner: IpcResponse::AuthChallenge { challenge, .. }, .. } => (
            challenge.nonce.clone(),
            challenge.daemon_pid,
            challenge.timestamp,
        ),
        _ => return Err(anyhow::anyhow!("Expected AuthChallenge response")),
    };
    
    // Verify timestamp (±30 seconds)
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| anyhow::anyhow!("Time error: {}", e))?
        .as_secs();
    
    if (now as i64 - timestamp as i64).abs() > 30 {
        return Err(anyhow::anyhow!("Auth challenge expired"));
    }
    
    // Compute HMAC-SHA256 signature
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| anyhow::anyhow!("HMAC init failed: {}", e))?;
    mac.update(nonce.as_bytes());
    mac.update(daemon_pid.to_le_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());
    
    // Send auth token
    let auth_token = AuthToken {
        client_pid: std::process::id(),
        nonce,
        signature,
    };
    
    let auth_msg = IpcMessage::request(IpcRequest::Auth {
        token: auth_token,
        request_id: None,
    });
    
    let json = serde_json::to_string(&auth_msg)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;
    
    // Read auth result
    let mut line = String::new();
    buf_reader = BufReader::new(stream);
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        buf_reader.read_line(&mut line),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Auth result read timeout"))?
    .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;
    
    let result: IpcMessage = serde_json::from_str(line.trim())?;
    
    match &result {
        IpcMessage::Response { inner: IpcResponse::AuthResult { accepted, reason, .. }, .. } => {
            if *accepted {
                tracing::info!("GUI authenticated with daemon");
                Ok(())
            } else {
                Err(anyhow::anyhow!("Auth rejected: {:?}", reason))
            }
        }
        _ => Err(anyhow::anyhow!("Expected AuthResult response")),
    }
}

/// Send a message over a Unix domain socket and await the response.
///
/// IMP-001: Authenticates with the daemon first, then sends the request.
async fn send_async(msg: &IpcMessage) -> Result<IpcMessage, anyhow::Error> {
    // IMP-001: Authenticate first
    authenticate().await?;
    
    let socket = get_socket_path();
    let mut stream = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        UnixStream::connect(&socket),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Daemon not responding (connection timeout)"))??;

    let json = serde_json::to_string(msg)?;
    stream.write_all(json.as_bytes()).await?;
    stream.write_all(b"\n").await?;
    stream.flush().await?;

    // Read the newline-delimited response.
    let mut buf_reader = BufReader::new(stream);
    let mut line = String::new();
    tokio::time::timeout(std::time::Duration::from_secs(5), buf_reader.read_line(&mut line))
        .await
        .map_err(|_| anyhow::anyhow!("Daemon read timeout (no response within 5s)"))?
        .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;
    let result: IpcMessage = serde_json::from_str(line.trim())?;
    Ok(result)
}

#[tauri::command(async)]
async fn get_daemon_status() -> Result<serde_json::Value, String> {
    match send_async(&IpcMessage::request(IpcRequest::GetStatus)).await {
        Ok(msg) => {
            if let IpcMessage::Response(IpcResponse::DaemonStatus {
                running,
                pid,
                version,
            }) = msg
            {
                Ok(serde_json::json!({
                    "type": "daemon_status",
                    "running": running,
                    "pid": pid,
                    "version": version
                }))
            } else {
                // FIX-021: Replace unwrap() with safe serialization
                serde_json::to_value(&msg).map_err(|e| e.to_string())
            }
        }
        Err(e) => Ok(serde_json::json!({
            "type": "error",
            "code": "io",
            "details": e.to_string()
        })),
    }
}

#[tauri::command(async)]
async fn list_keybindings() -> Result<Vec<KeyBindingJson>, String> {
    let path = config_ops::get_config_path();
    let config = addon_core::config::load(&path).map_err(|e| e.to_string())?;
    Ok(config
        .keybindings
        .into_iter()
        .map(KeyBindingJson::from)
        .collect())
}

#[tauri::command(async)]
async fn reload_config() -> Result<serde_json::Value, String> {
    // FIX-025: Removed redundant local config load — only the daemon reload matters.
    match send_async(&IpcMessage::request(IpcRequest::ReloadConfig)).await {
        Ok(msg) => {
            if let IpcMessage::Response(IpcResponse::ConfigLoaded { keys }) = msg {
                Ok(serde_json::json!({
                    "type": "config_loaded",
                    "keys": keys
                }))
            } else {
                // FIX-021: Replace unwrap() with safe serialization
                serde_json::to_value(&msg).map_err(|e| e.to_string())
            }
        }
        Err(e) => Ok(serde_json::json!({
            "type": "error",
            "code": "ipc",
            "details": e.to_string()
        })),
    }
}

#[tauri::command(async)]
async fn test_shortcut(
    keys: Vec<String>,
    action: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match send_async(&IpcMessage::request(IpcRequest::TestShortcut { keys, action })).await {
        Ok(msg) => {
            if let IpcMessage::Response(IpcResponse::TestResult {
                success,
                message,
            }) = msg
            {
                Ok(serde_json::json!({
                    "type": "test_result",
                    "success": success,
                    "message": message
                }))
            } else {
                // FIX-021: Replace unwrap() with safe serialization
                serde_json::to_value(&msg).map_err(|e| e.to_string())
            }
        }
        Err(e) => Ok(serde_json::json!({
            "type": "error",
            "code": "ipc",
            "details": e.to_string()
        })),
    }
}

/// Add a keybinding by properly constructing the action from user-supplied
/// `action_type` and `action_data` fields (rather than creating dummy values).
#[tauri::command(async)]
async fn add_keybinding(
    id: String,
    keys: String,
    action_type: String,
    action_data: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let path = config_ops::get_config_path();
    let content = config_ops::load_config(&path).map_err(|e| e.to_string())?;
    let mut config: addon_core::config::Config =
        serde_yaml::from_str(&content).map_err(|e| e.to_string())?;

    // FIX-SEC-001: Validate system_command BEFORE constructing Action
    // to prevent potentially dangerous commands from entering the config.
    if action_type.as_str() == "system_command" {
        let cmd = action_data
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        addon_core::actions::validate_system_command(cmd)
            .map_err(|e| format!("Command validation failed: {}", e))?;
    }

    // Properly construct the action from action_type + action_data.
    let action = match action_type.as_str() {
        "paste" => addon_core::actions::Action::Paste {
            text: action_data
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        "launch" => addon_core::actions::Action::Launch {
            path: action_data
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        "remap" => addon_core::actions::Action::Remap {
            to: action_data
                .get("to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        "shortcut" => addon_core::actions::Action::Shortcut {
            shortcut: action_data
                .get("shortcut")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        },
        "system_command" => addon_core::actions::Action::SystemCommand {
            command: action_data
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        "text_insert" => addon_core::actions::Action::TextInsert {
            text: action_data
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        },
        _ => {
            return Err(format!("Unknown action type: {action_type}"));
        }
    };

    let keys_vec: Vec<String> = keys.split(',').map(|s| s.trim().to_string()).collect();

    config.keybindings.push(addon_core::config::KeyBinding {
        id: id.clone(),
        keys: keys_vec,
        action,
        overrides: None,
    });

    let yaml = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;
    config_ops::save_config(&path, &yaml).map_err(|e| e.to_string())?;

    // Reload daemon to pick up the new binding.
    // FIX-024: Wrap connection with timeout so GUI doesn't hang if daemon is dead.
    let connect_result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::UnixStream::connect(get_socket_path()),
    )
    .await;
    match connect_result {
        Ok(Ok(mut stream)) => {
            let req = IpcMessage::request(IpcRequest::ReloadConfig);
            let json = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            stream
                .write_all(json.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            stream
                .write_all(b"\n")
                .await
                .map_err(|e| e.to_string())?;
            stream.flush().await.map_err(|e| e.to_string())?;

            let mut buf_reader =
                tokio::io::BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
            let mut line = String::new();
            buf_reader
                .read_line(&mut line)
                .await
                .map_err(|e| e.to_string())?;

            match serde_json::from_str::<IpcMessage>(line.trim()) {
                Ok(IpcMessage::Response(IpcResponse::ConfigLoaded { .. })) => Ok(
                    serde_json::json!({
                        "type": "success",
                        "message": format!("Added {} and reloaded", id)
                    }),
                ),
                _ => Ok(serde_json::json!({
                    "type": "partial_success",
                    "message": format!("Added {} but daemon reload failed", id)
                })),
            }
        }
        Ok(Err(_)) | Err(_) => Ok(serde_json::json!({
            "type": "partial_success",
            "message": format!("Added {} but daemon reload failed (not running or timed out)", id)
        })),
    }
}

#[tauri::command(async)]
async fn remove_keybinding(id: String) -> Result<serde_json::Value, String> {
    let path = config_ops::get_config_path();
    let content = config_ops::load_config(&path).map_err(|e| e.to_string())?;
    let mut config: addon_core::config::Config =
        serde_yaml::from_str(&content).map_err(|e| e.to_string())?;

    let before = config.keybindings.len();
    config.keybindings.retain(|b| b.id != id);

    if config.keybindings.len() == before {
        return Ok(serde_json::json!({
            "type": "error",
            "code": "not_found",
            "details": format!("Binding '{}' not found", id)
        }));
    }

    let yaml = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;
    config_ops::save_config(&path, &yaml).map_err(|e| e.to_string())?;

    // FIX-013: Notify daemon to reload config after removing keybinding
    // FIX-024: Wrap connection with timeout so GUI doesn't hang if daemon is dead.
    let connect_result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::UnixStream::connect(get_socket_path()),
    )
    .await;
    match connect_result {
        Ok(Ok(mut stream)) => {
            let req = IpcMessage::request(IpcRequest::ReloadConfig);
            let json = serde_json::to_string(&req).map_err(|e| e.to_string())?;
            stream
                .write_all(json.as_bytes())
                .await
                .map_err(|e| e.to_string())?;
            stream
                .write_all(b"\n")
                .await
                .map_err(|e| e.to_string())?;
            stream.flush().await.map_err(|e| e.to_string())?;

            let mut buf_reader =
                tokio::io::BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
            let mut line = String::new();
            buf_reader
                .read_line(&mut line)
                .await
                .map_err(|e| e.to_string())?;

            match serde_json::from_str::<IpcMessage>(line.trim()) {
                Ok(IpcMessage::Response(IpcResponse::ConfigLoaded { .. })) => Ok(
                    serde_json::json!({
                        "type": "success",
                        "message": format!("Removed {} and reloaded", id)
                    }),
                ),
                _ => Ok(serde_json::json!({
                    "type": "partial_success",
                    "message": format!("Removed {} but daemon reload failed", id)
                })),
            }
        }
        Ok(Err(_)) | Err(_) => Ok(serde_json::json!({
            "type": "partial_success",
            "message": format!("Removed {} but daemon reload failed (not running or timed out)", id)
        })),
    }
}

#[tauri::command(async)]
async fn export_config(format: String) -> Result<String, String> {
    let path = config_ops::get_config_path();
    config_ops::export_config(&path, &format).map_err(|e| e.to_string())
}
