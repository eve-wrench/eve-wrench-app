//! The Phobos-layout dump the refine fitting engine reads, converted from
//! CCP's JSONL export (taken from SheetStorm's SDE import). Keymap files are JSON objects keyed by stringified ids, one
//! per SDE file, in the `fsd_built`, `fsd_lite` and `phobos` directories
//! refine's filesystem data handler expects.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use serde_json::{Map, Value, json};

/// Attribute-id pairs (skill type id, required level) EVE dogma uses to
/// express skill requirements on a type.
const SKILL_REQUIREMENT_ATTR_PAIRS: [(i64, i64); 6] = [
    (182, 277),
    (183, 278),
    (184, 279),
    (1285, 1286),
    (1289, 1287),
    (1290, 1288),
];

/// Also read by the dump, shared with the reference import.
pub const TYPE_DOGMA_FILE: &str = "typeDogma.jsonl";

/// Record counts per output file, for the import's log line.
#[derive(Debug, Default, PartialEq)]
pub struct DumpSummary {
    pub client_build: u64,
    pub types: u64,
    pub groups: u64,
    pub type_lists: u64,
    pub attributes: u64,
    pub effects: u64,
    pub mutaplasmids: u64,
    pub buff_collections: u64,
    pub type_dogma: u64,
    pub required_skills: u64,
}

/// Writes the whole dump from the extracted SDE directory into `out`.
pub fn write_dump(sde: &Path, out: &Path) -> io::Result<DumpSummary> {
    let fsd_built = out.join("fsd_built");
    let fsd_lite = out.join("fsd_lite");
    let phobos = out.join("phobos");
    for dir in [&fsd_built, &fsd_lite, &phobos] {
        fs::create_dir_all(dir)?;
    }

    let mut summary = DumpSummary {
        client_build: write_metadata(&sde.join("_sde.jsonl"), &phobos.join("metadata.json"))?,
        ..DumpSummary::default()
    };

    summary.types = convert_keymap(
        &sde.join("types.jsonl"),
        &fsd_built.join("types.json"),
        convert_type,
    )?;
    summary.groups = convert_keymap(
        &sde.join("groups.jsonl"),
        &fsd_built.join("groups.json"),
        convert_group,
    )?;
    summary.type_lists = convert_keymap(
        &sde.join("typeLists.jsonl"),
        &fsd_built.join("typelist.json"),
        convert_type_list,
    )?;
    summary.attributes = convert_keymap(
        &sde.join("dogmaAttributes.jsonl"),
        &fsd_built.join("dogmaattributes.json"),
        convert_attribute,
    )?;
    summary.effects = convert_keymap(
        &sde.join("dogmaEffects.jsonl"),
        &fsd_built.join("dogmaeffects.json"),
        convert_effect,
    )?;
    summary.mutaplasmids = convert_keymap(
        &sde.join("dynamicItemAttributes.jsonl"),
        &fsd_built.join("dynamicitemattributes.json"),
        convert_mutaplasmid,
    )?;
    summary.buff_collections = convert_keymap(
        &sde.join("dbuffCollections.jsonl"),
        &fsd_lite.join("dbuffcollections.json"),
        convert_buff_collection,
    )?;
    (summary.type_dogma, summary.required_skills) = convert_type_dogma_file(
        &sde.join(TYPE_DOGMA_FILE),
        &fsd_built.join("typedogma.json"),
        &fsd_built.join("requiredskillsfortypes.json"),
    )?;

    // Not present in the JSONL SDE: space components (structures), fighter
    // abilities. Refine requires the files to exist; empty maps are valid.
    for path in [
        fsd_built.join("spacecomponentsbytype.json"),
        fsd_lite.join("fighterabilities.json"),
        fsd_lite.join("fighterabilitiesbytype.json"),
    ] {
        fs::write(&path, b"{}")?;
    }

    Ok(summary)
}

/// The build a dump was written from, read back from its metadata; `None`
/// when there is no complete dump.
pub fn dump_build(out: &Path) -> Option<u64> {
    let metadata: Value =
        serde_json::from_slice(&fs::read(out.join("phobos/metadata.json")).ok()?).ok()?;
    metadata
        .as_array()?
        .iter()
        .find(|field| field["field_name"] == "client_build")?
        .get("field_value")?
        .as_u64()
}

fn jsonl_records(
    path: &Path,
) -> io::Result<impl Iterator<Item = io::Result<(i64, Map<String, Value>)>>> {
    let file = File::open(path)?;
    let path = path.to_path_buf();
    Ok(BufReader::new(file)
        .lines()
        .filter(|line| !matches!(line, Ok(line) if line.trim().is_empty()))
        .map(move |line| {
            let record: Value = serde_json::from_str(&line?).map_err(io::Error::other)?;
            let Value::Object(record) = record else {
                return Err(io::Error::other(format!(
                    "non-object record in {}",
                    path.display()
                )));
            };
            let Some(key) = record_key(&record) else {
                return Err(io::Error::other(format!(
                    "record without integer _key in {}",
                    path.display()
                )));
            };
            Ok((key, record))
        }))
}

/// Streams a JSONL file into a keymap file, transforming each record.
fn convert_keymap(
    source: &Path,
    target: &Path,
    convert_record: impl Fn(&Map<String, Value>) -> Value,
) -> io::Result<u64> {
    let mut writer = KeymapWriter::create(target)?;
    let mut count = 0u64;
    for record in jsonl_records(source)? {
        let (key, record) = record?;
        writer.entry(key, &convert_record(&record))?;
        count += 1;
    }
    writer.finish()?;
    Ok(count)
}

/// `typeDogma.jsonl` produces two outputs in one pass: the converted dogma
/// per type and the derived skill requirements per type.
fn convert_type_dogma_file(
    source: &Path,
    dogma_target: &Path,
    skills_target: &Path,
) -> io::Result<(u64, u64)> {
    let mut dogma_writer = KeymapWriter::create(dogma_target)?;
    let mut skills_writer = KeymapWriter::create(skills_target)?;
    let mut dogma_count = 0u64;
    let mut skills_count = 0u64;
    for record in jsonl_records(source)? {
        let (key, record) = record?;
        let (converted, requirements) = convert_type_dogma(&record);
        dogma_writer.entry(key, &converted)?;
        dogma_count += 1;
        if !requirements.is_empty() {
            skills_writer.entry(key, &Value::Object(requirements))?;
            skills_count += 1;
        }
    }
    dogma_writer.finish()?;
    skills_writer.finish()?;
    Ok((dogma_count, skills_count))
}

/// `phobos/metadata.json`: the client build refine keys its caches by.
fn write_metadata(sde_meta: &Path, target: &Path) -> io::Result<u64> {
    let line = BufReader::new(File::open(sde_meta)?)
        .lines()
        .next()
        .ok_or_else(|| io::Error::other("_sde.jsonl is empty"))??;
    let record: Value = serde_json::from_str(&line).map_err(io::Error::other)?;
    let build_number = record
        .get("buildNumber")
        .and_then(Value::as_u64)
        .ok_or_else(|| io::Error::other("_sde.jsonl has no buildNumber"))?;

    let metadata = json!([{ "field_name": "client_build", "field_value": build_number }]);
    fs::write(target, serde_json::to_vec(&metadata)?)?;
    Ok(build_number)
}

struct KeymapWriter {
    writer: BufWriter<File>,
    first: bool,
}

impl KeymapWriter {
    fn create(path: &Path) -> io::Result<Self> {
        let mut writer = BufWriter::new(File::create(path)?);
        writer.write_all(b"{")?;
        Ok(Self {
            writer,
            first: true,
        })
    }

    fn entry(&mut self, key: i64, value: &Value) -> io::Result<()> {
        if !self.first {
            self.writer.write_all(b",")?;
        }
        self.first = false;
        write!(self.writer, "\"{key}\":")?;
        serde_json::to_writer(&mut self.writer, value)?;
        Ok(())
    }

    fn finish(mut self) -> io::Result<()> {
        self.writer.write_all(b"}")?;
        self.writer.flush()
    }
}

fn record_key(record: &Map<String, Value>) -> Option<i64> {
    record.get("_key")?.as_i64()
}

fn number_or_zero(record: &Map<String, Value>, field: &str) -> Value {
    match record.get(field) {
        Some(value) if value.is_number() => value.clone(),
        _ => json!(0.0),
    }
}

/// Phobos encodes most dogma booleans as 0/1 integers.
fn bool_as_int(record: &Map<String, Value>, field: &str, default: bool) -> Value {
    let value = record
        .get(field)
        .and_then(Value::as_bool)
        .unwrap_or(default);
    json!(value as i64)
}

fn copy_present_fields(record: &Map<String, Value>, fields: &[&str], out: &mut Map<String, Value>) {
    for field in fields {
        if let Some(value) = record.get(*field)
            && !value.is_null()
        {
            out.insert((*field).to_string(), value.clone());
        }
    }
}

/// `fsd_built/types.json`: the five physical properties refine reads per type.
pub fn convert_type(record: &Map<String, Value>) -> Value {
    json!({
        "groupID": record.get("groupID").cloned().unwrap_or(json!(0)),
        "capacity": number_or_zero(record, "capacity"),
        "mass": number_or_zero(record, "mass"),
        "radius": number_or_zero(record, "radius"),
        "volume": number_or_zero(record, "volume"),
    })
}

/// `fsd_built/groups.json`
pub fn convert_group(record: &Map<String, Value>) -> Value {
    json!({ "categoryID": record.get("categoryID").cloned().unwrap_or(json!(0)) })
}

/// `fsd_built/typelist.json`: six optional include/exclude id arrays.
pub fn convert_type_list(record: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    copy_present_fields(
        record,
        &[
            "includedTypeIDs",
            "includedGroupIDs",
            "includedCategoryIDs",
            "excludedTypeIDs",
            "excludedGroupIDs",
            "excludedCategoryIDs",
        ],
        &mut out,
    );
    Value::Object(out)
}

/// `fsd_built/dogmaattributes.json`
pub fn convert_attribute(record: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    out.insert("stackable".into(), bool_as_int(record, "stackable", true));
    out.insert("highIsGood".into(), bool_as_int(record, "highIsGood", true));
    out.insert(
        "defaultValue".into(),
        number_or_zero(record, "defaultValue"),
    );
    copy_present_fields(
        record,
        &["minAttributeID", "maxAttributeID", "unitID"],
        &mut out,
    );
    Value::Object(out)
}

/// `fsd_built/typedogma.json`. Returns the converted record plus the derived
/// skill-requirement map (skill type id -> level) for
/// `requiredskillsfortypes.json`.
pub fn convert_type_dogma(record: &Map<String, Value>) -> (Value, Map<String, Value>) {
    let attributes = record
        .get("dogmaAttributes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let effects: Vec<Value> = record
        .get("dogmaEffects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .map(|effect| {
            json!({
                "effectID": effect.get("effectID").cloned().unwrap_or(json!(0)),
                "isDefault": bool_as_int(effect, "isDefault", false),
            })
        })
        .collect();

    let converted = json!({ "dogmaAttributes": attributes, "dogmaEffects": effects });

    (converted, derive_skill_requirements(&attributes))
}

fn derive_skill_requirements(attributes: &[Value]) -> Map<String, Value> {
    let values: Map<String, Value> = attributes
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|attr| {
            let id = attr.get("attributeID")?.as_i64()?;
            Some((id.to_string(), attr.get("value")?.clone()))
        })
        .collect();

    let mut requirements = Map::new();
    for (skill_attr, level_attr) in SKILL_REQUIREMENT_ATTR_PAIRS {
        let skill_type_id = values.get(&skill_attr.to_string()).and_then(number_as_i64);
        let level = values.get(&level_attr.to_string()).and_then(number_as_i64);
        if let (Some(skill_type_id), Some(level)) = (skill_type_id, level)
            && skill_type_id > 0
        {
            requirements.insert(skill_type_id.to_string(), json!(level));
        }
    }
    requirements
}

fn number_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|float| float as i64))
}

/// `fsd_built/dogmaeffects.json`. Modifier args must be scalar; refine skips
/// the whole effect if any arg value is an array or object.
pub fn convert_effect(record: &Map<String, Value>) -> Value {
    let mut out = Map::new();
    out.insert(
        "effectCategory".into(),
        record.get("effectCategoryID").cloned().unwrap_or(json!(0)),
    );
    out.insert(
        "isAssistance".into(),
        bool_as_int(record, "isAssistance", false),
    );
    out.insert(
        "isOffensive".into(),
        bool_as_int(record, "isOffensive", false),
    );
    copy_present_fields(
        record,
        &[
            "dischargeAttributeID",
            "durationAttributeID",
            "rangeAttributeID",
            "falloffAttributeID",
            "trackingSpeedAttributeID",
            "fittingUsageChanceAttributeID",
            "resistanceAttributeID",
        ],
        &mut out,
    );

    let modifiers: Vec<Value> = record
        .get("modifierInfo")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|modifier| modifier.get("func").is_some_and(Value::is_string))
        .map(|modifier| {
            let scalar_args: Map<String, Value> = modifier
                .iter()
                .filter(|(_, value)| !value.is_array() && !value.is_object())
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect();
            Value::Object(scalar_args)
        })
        .collect();
    out.insert("modifierInfo".into(), Value::Array(modifiers));

    Value::Object(out)
}

/// `fsd_built/dynamicitemattributes.json`: `attributeIDs` comes as an array
/// of `{_key, min, max}` in the SDE but Phobos keys it by attribute id.
pub fn convert_mutaplasmid(record: &Map<String, Value>) -> Value {
    let attribute_ranges: Map<String, Value> = record
        .get("attributeIDs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|range| {
            let attribute_id = range.get("_key")?.as_i64()?;
            let min = range.get("min")?.clone();
            let max = range.get("max")?.clone();
            Some((attribute_id.to_string(), json!({ "min": min, "max": max })))
        })
        .collect();

    json!({
        "inputOutputMapping": record.get("inputOutputMapping").cloned().unwrap_or(json!([])),
        "attributeIDs": attribute_ranges,
    })
}

/// `fsd_lite/dbuffcollections.json`: the SDE record already matches what
/// refine reads; unknown extra fields are ignored by its parser.
pub fn convert_buff_collection(record: &Map<String, Value>) -> Value {
    let mut out = record.clone();
    out.remove("_key");
    Value::Object(out)
}
