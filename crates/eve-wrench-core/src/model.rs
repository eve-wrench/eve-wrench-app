use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Server {
    Tranquility,
    Singularity,
    Thunderdome,
    Serenity,
}

impl Server {
    pub const ALL: [Server; 4] = [
        Server::Tranquility,
        Server::Singularity,
        Server::Thunderdome,
        Server::Serenity,
    ];

    pub fn from_folder_name(name: &str) -> Option<Self> {
        let lower = name.to_lowercase();
        Self::ALL
            .into_iter()
            .find(|server| lower.contains(server.id()))
    }

    pub fn id(&self) -> &'static str {
        match self {
            Server::Tranquility => "tranquility",
            Server::Singularity => "singularity",
            Server::Thunderdome => "thunderdome",
            Server::Serenity => "serenity",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Server::Tranquility => "Tranquility",
            Server::Singularity => "Singularity",
            Server::Thunderdome => "Thunderdome",
            Server::Serenity => "Serenity",
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            Server::Tranquility => "TQ",
            Server::Singularity => "SISI",
            Server::Thunderdome => "TD",
            Server::Serenity => "CN",
        }
    }

    // Only Tranquility and Singularity character IDs resolve against ESI.
    pub fn supports_esi(&self) -> bool {
        matches!(self, Server::Tranquility | Server::Singularity)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, Copy)]
#[serde(rename_all = "lowercase")]
pub enum SettingsKind {
    User,
    Char,
}

impl SettingsKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "user" => Some(SettingsKind::User),
            "char" => Some(SettingsKind::Char),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerInfo {
    pub id: Server,
    pub brackets_always_show: bool,
    pub server_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CharacterDetails {
    pub name: String,
    pub corporation: Option<String>,
    // 64px JPEG from the EVE image server, when it could be fetched
    pub portrait: Option<std::sync::Arc<[u8]>>,
}

#[derive(Debug, Clone, PartialEq)]
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
}

impl SettingsEntry {
    // Accounts sort and display by alias, characters by their ESI name.
    pub fn sort_name(&self) -> &str {
        match self.kind {
            SettingsKind::User => self.alias.as_deref().unwrap_or(&self.id),
            SettingsKind::Char => self
                .character
                .as_ref()
                .map(|c| c.name.as_str())
                .unwrap_or(&self.id),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProfileData {
    pub name: String,
    pub path: String,
    pub accounts: Vec<SettingsEntry>,
    pub characters: Vec<SettingsEntry>,
}

impl ProfileData {
    pub fn entries(&self, kind: SettingsKind) -> &[SettingsEntry] {
        match kind {
            SettingsKind::User => &self.accounts,
            SettingsKind::Char => &self.characters,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerData {
    pub info: ServerInfo,
    pub profiles: Vec<ProfileData>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BackupEntry {
    pub id: String,
    pub name: String,
    pub path: String,
    pub timestamp: u64,
    pub kind: SettingsKind,
    pub original_id: String,
    pub original_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppData {
    pub servers: Vec<ServerData>,
    pub backups: Vec<BackupEntry>,
}

impl AppData {
    pub fn is_empty(&self) -> bool {
        self.servers.is_empty() && self.backups.is_empty()
    }

    pub fn entries(&self) -> impl Iterator<Item = &SettingsEntry> {
        self.servers
            .iter()
            .flat_map(|s| &s.profiles)
            .flat_map(|p| p.accounts.iter().chain(&p.characters))
    }

    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut SettingsEntry> {
        self.servers
            .iter_mut()
            .flat_map(|s| &mut s.profiles)
            .flat_map(|p| p.accounts.iter_mut().chain(&mut p.characters))
    }

    pub fn find_entry(&self, path: &str) -> Option<&SettingsEntry> {
        self.entries().find(|e| e.path == path)
    }

    pub fn backups_for(&self, entry: &SettingsEntry) -> impl Iterator<Item = &BackupEntry> {
        self.backups
            .iter()
            .filter(|b| b.kind == entry.kind && b.original_id == entry.id)
    }

    // Character IDs below 90,000,000 are NPC or test ranges ESI can't resolve.
    pub fn esi_character_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .entries()
            .filter(|e| e.kind == SettingsKind::Char && e.server.supports_esi())
            .filter_map(|e| e.id.parse::<i64>().ok())
            .filter(|id| *id >= 90_000_000)
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn apply_character_details(
        &mut self,
        details: &std::collections::HashMap<i64, CharacterDetails>,
    ) {
        for entry in self.entries_mut() {
            if entry.kind != SettingsKind::Char || !entry.server.supports_esi() {
                continue;
            }
            if let Some(info) = entry.id.parse::<i64>().ok().and_then(|id| details.get(&id)) {
                entry.display_name = info.name.clone();
                entry.character = Some(info.clone());
            }
        }
        self.resolve_backup_names();
    }

    pub(crate) fn resolve_backup_names(&mut self) {
        let names: Vec<Option<String>> = self
            .backups
            .iter()
            .map(|backup| {
                self.entries()
                    .find(|e| e.kind == backup.kind && e.id == backup.original_id)
                    .map(|e| e.display_name.clone())
            })
            .collect();
        for (backup, name) in self.backups.iter_mut().zip(names) {
            backup.original_name = name;
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FormationProbe {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub range: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProbeFormation {
    pub id: i64,
    pub name: String,
    pub probes: Vec<FormationProbe>,
}
