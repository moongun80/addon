//! Action type definitions.
//!
/// Defines the [`Action`] enum which represents the operation to perform
/// when a key binding is triggered.

use serde::{Deserialize, Serialize};

/// An action to perform when a key binding fires.
///
/// Each variant represents a different kind of operation:
/// - `paste`: Type text into the focused window
/// - `launch`: Open an application by path
/// - `remap`: Remap one key to another
/// - `shortcut`: Trigger a multi-key shortcut sequence
/// - `system_command`: Run a shell command
/// - `text_insert`: Insert text at the current cursor position
#[derive(Debug, Clone, Serialize, Deserialize)]
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
