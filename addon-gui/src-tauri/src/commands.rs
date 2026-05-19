//! Tauri command handlers for daemon IPC operations.
//!
//! Each function sends an IPC message to the daemon and returns the response.

use addon_core::config;
use addon_core::ipc::{IpcMessage, IpcRequest};
use std::path::PathBuf;

/// Returns the path to the configuration file.
fn get_config_path() -> PathBuf {
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

/// Get the current daemon status.
#[tauri::command]
pub async fn get_daemon_status() -> Result<IpcMessage, String> {
    let client = IpcClient::new().map_err(|e| e.to_string())?;
    let resp = client
        .send(IpcMessage::request(IpcRequest::GetStatus))
        .await?;
    Ok(resp)
}

/// Reload configuration from disk and push to daemon.
#[tauri::command]
pub async fn reload_config() -> Result<IpcMessage, String> {
    let client = IpcClient::new().map_err(|e| e.to_string())?;
    let resp = client
        .send(IpcMessage::request(IpcRequest::ReloadConfig))
        .await?;
    Ok(resp)
}

/// Test a shortcut by sending key strokes and action to the daemon.
#[tauri::command]
pub async fn test_shortcut(
    keys: Vec<String>,
    action: serde_json::Value,
) -> Result<IpcMessage, String> {
    let client = IpcClient::new().map_err(|e| e.to_string())?;
    let resp = client
        .send(IpcMessage::request(IpcRequest::TestShortcut { keys, action }))
        .await?;
    Ok(resp)
}

/// List all installed key bindings from the current config.
#[tauri::command]
pub fn list_installed_keybindings() -> Result<Vec<config::KeyBinding>, String> {
    let path = get_config_path();
    let cfg = config::load(&path).map_err(|e| e.to_string())?;
    Ok(cfg.keybindings)
}
