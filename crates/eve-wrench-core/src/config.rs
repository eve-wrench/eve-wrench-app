use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

// Persistent app settings. The file and key names match what the Tauri store
// plugin wrote, so existing installs keep their folder and backup choices.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_eve_path: Option<String>,
    #[serde(default = "default_true")]
    pub auto_backup: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemePreference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(flatten)]
    other: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThemePreference {
    Light,
    Dark,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            custom_eve_path: None,
            auto_backup: true,
            theme: None,
            locale: None,
            other: serde_json::Map::new(),
        }
    }
}

impl Config {
    // A missing or unreadable file falls back to defaults, which keep
    // auto-backup on: safer to over-back-up than to skip silently.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, content).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tauri_store_format_and_keeps_unknown_keys() {
        let config: Config =
            serde_json::from_str(r#"{"customEvePath":"/eve","autoBackup":false,"extra":1}"#)
                .unwrap();
        assert_eq!(config.custom_eve_path.as_deref(), Some("/eve"));
        assert!(!config.auto_backup);

        let written = serde_json::to_value(&config).unwrap();
        assert_eq!(written["extra"], 1);
    }

    #[test]
    fn auto_backup_defaults_on() {
        let config: Config = serde_json::from_str("{}").unwrap();
        assert!(config.auto_backup);
    }
}
