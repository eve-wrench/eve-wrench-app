// Share files: selected copy groups taken out of one settings file so another
// player can look at them and apply them to their own account or character.
// They travel as a JSON file or as a compressed paste string for chat.
//
// Imports are untrusted. Every key has to belong to the group it is filed
// under, unknown groups are dropped, and probe formations are rebuilt from
// parsed values, so a crafted share can't write anything a normal export
// wouldn't contain.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::backups::auto_backup;
use crate::confirmations;
use crate::copy::{COPY_GROUPS, CopyGroup, GroupRule, find_group, find_section_key, key_matches};
use crate::formations::{FORMATIONS_KEY, formations_from_settings, formations_into_settings};
use crate::marshal::{
    encode_settings_json, filetime_now, load_settings_json, strip_type_prefix, unix_now,
};
use crate::model::{FormationProbe, ProbeFormation, SettingsKind};
use crate::updates::APP_VERSION;

pub const FORMAT_VERSION: u32 = 1;
pub const FILE_EXTENSION: &str = "ewshare";

const PASTE_PREFIX: &str = "ewshare1:";
// Real shares are a few hundred KB at most; this only stops absurd input
const MAX_SIZE: u64 = 4 * 1024 * 1024;
const MAX_FORMATION_NAME: usize = 100;
const MAX_PROBES: usize = 8;
const PROBES_GROUP: &str = "probes";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct SharePack {
    pub format: u32,
    pub app_version: String,
    // Unix seconds
    pub created: u64,
    pub kind: SettingsKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    // Group id -> the group's keys exactly as stored in a settings file
    pub groups: BTreeMap<String, Map<String, Value>>,
}

// Groups that can go into a share file of this kind.
pub fn shareable_groups(kind: SettingsKind) -> impl Iterator<Item = &'static CopyGroup> {
    COPY_GROUPS
        .iter()
        .filter(move |g| g.share.is_some() && g.applies_to(kind))
}

fn shareable_group(id: &str, kind: SettingsKind) -> Option<&'static CopyGroup> {
    find_group(id).filter(|g| g.share.is_some() && g.applies_to(kind))
}

fn ui_map(root: &Map<String, Value>) -> Option<&Map<String, Value>> {
    root.get(&find_section_key(root, "ui"))
        .and_then(|v| v.as_object())
}

fn key_in_group(group: &CopyGroup, key: &str) -> bool {
    match group.rule {
        GroupRule::Sections(sections) => sections.contains(&strip_type_prefix(key)),
        GroupRule::UiPrefixes(prefixes) => key_matches(key, prefixes),
    }
}

// Formations are rebuilt from parsed values instead of copied, which drops
// the client's scratch entries (negative ids) and anything malformed.
fn formations_fragment(formations: &[ProbeFormation]) -> Result<Map<String, Value>, String> {
    let mut root = json!({});
    formations_into_settings(&mut root, formations)?;
    let mut fragment = Map::new();
    if let Some(value) = root
        .get("bytes:ui")
        .and_then(|ui| ui.get(FORMATIONS_KEY))
        .cloned()
    {
        fragment.insert(FORMATIONS_KEY.to_string(), value);
    }
    Ok(fragment)
}

fn fragment_formations(fragment: &Map<String, Value>) -> Vec<ProbeFormation> {
    formations_from_settings(&json!({ "bytes:ui": fragment }))
}

fn valid_formation(formation: &ProbeFormation) -> bool {
    let finite = |p: &FormationProbe| {
        [p.x, p.y, p.z, p.range].iter().all(|v| v.is_finite()) && p.range > 0.0
    };
    !formation.name.trim().is_empty()
        && !formation.probes.is_empty()
        && formation.probes.len() <= MAX_PROBES
        && formation.probes.iter().all(finite)
}

fn clean_formations(formations: Vec<ProbeFormation>) -> Vec<ProbeFormation> {
    formations
        .into_iter()
        .filter(valid_formation)
        .enumerate()
        .map(|(index, mut formation)| {
            formation.id = index as i64;
            formation.name = formation
                .name
                .trim()
                .chars()
                .take(MAX_FORMATION_NAME)
                .collect();
            formation
        })
        .collect()
}

fn extract_group(
    group: &CopyGroup,
    root: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    if group.id == PROBES_GROUP {
        let formations = formations_from_settings(&Value::Object(root.clone()));
        return formations_fragment(&clean_formations(formations));
    }
    let empty = Map::new();
    let source = match group.rule {
        GroupRule::Sections(_) => root,
        GroupRule::UiPrefixes(_) => ui_map(root).unwrap_or(&empty),
    };
    Ok(source
        .iter()
        .filter(|(key, _)| key_in_group(group, key))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}

impl SharePack {
    fn new(kind: SettingsKind, note: Option<String>) -> Self {
        Self {
            format: FORMAT_VERSION,
            app_version: APP_VERSION.to_string(),
            created: unix_now(),
            kind,
            note: note.map(|n| n.trim().to_string()).filter(|n| !n.is_empty()),
            groups: BTreeMap::new(),
        }
    }

    // Groups the source has no data for are left out.
    pub fn export(
        source: &Path,
        kind: SettingsKind,
        group_ids: &[&str],
        note: Option<String>,
    ) -> Result<Self, String> {
        let (json, _) = load_settings_json(source)?;
        let root = json
            .as_object()
            .ok_or("Settings file has an unexpected structure")?;

        let mut pack = Self::new(kind, note);
        for id in group_ids {
            let group = shareable_group(id, kind)
                .ok_or_else(|| format!("\"{id}\" can't be shared from this file"))?;
            let fragment = extract_group(group, root)?;
            if !fragment.is_empty() {
                pack.groups.insert(group.id.to_string(), fragment);
            }
        }
        if pack.groups.is_empty() {
            return Err("None of the selected settings exist in this file yet".into());
        }
        Ok(pack)
    }

    pub fn from_formations(
        formations: &[ProbeFormation],
        note: Option<String>,
    ) -> Result<Self, String> {
        let formations = clean_formations(formations.to_vec());
        if formations.is_empty() {
            return Err("No valid formations to share".into());
        }
        let mut pack = Self::new(SettingsKind::User, note);
        pack.groups
            .insert(PROBES_GROUP.to_string(), formations_fragment(&formations)?);
        Ok(pack)
    }

    // A copy with only these groups and a new note.
    pub fn select(&self, group_ids: &[&str], note: Option<String>) -> Result<Self, String> {
        let mut pack = Self::new(self.kind, note);
        pack.groups = self
            .groups
            .iter()
            .filter(|(id, _)| group_ids.contains(&id.as_str()))
            .map(|(id, fragment)| (id.clone(), fragment.clone()))
            .collect();
        if pack.groups.is_empty() {
            return Err("Nothing selected to share".into());
        }
        Ok(pack)
    }

    pub fn formations(&self) -> Vec<ProbeFormation> {
        self.groups
            .get(PROBES_GROUP)
            .map(fragment_formations)
            .unwrap_or_default()
    }

    pub fn group_ids(&self) -> impl Iterator<Item = &str> {
        self.groups.keys().map(String::as_str)
    }

    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Failed to build share file: {e}"))
    }

    pub fn to_paste_string(&self) -> Result<String, String> {
        let json = serde_json::to_vec(self).map_err(|e| format!("Failed to build share: {e}"))?;
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::best());
        encoder
            .write_all(&json)
            .and_then(|_| encoder.finish())
            .map(|compressed| format!("{PASTE_PREFIX}{}", URL_SAFE_NO_PAD.encode(compressed)))
            .map_err(|e| format!("Failed to compress share: {e}"))
    }

    pub fn write_file(&self, path: &Path) -> Result<(), String> {
        fs::write(path, self.to_json()?).map_err(|e| format!("Failed to write share file: {e}"))
    }

    pub fn read_file(path: &Path) -> Result<Self, String> {
        let file = fs::File::open(path).map_err(|e| format!("Failed to open file: {e}"))?;
        let mut text = String::new();
        file.take(MAX_SIZE + 1)
            .read_to_string(&mut text)
            .map_err(|_| "This is not a share file".to_string())?;
        if text.len() as u64 > MAX_SIZE {
            return Err("File is too large to be a share file".into());
        }
        Self::parse(&text)
    }

    // Accepts the JSON file contents or a paste string.
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        let json = match text.strip_prefix(PASTE_PREFIX) {
            Some(encoded) => decode_paste(encoded)?,
            None => text.to_string(),
        };

        let raw: Value =
            serde_json::from_str(&json).map_err(|_| "This is not a share file".to_string())?;
        let format = raw.get("format").and_then(Value::as_u64).unwrap_or(0);
        if format > FORMAT_VERSION as u64 {
            return Err(
                "This share was made with a newer version of EVE Wrench. Update to import it."
                    .into(),
            );
        }
        let pack: Self =
            serde_json::from_value(raw).map_err(|e| format!("Share file is damaged: {e}"))?;
        pack.validated()
    }

    fn validated(mut self) -> Result<Self, String> {
        let kind = self.kind;
        let mut groups = BTreeMap::new();
        for (id, fragment) in std::mem::take(&mut self.groups) {
            // Groups from a newer version, or ones that aren't shareable,
            // are skipped rather than failing the whole import
            let Some(group) = shareable_group(&id, kind) else {
                continue;
            };

            let fragment = if group.id == PROBES_GROUP {
                formations_fragment(&clean_formations(fragment_formations(&fragment)))?
            } else {
                if let Some(key) = fragment.keys().find(|k| !key_in_group(group, k)) {
                    return Err(format!(
                        "Share file is damaged: \"{}\" doesn't belong to {id}",
                        strip_type_prefix(key)
                    ));
                }
                blue_marshal::from_json(&Value::Object(fragment.clone()))
                    .map_err(|e| format!("Share file is damaged: {e}"))?;
                fragment
            };
            if !fragment.is_empty() {
                groups.insert(group.id.to_string(), fragment);
            }
        }
        if groups.is_empty() {
            return Err("This share contains nothing this version of EVE Wrench can import".into());
        }
        self.groups = groups;
        Ok(self)
    }
}

fn decode_paste(encoded: &str) -> Result<String, String> {
    let compact: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    let compressed = URL_SAFE_NO_PAD
        .decode(compact)
        .map_err(|_| "The pasted text is incomplete or damaged".to_string())?;
    let mut json = String::new();
    DeflateDecoder::new(compressed.as_slice())
        .take(MAX_SIZE + 1)
        .read_to_string(&mut json)
        .map_err(|_| "The pasted text is incomplete or damaged".to_string())?;
    if json.len() as u64 > MAX_SIZE {
        return Err("The pasted share is too large".into());
    }
    Ok(json)
}

// What a share contains, for showing before anything is written.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupItems {
    // Name and probe count
    Formations(Vec<(String, usize)>),
    // Overview profile names and (tab name, profile) pairs
    Overview {
        profiles: Vec<String>,
        tabs: Vec<(String, String)>,
    },
    // Dialog keys as used by the confirmations catalog
    Dialogs(Vec<String>),
    // Number of individual settings
    Settings(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Changes {
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub unchanged: usize,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added == 0 && self.changed == 0 && self.removed == 0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupPreview {
    pub id: &'static str,
    // Probe formations are added next to the target's own; every other group
    // replaces what the target has
    pub merges: bool,
    pub items: GroupItems,
    pub changes: Option<Changes>,
}

fn unstamped(value: &Value) -> &Value {
    value
        .get("tuple")
        .and_then(Value::as_array)
        .filter(|t| t.len() == 2 && t[0].as_str().is_some_and(|s| s.starts_with("long:")))
        .map(|t| &t[1])
        .unwrap_or(value)
}

fn stamp(value: &mut Value, now: &str) {
    if let Some(tuple) = value.get_mut("tuple").and_then(Value::as_array_mut)
        && tuple.len() == 2
        && tuple[0].as_str().is_some_and(|s| s.starts_with("long:"))
    {
        tuple[0] = Value::String(now.to_string());
    }
}

// Individual settings, without their timestamps. Sections contribute their
// children, so changes are counted per setting rather than per section.
fn leaves(group: &CopyGroup, fragment: &Map<String, Value>) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    for (key, value) in fragment {
        let name = strip_type_prefix(key);
        match (&group.rule, value.as_object()) {
            (GroupRule::Sections(_), Some(children)) if !children.contains_key("tuple") => {
                for (child, v) in children {
                    out.insert(
                        format!("{name}/{}", strip_type_prefix(child)),
                        unstamped(v).clone(),
                    );
                }
            }
            _ => {
                out.insert(name.to_string(), unstamped(value).clone());
            }
        }
    }
    out
}

fn sorted_strings(map: Option<&Map<String, Value>>) -> Vec<String> {
    let mut names: Vec<String> = map
        .map(|m| m.keys().map(|k| strip_type_prefix(k).to_string()).collect())
        .unwrap_or_default();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn overview_items(fragment: &Map<String, Value>) -> GroupItems {
    let overview = fragment
        .iter()
        .find(|(k, _)| strip_type_prefix(k) == "overview")
        .and_then(|(_, v)| v.as_object());
    let setting = |name: &str| {
        overview
            .and_then(|o| o.iter().find(|(k, _)| strip_type_prefix(k) == name))
            .map(|(_, v)| unstamped(v))
    };

    // The client keeps copies of the built-in presets under generated names
    let profiles = sorted_strings(setting("overviewProfilePresets").and_then(Value::as_object))
        .into_iter()
        .filter(|name| !name.starts_with("DefaultPreset_"))
        .collect();

    let mut tabs: Vec<(i64, String, String)> = setting("tabsettings_new")
        .and_then(Value::as_object)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|(key, tab)| {
                    let text = |field: &str| {
                        tab.as_object()?
                            .iter()
                            .find(|(k, _)| strip_type_prefix(k) == field)
                            .and_then(|(_, v)| v.as_str())
                            .map(|s| strip_type_prefix(s).to_string())
                    };
                    let index = strip_type_prefix(key).parse().unwrap_or(i64::MAX);
                    Some((index, text("name")?, text("overview").unwrap_or_default()))
                })
                .collect()
        })
        .unwrap_or_default();
    tabs.sort();

    GroupItems::Overview {
        profiles,
        tabs: tabs
            .into_iter()
            .map(|(_, name, profile)| (name, profile))
            .collect(),
    }
}

fn group_items(group: &CopyGroup, fragment: &Map<String, Value>) -> GroupItems {
    match group.id {
        PROBES_GROUP => GroupItems::Formations(
            fragment_formations(fragment)
                .into_iter()
                .map(|f| (f.name, f.probes.len()))
                .collect(),
        ),
        "overview" => overview_items(fragment),
        "suppress" => {
            let section = fragment.values().find_map(Value::as_object);
            GroupItems::Dialogs(
                sorted_strings(section)
                    .into_iter()
                    .map(|k| {
                        k.strip_prefix(confirmations::PREFIX)
                            .unwrap_or(&k)
                            .to_string()
                    })
                    .collect(),
            )
        }
        _ => GroupItems::Settings(leaves(group, fragment).len()),
    }
}

fn same_formation(a: &ProbeFormation, b: &ProbeFormation) -> bool {
    a.name == b.name && a.probes == b.probes
}

fn group_changes(
    group: &CopyGroup,
    fragment: &Map<String, Value>,
    target: &Map<String, Value>,
) -> Result<Changes, String> {
    if group.id == PROBES_GROUP {
        let existing = formations_from_settings(&Value::Object(target.clone()));
        let incoming = fragment_formations(fragment);
        let duplicates = incoming
            .iter()
            .filter(|f| existing.iter().any(|e| same_formation(e, f)))
            .count();
        return Ok(Changes {
            added: incoming.len() - duplicates,
            unchanged: duplicates,
            ..Default::default()
        });
    }

    let mut current = extract_group(group, target)?;
    // Sections the share doesn't have are left alone on apply
    if let GroupRule::Sections(_) = group.rule {
        current.retain(|key, _| {
            fragment
                .keys()
                .any(|k| strip_type_prefix(k) == strip_type_prefix(key))
        });
    }
    let incoming = leaves(group, fragment);
    let current = leaves(group, &current);

    let mut changes = Changes::default();
    for (key, value) in &incoming {
        match current.get(key) {
            None => changes.added += 1,
            Some(v) if v == value => changes.unchanged += 1,
            Some(_) => changes.changed += 1,
        }
    }
    changes.removed = current
        .keys()
        .filter(|k| !incoming.contains_key(*k))
        .count();
    Ok(changes)
}

// Without a target only the contents are described.
pub fn preview(pack: &SharePack, target: Option<&Path>) -> Result<Vec<GroupPreview>, String> {
    let target_json = target.map(load_settings_json).transpose()?;
    let target_root = target_json
        .as_ref()
        .map(|(json, _)| {
            json.as_object()
                .ok_or("Settings file has an unexpected structure")
        })
        .transpose()?;

    // Same order as everywhere else in the app
    COPY_GROUPS
        .iter()
        .filter_map(|group| pack.groups.get(group.id).map(|fragment| (group, fragment)))
        .map(|(group, fragment)| {
            Ok(GroupPreview {
                id: group.id,
                merges: group.id == PROBES_GROUP,
                items: group_items(group, fragment),
                changes: target_root
                    .map(|root| group_changes(group, fragment, root))
                    .transpose()?,
            })
        })
        .collect()
}

fn merge_formations(root: &mut Value, incoming: Vec<ProbeFormation>) -> Result<(), String> {
    let mut formations = formations_from_settings(root);
    let mut next_id = formations.iter().map(|f| f.id + 1).max().unwrap_or(0);
    for mut formation in incoming {
        if formations.iter().any(|e| same_formation(e, &formation)) {
            continue;
        }
        formation.id = next_id;
        next_id += 1;
        formations.push(formation);
    }
    formations_into_settings(root, &formations)
}

fn replace_group(
    group: &CopyGroup,
    root: &mut Map<String, Value>,
    fragment: &Map<String, Value>,
    now: &str,
) {
    let stamped = |value: &Value| {
        let mut value = value.clone();
        match &group.rule {
            GroupRule::Sections(_) => {
                if let Some(children) = value.as_object_mut().filter(|o| !o.contains_key("tuple")) {
                    children.values_mut().for_each(|v| stamp(v, now));
                } else {
                    stamp(&mut value, now);
                }
            }
            GroupRule::UiPrefixes(_) => stamp(&mut value, now),
        }
        value
    };

    match group.rule {
        GroupRule::Sections(_) => {
            for (key, value) in fragment {
                root.retain(|k, _| strip_type_prefix(k) != strip_type_prefix(key));
                root.insert(key.clone(), stamped(value));
            }
        }
        GroupRule::UiPrefixes(prefixes) => {
            let ui_key = find_section_key(root, "ui");
            let ui = root.entry(ui_key).or_insert_with(|| json!({}));
            if !ui.is_object() {
                *ui = json!({});
            }
            if let Some(ui) = ui.as_object_mut() {
                ui.retain(|k, _| !key_matches(k, prefixes));
                for (key, value) in fragment {
                    ui.insert(key.clone(), stamped(value));
                }
            }
        }
    }
}

// Applies the chosen groups of a share to one settings file of the same kind.
pub fn apply(
    pack: &SharePack,
    target: &Path,
    target_kind: SettingsKind,
    group_ids: &[&str],
    backup: bool,
) -> Result<(), String> {
    if target_kind != pack.kind {
        return Err(match pack.kind {
            SettingsKind::User => "This share is for an account, not a character".into(),
            SettingsKind::Char => "This share is for a character, not an account".into(),
        });
    }
    if !target.exists() {
        return Err("Settings file does not exist".into());
    }

    let (mut json, had_crc) = load_settings_json(target)?;
    let now = format!("long:{}", filetime_now());
    let wanted: HashSet<&str> = group_ids.iter().copied().collect();

    for (id, fragment) in pack
        .groups
        .iter()
        .filter(|(id, _)| wanted.contains(id.as_str()))
    {
        let Some(group) = shareable_group(id, pack.kind) else {
            continue;
        };
        if group.id == PROBES_GROUP {
            merge_formations(&mut json, fragment_formations(fragment))?;
        } else {
            let root = json
                .as_object_mut()
                .ok_or("Settings file has an unexpected structure")?;
            replace_group(group, root, fragment, &now);
        }
    }

    let bytes = encode_settings_json(&json, had_crc)?;
    if backup {
        auto_backup(target, "pre-share-import")?;
    }
    fs::write(target, bytes).map_err(|e| format!("Failed to write settings file: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(x: f64) -> FormationProbe {
        FormationProbe {
            x,
            y: 0.0,
            z: 0.0,
            range: 1_000_000.0,
        }
    }

    fn formation(name: &str, probes: Vec<FormationProbe>) -> ProbeFormation {
        ProbeFormation {
            id: 0,
            name: name.to_string(),
            probes,
        }
    }

    fn source_settings() -> Value {
        json!({
            "bytes:overview": {
                "bytes:overviewProfilePresets": {"tuple": ["long:1", {
                    "utf8:PVP": {"bytes:groups": [6, 10]},
                    "bytes:DefaultPreset_1": {"bytes:groups": [6]},
                }]},
                "bytes:tabsettings_new": {"tuple": ["long:1", {
                    "int:1": {"bytes:name": "utf8:Targets", "bytes:overview": "utf8:PVP"},
                    "int:0": {"bytes:name": "utf8:Main", "bytes:overview": "utf8:PVP"},
                }]},
            },
            "bytes:suppress": {
                "bytes:suppress.AskQuit": {"tuple": ["long:1", 1]},
            },
            "bytes:audio": {
                "bytes:custom_turrets": {"tuple": ["long:1", 0.5]},
            },
            "bytes:ui": {
                "bytes:probescanning.customFormations": {"tuple": ["long:1", {
                    "int:-4": {"tuple": ["bytes:tempFormation", []]},
                    "int:3": {"tuple": ["utf8:Pinpoint", [{"tuple": [{"tuple": [1.0, 2.0, 3.0]}, 1000.0]}]]},
                }]},
                "bytes:probescanning.selectedFormationID": {"tuple": ["long:1", 3]},
                "bytes:market_filter": {"tuple": ["long:1", "utf8:abc"]},
                "bytes:editHistory": {"tuple": ["long:1", "utf8:secret"]},
            },
        })
    }

    fn write_settings(name: &str, value: &Value) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("eve-wrench-share-{name}"));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core_user_1.dat");
        fs::write(&path, encode_settings_json(value, false).unwrap()).unwrap();
        path
    }

    #[test]
    fn export_round_trips_through_file_and_paste_string() {
        let path = write_settings("export", &source_settings());
        let pack = SharePack::export(
            &path,
            SettingsKind::User,
            &["overview", "probes", "suppress", "market"],
            Some("  for the fleet ".into()),
        )
        .unwrap();

        assert_eq!(pack.note.as_deref(), Some("for the fleet"));
        // Scratch formation and the selected id are not shared
        let probes = &pack.groups["probes"];
        assert_eq!(probes.len(), 1);
        assert_eq!(pack.formations().len(), 1);
        assert_eq!(pack.formations()[0].name, "Pinpoint");

        assert_eq!(SharePack::parse(&pack.to_json().unwrap()).unwrap(), pack);
        let paste = pack.to_paste_string().unwrap();
        assert!(paste.starts_with(PASTE_PREFIX));
        assert_eq!(SharePack::parse(&paste).unwrap(), pack);

        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn floats_survive_the_round_trip_exactly() {
        // A real audio level that the default serde_json parser reads back
        // one bit off, which made an unchanged share show as changed
        let mut settings = source_settings();
        settings["bytes:audio"]["bytes:custom_turrets"] =
            json!({"tuple": ["long:1", 0.14147286821705427]});
        let path = write_settings("floats", &settings);
        let pack = SharePack::export(&path, SettingsKind::User, &["audio"], None).unwrap();

        for text in [pack.to_json().unwrap(), pack.to_paste_string().unwrap()] {
            let parsed = SharePack::parse(&text).unwrap();
            assert_eq!(parsed, pack);
            let changes = preview(&parsed, Some(&path)).unwrap()[0].changes.unwrap();
            assert!(changes.is_empty());
        }

        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn personal_groups_cannot_be_exported() {
        let path = write_settings("personal", &source_settings());
        assert!(SharePack::export(&path, SettingsKind::User, &["search_history"], None).is_err());
        assert!(SharePack::export(&path, SettingsKind::User, &["windows"], None).is_err());
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn keys_outside_their_group_are_rejected() {
        let mut pack = SharePack::new(SettingsKind::User, None);
        let mut fragment = Map::new();
        fragment.insert("bytes:audio".into(), json!({}));
        fragment.insert("bytes:ui".into(), json!({}));
        pack.groups.insert("audio".into(), fragment);
        let text = pack.to_json().unwrap();
        assert!(
            SharePack::parse(&text)
                .unwrap_err()
                .contains("doesn't belong")
        );
    }

    #[test]
    fn unknown_and_personal_groups_are_dropped_on_import() {
        let mut pack = SharePack::new(SettingsKind::User, None);
        let mut audio = Map::new();
        audio.insert("bytes:audio".into(), json!({}));
        pack.groups.insert("audio".into(), audio);
        pack.groups.insert("from_the_future".into(), Map::new());
        let mut history = Map::new();
        history.insert(
            "bytes:editHistory".into(),
            json!({"tuple": ["long:1", "utf8:x"]}),
        );
        pack.groups.insert("search_history".into(), history);

        let parsed = SharePack::parse(&pack.to_json().unwrap()).unwrap();
        assert_eq!(parsed.group_ids().collect::<Vec<_>>(), ["audio"]);
    }

    #[test]
    fn newer_formats_are_refused() {
        let text = json!({"format": 99, "app_version": "9.0.0", "created": 0, "kind": "user", "groups": {}});
        assert!(
            SharePack::parse(&text.to_string())
                .unwrap_err()
                .contains("newer version")
        );
        assert!(SharePack::parse("hello").is_err());
        assert!(SharePack::parse("ewshare1:!!!").is_err());
    }

    #[test]
    fn invalid_formations_are_dropped() {
        let pack = SharePack::from_formations(
            &[
                formation("ok", vec![probe(1.0)]),
                formation("  ", vec![probe(1.0)]),
                formation("empty", vec![]),
                formation("too many", (0..9).map(|i| probe(i as f64)).collect()),
                formation("nan", vec![probe(f64::NAN)]),
            ],
            None,
        )
        .unwrap();
        let names: Vec<_> = pack.formations().into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["ok"]);
    }

    #[test]
    fn preview_describes_contents_and_changes() {
        let source = write_settings("preview-source", &source_settings());
        let pack = SharePack::export(
            &source,
            SettingsKind::User,
            &["overview", "probes", "suppress", "audio"],
            None,
        )
        .unwrap();

        let target_settings = json!({
            "bytes:audio": {
                "bytes:custom_turrets": {"tuple": ["long:9", 0.5]},
                "bytes:custom_music": {"tuple": ["long:9", 0.1]},
            },
            "bytes:ui": {},
        });
        let target = write_settings("preview-target", &target_settings);
        let previews = preview(&pack, Some(&target)).unwrap();
        let get = |id: &str| previews.iter().find(|p| p.id == id).unwrap();

        assert_eq!(
            get("overview").items,
            GroupItems::Overview {
                profiles: vec!["PVP".into()],
                tabs: vec![
                    ("Main".into(), "PVP".into()),
                    ("Targets".into(), "PVP".into())
                ],
            }
        );
        assert_eq!(
            get("suppress").items,
            GroupItems::Dialogs(vec!["AskQuit".into()])
        );
        assert_eq!(
            get("probes").items,
            GroupItems::Formations(vec![("Pinpoint".into(), 1)])
        );
        assert!(get("probes").merges);

        // Same value with a different timestamp counts as unchanged
        let audio = get("audio").changes.unwrap();
        assert_eq!(
            (audio.added, audio.changed, audio.removed, audio.unchanged),
            (0, 0, 1, 1)
        );
        assert_eq!(get("probes").changes.unwrap().added, 1);

        fs::remove_dir_all(source.parent().unwrap()).ok();
        fs::remove_dir_all(target.parent().unwrap()).ok();
    }

    #[test]
    fn apply_replaces_groups_and_merges_formations() {
        let source = write_settings("apply-source", &source_settings());
        let pack = SharePack::export(
            &source,
            SettingsKind::User,
            &["probes", "audio", "market"],
            None,
        )
        .unwrap();

        let target_settings = json!({
            "bytes:audio": {
                "bytes:custom_music": {"tuple": ["long:9", 0.1]},
            },
            "bytes:ui": {
                "bytes:probescanning.customFormations": {"tuple": ["long:9", {
                    "int:0": {"tuple": ["utf8:Mine", [{"tuple": [{"tuple": [0.0, 0.0, 0.0]}, 500.0]}]]},
                    "int:3": {"tuple": ["utf8:Pinpoint", [{"tuple": [{"tuple": [1.0, 2.0, 3.0]}, 1000.0]}]]},
                }]},
                "bytes:market_old": {"tuple": ["long:9", 1]},
                "bytes:keepMe": {"tuple": ["long:9", 1]},
            },
        });
        let target = write_settings("apply-target", &target_settings);

        // Only the chosen groups are applied
        apply(
            &pack,
            &target,
            SettingsKind::User,
            &["probes", "audio"],
            false,
        )
        .unwrap();
        let (json, _) = load_settings_json(&target).unwrap();
        let audio = json["bytes:audio"].as_object().unwrap();
        assert!(audio.contains_key("bytes:custom_turrets"));
        assert!(!audio.contains_key("bytes:custom_music"));
        // Imported values are stamped as fresh
        assert_ne!(audio["bytes:custom_turrets"]["tuple"][0], json!("long:1"));
        assert!(json["bytes:ui"].get("bytes:market_old").is_some());

        // The identical formation is not added twice
        let formations = formations_from_settings(&json);
        let names: Vec<_> = formations.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["Mine", "Pinpoint"]);

        apply(&pack, &target, SettingsKind::User, &["market"], false).unwrap();
        let (json, _) = load_settings_json(&target).unwrap();
        let ui = json["bytes:ui"].as_object().unwrap();
        assert!(ui.contains_key("bytes:market_filter"));
        assert!(!ui.contains_key("bytes:market_old"));
        assert!(ui.contains_key("bytes:keepMe"));

        assert!(apply(&pack, &target, SettingsKind::Char, &["audio"], false).is_err());

        fs::remove_dir_all(source.parent().unwrap()).ok();
        fs::remove_dir_all(target.parent().unwrap()).ok();
    }
}
