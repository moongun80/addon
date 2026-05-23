//! Action type definitions.
//!
/// Defines the [`Action`] enum which represents the operation to perform
/// when a key binding is triggered.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

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
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::Paste { .. } => "paste",
            Self::Launch { .. } => "launch",
            Self::Remap { .. } => "remap",
            Self::Shortcut { .. } => "shortcut",
            Self::SystemCommand { .. } => "system_command",
            Self::TextInsert { .. } => "text_insert",
        }
    }
}

/// Check whether a single character is safe for use in a system command.
///
/// Uses a strict allowlist: only ASCII alphanumeric characters and a minimal set
/// of commonly needed punctuation are permitted.
pub(crate) fn is_safe_cmd_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || [' ', '/', '-', '_', '.', ','].contains(&c)
}

/// Check whether a command string contains characters not on the safe allowlist.
///
/// Returns `true` if the command contains any character that is NOT in the
/// safe allowlist, indicating potential shell metacharacter injection.
pub(crate) fn has_shell_metacharacters(s: &str) -> bool {
    s.chars().any(|c| !is_safe_cmd_char(c))
}

/// Validate a system command string for safe execution.
///
/// Returns `Ok(())` if the command passes the strict allowlist check
/// (only alphanumeric characters and a minimal set of safe punctuation),
/// or `Err(String)` describing the disallowed characters found.
///
/// Commands containing shell metacharacters (e.g. `;`, `|`, `&`, `$`, `` ` ``,
/// `(`, `)`, `<`, `>`, `'`, `"`, `\`, `!`, `#`, `*`, `?`, `[`, `]`, `^`,
/// `~`, `%`, `{`, `}`, `|`, `=`, `:`, `@`, `+`) are rejected.
pub fn validate_system_command(command: &str) -> Result<(), String> {
    if command.is_empty() {
        return Err("Empty command".to_string());
    }
    if has_shell_metacharacters(command) {
        let bad_chars: String = command
            .chars()
            .filter(|c| !is_safe_cmd_char(*c))
            .collect::<BTreeSet<_>>()
            .iter()
            .collect();
        return Err(format!(
            "Command contains potentially unsafe shell metacharacters: {}",
            bad_chars
        ));
    }
    Ok(())
}
