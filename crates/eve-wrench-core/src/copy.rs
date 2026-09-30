// Selective settings copy: copies curated groups of settings between files of
// the same kind instead of overwriting the whole file. A group is either whole
// top-level sections or a set of key prefixes inside the "ui" section.

use std::path::Path;

use serde_json::{Map, Value};

use crate::backups::auto_backup;
use crate::marshal::{encode_settings_json, load_settings_json, strip_type_prefix};
use crate::model::SettingsKind;

enum GroupRule {
    Sections(&'static [&'static str]),
    UiPrefixes(&'static [&'static str]),
}

pub struct CopyGroup {
    pub id: &'static str,
    kinds: &'static [SettingsKind],
    // Off for groups people legitimately customize per identity and would be
    // annoyed to have overwritten (HUD module slots, typed search history).
    pub default_on: bool,
    rule: GroupRule,
}

impl CopyGroup {
    pub fn applies_to(&self, kind: SettingsKind) -> bool {
        self.kinds.contains(&kind)
    }
}

const USER: &[SettingsKind] = &[SettingsKind::User];
const CHAR: &[SettingsKind] = &[SettingsKind::Char];
const BOTH: &[SettingsKind] = &[SettingsKind::User, SettingsKind::Char];

pub const COPY_GROUPS: &[CopyGroup] = &[
    CopyGroup {
        id: "overview",
        kinds: USER,
        default_on: true,
        rule: GroupRule::Sections(&["overview", "defaultoverview"]),
    },
    CopyGroup {
        id: "probes",
        kinds: USER,
        default_on: true,
        rule: GroupRule::UiPrefixes(&["probescanning."]),
    },
    CopyGroup {
        id: "suppress",
        kinds: USER,
        default_on: true,
        rule: GroupRule::Sections(&["suppress"]),
    },
    CopyGroup {
        id: "audio",
        kinds: USER,
        default_on: true,
        rule: GroupRule::Sections(&["audio"]),
    },
    CopyGroup {
        id: "camera_graphics",
        kinds: USER,
        default_on: true,
        rule: GroupRule::UiPrefixes(&[
            "camera",
            "spaceMouse",
            "offsetUIwithCamera",
            "invertCameraZoom",
            "advancedCamera",
            "missilesEnabled",
            "turretsEnabled",
            "trailsEnabled",
            "effectsEnabled",
            "explosionEffectsEnabled",
            "gpuParticlesEnabled",
            "droneModelsEnabled",
            "modelSkinsInSpaceEnabled",
            "UI_ASTEROID_",
        ]),
    },
    CopyGroup {
        id: "market",
        kinds: USER,
        default_on: true,
        rule: GroupRule::UiPrefixes(&[
            "market_",
            "minEdit_market",
            "maxEdit_market",
            "quickbar",
            "contracts_search_",
            "mycontracts_",
            "pricehistorytype",
        ]),
    },
    CopyGroup {
        id: "slots",
        kinds: USER,
        default_on: false,
        rule: GroupRule::UiPrefixes(&["slotOrder", "linkedWeapons_"]),
    },
    CopyGroup {
        id: "tabgroups",
        kinds: USER,
        default_on: true,
        rule: GroupRule::Sections(&["tabgroups"]),
    },
    CopyGroup {
        id: "windows",
        kinds: CHAR,
        default_on: true,
        rule: GroupRule::Sections(&["windows"]),
    },
    CopyGroup {
        id: "neocom",
        kinds: CHAR,
        default_on: true,
        rule: GroupRule::UiPrefixes(&["neocomButtonRawData"]),
    },
    CopyGroup {
        id: "chat",
        kinds: CHAR,
        default_on: true,
        rule: GroupRule::UiPrefixes(&["chatchannels"]),
    },
    CopyGroup {
        id: "infopanels",
        kinds: CHAR,
        default_on: true,
        rule: GroupRule::UiPrefixes(&["InfoPanelModes_"]),
    },
    CopyGroup {
        id: "dockpanels",
        kinds: CHAR,
        default_on: true,
        rule: GroupRule::Sections(&["dockPanels"]),
    },
    // Typed-text autocomplete and recent-search data; keys missing from a
    // source are simply skipped
    CopyGroup {
        id: "search_history",
        kinds: BOTH,
        default_on: false,
        rule: GroupRule::UiPrefixes(&[
            "editHistory",
            "contracts_history",
            "market_searchText",
            "assetsSearch",
        ]),
    },
];

fn find_group(id: &str) -> Option<&'static CopyGroup> {
    COPY_GROUPS.iter().find(|g| g.id == id)
}

fn key_matches(key: &str, prefixes: &[&str]) -> bool {
    let stripped = strip_type_prefix(key);
    prefixes.iter().any(|p| stripped.starts_with(p))
}

// Sections are usually "bytes:<name>"; fall back to constructing it
fn find_section_key(root: &Map<String, Value>, section: &str) -> String {
    root.keys()
        .find(|k| strip_type_prefix(k) == section)
        .cloned()
        .unwrap_or_else(|| format!("bytes:{}", section))
}

// The copy starts as a full clone of the source; excluded groups are then
// restored to exactly what the target had, so everything not explicitly
// opted out (including keys no group covers) behaves like a normal copy.
fn preserve_excluded_groups(
    result: &mut Value,
    target: &Value,
    excluded_groups: &[&str],
) -> Result<(), String> {
    let result_map = result
        .as_object_mut()
        .ok_or("Source file has an unexpected structure")?;
    let target_map = target
        .as_object()
        .ok_or("Target file has an unexpected structure")?;

    for group in excluded_groups.iter().filter_map(|id| find_group(id)) {
        match group.rule {
            GroupRule::Sections(sections) => {
                for section in sections {
                    let key = find_section_key(result_map, section);
                    result_map.remove(&key);
                    let target_key = find_section_key(target_map, section);
                    if let Some(value) = target_map.get(&target_key) {
                        result_map.insert(target_key, value.clone());
                    }
                }
            }
            GroupRule::UiPrefixes(prefixes) => {
                let ui_key = find_section_key(result_map, "ui");
                let empty = Map::new();
                let target_ui = target_map
                    .get(&find_section_key(target_map, "ui"))
                    .and_then(|v| v.as_object())
                    .unwrap_or(&empty);

                let result_ui = result_map
                    .entry(ui_key)
                    .or_insert_with(|| Value::Object(Map::new()));
                let Some(result_ui) = result_ui.as_object_mut() else {
                    continue;
                };

                result_ui.retain(|k, _| !key_matches(k, prefixes));
                for (k, v) in target_ui {
                    if key_matches(k, prefixes) {
                        result_ui.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }

    Ok(())
}

// Returns how many targets were written. Targets that fail to decode, encode
// or back up are skipped without touching them.
pub fn copy_settings_selective(
    source: &Path,
    targets: &[String],
    excluded_groups: &[&str],
    backup: bool,
) -> Result<usize, String> {
    let (source_json, _) = load_settings_json(source)?;
    let now = filetime::FileTime::now();
    let mut success_count = 0;

    for target in targets.iter().map(Path::new) {
        if target == source {
            continue;
        }
        let Ok((target_json, had_crc)) = load_settings_json(target) else {
            continue;
        };

        let mut result = source_json.clone();
        if preserve_excluded_groups(&mut result, &target_json, excluded_groups).is_err() {
            continue;
        }
        let Ok(bytes) = encode_settings_json(&result, had_crc) else {
            continue;
        };

        if backup && auto_backup(target, "pre-selective-copy").is_err() {
            continue;
        }
        if std::fs::write(target, bytes).is_ok() {
            let _ = filetime::set_file_mtime(target, now);
            success_count += 1;
        }
    }

    Ok(success_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selective_copy_preserves_only_excluded_groups() {
        let source = json!({
            "bytes:overview": {
                "bytes:shipLabels": {"tuple": ["long:1", "utf8:source-labels"]},
            },
            "bytes:ui": {
                "bytes:probescanning.customFormations": {"tuple": ["long:1", {"int:0": {"tuple": ["utf8:src", []]}}]},
                "bytes:slotOrder": {"tuple": ["long:1", "utf8:source-slots"]},
                "bytes:someUnmappedKey": {"tuple": ["long:1", "utf8:source-unmapped"]},
            },
        });
        let target = json!({
            "bytes:overview": {
                "bytes:shipLabels": {"tuple": ["long:2", "utf8:target-labels"]},
            },
            "bytes:ui": {
                "bytes:probescanning.customFormations": {"tuple": ["long:2", {}]},
                "bytes:slotOrder": {"tuple": ["long:2", "utf8:target-slots"]},
                "bytes:linkedWeapons_groupsDict": {"tuple": ["long:2", "utf8:target-links"]},
                "bytes:someUnmappedKey": {"tuple": ["long:2", "utf8:target-unmapped"]},
            },
        });

        let mut result = source.clone();
        preserve_excluded_groups(&mut result, &target, &["slots"]).unwrap();

        let ui = result.get("bytes:ui").unwrap();
        // Not excluded: mapped and unmapped keys alike come from source
        assert_eq!(
            result["bytes:overview"]["bytes:shipLabels"]["tuple"][1],
            json!("utf8:source-labels")
        );
        assert!(
            ui["bytes:probescanning.customFormations"]["tuple"][1]
                .as_object()
                .unwrap()
                .contains_key("int:0")
        );
        assert_eq!(
            ui["bytes:someUnmappedKey"]["tuple"][1],
            json!("utf8:source-unmapped")
        );
        // Excluded: slot layout keeps exactly what the target had, including
        // keys the source does not have at all
        assert_eq!(
            ui["bytes:slotOrder"]["tuple"][1],
            json!("utf8:target-slots")
        );
        assert_eq!(
            ui["bytes:linkedWeapons_groupsDict"]["tuple"][1],
            json!("utf8:target-links")
        );
    }

    #[test]
    fn group_ids_are_unique() {
        let mut ids: Vec<_> = COPY_GROUPS.iter().map(|g| g.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), COPY_GROUPS.len());
    }
}
