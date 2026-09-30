// Stored in the account (core_user) file under ui -> probescanning.customFormations
// as (timestamp, {id: (name, [((x, y, z), range), ...])}). Every leaf setting is
// wrapped in a (FILETIME timestamp, value) tuple; positions and ranges are meters.

use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::backups::auto_backup;
use crate::marshal::{encode_settings_json, filetime_now, load_settings_json, strip_type_prefix};
use crate::model::{FormationProbe, ProbeFormation};

const UI_KEY: &str = "bytes:ui";
const FORMATIONS_KEY: &str = "bytes:probescanning.customFormations";
const SELECTED_FORMATION_KEY: &str = "bytes:probescanning.selectedFormationID";

fn stored_entries(root: &Value) -> Option<&serde_json::Map<String, Value>> {
    root.get(UI_KEY)
        .and_then(|ui| ui.get(FORMATIONS_KEY))
        .and_then(|v| v.get("tuple"))
        .and_then(|t| t.get(1))
        .and_then(|v| v.as_object())
}

fn parse_probe(value: &Value) -> Option<FormationProbe> {
    let outer = value.get("tuple")?.as_array()?;
    let pos = outer.first()?.get("tuple")?.as_array()?;
    Some(FormationProbe {
        x: pos.first()?.as_f64()?,
        y: pos.get(1)?.as_f64()?,
        z: pos.get(2)?.as_f64()?,
        range: outer.get(1)?.as_f64()?,
    })
}

fn formations_from_settings(root: &Value) -> Vec<ProbeFormation> {
    let Some(entries) = stored_entries(root) else {
        return Vec::new();
    };

    let mut formations: Vec<ProbeFormation> = entries
        .iter()
        .filter_map(|(key, value)| {
            let id = strip_type_prefix(key).parse::<i64>().ok()?;
            // Negative IDs are client-internal scratch state (e.g. -4 "tempFormation"
            // holding the currently launched probe positions), not user formations
            if id < 0 {
                return None;
            }
            let tuple = value.get("tuple")?.as_array()?;
            let name = tuple.first()?.as_str()?;
            let probes = tuple
                .get(1)
                .and_then(|v| v.as_array())
                .map(|list| list.iter().filter_map(parse_probe).collect())
                .unwrap_or_default();
            Some(ProbeFormation {
                id,
                name: strip_type_prefix(name).to_string(),
                probes,
            })
        })
        .collect();

    formations.sort_by_key(|f| f.id);
    formations
}

fn formations_into_settings(root: &mut Value, formations: &[ProbeFormation]) -> Result<(), String> {
    // Carry over client-internal entries (negative IDs) untouched
    let mut entries: serde_json::Map<String, Value> = stored_entries(root)
        .map(|existing| {
            existing
                .iter()
                .filter(|(key, _)| strip_type_prefix(key).parse::<i64>().unwrap_or(0) < 0)
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
        .unwrap_or_default();

    for formation in formations {
        let probes: Vec<Value> = formation
            .probes
            .iter()
            .map(|p| json!({"tuple": [{"tuple": [p.x, p.y, p.z]}, p.range]}))
            .collect();
        entries.insert(
            format!("int:{}", formation.id),
            json!({"tuple": [format!("utf8:{}", formation.name), probes]}),
        );
    }
    let timestamp = format!("long:{}", filetime_now());

    let ui_map = root
        .as_object_mut()
        .ok_or("Settings file has an unexpected structure")?
        .entry(UI_KEY.to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Settings 'ui' section has an unexpected structure")?;

    ui_map.insert(
        FORMATIONS_KEY.to_string(),
        json!({"tuple": [timestamp, entries]}),
    );

    // Keep the selected-formation pointer valid after edits
    if let Some(selected) = ui_map
        .get_mut(SELECTED_FORMATION_KEY)
        .and_then(|v| v.get_mut("tuple"))
        .and_then(|t| t.as_array_mut())
    {
        let current = selected.get(1).and_then(|v| v.as_i64());
        let still_valid = current.is_some_and(|id| formations.iter().any(|f| f.id == id));
        if !still_valid && let Some(slot) = selected.get_mut(1) {
            *slot = json!(formations.first().map(|f| f.id).unwrap_or(0));
        }
    }

    Ok(())
}

pub fn read_probe_formations(path: &Path) -> Result<Vec<ProbeFormation>, String> {
    let (json, _) = load_settings_json(path)?;
    Ok(formations_from_settings(&json))
}

pub fn write_probe_formations(
    path: &Path,
    formations: &[ProbeFormation],
    backup: bool,
) -> Result<(), String> {
    if !path.exists() {
        return Err("Settings file does not exist".into());
    }
    if backup {
        auto_backup(path, "pre-formation-edit")?;
    }

    let (mut json, had_crc) = load_settings_json(path)?;
    formations_into_settings(&mut json, formations)?;
    let bytes = encode_settings_json(&json, had_crc)?;
    fs::write(path, bytes).map_err(|e| format!("Failed to write settings file: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_formations() -> Vec<ProbeFormation> {
        vec![ProbeFormation {
            id: 0,
            name: "pinpoint".to_string(),
            probes: vec![
                FormationProbe {
                    x: 250_000.0,
                    y: 0.0,
                    z: 0.0,
                    range: 37_399_467_675.0,
                },
                FormationProbe {
                    x: 0.0,
                    y: -500_000.0,
                    z: 0.0,
                    range: 37_399_467_675.0,
                },
            ],
        }]
    }

    #[test]
    fn formations_round_trip_through_marshal() {
        let dir = std::env::temp_dir().join("eve-wrench-formation-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core_user_12345.dat");

        // Minimal settings file with an unrelated key that must survive and a
        // client-internal temp formation that must be hidden but preserved
        let initial = json!({
            "bytes:ui": {
                "bytes:missilesEnabled": {"tuple": ["long:134280801871504062", true]},
                "bytes:probescanning.selectedFormationID": {"tuple": ["long:134280801860500867", 7]},
                "bytes:probescanning.customFormations": {"tuple": ["long:134280801871504062", {
                    "int:-4": {"tuple": ["bytes:tempFormation", []]},
                }]},
            }
        });
        fs::write(&path, encode_settings_json(&initial, false).unwrap()).unwrap();

        assert!(read_probe_formations(&path).unwrap().is_empty());

        write_probe_formations(&path, &sample_formations(), false).unwrap();

        let read_back = read_probe_formations(&path).unwrap();
        assert_eq!(read_back.len(), 1);
        assert_eq!(read_back[0].name, "pinpoint");
        assert_eq!(read_back[0].probes.len(), 2);
        assert_eq!(read_back[0].probes[0].x, 250_000.0);
        assert_eq!(read_back[0].probes[1].y, -500_000.0);

        // Unrelated key untouched, selected id repaired to a valid one
        let (json, _) = load_settings_json(&path).unwrap();
        let ui = json.get(UI_KEY).unwrap();
        assert_eq!(ui["bytes:missilesEnabled"]["tuple"][1], json!(true));
        assert_eq!(ui[SELECTED_FORMATION_KEY]["tuple"][1], json!(0));

        // Client-internal temp formation preserved alongside user formations
        let stored = ui[FORMATIONS_KEY]["tuple"][1].as_object().unwrap();
        assert!(stored.contains_key("int:-4"));
        assert!(stored.contains_key("int:0"));

        fs::remove_dir_all(&dir).ok();
    }
}
