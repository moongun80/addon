//! YAML configuration data model.
//!
//! Provides [`Config`] and related types for representing the addon's
//! configuration file, which maps key bindings to actions per platform.

use serde::{Deserialize, Serialize};

/// Top-level configuration.
///
/// The config file has three logical sections:
/// 1. `version` — file format version
/// 2. `global` — global settings like modifier remapping
/// 3. `keybindings` — the list of individual bindings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Config file version (e.g. `"1.0"`).
    pub version: String,
    /// Global settings that apply to all key bindings.
    pub global: GlobalSettings,
    /// List of key bindings to install.
    pub keybindings: Vec<KeyBinding>,
}

/// Global settings shared across all bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    /// Modifier key remapping configuration.
    pub modifier_map: ModifierMap,
}

/// Configures how the command key is remapped.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CommandFallback {
    /// Use Alt + Ctrl as a command-key fallback (Windows / Linux).
    #[serde(rename = "alt_ctrl")]
    AltCtrl,
    /// Use Alt alone as a command-key fallback.
    #[serde(rename = "alt")]
    Alt,
}

/// Maps command key behavior to platform-specific alternatives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModifierMap {
    /// How the command key should fall back on Windows / Linux.
    pub command: CommandFallback,
}

impl Default for ModifierMap {
    fn default() -> Self {
        Self {
            command: CommandFallback::AltCtrl,
        }
    }
}

impl Default for GlobalSettings {
    fn default() -> Self {
        Self {
            modifier_map: ModifierMap::default(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            global: GlobalSettings::default(),
            keybindings: Vec::new(),
        }
    }
}

/// A single key binding entry.
///
/// Each binding has a unique ID, one or more key strokes that trigger it,
/// and an action to perform. Platform-specific overrides can be provided.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    /// Unique identifier for this binding.
    pub id: String,
    /// Key stroke representations (e.g. `["Ctrl+V"]`).
    pub keys: Vec<String>,
    /// The action to perform when triggered.
    pub action: crate::actions::Action,
    /// Optional per-platform key override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<PlatformOverrides>,
}

/// Per-platform key binding overrides.
///
/// If present, the platform-specific list replaces the default keys for that OS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformOverrides {
    /// macOS-specific key strokes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos: Option<Vec<String>>,
    /// Windows-specific key strokes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<String>>,
}

impl KeyBinding {
    /// Returns the effective key strokes for the given platform.
    pub fn effective_keys(&self, platform: &str) -> &[String] {
        if let Some(ref overrides) = self.overrides {
            match platform {
                "macos" => overrides.macos.as_deref().unwrap_or(&self.keys),
                "windows" => overrides.windows.as_deref().unwrap_or(&self.keys),
                _ => &self.keys,
            }
        } else {
            &self.keys
        }
    }
}

use std::path::Path;

/// Loads configuration from a YAML file at the given path.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the file cannot be read or parsed.
pub fn load(path: &Path) -> crate::error::Result<Config> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        crate::error::Error::Parse(format!("failed to read config file {:?}: {}", path, e))
    })?;

    let config: Config = serde_yaml::from_str(&contents).map_err(|e| {
        crate::error::Error::Parse(format!("failed to parse config file {:?}: {}", path, e))
    })?;

    Ok(config)
}
