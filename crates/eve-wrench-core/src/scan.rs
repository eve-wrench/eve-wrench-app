use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::locations::Locations;
use crate::model::{
    AppData, BackupEntry, ProfileData, Server, ServerData, ServerInfo, SettingsEntry, SettingsKind,
};

// Scans every EVE installation plus its backups. Character names are not
// resolved here; that needs the network, see `esi::fetch_character_details`.
pub fn load_app_data(locations: &Locations) -> Result<AppData, String> {
    let root = locations
        .eve_root()
        .ok_or("EVE settings directory not found")?;
    let aliases = load_aliases(locations);

    let mut servers = scan_installations(&root)?;
    for entry in servers
        .iter_mut()
        .flat_map(|s| &mut s.profiles)
        .flat_map(|p| p.accounts.iter_mut().chain(&mut p.characters))
    {
        if let Some(alias) = aliases.get(&entry.id) {
            entry.alias = Some(alias.clone());
            entry.display_name = alias.clone();
        }
    }

    let mut data = AppData {
        servers,
        backups: scan_backups(&root),
    };
    data.resolve_backup_names();
    Ok(data)
}

fn settings_profile_dirs(server_path: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(server_path) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("settings_"))
        })
        .collect()
}

fn modified_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// Splits a "core_{kind}_{id}.dat" filename into (kind, id).
pub(crate) fn parse_core_filename(path: &Path) -> Result<(SettingsKind, &str), String> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid filename")?;
    let stem = filename
        .strip_prefix("core_")
        .and_then(|s| s.strip_suffix(".dat"))
        .ok_or("Invalid settings file")?;
    let (kind, id) = stem.split_once('_').ok_or("Invalid settings file format")?;
    let kind = SettingsKind::parse(kind).ok_or("Unknown settings type")?;
    Ok((kind, id))
}

fn parse_settings_file(path: &Path, server: Server, profile: &str) -> Option<SettingsEntry> {
    let (kind, id) = parse_core_filename(path).ok()?;
    if id.is_empty() || id.parse::<u64>().is_err() {
        return None;
    }
    Some(SettingsEntry {
        path: path.to_string_lossy().into_owned(),
        id: id.to_string(),
        kind,
        server,
        profile: profile.to_string(),
        display_name: id.to_string(),
        character: None,
        alias: None,
        modified_time: modified_secs(path),
    })
}

fn scan_installations(root: &Path) -> Result<Vec<ServerData>, String> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut found: HashMap<Server, (PathBuf, Vec<ProfileData>)> = HashMap::new();
    for entry in fs::read_dir(root).map_err(|e| e.to_string())?.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(server) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(Server::from_folder_name)
        else {
            continue;
        };

        let mut profiles = Vec::new();
        for profile_path in settings_profile_dirs(&path) {
            let dir_name = profile_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let profile_name = dir_name.trim_start_matches("settings_");

            let mut accounts = Vec::new();
            let mut characters = Vec::new();
            if let Ok(files) = fs::read_dir(&profile_path) {
                for file in files.flatten() {
                    if let Some(settings) = parse_settings_file(&file.path(), server, profile_name)
                    {
                        match settings.kind {
                            SettingsKind::User => accounts.push(settings),
                            SettingsKind::Char => characters.push(settings),
                        }
                    }
                }
            }
            accounts.sort_by(|a, b| a.id.cmp(&b.id));
            characters.sort_by(|a, b| a.id.cmp(&b.id));

            profiles.push(ProfileData {
                name: profile_name.to_string(),
                path: profile_path.to_string_lossy().into_owned(),
                accounts,
                characters,
            });
        }
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        found.insert(server, (path, profiles));
    }

    Ok(Server::ALL
        .into_iter()
        .filter_map(|server| {
            let (path, profiles) = found.remove(&server)?;
            if profiles.is_empty() {
                return None;
            }
            Some(ServerData {
                info: ServerInfo {
                    id: server,
                    brackets_always_show: read_brackets_setting(&path),
                    server_path: path.to_string_lossy().into_owned(),
                },
                profiles,
            })
        })
        .collect())
}

fn scan_backups(root: &Path) -> Vec<BackupEntry> {
    let Ok(server_dirs) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut backups = Vec::new();
    for server_entry in server_dirs.flatten() {
        let server_path = server_entry.path();
        if !server_path.is_dir() {
            continue;
        }
        for profile_path in settings_profile_dirs(&server_path) {
            let Ok(entries) = fs::read_dir(profile_path.join("backups")) else {
                continue;
            };
            for entry in entries.flatten() {
                if let Some(backup) = parse_backup_file(&entry.path()) {
                    backups.push(backup);
                }
            }
        }
    }

    backups.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
    backups
}

// Backups are named "{name}_{kind}_{id}_{timestamp}.bak"; the name itself may
// contain underscores, so the fixed fields are split off from the right.
pub(crate) fn parse_backup_file(path: &Path) -> Option<BackupEntry> {
    let filename = path.file_name()?.to_str()?;
    let stem = filename.strip_suffix(".bak")?;
    let parts: Vec<&str> = stem.rsplitn(4, '_').collect();
    if parts.len() < 4 {
        return None;
    }

    let timestamp = parts[0].parse::<u64>().unwrap_or(0);
    let kind = SettingsKind::parse(parts[2])?;
    let name = parts[3].to_string();
    Some(BackupEntry {
        id: format!("{}_{}", name, timestamp),
        name,
        path: path.to_string_lossy().into_owned(),
        timestamp,
        kind,
        original_id: parts[1].to_string(),
        original_name: None,
    })
}

fn load_aliases(locations: &Locations) -> HashMap<String, String> {
    fs::read_to_string(locations.aliases_file())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn set_alias(locations: &Locations, id: &str, alias: Option<&str>) -> Result<(), String> {
    let mut aliases = load_aliases(locations);
    match alias.map(str::trim) {
        Some(a) if !a.is_empty() => {
            aliases.insert(id.to_string(), a.to_string());
        }
        _ => {
            aliases.remove(id);
        }
    }

    fs::create_dir_all(locations.data_dir()).map_err(|e| e.to_string())?;
    let content = serde_json::to_string_pretty(&aliases).map_err(|e| e.to_string())?;
    fs::write(locations.aliases_file(), content).map_err(|e| e.to_string())
}

// Current display name (alias if set, otherwise the raw id) for a settings file.
pub fn entry_display_name(locations: &Locations, file_path: &Path) -> Result<String, String> {
    let (_, id) = parse_core_filename(file_path)?;
    Ok(load_aliases(locations)
        .remove(id)
        .unwrap_or_else(|| id.to_string()))
}

const BRACKETS_KEY: &str = "bracketsAlwaysShowShipText=";

fn read_brackets_setting(server_path: &Path) -> bool {
    settings_profile_dirs(server_path).iter().any(|profile| {
        fs::read_to_string(profile.join("prefs.ini")).is_ok_and(|content| {
            content.lines().any(|line| {
                line.trim()
                    .strip_prefix(BRACKETS_KEY)
                    .is_some_and(|v| v.trim() == "1")
            })
        })
    })
}

// Applies to every profile folder of the server, since EVE reads prefs.ini
// from whichever profile the client launches with.
pub fn set_brackets_always_show(server_path: &Path, enabled: bool) -> Result<(), String> {
    let setting_line = format!("{}{}", BRACKETS_KEY, if enabled { "1" } else { "0" });

    for profile in settings_profile_dirs(server_path) {
        let prefs_path = profile.join("prefs.ini");
        let content = if prefs_path.exists() {
            let existing = fs::read_to_string(&prefs_path).map_err(|e| e.to_string())?;
            let mut found = false;
            let mut lines: Vec<String> = existing
                .lines()
                .map(|line| {
                    if line.trim().starts_with(BRACKETS_KEY) {
                        found = true;
                        setting_line.clone()
                    } else {
                        line.to_string()
                    }
                })
                .collect();
            if !found {
                lines.push(setting_line.clone());
            }
            lines.join("\n")
        } else {
            setting_line.clone()
        };
        fs::write(&prefs_path, content).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_names_may_contain_underscores() {
        let backup =
            parse_backup_file(Path::new("/x/pre_formation_edit_user_12345_1700000000.bak"))
                .unwrap();
        assert_eq!(backup.name, "pre_formation_edit");
        assert_eq!(backup.kind, SettingsKind::User);
        assert_eq!(backup.original_id, "12345");
        assert_eq!(backup.timestamp, 1_700_000_000);
    }

    #[test]
    fn core_filenames_parse_kind_and_id() {
        let (kind, id) = parse_core_filename(Path::new("/x/core_char_2112345678.dat")).unwrap();
        assert_eq!(kind, SettingsKind::Char);
        assert_eq!(id, "2112345678");
        assert!(parse_core_filename(Path::new("/x/prefs.ini")).is_err());
    }
}
