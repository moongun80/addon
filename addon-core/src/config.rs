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
    #[must_use]
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

    let config: Config = serde_yaml::from_str(&contents).map_err(|e| {
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

/// Saves the configuration to a YAML file at the given path.
///
/// # Errors
///
/// Returns [`Error::Parse`] if the file cannot be written.
pub fn save_to_disk(path: &Path, config: &Config) -> crate::error::Result<()> {
    let contents = serde_yaml::to_string(config).map_err(|e| {
        crate::error::Error::Parse(format!("failed to serialize config: {}", e))
    })?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            crate::error::Error::Parse(format!("failed to create config directory: {}", e))
        })?;
    }

    std::fs::write(path, contents).map_err(|e| {
        crate::error::Error::Parse(format!("failed to write config file {:?}: {}", path, e))
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::Action;

    fn sample_config() -> Config {
        Config {
            version: "1.0".to_string(),
            global: crate::config::GlobalSettings::default(),
            keybindings: vec![
                KeyBinding {
                    id: "test1".to_string(),
                    keys: vec!["Ctrl+A".to_string()],
                    action: Action::Paste { text: "hello".to_string() },
                    overrides: None,
                },
            ],
        }
    }

    #[test]
    fn test_validate_no_errors() {
        let config = sample_config();
        assert!(config.validate().is_empty());
    }

    #[test]
    fn test_validate_duplicate_ids() {
        let mut config = sample_config();
        config.keybindings.push(KeyBinding {
            id: "test1".to_string(),
            keys: vec!["Ctrl+B".to_string()],
            action: Action::Paste { text: "world".to_string() },
            overrides: None,
        });
        let errors = config.validate();
        assert!(errors.iter().any(|e| e.contains("duplicate")));
    }

    #[test]
    fn test_validate_empty_keys() {
        let mut config = sample_config();
        config.keybindings.push(KeyBinding {
            id: "empty".to_string(),
            keys: vec![],
            action: Action::Paste { text: "".to_string() },
            overrides: None,
        });
        let errors = config.validate();
        assert!(errors.iter().any(|e| e.contains("no keys")));
    }

    #[test]
    fn test_validate_invalid_command() {
        let mut config = sample_config();
        config.keybindings.push(KeyBinding {
            id: "bad_cmd".to_string(),
            keys: vec!["Ctrl+X".to_string()],
            action: Action::SystemCommand { command: "rm -rf /; echo hack".to_string() },
            overrides: None,
        });
        let errors = config.validate();
        assert!(errors.iter().any(|e| e.contains("shell metacharacter")));
    }

    #[test]
    fn test_build_keymapper() {
        let config = sample_config();
        let mapper = config.build_keymapper(OsPlatform::Linux);
        let stroke = crate::keymap::KeyStroke::parse("Ctrl+A").unwrap();
        assert!(mapper.lookup(&stroke).is_some());
    }

    #[test]
    fn test_effective_keys_with_overrides() {
        let mut config = sample_config();
        config.keybindings[0].overrides = Some(crate::config::PlatformOverrides {
            macos: Some(vec!["Cmd+A".to_string()]),
            windows: None,
            linux: None,
        });

        let binding = &config.keybindings[0];
        assert_eq!(binding.effective_keys(OsPlatform::Macos), &["Cmd+A"]);
        assert_eq!(binding.effective_keys(OsPlatform::Linux), &["Ctrl+A"]);
    }
}
