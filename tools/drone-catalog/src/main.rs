//! Generates the drone group presets EVE Wrench offers: one group per drone
//! family, with max-skill stats from DarkFenX's refine engine that the app
//! turns into group names.
//!
//! Usage: `cargo run --release` from this directory. Writes
//! `crates/eve-wrench-core/data/drone_groups.json`; downloads and the engine
//! cache live in this tool's `target/` directory.
//!
//! Families: every T2 drone starts one, named after it without "II", taking
//! the T1, civilian and faction drones of the same group and size whose name
//! contains that base name; its stats are the T2 drone's. EWAR, neut and web
//! drones (and hybrid drones carrying the same effect) collapse into one group
//! per effect and size. Whatever is left keeps a group of its own.

mod phobos;
mod sim;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};

use crate::sim::Simulated;

const SDE_BASE: &str = "https://developers.eveonline.com/static-data/tranquility";
const SKILL_CATEGORY: i64 = 16;
const DRONE_CATEGORY: i64 = 18;
const META_TECH_II: i64 = 2;
const META_ABYSSAL: i64 = 15;

const COMBAT_GROUP: i64 = 100;
const MINING_GROUP: i64 = 101;
const LOGISTIC_GROUP: i64 = 640;
const SALVAGE_GROUP: i64 = 1159;
const HYBRID_GROUP: i64 = 5239;

// Drones V puts five in space; the biggest drone bays carry 125 Mbit/s.
const MAX_DRONES: f64 = 5.0;
const MAX_BANDWIDTH: f64 = 125.0;

const MAX_VELOCITY: i64 = 37;
const CYCLE_DURATION: i64 = 73;
const MINING_AMOUNT: i64 = 77;
const SHIELD_BONUS: i64 = 68;
const STRUCTURE_REPAIR: i64 = 83;
const ARMOR_REPAIR: i64 = 84;
const OPTIMAL: i64 = 54;
const FALLOFF: i64 = 158;
const BANDWIDTH_USED: i64 = 1272;

// EWAR effect code in drone names, preset name, strength attribute, unit
const EFFECTS: [(&str, &str, &str, &str); 6] = [
    ("EC", "ECM", "scanRadarStrengthBonus", " jam"),
    ("TD", "Tracking Disruptor", "maxRangeBonus", "%"),
    ("SD", "Sensor Damp", "maxTargetRangeBonus", "%"),
    ("TP", "Target Painter", "signatureRadiusBonus", "% sig"),
    ("EV", "Energy Neutralizer", "energyNeutralizerAmount", " GJ"),
    ("SW", "Stasis Web", "speedFactor", "% speed"),
];

const DAMAGE_TYPES: [&str; 4] = ["EM", "Therm", "Kin", "Exp"];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Size {
    Light,
    Medium,
    Heavy,
    Sentry,
    Other,
}

struct Drone {
    type_id: i64,
    name: String,
    group_id: i64,
    meta_group: Option<i64>,
    size: Size,
    sim: Simulated,
}

impl Drone {
    fn attr(&self, id: i64) -> f64 {
        self.sim.attrs.get(&id).copied().unwrap_or(0.0)
    }

    fn total_dps(&self) -> f64 {
        self.sim.dps.iter().sum()
    }
}

#[derive(Serialize)]
struct Preset {
    id: String,
    name: String,
    category: &'static str,
    group_id: i64,
    // The drone whose icon represents the group
    type_id: i64,
    members: Vec<String>,
    stats: Stats,
    #[serde(skip)]
    size: Size,
}

// Raw max-skill numbers; the app turns them into group names with the
// player's naming pattern. Absent values are omitted.
#[derive(Serialize, Default)]
struct Stats {
    #[serde(skip_serializing_if = "Option::is_none")]
    damage: Option<String>,
    // DPS of a full flight: five drones, or as many as fit in the bandwidth
    #[serde(skip_serializing_if = "Option::is_none")]
    dps: Option<f64>,
    // m/s; absent for sentries
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f64>,
    // Meters, for drones that fire from where they are (sentries)
    #[serde(skip_serializing_if = "Option::is_none")]
    optimal: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    falloff: Option<f64>,
    // Repair, yield or EWAR strength, already formatted
    #[serde(skip_serializing_if = "Option::is_none")]
    effect: Option<String>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let work = manifest.join("target/catalog");
    let out = manifest.join("../../crates/eve-wrench-core/data/drone_groups.json");

    let (build, sde) = fetch_sde(&work);
    println!("SDE build {build}");

    let data = work.join(format!("refine-data-{build}"));
    if phobos::dump_build(&data) != Some(build) {
        phobos::write_dump(&sde, &data).expect("Phobos dump");
    }

    let categories: HashMap<i64, i64> = jsonl(&sde.join("groups.jsonl"))
        .map(|g| (key(&g), g["categoryID"].as_i64().unwrap_or(0)))
        .collect();
    let types: Vec<Value> = jsonl(&sde.join("types.jsonl"))
        .filter(|t| t["published"].as_bool() == Some(true))
        .collect();
    let in_category = |category: i64| -> Vec<&Value> {
        types
            .iter()
            .filter(|t| categories.get(&t["groupID"].as_i64().unwrap_or(0)) == Some(&category))
            .collect()
    };
    let skills: Vec<i64> = in_category(SKILL_CATEGORY).into_iter().map(key).collect();
    let drone_types: Vec<&Value> = in_category(DRONE_CATEGORY)
        .into_iter()
        // Mutated drones carry their stats per item, not per type
        .filter(|t| t["metaGroupID"].as_i64() != Some(META_ABYSSAL))
        .collect();
    let attr_ids: HashMap<String, i64> = jsonl(&sde.join("dogmaAttributes.jsonl"))
        .map(|a| (a["name"].as_str().unwrap_or("").to_string(), key(&a)))
        .collect();

    let ids: Vec<i64> = drone_types.iter().map(|t| key(t)).collect();
    let mut simulated = sim::simulate(&data, &work.join("refine-cache"), &skills, &ids).await;
    println!(
        "simulated {} drones with {} skills at V",
        ids.len(),
        skills.len()
    );

    let drones: Vec<Drone> = drone_types
        .iter()
        .map(|t| {
            let sim = simulated.remove(&key(t)).expect("simulated drone");
            let speed = sim.attrs.get(&MAX_VELOCITY).copied().unwrap_or(0.0);
            let bandwidth = sim.attrs.get(&BANDWIDTH_USED).copied().unwrap_or(0.0);
            Drone {
                type_id: key(t),
                name: t["name"]["en"].as_str().unwrap_or("").to_string(),
                group_id: t["groupID"].as_i64().unwrap_or(0),
                meta_group: t["metaGroupID"].as_i64(),
                size: size(speed, bandwidth),
                sim,
            }
        })
        .collect();

    let presets = build_presets(&drones, &attr_ids);
    // Bump the schema together with CATALOG_SCHEMA in the app when the format changes
    let file = json!({ "schema": 1, "sde_build": build, "presets": presets });
    fs::write(&out, serde_json::to_string_pretty(&file).unwrap() + "\n").expect("write catalog");
    println!("wrote {} presets to {}", presets.len(), out.display());
}

fn build_presets(drones: &[Drone], attr_ids: &HashMap<String, i64>) -> Vec<Preset> {
    let mut presets = Vec::new();

    for t2 in drones.iter().filter(|d| d.meta_group == Some(META_TECH_II)) {
        let base = t2.name.trim_end_matches(" II");
        let members = drones
            .iter()
            .filter(|d| d.group_id == t2.group_id && d.size == t2.size && d.name.contains(base))
            .map(|d| d.name.clone())
            .collect();
        presets.push(Preset {
            id: slug(base),
            name: base.to_string(),
            category: category(t2),
            group_id: t2.group_id,
            type_id: t2.type_id,
            members,
            stats: stats(t2),
            size: t2.size,
        });
    }

    // One group per effect and size: a ship often carries light and heavy
    // EWAR drones side by side
    for (code, name, attr, unit) in EFFECTS {
        for (size, size_name) in [
            (Size::Light, "Light"),
            (Size::Medium, "Medium"),
            (Size::Heavy, "Heavy"),
        ] {
            let carriers: Vec<&Drone> = drones
                .iter()
                .filter(|d| d.size == size && effect_code(&d.name) == Some(code))
                .collect();
            // Strength and speed from the plain T1 EWAR drone; hybrid drones
            // carry the same strength
            let Some(plain) = carriers
                .iter()
                .find(|d| d.group_id != HYBRID_GROUP && d.meta_group.is_none_or(|meta| meta == 1))
            else {
                continue;
            };
            let value = plain.attr(attr_ids[attr]);
            let strength = if value > 0.0 && unit.starts_with('%') {
                format!("+{}", number(value))
            } else {
                number(value)
            };
            let preset_name = format!("{size_name} {name}");
            presets.push(Preset {
                id: slug(&preset_name),
                name: preset_name,
                category: "ewar",
                group_id: plain.group_id,
                type_id: plain.type_id,
                members: carriers.iter().map(|d| d.name.clone()).collect(),
                stats: Stats {
                    speed: Some(plain.attr(MAX_VELOCITY).round()),
                    effect: Some(format!("{strength}{unit}")),
                    ..Stats::default()
                },
                size,
            });
        }
    }

    let covered: HashSet<String> = presets
        .iter()
        .flat_map(|p| p.members.iter().cloned())
        .collect();
    for drone in drones.iter().filter(|d| !covered.contains(&d.name)) {
        presets.push(Preset {
            id: slug(&drone.name),
            name: drone.name.clone(),
            category: if drone.group_id == COMBAT_GROUP {
                "special"
            } else {
                category(drone)
            },
            group_id: drone.group_id,
            type_id: drone.type_id,
            members: vec![drone.name.clone()],
            stats: stats(drone),
            size: drone.size,
        });
    }

    let order = [
        "light",
        "medium",
        "heavy",
        "sentry",
        "special",
        "ewar",
        "logistics",
        "industry",
    ];
    // EWAR stays in effect order, so each effect's sizes sit side by side
    presets.sort_by_key(|p| {
        let category = order.iter().position(|c| *c == p.category);
        let size = if p.category == "ewar" {
            Size::Other
        } else {
            p.size
        };
        (category.unwrap_or(order.len()), size)
    });
    let mut seen = HashSet::new();
    for preset in &presets {
        assert!(
            seen.insert(preset.id.clone()),
            "duplicate preset id {}",
            preset.id
        );
    }
    presets
}

fn size(speed: f64, bandwidth: f64) -> Size {
    // Sentries carry a 1.25e-05 placeholder speed rather than zero
    if speed < 1.0 {
        return Size::Sentry;
    }
    match bandwidth as i64 {
        5 => Size::Light,
        10 => Size::Medium,
        25 => Size::Heavy,
        _ => Size::Other,
    }
}

fn category(drone: &Drone) -> &'static str {
    match drone.group_id {
        LOGISTIC_GROUP => "logistics",
        MINING_GROUP | SALVAGE_GROUP => "industry",
        _ => match drone.size {
            Size::Light => "light",
            Size::Medium => "medium",
            Size::Heavy => "heavy",
            Size::Sentry => "sentry",
            Size::Other => "special",
        },
    }
}

// "Hornet EC-300" and "Inshore EC-300-I" both carry "EC"
fn effect_code(name: &str) -> Option<&'static str> {
    name.split_whitespace().find_map(|word| {
        EFFECTS.iter().map(|(code, ..)| *code).find(|code| {
            word.strip_prefix(code)
                .and_then(|rest| rest.strip_prefix('-'))
                .is_some_and(|rest| rest.starts_with(['3', '6', '9']))
        })
    })
}

fn stats(drone: &Drone) -> Stats {
    let total = drone.total_dps();
    let mut stats = Stats::default();
    if drone.size == Size::Sentry {
        stats.optimal = Some(drone.attr(OPTIMAL).round());
        stats.falloff = Some(drone.attr(FALLOFF).round()).filter(|f| *f > 0.0);
    } else {
        stats.speed = Some(drone.attr(MAX_VELOCITY).round());
    }

    if total > 0.0 {
        let mut split: Vec<(usize, f64)> = (0..4)
            .map(|i| (i, drone.sim.dps[i] / total))
            .filter(|(_, share)| *share >= 0.2)
            .collect();
        split.sort_by(|a, b| b.1.total_cmp(&a.1));
        stats.damage = Some(if split.len() == 4 {
            "Omni".to_string()
        } else {
            split
                .iter()
                .map(|(i, _)| DAMAGE_TYPES[*i])
                .collect::<Vec<_>>()
                .join("/")
        });
        let flight = MAX_DRONES.min((MAX_BANDWIDTH / drone.attr(BANDWIDTH_USED)).floor());
        stats.dps = Some((flight * total * 10.0).round() / 10.0);
    } else if drone.group_id == LOGISTIC_GROUP {
        let repair = [SHIELD_BONUS, ARMOR_REPAIR, STRUCTURE_REPAIR]
            .into_iter()
            .map(|id| drone.attr(id))
            .find(|v| *v > 0.0)
            .unwrap_or(0.0);
        let cycle = number(drone.attr(CYCLE_DURATION) / 1000.0);
        stats.effect = Some(format!("{}hp/{cycle}s", number(repair)));
    } else if drone.group_id == MINING_GROUP {
        let cycle = drone.attr(CYCLE_DURATION).max(1.0);
        stats.effect = Some(format!(
            "{:.1}m3/s",
            drone.attr(MINING_AMOUNT) / cycle * 1000.0
        ));
    }
    stats
}

fn number(value: f64) -> String {
    let rounded = (value * 100.0).round() / 100.0;
    format!("{rounded}")
}

fn slug(name: &str) -> String {
    name.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn key(record: &Value) -> i64 {
    record["_key"].as_i64().unwrap_or(0)
}

fn jsonl(path: &Path) -> impl Iterator<Item = Value> {
    let file = File::open(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    BufReader::new(file)
        .lines()
        .map(|line| serde_json::from_str(&line.expect("line")).expect("jsonl record"))
}

// Downloads and unpacks the latest SDE once per build.
fn fetch_sde(work: &Path) -> (u64, PathBuf) {
    let latest: Value = ureq::get(format!("{SDE_BASE}/latest.jsonl"))
        .call()
        .and_then(|mut r| r.body_mut().read_json())
        .expect("latest SDE build");
    let build = latest["buildNumber"].as_u64().expect("build number");

    let dir = work.join(format!("sde-{build}"));
    if dir.join("types.jsonl").exists() {
        return (build, dir);
    }
    fs::create_dir_all(&dir).expect("SDE dir");
    let bytes = ureq::get(format!(
        "{SDE_BASE}/eve-online-static-data-{build}-jsonl.zip"
    ))
    .call()
    .and_then(|mut r| r.body_mut().with_config().limit(1 << 30).read_to_vec())
    .expect("SDE download");
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("SDE zip");
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("zip entry");
        let Some(name) = entry
            .enclosed_name()
            .and_then(|p| p.file_name().map(PathBuf::from))
        else {
            continue;
        };
        if name.extension().is_some_and(|ext| ext == "jsonl") {
            let mut file = File::create(dir.join(name)).expect("SDE file");
            std::io::copy(&mut entry, &mut file).expect("unzip");
        }
    }
    (build, dir)
}
