//! Generates the catalog of confirmation dialogs EVE Wrench can suppress: every
//! client dialog with a "don't show again" box, its title and text in English
//! and Chinese, and the answer the client expects back when it is suppressed.
//!
//! Usage: `cargo run --release` from this directory. Writes
//! `crates/eve-wrench-core/data/confirmations.json`; downloads live in this
//! tool's `target/` directory.
//!
//! Everything comes from CCP's client CDN, no game install needed: the dialog
//! definitions (`res:/staticdata/dialogs.static`), the localization pickles and
//! the client code (`code.ccp`). A suppressed dialog returns the stored button
//! to its caller, so the answer has to match the buttons the caller shows it
//! with (Yes for Yes/No, OK for OK/Cancel); `bytecode` finds those.

mod bytecode;
mod dialogs;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Value, json};
use serde_pickle::{HashableValue, Value as Pickle};

use crate::bytecode::Buttons;
use crate::dialogs::{Dialog, DialogType, Suppressable};

const BINARIES: &str = "https://binaries.eveonline.com";
const RESOURCES: &str = "https://resources.eveonline.com";

// Windows message box results, which is what the client stores
const ID_OK: u8 = 1;
const ID_YES: u8 = 6;
const ID_NO: u8 = 7;

const LANGUAGES: [(&str, &str); 2] = [("en", "en-us"), ("zh", "zh")];

#[derive(Serialize)]
struct Entry {
    key: String,
    kind: &'static str,
    // Stored when suppressing; None when the buttons are unknown, so the app
    // only offers to clear an existing suppression
    answer: Option<u8>,
    title: BTreeMap<&'static str, String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    body: BTreeMap<&'static str, String>,
}

fn main() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = manifest.join("../../crates/eve-wrench-core/data/confirmations.json");

    let client: Value = ureq::get(format!("{BINARIES}/eveclient_TQ.json"))
        .call()
        .and_then(|mut r| r.body_mut().read_json())
        .expect("client build");
    let build: u64 = client["build"]
        .as_str()
        .and_then(|b| b.parse().ok())
        .expect("build number");
    println!("client build {build}");
    let work = manifest.join(format!("target/client-{build}"));
    fs::create_dir_all(&work).expect("work dir");

    let app_index = text(&fetch(
        &work,
        "app-index.txt",
        &format!("{BINARIES}/eveonline_{build}.txt"),
    ));
    let res_index = text(&fetch(
        &work,
        "resfileindex.txt",
        &format!(
            "{BINARIES}/{}",
            index_path(&app_index, "app:/resfileindex.txt")
        ),
    ));
    let resource = |name: &str| {
        let path = index_path(&res_index, name);
        fetch(
            &work,
            name.rsplit('/').next().expect("resource name"),
            &format!("{RESOURCES}/{path}"),
        )
    };

    let dialogs = dialogs::parse(&resource("res:/staticdata/dialogs.static"));
    let suppressable: Vec<&Dialog> = dialogs
        .iter()
        .filter(|d| d.suppressable != Suppressable::No)
        .collect();

    let texts: Vec<(&str, HashMap<i64, String>)> = LANGUAGES
        .iter()
        .map(|(code, file)| {
            let pickle = resource(&format!(
                "res:/localizationfsd/localization_fsd_{file}.pickle"
            ));
            (*code, localization(&pickle))
        })
        .collect();

    let code = fetch(
        &work,
        "code.ccp",
        &format!("{BINARIES}/{}", index_path(&app_index, "app:/code.ccp")),
    );
    let keys: HashSet<&str> = suppressable.iter().map(|d| d.key.as_str()).collect();
    let buttons = bytecode::scan(&code, &keys);

    let mut entries: Vec<Entry> = suppressable
        .iter()
        .map(|d| {
            let pick = |id: Option<i64>| -> BTreeMap<&'static str, String> {
                let Some(id) = id else {
                    return BTreeMap::new();
                };
                texts
                    .iter()
                    .filter_map(|(code, texts)| {
                        let text = clean(texts.get(&id)?);
                        (!text.is_empty()).then_some((*code, text))
                    })
                    .collect()
            };
            let mut title = pick(d.title);
            title.entry("en").or_insert_with(|| words(&d.key));
            Entry {
                key: d.key.clone(),
                kind: kind(d.kind),
                answer: answer(d, buttons.get(d.key.as_str())),
                title,
                body: pick(d.body),
            }
        })
        .collect();
    entries.sort_by(|a, b| a.key.cmp(&b.key));

    let known = entries.iter().filter(|e| e.answer.is_some()).count();
    // Bump the schema together with CATALOG_SCHEMA in the app when the format changes
    let file = json!({ "schema": 1, "client_build": build, "dialogs": entries });
    fs::write(
        &out,
        serde_json::to_string_pretty(&file).expect("catalog serializes") + "\n",
    )
    .expect("write catalog");
    println!(
        "wrote {} dialogs ({known} with a known answer) to {}",
        entries.len(),
        out.display()
    );
}

fn kind(kind: DialogType) -> &'static str {
    match kind {
        DialogType::Question => "question",
        DialogType::Warning | DialogType::Error | DialogType::Fatal => "warning",
        DialogType::WindowHelp => "welcome",
        _ => "notice",
    }
}

// The button a suppressed dialog hands back to its caller.
fn answer(dialog: &Dialog, buttons: Option<&HashSet<Buttons>>) -> Option<u8> {
    match dialog.suppressable {
        Suppressable::OnlyYes => return Some(ID_YES),
        Suppressable::OnlyNo => return Some(ID_NO),
        _ => {}
    }
    let found: HashSet<u8> = buttons
        .into_iter()
        .flatten()
        .map(|b| match b {
            Buttons::YesNo => ID_YES,
            Buttons::Ok => ID_OK,
        })
        .collect();
    match found.len() {
        1 => found.into_iter().next(),
        // Callers disagree, so no single answer is safe
        2.. => None,
        // Shown with the default OK button
        0 => matches!(
            dialog.kind,
            DialogType::Hint | DialogType::Info | DialogType::Notify | DialogType::WindowHelp
        )
        .then_some(ID_OK),
    }
}

// The client's localization pickle: (language, {message id: (text, ...)}).
fn localization(bytes: &[u8]) -> HashMap<i64, String> {
    let value = serde_pickle::value_from_slice(bytes, Default::default()).expect("pickle");
    let Pickle::Tuple(parts) = value else {
        panic!("unexpected localization pickle");
    };
    let Some(Pickle::Dict(messages)) = parts.into_iter().nth(1) else {
        panic!("unexpected localization pickle");
    };
    messages
        .into_iter()
        .filter_map(|(id, message)| {
            let HashableValue::I64(id) = id else {
                return None;
            };
            let text = match message {
                Pickle::Tuple(fields) | Pickle::List(fields) => fields.into_iter().next()?,
                other => other,
            };
            match text {
                Pickle::String(s) => Some((id, s)),
                Pickle::Bytes(b) => Some((id, String::from_utf8_lossy(&b).into_owned())),
                _ => None,
            }
        })
        .collect()
}

// EVE HTML to plain text: line breaks kept, tags dropped, entities decoded,
// and placeholders the client fills in ("{names}") shown as an ellipsis.
fn clean(text: &str) -> String {
    let text = text
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let mut out = String::with_capacity(text.len());
    let (mut in_tag, mut in_placeholder) = (false, false);
    for c in text.chars() {
        match c {
            '<' if !in_placeholder => in_tag = true,
            '>' if in_tag => in_tag = false,
            '{' if !in_tag => in_placeholder = true,
            '}' if in_placeholder => {
                in_placeholder = false;
                out.push('…');
            }
            _ if !in_tag && !in_placeholder => out.push(c),
            _ => {}
        }
    }
    let out = out
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&");
    out.lines()
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
        .split("\n\n\n")
        .collect::<Vec<_>>()
        .join("\n\n")
        .trim()
        .to_string()
}

// "AskQuitGame" -> "Ask quit game", for dialogs without a title.
fn words(key: &str) -> String {
    let mut out = String::new();
    for (i, c) in key.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out.replace('_', " ")
}

// A file's CDN path from an index line "name,path,md5,size,compressed".
fn index_path<'a>(index: &'a str, name: &str) -> &'a str {
    index
        .lines()
        .find_map(|line| {
            let mut fields = line.split(',');
            (fields.next() == Some(name)).then(|| fields.next())?
        })
        .unwrap_or_else(|| panic!("{name} not in index"))
}

// Downloads a file once per client build. CDN files are gzipped, unless the
// server already sent them with a gzip content encoding that ureq undid.
fn fetch(work: &Path, name: &str, url: &str) -> Vec<u8> {
    let path: PathBuf = work.join(name);
    if let Ok(bytes) = fs::read(&path) {
        return bytes;
    }
    let raw = ureq::get(url)
        .call()
        .and_then(|mut r| r.body_mut().with_config().limit(1 << 30).read_to_vec())
        .unwrap_or_else(|e| panic!("{url}: {e}"));
    let bytes = if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_end(&mut out)
            .unwrap_or_else(|e| panic!("{url}: {e}"));
        out
    } else {
        raw
    };
    fs::write(&path, &bytes).expect("cache download");
    bytes
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}
