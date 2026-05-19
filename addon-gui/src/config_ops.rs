pub fn get_config_path() -> String {
    let config_dir = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    config_dir
        .join("addon")
        .join("config.yaml")
        .to_string_lossy()
        .into_owned()
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
