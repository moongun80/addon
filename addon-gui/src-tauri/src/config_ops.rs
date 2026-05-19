//! Configuration file CRUD operations for the Tauri GUI.
//!
//! Provides functions to read, validate, and write the addon's
//! YAML configuration file.

use addon_core::config;
use std::path::PathBuf;

/// Returns the path to the configuration file.
pub fn get_config_path() -> PathBuf {
    if let Ok(path) = std::env::var("ADDON_CONFIG") {
        return PathBuf::from(path);
    }
    if let Some(home) = dirs::home_dir() {
        let p = home.join(".addon").join("config.yaml");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("config.yaml")
}

/// Load the current configuration from disk.
pub fn load_config() -> Result<config::Config, String> {
    let path = get_config_path();
    config::load(&path).map_err(|e| e.to_string())
}

/// Validate a configuration (conflict detection, schema checks).
pub fn validate_config(cfg: &config::Config) -> Vec<String> {
    let mut errors = Vec::new();

    // Check for version.
    if cfg.version.is_empty() {
        errors.push("Config version is empty".to_string());
    }

    // Check for conflicts.
    let conflicts = addon_core::conflict::detect_conflicts(&cfg.keybindings);
    for c in &conflicts {
        errors.push(format!(
            "Conflict: {} ↔ {} [platform: {:?}]",
            c.binding1, c.binding2, c.platform
        ));
    }

    // Validate key strokes.
    for binding in &cfg.keybindings {
        for key_str in &binding.keys {
            if addon_core::keymap::KeyStroke::parse(key_str).is_err() {
                errors.push(format!(
                    "Invalid key stroke '{}' in binding '{}'",
                    key_str, binding.id
                ));
            }
        }
    }

    errors
}

/// Write configuration to disk.
pub fn save_config(cfg: &config::Config) -> Result<(), String> {
    let path = get_config_path();
    let parent = path.parent().ok_or("Config has no parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;

    let yaml = serde_yaml::to_string(cfg).map_err(|e| e.to_string())?;
    std::fs::write(&path, yaml).map_err(|e| format!("Failed to write {:?}: {}", path, e))
}
