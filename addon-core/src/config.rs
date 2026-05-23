//! YAML configuration data model.
//!
//! Provides [`Config`] and related types for representing the addon's
//! configuration file, which maps key bindings to actions per platform.

use crate::actions::{validate_system_command, Action};
use crate::keymap::KeyStroke;
use crate::mapper::KeyMapper;
use crate::os::OsPlatform;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level configuration.
///
/// The config file has three logical sections:
/// 1. `version` — file format version
/// 2. `global` — global settings like modifier remapping
/// 3. `keybindings` — the list of individual bindings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Config {
    /// Config file version (e.g. `"1.0"`).
    pub version: String,
    /// Global settings that apply to all key bindings.
    #[serde(default)]
    pub global: GlobalSettings,
    /// List of key bindings to install.
    pub keybindings: Vec<KeyBinding>,
}

impl Config {
    /// Validate this configuration for common errors.
    ///
    /// Returns a list of validation error messages. An empty list means the
    /// configuration is valid.
    ///
    /// Checks performed:
    /// - Duplicate binding IDs
    /// - Empty key lists
    /// - Unknown action types (checked via serde deserialization already,
    ///   so we focus on runtime structural issues)
    /// - Invalid system commands (shell metacharacter injection)
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for binding in &self.keybindings {
            // Check for duplicate IDs
            if !seen_ids.insert(&binding.id) {
                errors.push(format!("duplicate binding ID: {}", binding.id));
            }

            // Check for empty keys
            if binding.keys.is_empty() {
                errors.push(format!("binding '{}' has no keys defined", binding.id));
            }

            // Check for empty key strings within keys
            for key_str in &binding.keys {
                if key_str.trim().is_empty() {
                    errors.push(format!("binding '{}' has an empty key string", binding.id));
                }
            }

            // Check overrides for empty keys
            if let Some(ref overrides) = binding.overrides {
                // Check macOS override keys
                if let Some(ref macos_keys) = overrides.macos {
                    for key_str in macos_keys {
                        if key_str.trim().is_empty() {
                            errors.push(format!(
                                "binding '{}' has an empty macOS override key string",
                                binding.id
                            ));
                        }
                    }
                }
                // Check Windows override keys
                if let Some(ref windows_keys) = overrides.windows {
                    for key_str in windows_keys {
                        if key_str.trim().is_empty() {
                            errors.push(format!(
                                "binding '{}' has an empty Windows override key string",
                                binding.id
                            ));
                        }
                    }
                }
                // Check Linux override keys
                if let Some(ref linux_keys) = overrides.linux {
                    for key_str in linux_keys {
                        if key_str.trim().is_empty() {
                            errors.push(format!(
                                "binding '{}' has an empty Linux override key string",
                                binding.id
                            ));
                        }
                    }
                }
            }

            // Validate system commands for shell metacharacter injection
            if let Action::SystemCommand { command } = &binding.action {
                if let Err(e) = validate_system_command(command) {
                    errors.push(format!(
                        "Binding '{}' has invalid system command: {}",
                        binding.id, e
                    ));
                }
            }
        }

        errors
    }

    /// Builds a [`KeyMapper`] from this configuration for the given platform.
    ///
    /// The mapper includes default keys and per-platform overrides.
    pub fn build_keymapper(&self, platform: OsPlatform) -> Box<dyn KeyMapper> {
        let mut map: HashMap<KeyStroke, Action> = HashMap::new();

        for binding in &self.keybindings {
            let keys = binding.effective_keys(platform);
            for key_str in keys {
                match KeyStroke::parse(key_str) {
                    Ok(stroke) => {
                        map.insert(stroke, binding.action.clone());
                    }
                    Err(_) => {
                        tracing::warn!(
                            key = %key_str,
                            binding_id = %binding.id,
                            "Invalid key binding, skipping"
                        );
                    }
                }
            }
        }

        Box::new(ConfigKeyMapper { map })
    }
}

/// Global settings shared across all bindings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeyBinding {
    /// Unique identifier for this binding.
    pub id: String,
    /// Key stroke representations (e.g. `["Ctrl+V"]`).
    pub keys: Vec<String>,
    /// The action to perform when triggered.
    pub action: Action,
    /// Optional per-platform key override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overrides: Option<PlatformOverrides>,
}

impl KeyBinding {
    /// Returns the effective key strokes for the given platform.
    pub fn effective_keys(&self, platform: OsPlatform) -> &[String] {
        if let Some(ref overrides) = self.overrides {
            match platform {
                OsPlatform::Macos => overrides.macos.as_deref().unwrap_or(&self.keys),
                OsPlatform::Windows => overrides.windows.as_deref().unwrap_or(&self.keys),
                OsPlatform::Linux => overrides.linux.as_deref().unwrap_or(&self.keys),
            }
        } else {
            &self.keys
        }
    }
}

/// Per-platform key binding overrides.
///
/// If present, the platform-specific list replaces the default keys for that OS.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlatformOverrides {
    /// macOS-specific key strokes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos: Option<Vec<String>>,
    /// Windows-specific key strokes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows: Option<Vec<String>>,
    /// Linux-specific key strokes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linux: Option<Vec<String>>,
}

/// A concrete [`KeyMapper`] built from [`Config`].
pub struct ConfigKeyMapper {
    map: HashMap<KeyStroke, Action>,
}

impl KeyMapper for ConfigKeyMapper {
    fn lookup(&self, stroke: &KeyStroke) -> Option<&Action> {
        self.map.get(stroke)
    }
}

/// Loads configuration from a YAML file at the given path.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the file cannot be read or parsed.
pub fn load(path: &Path) -> crate::error::Result<Config> {
    let contents = std::fs::read_to_string(path).map_err(|e| {
        crate::error::Error::Parse(format!("failed to read config file {:?}: {}", path, e))
    })?;

    let mut config: Config = serde_yaml::from_str(&contents).map_err(|e| {
        crate::error::Error::Parse(format!("failed to parse config file {:?}: {}", path, e))
    })?;

    // Validate the loaded configuration
    let errors = config.validate();
    if !errors.is_empty() {
        return Err(crate::error::Error::Parse(format!(
            "config validation failed for {:?}: {}",
            path,
            errors.join("; ")
        )));
    }

    Ok(config)
}
