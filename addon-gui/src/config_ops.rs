pub fn get_config_path() -> String {
    // 1. Check ADDON_CONFIG env var first
    if let Ok(path) = std::env::var("ADDON_CONFIG") {
        return path;
    }

    // 2. Try XDG config directory (~/.config/addon/config.yaml)
    if let Some(config_dir) = dirs::config_dir() {
        let xdg = config_dir.join("addon").join("config.yaml");
        if xdg.exists() {
            return xdg.to_string_lossy().into_owned();
        }
    }

    // 3. Fallback to home directory (~/.addon/config.yaml)
    if let Some(home) = dirs::home_dir() {
        let home_config = home.join(".addon").join("config.yaml");
        if home_config.exists() {
            return home_config.to_string_lossy().into_owned();
        }
    }

    // 4. Final fallback: current directory
    String::from("./config.yaml")
}

pub fn load_config(path: &str) -> Result<String, anyhow::Error> {
    std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("Config not found: {}", e))
}

pub fn save_config(path: &str, content: &str) -> Result<(), anyhow::Error> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

pub fn export_config(path: &str, format: &str) -> Result<String, anyhow::Error> {
    let content = std::fs::read_to_string(path)?;
    match format {
        "yaml" => Ok(content),
        "json" => {
            let value: serde_json::Value = serde_yaml::from_str(&content)?;
            Ok(serde_json::to_string_pretty(&value)?)
        }
        _ => Err(anyhow::anyhow!("Unsupported format: {}", format)),
    }
}
