use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcMessage {
    GetStatus,
    SetConfig {
        config: serde_json::Value,
    },
    ReloadConfig,
    TestShortcut {
        keys: Vec<String>,
        action: serde_json::Value,
    },
    DaemonStatus {
        running: bool,
        pid: u32,
    },
    ConfigLoaded {
        keys: Vec<KeyBindingJson>,
    },
    TestResult {
        success: bool,
        message: String,
    },
    Error {
        code: String,
        details: String,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KeyBindingJson {
    pub id: String,
    pub keys: Vec<String>,
    pub action_type: String,
}

const SOCKET_PATH: &str = "/tmp/addon.sock";

fn send_sync(msg: &IpcMessage) -> Result<IpcMessage, anyhow::Error> {
    let mut stream = TcpStream::connect(SOCKET_PATH)?;
    let json = serde_json::to_string(msg)?;
    stream.write_all(json.as_bytes())?;
    stream.flush()?;

    let mut response = String::new();
    let mut buf = [0u8; 4096];
    let mut total = 0;
    loop {
        let n = stream.read(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
        if total >= buf.len() {
            break;
        }
    }
    let response = String::from_utf8_lossy(&buf[..total]);
    let result: IpcMessage = serde_json::from_str(&response)?;
    Ok(result)
}

#[tauri::command]
fn get_daemon_status() -> Result<serde_json::Value, String> {
    match send_sync(&IpcMessage::GetStatus) {
        Ok(msg) => Ok(serde_json::to_value(&msg).unwrap()),
        Err(e) => Ok(serde_json::json!({"type":"error","code":"io","details":e.to_string()})),
    }
}

#[tauri::command]
fn list_keybindings() -> Result<Vec<KeyBindingJson>, String> {
    let path = config_ops::get_config_path();
    let config = addon_core::config::load(&path).map_err(|e| e.to_string())?;
    Ok(config
        .keybindings
        .into_iter()
        .map(|b| KeyBindingJson {
            id: b.id.clone(),
            keys: b.keys.clone(),
            action_type: format!("{:?}", b.action),
        })
        .collect())
}

#[tauri::command]
fn reload_config() -> Result<serde_json::Value, String> {
    let path = config_ops::get_config_path();
    let config = addon_core::config::load(&path).map_err(|e| e.to_string())?;
    match send_sync(&IpcMessage::SetConfig {
        config: serde_json::to_value(&config).map_err(|e| e.to_string())?,
    }) {
        Ok(msg) => Ok(serde_json::to_value(&msg).unwrap()),
        Err(e) => Ok(serde_json::json!({"type":"error","code":"ipc","details":e.to_string()})),
    }
}

#[tauri::command]
fn test_shortcut(
    keys: Vec<String>,
    action: serde_json::Value,
) -> Result<serde_json::Value, String> {
    match send_sync(&IpcMessage::TestShortcut { keys, action }) {
        Ok(msg) => Ok(serde_json::to_value(&msg).unwrap()),
        Err(e) => Ok(serde_json::json!({"type":"error","code":"ipc","details":e.to_string()})),
    }
}

#[tauri::command]
fn add_keybinding(
    id: String,
    keys: String,
    action_type: String,
) -> Result<serde_json::Value, String> {
    let path = config_ops::get_config_path();
    let content = config_ops::load_config(&path).map_err(|e| e.to_string())?;
    let mut config: addon_core::config::Config =
        serde_yaml::from_str(&content).map_err(|e| e.to_string())?;

    let keys_vec: Vec<String> = keys.split(',').map(|s| s.trim().to_string()).collect();
    let action = match action_type.as_str() {
        "paste" => addon_core::actions::Action::Paste {
            text: "test".into(),
        },
        "launch" => addon_core::actions::Action::Launch {
            path: "/tmp".into(),
        },
        "remap" => addon_core::actions::Action::Remap {
            to: "Escape".into(),
        },
        "shortcut" => addon_core::actions::Action::Shortcut {
            shortcut: vec!["Ctrl".into(), "C".into()],
        },
        "system" => addon_core::actions::Action::SystemCommand {
            command: "volume_down".into(),
        },
        _ => addon_core::actions::Action::Paste {
            text: action_type.into(),
        },
    };

    config.keybindings.push(addon_core::config::KeyBinding {
        id: id.clone(),
        keys: keys_vec,
        action,
        overrides: None,
    });

    let yaml = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;
    config_ops::save_config(&path, &yaml).map_err(|e| e.to_string())?;

    // Reload daemon
    match send_sync(&IpcMessage::ReloadConfig) {
        Ok(_) => {
            Ok(serde_json::json!({"type":"success","message":format!("Added {} and reloaded", id)}))
        }
        Err(e) => Ok(
            serde_json::json!({"type":"partial_success","message":format!("Added {} but daemon reload failed: {}", id, e)}),
        ),
    }
}

#[tauri::command]
fn remove_keybinding(id: String) -> Result<serde_json::Value, String> {
    let path = config_ops::get_config_path();
    let content = config_ops::load_config(&path).map_err(|e| e.to_string())?;
    let mut config: addon_core::config::Config =
        serde_yaml::from_str(&content).map_err(|e| e.to_string())?;

    let before = config.keybindings.len();
    config.keybindings.retain(|b| b.id != id);

    if config.keybindings.len() == before {
        return Ok(
            serde_json::json!({"type":"error","code":"not_found","details":format!("Binding '{}' not found", id)}),
        );
    }

    let yaml = serde_yaml::to_string(&config).map_err(|e| e.to_string())?;
    config_ops::save_config(&path, &yaml).map_err(|e| e.to_string())?;

    Ok(serde_json::json!({"type":"success","message":format!("Removed {}", id)}))
}

#[tauri::command]
fn export_config(format: String) -> Result<String, String> {
    let path = config_ops::get_config_path();
    config_ops::export_config(&path, &format).map_err(|e| e.to_string())
}
