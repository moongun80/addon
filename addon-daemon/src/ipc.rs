//! IPC server — Unix domain socket listener and handler.
//!
//! The daemon listens on a Unix socket for JSON messages from the
//! Tauri GUI client. Each message is newline-delimited (`\n`),
//! following the same protocol used by the GUI client.
//!
//! IMP-001: All client connections must authenticate via HMAC-SHA256
//! challenge-response before any requests are processed.

use std::path::Path;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};

use crate::daemon::DaemonState;
use crate::get_config_path;
use addon_core::ipc::{IpcMessage, IpcRequest, IpcResponse, AuthChallenge, AuthToken};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Authentication helpers — IMP-001
// ---------------------------------------------------------------------------

/// Generate a cryptographically random 32-byte hex nonce.
fn generate_nonce() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

/// Compute HMAC-SHA256(secret, nonce + daemon_pid_bytes)
fn compute_hmac(secret: &str, nonce: &str, daemon_pid: u32) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(nonce.as_bytes());
    mac.update(&daemon_pid.to_le_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// Verify the client's auth token against the shared secret.
fn verify_auth_token(secret: &str, token: &AuthToken, daemon_pid: u32, timestamp: u64) -> Result<(), &'static str> {
    let expected_sig = compute_hmac(secret, &token.nonce, daemon_pid);
    if expected_sig != token.signature {
        return Err("invalid_signature");
    }
    
    // Verify timestamp (±30 seconds)
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "time_error")?
        .as_secs();
    
    if (now as i64 - timestamp as i64).abs() > 30 {
        return Err("expired");
    }
    
    Ok(())
}

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
        // FIX-SEC-002: Fail daemon startup if permissions cannot be set,
        // because a world-accessible socket is a security risk.
        std::fs::set_permissions(
            &address,
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        ).map_err(|e| {
            tracing::error!("CRITICAL: Failed to restrict socket permissions: {}", e);
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("cannot restrict socket permissions: {}", e),
            )
        })?;

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
/// IMP-001: The first message MUST be an authentication request.
/// The daemon sends an `AuthChallenge` if the client hasn't authenticated yet.
///
/// The protocol is newline-delimited JSON: each message ends with `\n`.
/// We use a `BufReader` with `read_line` to handle partial reads correctly.
async fn handle_client(
    stream: UnixStream,
    daemon_state: Arc<std::sync::RwLock<DaemonState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (reader, mut writer) = stream.into_split();
    let mut buf_reader = tokio::io::BufReader::new(reader);

    // FIX-020: Track consecutive parse failures to prevent DoS from malformed input.
    let mut consecutive_parse_failures: u32 = 0;
    const MAX_PARSE_FAILURES: u32 = 10;

    // IMP-001: Track authentication state per connection
    let mut authenticated = false;

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
            Ok(m) => {
                // Reset failure counter on successful parse
                consecutive_parse_failures = 0;
                m
            }
            Err(e) => {
                consecutive_parse_failures += 1;
                if consecutive_parse_failures >= MAX_PARSE_FAILURES {
                    tracing::warn!(
                        "Dropping IPC connection after {} consecutive parse failures",
                        consecutive_parse_failures
                    );
                    break;
                }
                tracing::error!("Failed to parse IPC message (failure {}/{}): {}", 
                    consecutive_parse_failures, MAX_PARSE_FAILURES, e);
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

        // IMP-005: Protocol version check
        if !msg.is_compatible() {
            let err_resp = IpcMessage::response(IpcResponse::Error {
                code: "PROTOCOL_VERSION_MISMATCH".to_string(),
                details: format!(
                    "Incoming version {} not in supported range [{}, {}]",
                    msg.version(),
                    addon_core::ipc::PROTOCOL_MIN_VERSION,
                    addon_core::ipc::PROTOCOL_MAX_VERSION,
                ),
                request_id: None,
            });
            let json = serde_json::to_string(&err_resp)?;
            let _ = writer.write_all(json.as_bytes()).await;
            let _ = writer.write_all(b"\n").await;
            let _ = writer.flush().await;
            continue;
        }

        // IMP-001: Authentication gate
        if !authenticated {
            match &msg {
                IpcMessage::Request { inner: IpcRequest::Auth { token, .. }, .. } => {
                    // Verify the auth token
                    let (secret, daemon_pid) = {
                        let state_guard = daemon_state.read().map_err(|e| {
                            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                        })?;
                        (state_guard.auth_secret.clone(), std::process::id())
                    };

                    let auth_resp = match verify_auth_token(&secret, token, daemon_pid, token.timestamp) {
                        Ok(()) => {
                            tracing::info!("Client authenticated successfully (PID {})", token.client_pid);
                            IpcMessage::response(IpcResponse::AuthResult {
                                accepted: true,
                                reason: None,
                                request_id: None,
                            })
                        }
                        Err(reason) => {
                            tracing::warn!("Authentication failed: {}", reason);
                            IpcMessage::response(IpcResponse::AuthResult {
                                accepted: false,
                                reason: Some(reason.to_string()),
                                request_id: None,
                            })
                        }
                    };
                    
                    let json = serde_json::to_string(&auth_resp)?;
                    writer.write_all(json.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;

                    if auth_resp.is_response() {
                        if let IpcMessage::Response { inner: IpcResponse::AuthResult { accepted, .. }, .. } = &auth_resp {
                            if *accepted {
                                authenticated = true;
                            } else {
                                tracing::warn!("Client rejected after auth failure, closing connection");
                                break;
                            }
                        }
                    }
                    continue;
                }
                IpcMessage::Request { .. } => {
                    // Client tried to send a non-auth request without authenticating
                    // Send an AuthChallenge to prompt authentication
                    let (_secret, daemon_pid) = {
                        let state_guard = daemon_state.read().map_err(|e| {
                            std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
                        })?;
                        (state_guard.auth_secret.clone(), std::process::id())
                    };

                    let nonce = generate_nonce();
                    let timestamp = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?
                        .as_secs();

                    let challenge = AuthChallenge {
                        nonce,
                        daemon_pid,
                        timestamp,
                    };
                    let challenge_resp = IpcMessage::response(IpcResponse::AuthChallenge {
                        challenge,
                        request_id: None,
                    });

                    let json = serde_json::to_string(&challenge_resp)?;
                    writer.write_all(json.as_bytes()).await?;
                    writer.write_all(b"\n").await?;
                    writer.flush().await?;
                    continue;
                }
                IpcMessage::Response { .. } => {
                    tracing::warn!("Unexpected response from client, ignoring");
                    continue;
                }
            }
        }

        // IMP-001: Client is authenticated — process the request
        let resp = match &msg {
            IpcMessage::Request { inner: req, .. } => process_request(req, &daemon_state),
            IpcMessage::Response { .. } => {
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

    // IMP-009-A: Shared adapter reinit logic extracted below
    fn reinit_adapter(state: &Arc<std::sync::RwLock<DaemonState>>, cfg: &addon_core::config::Config) -> bool {
        let adapter = {
            let mut g = match state.write() {
                Ok(g) => g,
                Err(e) => e.into_inner(),
            };
            g.adapter.take()
        };

        let result = if let Some(mut adapter) = adapter {
            adapter.set_config(cfg);
            let stop_ok = adapter.stop().is_ok();
            let init_ok = if stop_ok { adapter.init().is_ok() } else { false };
            let start_ok = if init_ok { adapter.start().is_ok() } else { true };
            if !stop_ok { tracing::warn!("Adapter stop during reinit"); }
            else if !init_ok { tracing::warn!("Adapter reinit during reinit"); }
            else if !start_ok { tracing::warn!("Adapter start during reinit"); }
            {
                let mut g = match state.write() {
                    Ok(g) => g,
                    Err(e) => e.into_inner(),
                };
                g.adapter = Some(adapter);
                g.initialized = stop_ok && init_ok && start_ok;
            }
            stop_ok && init_ok && start_ok
        } else {
            true
        };

        result
    }

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

                let new_cfg = guard.config.clone();
                drop(guard);

                // IMP-009-A: Reinitialize adapter using shared helper
                if old_running {
                    reinit_adapter(state, &new_cfg);
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
                            "Conflict: {} ↔ {} [key: {}]",
                            c.bindings[0],
                            c.bindings[1],
                            c.key
                        );
                    }
                }

                // Update state and collect keys for response.
                guard.config = new_config.clone();
                let keys: Vec<addon_core::ipc::KeyBindingJson> = guard
                    .config
                    .keybindings
                    .iter()
                    .cloned()
                    .map(addon_core::ipc::KeyBindingJson::from)
                    .collect();

                drop(guard);

                // IMP-009-A: Reinitialize adapter using shared helper
                reinit_adapter(state, &new_config);

                return IpcMessage::response(
                    IpcResponse::ConfigLoaded {
                        keys,
                        request_id: None,
                    }
                    .with_request_id(request_id),
                );
            }

            // IMP-007: Add a keybinding through the single write entry point
            IpcRequest::AddKeybinding { id, keys, action, .. } => {
                // Parse the action
                let new_action: addon_core::actions::Action =
                    match serde_json::from_value(action.clone()) {
                        Ok(a) => a,
                        Err(e) => {
                            drop(guard);
                            return IpcMessage::response(
                                IpcResponse::Error {
                                    code: "ACTION_PARSE_ERROR".to_string(),
                                    details: e.to_string(),
                                    request_id: None,
                                }
                                .with_request_id(request_id),
                            );
                        }
                    };

                // Validate system commands
                if let addon_core::actions::Action::SystemCommand { command } = &new_action {
                    if let Err(e) = addon_core::actions::validate_system_command(command) {
                        drop(guard);
                        return IpcMessage::response(
                            IpcResponse::Error {
                                code: "COMMAND_VALIDATION_ERROR".to_string(),
                                details: format!("New binding '{}': {}", id, e),
                                request_id: None,
                            }
                            .with_request_id(request_id),
                        );
                    }
                }

                // Check for duplicate ID
                if guard.config.keybindings.iter().any(|b| b.id == *id) {
                    drop(guard);
                    return IpcMessage::response(
                        IpcResponse::Error {
                            code: "DUPLICATE_ID".to_string(),
                            details: format!("Keybinding '{}' already exists", id),
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }

                // Add the binding
                guard.config.keybindings.push(addon_core::config::KeyBinding {
                    id: id.clone(),
                    keys: keys.clone(),
                    action: new_action,
                    overrides: None,
                });

                // Save to disk
                let path = match get_config_path() {
                    Ok(p) => p,
                    Err(e) => {
                        drop(guard);
                        return IpcMessage::response(
                            IpcResponse::Error {
                                code: "SAVE_ERROR".to_string(),
                                details: e.to_string(),
                                request_id: None,
                            }
                            .with_request_id(request_id),
                        );
                    }
                };

                if let Err(e) = addon_core::config::save_to_disk(&path, &guard.config) {
                    drop(guard);
                    return IpcMessage::response(
                        IpcResponse::Error {
                            code: "SAVE_ERROR".to_string(),
                            details: e.to_string(),
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }

                // Collect keys for response, then reinit adapter
                let cfg_for_adapter = guard.config.clone();
                let keys_json: Vec<addon_core::ipc::KeyBindingJson> = guard
                    .config
                    .keybindings
                    .iter()
                    .cloned()
                    .map(addon_core::ipc::KeyBindingJson::from)
                    .collect();

                drop(guard);

                // IMP-009-A: Reinitialize adapter using shared helper
                reinit_adapter(state, &cfg_for_adapter);

                return IpcMessage::response(
                    IpcResponse::ConfigLoaded {
                        keys: keys_json,
                        request_id: None,
                    }
                    .with_request_id(request_id),
                );
            }

            // IMP-007: Remove a keybinding through the single write entry point
            IpcRequest::RemoveKeybinding { id, .. } => {
                let initial_len = guard.config.keybindings.len();
                guard.config.keybindings.retain(|b| b.id != *id);

                if guard.config.keybindings.len() == initial_len {
                    drop(guard);
                    return IpcMessage::response(
                        IpcResponse::Error {
                            code: "NOT_FOUND".to_string(),
                            details: format!("Keybinding '{}' not found", id),
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }

                // Save to disk
                let path = match get_config_path() {
                    Ok(p) => p,
                    Err(e) => {
                        drop(guard);
                        return IpcMessage::response(
                            IpcResponse::Error {
                                code: "SAVE_ERROR".to_string(),
                                details: e.to_string(),
                                request_id: None,
                            }
                            .with_request_id(request_id),
                        );
                    }
                };

                if let Err(e) = addon_core::config::save_to_disk(&path, &guard.config) {
                    drop(guard);
                    return IpcMessage::response(
                        IpcResponse::Error {
                            code: "SAVE_ERROR".to_string(),
                            details: e.to_string(),
                            request_id: None,
                        }
                        .with_request_id(request_id),
                    );
                }

                // Collect keys for response, then reinit adapter
                let cfg_for_adapter = guard.config.clone();
                let keys_json: Vec<addon_core::ipc::KeyBindingJson> = guard
                    .config
                    .keybindings
                    .iter()
                    .cloned()
                    .map(addon_core::ipc::KeyBindingJson::from)
                    .collect();

                drop(guard);

                // IMP-009-A: Reinitialize adapter using shared helper
                reinit_adapter(state, &cfg_for_adapter);

                return IpcMessage::response(
                    IpcResponse::ConfigLoaded {
                        keys: keys_json,
                        request_id: None,
                    }
                    .with_request_id(request_id),
                );
            }

            // Fallback for unknown request variants — prevents panic if new
            // IpcRequest variants are added to addon-core without updating here.
            _ => {
                drop(guard);
                return IpcMessage::response(
                    IpcResponse::Error {
                        code: "UNKNOWN_REQUEST".to_string(),
                        details: "Unknown IPC request type received".to_string(),
                        request_id: None,
                    }
                    .with_request_id(request_id),
                );
            }
        }
    };

    IpcMessage::response(resp.with_request_id(request_id))
}
