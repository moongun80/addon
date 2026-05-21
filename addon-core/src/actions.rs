//! Action type definitions.
//!
/// Defines the [`Action`] enum which represents the operation to perform
/// when a key binding is triggered.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// An action to perform when a key binding fires.
///
/// Each variant represents a different kind of operation:
/// - `paste`: Type text into the focused window
/// - `launch`: Open an application by path
/// - `remap`: Remap one key to another
/// - `shortcut`: Trigger a multi-key shortcut sequence
/// - `system_command`: Run a shell command
/// - `text_insert`: Insert text at the current cursor position
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Paste or type text.
    Paste {
        /// The text to paste.
        text: String,
    },
    /// Launch an application.
    Launch {
        /// Absolute or resolved path to the executable.
        path: String,
    },
    /// Remap one key to another (internal forwarding).
    Remap {
        /// The key to remap to, as a string.
        to: String,
    },
    /// Trigger a sequence of key strokes (shortcut).
    Shortcut {
        /// Ordered list of key stroke strings, e.g. `["Alt+F4"]`.
        shortcut: Vec<String>,
    },
    /// Execute a system/shell command.
    SystemCommand {
        /// The command string to execute.
        command: String,
    },
    /// Insert text at the current cursor position.
    TextInsert {
        /// The text to insert.
        text: String,
    },
}

impl Action {
    /// Return a short string identifying the action variant.
    pub fn variant_name(&self) -> String {
        match self {
            Self::Paste { .. } => "paste".to_string(),
            Self::Launch { .. } => "launch".to_string(),
            Self::Remap { .. } => "remap".to_string(),
            Self::Shortcut { .. } => "shortcut".to_string(),
            Self::SystemCommand { .. } => "system_command".to_string(),
            Self::TextInsert { .. } => "text_insert".to_string(),
        }
    }
}

/// Shell metacharacters that could lead to command injection.
/// These characters allow chaining or redirecting commands when passed
/// to a shell interpreter.
const SHELL_METACHARACTERS: &[char] = &[
    ';', '|', '&', '$', '`', '(', ')', '{', '}', '<', '>', '\\', '"', '\'',
];

/// Check whether a command string contains dangerous shell metacharacters.
///
/// Returns `true` if the command contains any character that could enable
/// command injection when executed through a shell.
pub fn has_shell_metacharacters(command: &str) -> bool {
    command.chars().any(|c| SHELL_METACHARACTERS.contains(&c))
}

/// Validate a system command string for safe execution.
///
/// Returns `Ok(())` if the command appears safe (no shell metacharacters),
/// or `Err(String)` describing the issue.
///
/// **Note:** This is a best-effort defense. For maximum safety, commands
/// should be executed via `std::process::Command` with explicit argument
/// splitting rather than through a shell.
pub fn validate_system_command(command: &str) -> Result<(), String> {
    if command.is_empty() {
        return Err("Empty command".to_string());
    }
    if has_shell_metacharacters(command) {
        let bad_chars: String = command
            .chars()
            .filter(|c| SHELL_METACHARACTERS.contains(c))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        return Err(format!(
            "Command contains potentially unsafe shell metacharacters: {}",
            bad_chars
        ));
    }
    Ok(())
}
