use std::path::{Path, PathBuf};

// Matches the Tauri bundle identifier the app shipped with, so aliases and
// settings written by earlier versions are picked up unchanged.
const APP_IDENTIFIER: &str = "com.timkunze.eve-wrench";

#[derive(Debug, Clone)]
pub struct Locations {
    data_dir: PathBuf,
    custom_eve_path: Option<PathBuf>,
}

impl Locations {
    pub fn new(data_dir: PathBuf, custom_eve_path: Option<PathBuf>) -> Self {
        Self {
            data_dir,
            custom_eve_path,
        }
    }

    pub fn default_data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join(APP_IDENTIFIER)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn aliases_file(&self) -> PathBuf {
        self.data_dir.join("aliases.json")
    }

    pub fn config_file(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    pub fn eve_root(&self) -> Option<PathBuf> {
        if let Some(path) = &self.custom_eve_path
            && path.is_dir()
        {
            return Some(path.clone());
        }
        default_eve_root()
    }
}

fn default_eve_root() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Application Support/CCP/EVE"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().map(|d| d.join("CCP/EVE"))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::home_dir().map(|h| {
            h.join(".local/share/Steam/steamapps/compatdata/8500/pfx/drive_c/users/steamuser/AppData/Local/CCP/EVE")
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        None
    }
}
