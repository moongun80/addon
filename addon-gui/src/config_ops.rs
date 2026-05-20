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

    // 3. FIX-023: Default to ~/.addon/config.yaml, create directory if needed
    if let Some(home) = dirs::home_dir() {
        let home_config_dir = home.join(".addon");
        let home_config = home_config_dir.join("config.yaml");
        if !home_config_dir.exists() {
            let _ = std::fs::create_dir_all(&home_config_dir);
        }
        return home_config.to_string_lossy().into_owned();
    }

    // 4. Last resort fallback (shouldn't happen on any sane system)
    String::from("./config.yaml")
}

pub fn load_config(path: &str) -> Result<String, anyhow::Error> {
    std::fs::read_to_string(path).map_err(|e| anyhow::anyhow!("Config not found: {}", e))
}

pub fn save_config(path: &str, content: &str) -> Result<(), anyhow::Error> {
    let path_buf = std::path::Path::new(path);

    // Ensure parent directory exists
    if let Some(parent) = path_buf.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // FIX-022: Atomic save — write to temp file, then rename
    let tmp_path = path_buf.with_extension("yaml.tmp");
    std::fs::write(&tmp_path, content)?;
    std::fs::rename(&tmp_path, path_buf)?;

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
