//! Finds the buttons each dialog is shown with by scanning the client's
//! Python 2.7 bytecode. `code.ccp` is a zip of zlib-compressed .pyc files; a
//! call like `eve.Message('ConfirmPlugInImplant', {}, uiconst.OKCANCEL)`
//! compiles to a LOAD_CONST of the key followed, within the same call, by a
//! LOAD_ATTR of the button set.

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Buttons {
    YesNo,
    Ok,
}

fn buttons(name: &str) -> Option<Buttons> {
    match name {
        "YESNO" | "YESNOCANCEL" => Some(Buttons::YesNo),
        "OK" | "OKCANCEL" => Some(Buttons::Ok),
        _ => None,
    }
}

const LOAD_CONST: u8 = 100;
const LOAD_NAME: u8 = 101;
const LOAD_ATTR: u8 = 106;
const LOAD_GLOBAL: u8 = 116;
const HAVE_ARGUMENT: u8 = 90;
// How far after the key its button set can follow (the params dict and other
// arguments come in between)
const LOOKAHEAD: usize = 40;

// Button sets per dialog key, from every call site that names one.
pub fn scan(archive: &[u8], keys: &HashSet<&str>) -> HashMap<String, HashSet<Buttons>> {
    let mut zip = zip::ZipArchive::new(Cursor::new(archive)).expect("code.ccp");
    let mut found: HashMap<String, HashSet<Buttons>> = HashMap::new();
    for i in 0..zip.len() {
        let mut compressed = Vec::new();
        zip.by_index(i)
            .and_then(|mut f| Ok(f.read_to_end(&mut compressed)?))
            .expect("code.ccp entry");
        let mut pyc = Vec::new();
        if flate2::read::ZlibDecoder::new(&compressed[..])
            .read_to_end(&mut pyc)
            .is_err()
            || pyc.len() < 8
        {
            continue;
        }
        // Skip the magic and timestamp
        let mut reader = Marshal::new(&pyc[8..]);
        if let Some(Object::Code(code)) = reader.object() {
            code.scan(keys, &mut found);
        }
    }
    found
}

struct Code {
    bytecode: Vec<u8>,
    consts: Vec<Object>,
    names: Vec<String>,
}

impl Code {
    fn scan(&self, keys: &HashSet<&str>, found: &mut HashMap<String, HashSet<Buttons>>) {
        for constant in &self.consts {
            if let Object::Code(inner) = constant {
                inner.scan(keys, found);
            }
        }

        let mut instructions = Vec::new();
        let mut at = 0;
        while at < self.bytecode.len() {
            let op = self.bytecode[at];
            if op >= HAVE_ARGUMENT && at + 2 < self.bytecode.len() {
                let arg = u16::from_le_bytes([self.bytecode[at + 1], self.bytecode[at + 2]]);
                instructions.push((op, arg as usize));
                at += 3;
            } else {
                instructions.push((op, 0));
                at += 1;
            }
        }

        let key_at = |&(op, arg): &(u8, usize)| -> Option<&str> {
            if op != LOAD_CONST {
                return None;
            }
            match self.consts.get(arg)? {
                Object::Str(s) if keys.contains(s.as_str()) => Some(s),
                _ => None,
            }
        };
        for (i, instruction) in instructions.iter().enumerate() {
            let Some(key) = key_at(instruction) else {
                continue;
            };
            let set = instructions[i + 1..]
                .iter()
                .take(LOOKAHEAD)
                .take_while(|next| key_at(next).is_none())
                .find_map(|&(op, arg)| {
                    matches!(op, LOAD_ATTR | LOAD_GLOBAL | LOAD_NAME)
                        .then(|| self.names.get(arg).and_then(|n| buttons(n)))
                        .flatten()
                });
            if let Some(set) = set {
                found.entry(key.to_string()).or_default().insert(set);
            }
        }
    }
}

enum Object {
    None,
    Int,
    Str(String),
    Tuple(Vec<Object>),
    Code(Box<Code>),
}

// Python 2.7 marshal, as much of it as .pyc files use.
struct Marshal<'a> {
    bytes: &'a [u8],
    at: usize,
    interned: Vec<String>,
}

impl<'a> Marshal<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            at: 0,
            interned: Vec::new(),
        }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let slice = self.bytes.get(self.at..self.at + n)?;
        self.at += n;
        Some(slice)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }

    fn i32(&mut self) -> Option<i32> {
        self.take(4)
            .map(|b| i32::from_le_bytes(b.try_into().expect("four bytes")))
    }

    fn len(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok()
    }

    fn sequence(&mut self) -> Option<Vec<Object>> {
        let n = self.len()?;
        (0..n).map(|_| self.object()).collect()
    }

    fn string(&mut self) -> Option<String> {
        match self.object()? {
            Object::Str(s) => Some(s),
            _ => None,
        }
    }

    fn object(&mut self) -> Option<Object> {
        Some(match self.u8()? {
            b'0' | b'N' | b'F' | b'T' | b'S' | b'.' => Object::None,
            b'i' => {
                self.take(4)?;
                Object::Int
            }
            b'I' | b'g' => {
                self.take(8)?;
                Object::Int
            }
            b'y' => {
                self.take(16)?;
                Object::Int
            }
            b'f' => {
                let n = self.u8()? as usize;
                self.take(n)?;
                Object::Int
            }
            b'x' => {
                for _ in 0..2 {
                    let n = self.u8()? as usize;
                    self.take(n)?;
                }
                Object::Int
            }
            b'l' => {
                let digits = self.i32()?.unsigned_abs() as usize;
                self.take(digits * 2)?;
                Object::Int
            }
            tag @ (b's' | b't' | b'u') => {
                let n = self.len()?;
                let bytes = self.take(n)?;
                let s = if tag == b'u' {
                    String::from_utf8_lossy(bytes).into_owned()
                } else {
                    // Byte strings, kept byte for byte (bytecode is one)
                    bytes.iter().map(|&b| b as char).collect()
                };
                if tag == b't' {
                    self.interned.push(s.clone());
                }
                Object::Str(s)
            }
            b'R' => {
                let index = self.len()?;
                Object::Str(self.interned.get(index)?.clone())
            }
            b'(' | b'[' | b'<' | b'>' => Object::Tuple(self.sequence()?),
            b'{' => {
                loop {
                    if self.bytes.get(self.at) == Some(&b'0') {
                        self.at += 1;
                        break;
                    }
                    self.object()?;
                    self.object()?;
                }
                Object::None
            }
            b'c' => {
                // argcount, nlocals, stacksize, flags
                self.take(16)?;
                let bytecode = self.string()?.chars().map(|c| c as u8).collect();
                let consts = match self.object()? {
                    Object::Tuple(items) => items,
                    _ => return None,
                };
                let names = match self.object()? {
                    Object::Tuple(items) => items
                        .into_iter()
                        .filter_map(|o| match o {
                            Object::Str(s) => Some(s),
                            _ => None,
                        })
                        .collect(),
                    _ => return None,
                };
                // varnames, freevars, cellvars, filename, name
                for _ in 0..5 {
                    self.object()?;
                }
                self.take(4)?; // firstlineno
                self.object()?; // lnotab
                Object::Code(Box::new(Code {
                    bytecode,
                    consts,
                    names,
                }))
            }
            _ => return None,
        })
    }
}
