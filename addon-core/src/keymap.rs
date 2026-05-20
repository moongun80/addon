//! KeyStroke definition and parsing utilities.
//!
//! Provides [`KeyStroke`] — a canonical representation of a keyboard shortcut
//! including its modifiers and key code — and a [`parse`] function for converting
//! human-readable strings (e.g. `"Ctrl+Shift+V"`) into typed key strokes.

use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Represents a keyboard modifier key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Modifier {
    /// Control key (Ctrl on Windows/Linux, Control on macOS)
    Control,
    /// Shift key (Shift on all platforms)
    Shift,
    /// Alt key (Windows/Linux terminology)
    Alt,
    /// Option key (macOS terminology; equivalent to Alt)
    Option,
    /// Command key on macOS / Super/Win key on Windows
    Command,
    /// CapsLock key
    CapsLock,
}

/// A single keyboard key (letter, number, function key, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    /// The literal key character or name: `"A"`, `"V"`, `"Space"`, `"F1"`, etc.
    pub code: String,
}

/// A key stroke — a combination of modifiers and a key code.
///
/// This is the canonical type used to look up actions in a binding table.
///
/// # Examples
///
/// ```
/// use addon_core::keymap::{Modifier, Key, KeyStroke};
///
/// let stroke = KeyStroke {
///     modifiers: vec![Modifier::Control, Modifier::Shift],
///     key: Key { code: "V".to_string() },
/// };
/// assert_eq!(stroke.display(), "Ctrl+Shift+V");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyStroke {
    /// Ordered list of modifiers.
    pub modifiers: Vec<Modifier>,
    /// The key that is pressed together with the modifiers.
    pub key: Key,
}

/// Normalize a key code string for consistent comparison.
///
/// FIX-300: All key codes are uppercased for consistency.
/// - Single-character keys: `"a"` → `"A"`
/// - Multi-character named keys: `"enter"` → `"ENTER"`, `"F1"` → `"F1"`
pub fn normalize_key_code(key: &str) -> String {
    key.to_uppercase()
}

impl KeyStroke {
    /// Parses a key stroke from a human-readable string.
    ///
    /// Supported formats:
    /// - `"V"` — just a key
    /// - `"Ctrl+V"` — one modifier
    /// - `"Ctrl+Shift+V"` — multiple modifiers
    /// - `"Cmd+Shift+V"` — macOS modifier alias for Command
    /// - `"Alt+V"` — Alt modifier
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidKey`] if the key code is empty or the format
    /// is not recognized.
    pub fn parse(s: &str) -> Result<Self, Error> {
        let parts: Vec<&str> = s.trim().split('+').collect();

        if parts.is_empty() {
            return Err(Error::InvalidKey(format!("empty key stroke: {s:?}")));
        }

        let mut modifiers = Vec::new();
        let key_part = parts[parts.len() - 1];

        for &part in &parts[..parts.len() - 1] {
            let modifier = match part.trim() {
                "Ctrl" | "Control" => Modifier::Control,
                "Shift" => Modifier::Shift,
                "Alt" => Modifier::Alt,
                "Option" => Modifier::Option,
                "Cmd" | "Command" | "Super" => Modifier::Command,
                "CapsLock" | "Caps" => Modifier::CapsLock,
                other => {
                    return Err(Error::InvalidKey(format!("unknown modifier: {other:?}")));
                }
            };
            modifiers.push(modifier);
        }

        // Sort modifiers by discriminant value so the order is always
        // deterministic regardless of input order. This ensures that
        // "Ctrl+Shift+V" and "Shift+Ctrl+V" produce identical KeyStroke
        // values, so Hash/Eq work correctly.
        modifiers.sort_by_key(|m| match m {
            Modifier::Control => 0,
            Modifier::Shift => 1,
            Modifier::Alt => 2,
            Modifier::Option => 3,
            Modifier::Command => 4,
            Modifier::CapsLock => 5,
        });
        // Deduplicate since same modifiers will be adjacent after sort.
        modifiers.dedup();

        if key_part.is_empty() {
            return Err(Error::InvalidKey(format!("empty key in stroke: {s:?}")));
        }

        // Normalize the key code
        let code = normalize_key_code(key_part);

        Ok(Self {
            modifiers,
            key: Key { code },
        })
    }

    /// Returns a human-readable display string, e.g. `"Ctrl+Shift+V"`.
    ///
    /// Modifiers are ordered as Control → Shift → Alt/Option → Command → CapsLock.
    pub fn display(&self) -> String {
        let mut parts = Vec::new();

        // Group Alt and Option into a single display name
        let mut has_alt = false;
        let mut has_option = false;
        for m in &self.modifiers {
            match m {
                Modifier::Alt => has_alt = true,
                Modifier::Option => has_option = true,
                Modifier::Control => parts.push("Ctrl"),
                Modifier::Shift => parts.push("Shift"),
                Modifier::Command => parts.push("Cmd"),
                Modifier::CapsLock => parts.push("CapsLock"),
            }
        }

        // If both Alt and Option appear, prefer "Alt" for display
        if has_alt || has_option {
            parts.push("Alt");
        }

        parts.push(&self.key.code);
        parts.join("+")
    }
}

impl std::fmt::Display for KeyStroke {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}
