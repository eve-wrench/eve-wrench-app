//! Reads `dialogs.static`, an FSD binary dict of dialog key to definition
//! (layout in the client's `dialogs.schema`):
//!
//! - u32 size of the data that follows, then the values back to back
//! - a key footer at the end: u32 count, u32 offset per key (from the footer
//!   start), each pointing at u32 value offset, 12 bytes, u32 length, key
//! - the footer's own size in the last u32
//!
//! A value is u32 dialogID, u8 dialogType, u32 mask of the optional fields
//! present, 8 bytes, one u32 offset per present variable field after the first,
//! then the fields themselves.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialogType {
    Hint,
    Info,
    Notify,
    Question,
    Audio,
    Warning,
    Error,
    Fatal,
    WindowHelp,
    Other,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Suppressable {
    No,
    // Any answer is remembered
    Any,
    OnlyYes,
    OnlyNo,
}

pub struct Dialog {
    pub key: String,
    pub kind: DialogType,
    pub suppressable: Suppressable,
    pub title: Option<i64>,
    pub body: Option<i64>,
}

// Optional fields in mask order: bit, size (None for strings)
const BODY: u32 = 1;
const SUPPRESSABLE: u32 = 2;
const TITLE: u32 = 8;
const OPTIONAL: [(u32, Option<usize>); 6] = [
    (BODY, Some(4)),
    (SUPPRESSABLE, Some(1)),
    (4, Some(1)), // closable
    (TITLE, Some(4)),
    (16, None), // urlAudio
    (32, None), // urlIcon
];

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

pub fn parse(bytes: &[u8]) -> Vec<Dialog> {
    let footer = bytes.len() - 4 - u32_at(bytes, bytes.len() - 4) as usize;
    let count = u32_at(bytes, footer) as usize;
    (0..count)
        .map(|i| {
            let entry = footer + u32_at(bytes, footer + 4 + 4 * i) as usize;
            let value = 4 + u32_at(bytes, entry) as usize;
            let len = u32_at(bytes, entry + 16) as usize;
            let key = String::from_utf8_lossy(&bytes[entry + 20..entry + 20 + len]).into_owned();
            read_dialog(bytes, value, key)
        })
        .collect()
}

fn read_dialog(bytes: &[u8], at: usize, key: String) -> Dialog {
    let kind = match bytes[at + 4] {
        1 => DialogType::Hint,
        2 => DialogType::Info,
        3 => DialogType::Notify,
        4 => DialogType::Question,
        5 => DialogType::Audio,
        6 => DialogType::Warning,
        7 => DialogType::Error,
        8 => DialogType::Fatal,
        9 => DialogType::WindowHelp,
        _ => DialogType::Other,
    };
    let mask = u32_at(bytes, at + 5);
    let present: Vec<(u32, Option<usize>)> = OPTIONAL
        .into_iter()
        .filter(|(bit, _)| mask & bit != 0)
        .collect();
    let table = at + 17;
    let data = table + 4 * present.len().saturating_sub(1);
    let field = |bit: u32| -> Option<usize> {
        let index = present.iter().position(|(b, _)| *b == bit)?;
        let offset = if index == 0 {
            0
        } else {
            u32_at(bytes, table + 4 * (index - 1)) as usize
        };
        Some(data + offset)
    };

    Dialog {
        key,
        kind,
        suppressable: match field(SUPPRESSABLE).map(|at| bytes[at]) {
            Some(1) => Suppressable::Any,
            Some(6) => Suppressable::OnlyYes,
            Some(7) => Suppressable::OnlyNo,
            _ => Suppressable::No,
        },
        title: field(TITLE).map(|at| i64::from(u32_at(bytes, at))),
        body: field(BODY).map(|at| i64::from(u32_at(bytes, at))),
    }
}
