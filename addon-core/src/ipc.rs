//! IPC message types and protocol for GUI ↔ Daemon communication.
//!
//! Provides the [`IpcMessage`] enum that carries all requests from the GUI
//! client to the daemon and all responses back. Messages are serialized
//! as JSON, one per line (newline-delimited frames).
//!
//! The `#[serde(tag = "type")]` attribute enables envelope-style
//! discrimination so the receiver can match responses to requests.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Authentication types — IMP-001
// ---------------------------------------------------------------------------

/// Authentication challenge sent by the daemon to the client.
/// The client must sign the nonce within ±30 seconds to prove identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthChallenge {
    /// 32-byte hex-encoded random nonce (replay attack prevention)
    pub nonce: String,
    /// Daemon PID at startup (replay attack prevention)
    pub daemon_pid: u32,
    /// Unix timestamp in seconds when the challenge was issued
    pub timestamp: u64,
}

/// Authentication token sent by the client in response to a challenge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthToken {
    /// Client PID
    pub client_pid: u32,
    /// Echo of the challenge nonce
    pub nonce: String,
    /// Echo of the challenge timestamp (for replay protection)
    pub timestamp: u64,
    /// HMAC-SHA256(secret, nonce + daemon_pid.to_le_bytes()) as hex string
    pub signature: String,
}

/// Authentication result returned by the daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResult {
    /// Whether authentication succeeded
    pub accepted: bool,
    /// Optional rejection reason: "expired", "invalid_signature", "unknown"
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Message types — Client → Daemon
// ---------------------------------------------------------------------------

/// Requests sent from the Tauri GUI client to the daemon.
///
/// Each variant carries the parameters needed to execute the operation.
///
/// ## Protocol Notes
///
/// - Messages are JSON-encoded with a trailing newline.
/// - The `type` field (enforced by `serde(tag = "type")`) is used to
///   discriminate the variant.
/// - The daemon responds with a matching `IpcMessage` response variant.
/// - `request_id` is an optional correlation ID echoed back in responses.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    /// Authentication — must be the first message sent after connecting
    Auth {
        token: AuthToken,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Update daemon configuration.
    SetConfig {
        config: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Ask the daemon to reload configuration from disk.
    ReloadConfig {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Test whether a shortcut can be processed.
    TestShortcut {
        keys: Vec<String>,
        action: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Query current daemon status.
    GetStatus {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Start the daemon (if not running).
    StartDaemon {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Stop the daemon (if running).
    StopDaemon {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Add a new keybinding (IMP-007: single write entry point).
    AddKeybinding {
        /// Unique identifier for this binding.
        id: String,
        /// Key stroke representations (e.g. `["Ctrl+V"]`).
        keys: Vec<String>,
        /// The serialized action.
        action: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Remove an existing keybinding by ID (IMP-007).
    RemoveKeybinding {
        /// Identifier of the binding to remove.
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
}

impl IpcRequest {
    /// Extract the optional request correlation ID.
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Self::Auth { request_id, .. }
            | Self::SetConfig { request_id, .. }
            | Self::ReloadConfig { request_id }
            | Self::TestShortcut { request_id, .. }
            | Self::GetStatus { request_id }
            | Self::StartDaemon { request_id }
            | Self::StopDaemon { request_id }
            | Self::AddKeybinding { request_id, .. }
            | Self::RemoveKeybinding { request_id, .. } => request_id.as_deref(),
        }
    }
}

// ---------------------------------------------------------------------------
// Message types — Daemon → Client
// ---------------------------------------------------------------------------

/// Responses sent from the daemon to the Tauri GUI client.
///
/// Each response variant corresponds to one or more [`IpcRequest`] variants.
/// `request_id` is echoed from the original request for correlation.

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    /// Authentication challenge — sent by daemon when client connects
    AuthChallenge {
        challenge: AuthChallenge,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Authentication result — sent by daemon after client sends Auth token
    AuthResult {
        accepted: bool,
        reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Current daemon status.
    DaemonStatus {
        running: bool,
        pid: u32,
        version: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Acknowledgment that configuration was loaded with the given bindings.
    ConfigLoaded {
        keys: Vec<KeyBindingJson>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// Result of a shortcut test.
    TestResult {
        success: bool,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// A log entry generated by the daemon.
    LogEntry {
        level: String,
        target: String,
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },

    /// An error occurred.
    Error {
        code: String,
        details: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
    },
}

impl IpcResponse {
    /// Echo the request_id from the original request into this response.
    ///
    /// This enables the caller to correlate responses with requests.
    pub fn with_request_id(self, request_id: impl Into<Option<String>>) -> Self {
        let rid = request_id.into();
        match self {
            Self::AuthChallenge { challenge, .. } => Self::AuthChallenge {
                challenge,
                request_id: rid,
            },
            Self::AuthResult {
                accepted,
                reason,
                ..
            } => Self::AuthResult {
                accepted,
                reason,
                request_id: rid,
            },
            Self::DaemonStatus {
                running,
                pid,
                version,
                ..
            } => Self::DaemonStatus {
                running,
                pid,
                version,
                request_id: rid,
            },
            Self::ConfigLoaded { keys, .. } => Self::ConfigLoaded {
                keys,
                request_id: rid,
            },
            Self::TestResult {
                success, message, ..
            } => Self::TestResult {
                success,
                message,
                request_id: rid,
            },
            Self::LogEntry {
                level,
                target,
                message,
                ..
            } => Self::LogEntry {
                level,
                target,
                message,
                request_id: rid,
            },
            Self::Error { code, details, .. } => Self::Error {
                code,
                details,
                request_id: rid,
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Protocol version constants
// ---------------------------------------------------------------------------

/// Current IPC protocol version.
pub const PROTOCOL_VERSION: u16 = 2;

/// Minimum supported protocol version (for backward compatibility).
pub const PROTOCOL_MIN_VERSION: u16 = 1;

/// Maximum supported protocol version.
pub const PROTOCOL_MAX_VERSION: u16 = 2;

// ---------------------------------------------------------------------------
// Unified message type (client → server envelope)
// ---------------------------------------------------------------------------

/// A single IPC message that can flow in either direction.
///
/// The `version` field enables protocol evolution. Messages without an
/// explicit version are treated as v1 for backward compatibility.
///
/// Supports two JSON formats for backward compatibility:
/// - **Envelope**: `{ "version": 2, "type": "get_status" }`
/// - **Untagged (v1)**: `{ "type": "get_status" }` (shorthand)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IpcMessage {
    /// A request from the GUI to the daemon (with optional version).
    Request {
        /// Protocol version (defaults to 1 if omitted).
        #[serde(default = "default_version")]
        version: u16,
        #[serde(flatten)]
        inner: IpcRequest,
    },
    /// A response from the daemon to the GUI (with optional version).
    Response {
        /// Protocol version.
        #[serde(default = "default_version")]
        version: u16,
        #[serde(flatten)]
        inner: IpcResponse,
    },
}

/// Default version for backward-compatible deserialization.
fn default_version() -> u16 {
    PROTOCOL_MIN_VERSION
}

impl IpcMessage {
    /// Convert a request into this envelope type.
    pub fn request(req: IpcRequest) -> Self {
        Self::Request {
            version: PROTOCOL_VERSION,
            inner: req,
        }
    }

    /// Convert a response into this envelope type.
    pub fn response(resp: IpcResponse) -> Self {
        Self::Response {
            version: PROTOCOL_VERSION,
            inner: resp,
        }
    }

    /// Check whether this is a request.
    pub fn is_request(&self) -> bool {
        matches!(self, Self::Request { .. })
    }

    /// Check whether this is a response.
    pub fn is_response(&self) -> bool {
        matches!(self, Self::Response { .. })
    }

    /// Return the variant kind as a string for logging.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Request { .. } => "request",
            Self::Response { .. } => "response",
        }
    }

    /// Get the protocol version of this message.
    pub fn version(&self) -> u16 {
        match self {
            Self::Request { version, .. } => *version,
            Self::Response { version, .. } => *version,
        }
    }

    /// Check whether this message's version is compatible.
    pub fn is_compatible(&self) -> bool {
        let v = self.version();
        v >= PROTOCOL_MIN_VERSION && v <= PROTOCOL_MAX_VERSION
    }
}

// ---------------------------------------------------------------------------
// JSON-serializable key binding representation
// ---------------------------------------------------------------------------

/// Platform-specific key overrides for IPC serialization.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlatformOverridesJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux: Option<Vec<String>>,
}

/// A key binding representation suitable for IPC serialization.
///
/// This is a flattened, JSON-friendly view of [`config::KeyBinding`]
/// used in response messages.

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingJson {
    /// Unique identifier for this binding.
    pub id: String,
    /// Key stroke representations (e.g. `["Ctrl+V"]`).
    pub keys: Vec<String>,
    /// The action type (e.g. `"paste"`, `"launch"`, `"system_command"`).
    pub action_type: String,
    /// Platform-specific key overrides (FIX-019).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<PlatformOverridesJson>,
}

impl From<crate::config::KeyBinding> for KeyBindingJson {
    fn from(binding: crate::config::KeyBinding) -> Self {
        Self {
            id: binding.id,
            keys: binding.keys,
            action_type: binding.action.variant_name().to_string(),
            overrides: binding.overrides.map(|o| PlatformOverridesJson {
                macos: o.macos,
                windows: o.windows,
                linux: o.linux,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deserialize an IpcMessage from JSON, defaulting version to 1 for
/// backward compatibility with older clients that don't send the version field.
pub fn deserialize_message(json: &str) -> Result<IpcMessage, serde_json::Error> {
    serde_json::from_str(json)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constants() {
        assert_eq!(PROTOCOL_VERSION, 2);
        assert_eq!(PROTOCOL_MIN_VERSION, 1);
    }

    #[test]
    fn test_version_compatibility() {
        let msg = IpcMessage::request(IpcRequest::GetStatus { request_id: None });
        assert!(msg.is_compatible());
        assert_eq!(msg.version(), 2);
    }

    #[test]
    fn test_old_format_rejected() {
        // Old-format messages (without version) cannot be deserialized
        // because serde's flatten + untagged enum combination doesn't support
        // deserializing old-format messages. Clients must upgrade.
        let old_json = r#"{"Request":"GetStatus"}"#;
        let result: Result<IpcMessage, _> = deserialize_message(old_json);
        assert!(result.is_err(), "old-format messages should be rejected");
    }

    #[test]
    fn test_new_message_has_version() {
        let msg = IpcMessage::request(IpcRequest::GetStatus { request_id: None });
        let json = serde_json::to_string(&msg).expect("should serialize");
        assert!(json.contains("\"version\":2"));
    }

    #[test]
    fn test_auth_challenge_roundtrip() {
        let msg = IpcMessage::response(IpcResponse::AuthChallenge {
            challenge: AuthChallenge {
                nonce: "test-nonce".to_string(),
                daemon_pid: 1234,
                timestamp: 1000,
            },
            request_id: None,
        });
        assert_eq!(msg.version(), 2);
        // Serialize and deserialize to verify roundtrip
        let json = serde_json::to_string(&msg).expect("should serialize");
        let msg2 = deserialize_message(&json).expect("should deserialize");
        assert_eq!(msg2.version(), 2);
    }

    #[test]
    fn test_auth_result_roundtrip() {
        let msg = IpcMessage::response(IpcResponse::AuthResult {
            accepted: true,
            reason: None,
            request_id: None,
        });
        let json = serde_json::to_string(&msg).expect("should serialize");
        let msg2 = deserialize_message(&json).expect("should deserialize");
        assert_eq!(msg2.version(), 2);
    }

    #[test]
    fn test_error_response_roundtrip() {
        let msg = IpcMessage::response(IpcResponse::Error {
            code: "TEST_ERROR".to_string(),
            details: "test details".to_string(),
            request_id: None,
        });
        let json = serde_json::to_string(&msg).expect("should serialize");
        let msg2 = deserialize_message(&json).expect("should deserialize");
        assert_eq!(msg2.version(), 2);
    }

    #[test]
    fn test_request_id_extraction() {
        let req = IpcRequest::GetStatus {
            request_id: Some("req-123".to_string()),
        };
        assert_eq!(req.request_id(), Some("req-123"));

        let req2 = IpcRequest::GetStatus { request_id: None };
        assert_eq!(req2.request_id(), None);
    }

    #[test]
    fn test_add_keybinding_request() {
        let req = IpcRequest::AddKeybinding {
            id: "test-binding".to_string(),
            keys: vec!["Ctrl+Shift+T".to_string()],
            action: serde_json::json!({"Paste": {"text": "hello"}}),
            request_id: Some("add-1".to_string()),
        };
        assert_eq!(req.request_id(), Some("add-1"));
    }

    #[test]
    fn test_remove_keybinding_request() {
        let req = IpcRequest::RemoveKeybinding {
            id: "test-binding".to_string(),
            request_id: None,
        };
        assert_eq!(req.request_id(), None);
    }

    #[test]
    fn test_key_binding_json_conversion() {
        let binding = crate::config::KeyBinding {
            id: "kb1".to_string(),
            keys: vec!["Ctrl+K".to_string()],
            action: crate::actions::Action::Paste { text: "hi".to_string() },
            overrides: None,
        };
        let json = crate::ipc::KeyBindingJson::from(binding);
        assert_eq!(json.id, "kb1");
        assert_eq!(json.keys, vec!["Ctrl+K"]);
        assert_eq!(json.action_type, "paste");
    }
}
