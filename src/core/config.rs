use super::models::Settings;
use anyhow::{Context, Result};
use std::{fs, path::PathBuf};

pub fn app_root() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

pub fn config_dir() -> PathBuf {
    app_root().join("config")
}

pub fn logs_dir() -> PathBuf {
    app_root().join("logs")
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load_settings() -> Result<Settings> {
    let path = settings_path();
    if !path.exists() {
        return Ok(Settings::default());
    }

    let contents =
        fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    let settings = serde_json::from_str(&contents)
        .with_context(|| format!("Failed to parse {}", path.display()))?;
    Ok(settings)
}

pub fn save_settings(settings: &Settings) -> Result<()> {
    let directory = config_dir();
    fs::create_dir_all(&directory)
        .with_context(|| format!("Failed to create {}", directory.display()))?;

    let path = settings_path();
    let contents =
        serde_json::to_string_pretty(settings).context("Failed to serialize settings")?;
    fs::write(&path, contents).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}
