use crate::esi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

fn emit_data_changed(app: &tauri::AppHandle) {
    let _ = app.emit("data-changed", ());
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Server {
    Tranquility,
    Singularity,
    Thunderdome,
    Serenity,
}

impl Server {
    fn from_folder_name(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        if lower.contains("tranquility") {
            Some(Server::Tranquility)
        } else if lower.contains("singularity") {
            Some(Server::Singularity)
        } else if lower.contains("thunderdome") {
            Some(Server::Thunderdome)
        } else if lower.contains("serenity") {
            Some(Server::Serenity)
        } else {
            None
        }
    }

    fn supports_esi(&self) -> bool {
        matches!(self, Server::Tranquility)
    }

    fn display_name(&self) -> &'static str {
        match self {
            Server::Tranquility => "Tranquility",
            Server::Singularity => "Singularity",
            Server::Thunderdome => "Thunderdome",
            Server::Serenity => "Serenity",
        }
    }

    fn short_name(&self) -> &'static str {
        match self {
            Server::Tranquility => "TQ",
            Server::Singularity => "SISI",
            Server::Thunderdome => "TD",
            Server::Serenity => "CN",
        }
    }

    fn color(&self) -> &'static str {
        match self {
            Server::Tranquility => "#00d4aa",
            Server::Singularity => "#f0b429",
            Server::Thunderdome => "#f85149",
            Server::Serenity => "#a78bfa",
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SettingsKind {
    User,
    Char,
}

#[derive(Serialize, Debug, Clone)]
pub struct ServerInfo {
    pub id: Server,
    pub name: String,
    pub short_name: String,
    pub color: String,
    pub supports_esi: bool,
    pub brackets_always_show: bool,
    pub server_path: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct CharacterDetails {
    pub name: String,
    pub corporation: Option<String>,
    pub portrait_url: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct SettingsEntry {
    pub path: String,
    pub id: String,
    pub kind: SettingsKind,
    pub server: Server,
    pub profile: String,
    pub display_name: String,
    pub character: Option<CharacterDetails>,
    pub alias: Option<String>,
    pub modified_time: u64,
    pub relative_time: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct ProfileData {
    pub name: String,
    pub path: String,
    pub accounts: Vec<SettingsEntry>,
    pub characters: Vec<SettingsEntry>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ServerData {
    pub info: ServerInfo,
    pub profiles: Vec<ProfileData>,
}

#[derive(Serialize, Debug, Clone)]
pub struct BackupEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub timestamp: u64,
    pub kind: SettingsKind,
    pub original_id: String,
    pub original_name: Option<String>,
    pub display_name: String,
    pub relative_time: String,
}

#[derive(Serialize, Debug, Clone)]
pub struct AppData {
    pub servers: Vec<ServerData>,
    pub backups: Vec<BackupEntry>,
}

fn eve_settings_root() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| h.join("Library/Application Support/CCP/EVE"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().map(|d| d.join("CCP/EVE"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

fn backup_directory_for_path(source_path: &PathBuf) -> Result<PathBuf, String> {
    let profile_dir = source_path
        .parent() // settings_profile dir (e.g., settings_Default)
        .ok_or("Could not determine profile directory")?;

    let path = profile_dir.join("backups");
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

fn aliases_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let mut path = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    path.push("aliases.json");
    Ok(path)
}

fn load_aliases(app: &tauri::AppHandle) -> HashMap<String, String> {
    let path = match aliases_file(app) {
        Ok(p) => p,
        Err(_) => return HashMap::new(),
    };

    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_aliases(app: &tauri::AppHandle, aliases: &HashMap<String, String>) -> Result<(), String> {
    let path = aliases_file(app)?;
    let content = serde_json::to_string_pretty(aliases).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

fn read_brackets_setting(server_path: &PathBuf) -> bool {
    // Check all profile folders (settings_*) for the setting
    if let Ok(entries) = fs::read_dir(server_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if path.is_dir() && name.starts_with("settings_") {
                let prefs_path = path.join("prefs.ini");
                if let Ok(content) = fs::read_to_string(&prefs_path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("bracketsAlwaysShowShipText=") {
                            if let Some(value) = trimmed.strip_prefix("bracketsAlwaysShowShipText=")
                            {
                                if value.trim() == "1" {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn write_brackets_setting(server_path: &PathBuf, enabled: bool) -> Result<(), String> {
    let setting_line = format!(
        "bracketsAlwaysShowShipText={}",
        if enabled { "1" } else { "0" }
    );

    // Apply to all profile folders (settings_*)
    let entries = fs::read_dir(server_path).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() && name.starts_with("settings_") {
            let prefs_path = path.join("prefs.ini");

            let content = if prefs_path.exists() {
                let existing = fs::read_to_string(&prefs_path).map_err(|e| e.to_string())?;
                let mut found = false;
                let mut lines: Vec<String> = existing
                    .lines()
                    .map(|line| {
                        if line.trim().starts_with("bracketsAlwaysShowShipText=") {
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
    }
    Ok(())
}

fn format_relative_time(timestamp: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let diff = now.saturating_sub(timestamp);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

struct RawSettingsFile {
    path: String,
    id: String,
    kind: SettingsKind,
    server: Server,
    profile: String,
    modified_time: u64,
}

fn parse_settings_file(
    path: &PathBuf,
    server: Server,
    profile_name: &str,
) -> Option<RawSettingsFile> {
    let filename = path.file_name()?.to_str()?;

    if !filename.starts_with("core_") || !filename.ends_with(".dat") {
        return None;
    }

    let stem = filename
        .trim_start_matches("core_")
        .trim_end_matches(".dat");
    let (kind_str, id) = stem.split_once('_')?;

    if id.is_empty() || id.parse::<u64>().is_err() {
        return None;
    }

    let kind = match kind_str {
        "user" => SettingsKind::User,
        "char" => SettingsKind::Char,
        _ => return None,
    };

    let modified_time = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    Some(RawSettingsFile {
        path: path.to_string_lossy().into_owned(),
        id: id.to_string(),
        kind,
        server,
        profile: profile_name.to_string(),
        modified_time,
    })
}

fn scan_installations(
) -> Result<(HashMap<Server, Vec<ProfileData>>, HashMap<Server, PathBuf>), String> {
    let root = eve_settings_root().ok_or("EVE settings directory not found")?;

    if !root.exists() {
        return Ok((HashMap::new(), HashMap::new()));
    }

    let mut server_profiles: HashMap<Server, Vec<ProfileData>> = HashMap::new();
    let mut server_paths: HashMap<Server, PathBuf> = HashMap::new();
    let entries = fs::read_dir(&root).map_err(|e| e.to_string())?;

    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        if !path.is_dir() {
            continue;
        }

        let server = match Server::from_folder_name(name) {
            Some(s) => s,
            None => continue,
        };

        server_paths.insert(server, path.clone());

        let sub_entries = match fs::read_dir(&path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for sub_entry in sub_entries.flatten() {
            let sub_path = sub_entry.path();
            let sub_name = sub_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();

            if !sub_path.is_dir() || !sub_name.starts_with("settings_") {
                continue;
            }

            let profile_name = sub_name.trim_start_matches("settings_");
            let mut accounts: Vec<RawSettingsFile> = Vec::new();
            let mut characters: Vec<RawSettingsFile> = Vec::new();

            if let Ok(files) = fs::read_dir(&sub_path) {
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

            let profile = ProfileData {
                name: profile_name.to_string(),
                path: sub_path.to_string_lossy().into_owned(),
                accounts: accounts
                    .into_iter()
                    .map(|f| SettingsEntry {
                        display_name: f.id.clone(),
                        relative_time: format_relative_time(f.modified_time),
                        modified_time: f.modified_time,
                        path: f.path,
                        id: f.id,
                        kind: f.kind,
                        server: f.server,
                        profile: f.profile,
                        character: None,
                        alias: None,
                    })
                    .collect(),
                characters: characters
                    .into_iter()
                    .map(|f| SettingsEntry {
                        display_name: f.id.clone(),
                        relative_time: format_relative_time(f.modified_time),
                        modified_time: f.modified_time,
                        path: f.path,
                        id: f.id,
                        kind: f.kind,
                        server: f.server,
                        profile: f.profile,
                        character: None,
                        alias: None,
                    })
                    .collect(),
            };

            server_profiles.entry(server).or_default().push(profile);
        }
    }

    for profiles in server_profiles.values_mut() {
        profiles.sort_by(|a, b| a.name.cmp(&b.name));
    }

    Ok((server_profiles, server_paths))
}

fn scan_backups() -> Result<Vec<BackupEntry>, String> {
    let eve_root = match eve_settings_root() {
        Some(r) => r,
        None => return Ok(Vec::new()),
    };

    let mut backups = Vec::new();

    let server_dirs = match fs::read_dir(&eve_root) {
        Ok(e) => e,
        Err(_) => return Ok(Vec::new()),
    };

    for server_entry in server_dirs.flatten() {
        let server_path = server_entry.path();
        if !server_path.is_dir() {
            continue;
        }

        let profile_dirs = match fs::read_dir(&server_path) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for profile_entry in profile_dirs.flatten() {
            let profile_path = profile_entry.path();
            if !profile_path.is_dir() {
                continue;
            }

            let dir_name = profile_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !dir_name.starts_with("settings_") {
                continue;
            }

            let backup_dir = profile_path.join("backups");
            let entries = match fs::read_dir(&backup_dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let filename = match path.file_name().and_then(|n| n.to_str()) {
                    Some(f) if f.ends_with(".bak") => f,
                    _ => continue,
                };

                let stem = filename.trim_end_matches(".bak");
                let parts: Vec<&str> = stem.rsplitn(4, '_').collect();

                if parts.len() < 4 {
                    continue;
                }

                let timestamp = parts[0].parse::<u64>().unwrap_or(0);
                let original_id = parts[1].to_string();
                let kind = match parts[2] {
                    "user" => SettingsKind::User,
                    "char" => SettingsKind::Char,
                    _ => continue,
                };
                let name = parts[3].to_string();

                backups.push(BackupEntry {
                    id: format!("{}_{}", name, timestamp),
                    display_name: name.clone(),
                    name,
                    path: path.to_string_lossy().into_owned(),
                    timestamp,
                    kind,
                    original_id,
                    original_name: None,
                    relative_time: format_relative_time(timestamp),
                });
            }
        }
    }

    backups.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(backups)
}

#[tauri::command]
pub async fn get_app_data(app: tauri::AppHandle) -> Result<AppData, String> {
    let (mut server_profiles, server_paths) = scan_installations()?;
    let mut backups = scan_backups()?;
    let aliases = load_aliases(&app);

    for profiles in server_profiles.values_mut() {
        for profile in profiles.iter_mut() {
            for account in profile.accounts.iter_mut() {
                if let Some(alias) = aliases.get(&account.id) {
                    account.alias = Some(alias.clone());
                    account.display_name = alias.clone();
                }
            }
        }
    }

    if let Some(tq_profiles) = server_profiles.get_mut(&Server::Tranquility) {
        for profile in tq_profiles.iter_mut() {
            for character in profile.characters.iter_mut() {
                if let Ok(char_id) = character.id.parse::<i64>() {
                    if char_id >= 90_000_000 {
                        if let Ok(info) = esi::get_character(char_id).await {
                            character.display_name = info.name.clone();
                            character.character = Some(CharacterDetails {
                                name: info.name,
                                corporation: info.corporation_name,
                                portrait_url: format!(
                                    "https://images.evetech.net/characters/{}/portrait?size=64",
                                    char_id
                                ),
                            });
                        }
                    }
                }
            }
        }
    }

    let mut servers: Vec<ServerData> = Vec::new();
    let all_servers = [
        Server::Tranquility,
        Server::Singularity,
        Server::Thunderdome,
        Server::Serenity,
    ];

    for server in all_servers {
        if let Some(profiles) = server_profiles.remove(&server) {
            if !profiles.is_empty() {
                let server_path = server_paths.get(&server).cloned().unwrap_or_default();
                let brackets_always_show = read_brackets_setting(&server_path);
                servers.push(ServerData {
                    info: ServerInfo {
                        id: server,
                        name: server.display_name().to_string(),
                        short_name: server.short_name().to_string(),
                        color: server.color().to_string(),
                        supports_esi: server.supports_esi(),
                        brackets_always_show,
                        server_path: server_path.to_string_lossy().into_owned(),
                    },
                    profiles,
                });
            }
        }
    }

    // Enrich backups with entity names from settings entries
    for backup in backups.iter_mut() {
        for server in &servers {
            for profile in &server.profiles {
                let entries = if backup.kind == SettingsKind::User {
                    &profile.accounts
                } else {
                    &profile.characters
                };
                if let Some(entry) = entries.iter().find(|e| e.id == backup.original_id) {
                    backup.original_name = Some(entry.display_name.clone());
                    break;
                }
            }
            if backup.original_name.is_some() {
                break;
            }
        }
    }

    Ok(AppData { servers, backups })
}

#[tauri::command]
pub fn create_backup(
    app: tauri::AppHandle,
    source_path: String,
    backup_name: String,
) -> Result<BackupEntry, String> {
    let source = PathBuf::from(&source_path);

    if !source.exists() {
        return Err("Source file does not exist".into());
    }

    let filename = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid filename")?;

    if !filename.starts_with("core_") || !filename.ends_with(".dat") {
        return Err("Invalid settings file".into());
    }

    let stem = filename
        .trim_start_matches("core_")
        .trim_end_matches(".dat");
    let (kind_str, id) = stem.split_once('_').ok_or("Invalid settings file format")?;

    let kind = match kind_str {
        "user" => SettingsKind::User,
        "char" => SettingsKind::Char,
        _ => return Err("Unknown settings type".into()),
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    let backup_filename = format!("{}_{}_{}_{}.bak", backup_name, kind_str, id, timestamp);

    let backup_dir = backup_directory_for_path(&source)?;
    let dest = backup_dir.join(&backup_filename);

    fs::copy(&source, &dest).map_err(|e| e.to_string())?;

    let entry = BackupEntry {
        id: format!("{}_{}", backup_name, timestamp),
        display_name: backup_name.clone(),
        name: backup_name,
        path: dest.to_string_lossy().into_owned(),
        timestamp,
        kind,
        original_id: id.to_string(),
        original_name: None,
        relative_time: format_relative_time(timestamp),
    };

    emit_data_changed(&app);
    Ok(entry)
}

#[tauri::command]
pub fn delete_backup(app: tauri::AppHandle, backup_path: String) -> Result<(), String> {
    let path = PathBuf::from(&backup_path);

    if !path.exists() {
        return Err("Backup file not found".into());
    }

    fs::remove_file(path).map_err(|e| e.to_string())?;
    emit_data_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn copy_settings(
    app: tauri::AppHandle,
    source_path: String,
    target_paths: Vec<String>,
) -> Result<u32, String> {
    use filetime::FileTime;

    let src = PathBuf::from(&source_path);

    if !src.exists() {
        return Err("Source file not found".into());
    }

    let mut success_count = 0u32;
    let now = FileTime::now();

    for target_path in target_paths {
        let dest = PathBuf::from(&target_path);

        if src == dest {
            continue;
        }

        if fs::copy(&src, &dest).is_ok() {
            let _ = filetime::set_file_mtime(&dest, now);
            success_count += 1;
        }
    }

    if success_count > 0 {
        emit_data_changed(&app);
    }
    Ok(success_count)
}

#[tauri::command]
pub fn set_alias(
    app: tauri::AppHandle,
    account_id: String,
    alias: Option<String>,
) -> Result<(), String> {
    let mut aliases = load_aliases(&app);

    match alias {
        Some(a) if !a.trim().is_empty() => {
            aliases.insert(account_id, a.trim().to_string());
        }
        _ => {
            aliases.remove(&account_id);
        }
    }

    save_aliases(&app, &aliases)?;
    emit_data_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn set_brackets_always_show(
    app: tauri::AppHandle,
    server_path: String,
    enabled: bool,
) -> Result<(), String> {
    let path = PathBuf::from(&server_path);
    write_brackets_setting(&path, enabled)?;
    emit_data_changed(&app);
    Ok(())
}
