// Window layouts live in each character (core_char) file under windows, one
// (timestamp, {window id: value}) map per property:
//
// - windowSizesAndPositions_1: (x, y, width, height, screen width, screen
//   height) in pixels, the screen being the client area it was saved on
// - openWindows, lockedWindows, minimizedWindows: flags
// - compactWindows, isLightBackgroundWindows: per-window looks; a window
//   without a compact entry follows ui -> window_compact_mode_default
// - stacksWindows: window id -> id of the stack it is a tab in; stacks are
//   windows themselves (numeric ids) and their tabs share the stack's rect
//
// Window ids keep their stored type prefix ("bytes:overview",
// "utf8:chatchannel_fleet"), so writing back never changes a key.
//
// The Neocom (ui -> neocomAlign, neocomWidth) shifts windows docked against
// it: the client stores a window resting on a left Neocom at x = 0, and one
// resting on a right Neocom flush with the screen edge, then adds the Neocom
// back when it places them. `Neocom::place` and `Neocom::store` do the same.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Map, Value, json};

use crate::backups::auto_backup;
use crate::marshal::{encode_settings_json, filetime_now, load_settings_json, strip_type_prefix};

const WINDOWS_KEY: &str = "bytes:windows";
const RECTS_KEY: &str = "bytes:windowSizesAndPositions_1";
const OPEN_KEY: &str = "bytes:openWindows";
const LOCKED_KEY: &str = "bytes:lockedWindows";
const MINIMIZED_KEY: &str = "bytes:minimizedWindows";
const STACKS_KEY: &str = "bytes:stacksWindows";
const COMPACT_KEY: &str = "bytes:compactWindows";
const LIGHT_BACKGROUND_KEY: &str = "bytes:isLightBackgroundWindows";
const COMPACT_DEFAULT_KEY: &str = "bytes:window_compact_mode_default";
const UI_KEY: &str = "bytes:ui";
const NEOCOM_WIDTH_KEY: &str = "bytes:neocomWidth";
const NEOCOM_ALIGN_KEY: &str = "bytes:neocomAlign";

// The client's uiconst.TORIGHT; anything else is its default, TOLEFT
const ALIGN_RIGHT: i64 = 6;
// neocomConst.NEOCOM_DEFAULT_WIDTH
const NEOCOM_DEFAULT_WIDTH: i64 = 48;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeocomSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Neocom {
    pub side: NeocomSide,
    pub width: i64,
}

impl Default for Neocom {
    fn default() -> Self {
        Self {
            side: NeocomSide::Left,
            width: NEOCOM_DEFAULT_WIDTH,
        }
    }
}

impl Neocom {
    // Where the client shows a stored rect.
    pub fn place(&self, mut r: WindowRect) -> WindowRect {
        match self.side {
            NeocomSide::Left if r.x == 0 => r.x = self.width,
            NeocomSide::Right if r.x + r.width == r.screen_width => r.x -= self.width,
            _ => {}
        }
        r
    }

    // What the client stores for a rect shown on screen.
    pub fn store(&self, mut r: WindowRect) -> WindowRect {
        match self.side {
            NeocomSide::Left if r.x == self.width => r.x = 0,
            NeocomSide::Right if r.x + r.width == r.screen_width - self.width => r.x += self.width,
            _ => {}
        }
        r
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowLayout {
    pub windows: Vec<WindowState>,
    pub neocom: Neocom,
    // Whether windows without their own compact entry are compact
    pub compact_default: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowRect {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    // Client area the rect was saved on
    pub screen_width: i64,
    pub screen_height: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowState {
    // As stored, type prefix included
    pub key: String,
    pub rect: Option<WindowRect>,
    pub open: bool,
    pub locked: bool,
    pub minimized: bool,
    // None when the window has no entry and the client's default applies
    pub compact: Option<bool>,
    pub light_background: Option<bool>,
    // Key of the stack this window is a tab in
    pub stack: Option<String>,
}

// Looks that only some windows have an entry for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    Compact,
    LightBackground,
}

impl Style {
    const ALL: [Style; 2] = [Style::Compact, Style::LightBackground];

    fn key(self) -> &'static str {
        match self {
            Style::Compact => COMPACT_KEY,
            Style::LightBackground => LIGHT_BACKGROUND_KEY,
        }
    }

    fn get(self, window: &WindowState) -> Option<bool> {
        match self {
            Style::Compact => window.compact,
            Style::LightBackground => window.light_background,
        }
    }

    fn set(self, window: &mut WindowState, on: Option<bool>) {
        match self {
            Style::Compact => window.compact = on,
            Style::LightBackground => window.light_background = on,
        }
    }
}

// The on/off properties, each stored as its own map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flag {
    Open,
    Locked,
    Minimized,
}

impl Flag {
    const ALL: [Flag; 3] = [Flag::Open, Flag::Locked, Flag::Minimized];

    fn key(self) -> &'static str {
        match self {
            Flag::Open => OPEN_KEY,
            Flag::Locked => LOCKED_KEY,
            Flag::Minimized => MINIMIZED_KEY,
        }
    }

    fn get(self, window: &WindowState) -> bool {
        match self {
            Flag::Open => window.open,
            Flag::Locked => window.locked,
            Flag::Minimized => window.minimized,
        }
    }

    fn set(self, window: &mut WindowState, on: bool) {
        match self {
            Flag::Open => window.open = on,
            Flag::Locked => window.locked = on,
            Flag::Minimized => window.minimized = on,
        }
    }
}

impl WindowState {
    pub fn id(&self) -> &str {
        strip_type_prefix(&self.key)
    }

    // Stacks are stored under numeric ids.
    pub fn is_stack(&self) -> bool {
        let id = self.id();
        id == "ChatWindowStack" || (!id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()))
    }
}

fn section<'a>(windows: &'a Map<String, Value>, key: &str) -> Option<&'a Map<String, Value>> {
    windows
        .get(key)
        .and_then(|v| v.get("tuple"))
        .and_then(|t| t.get(1))
        .and_then(Value::as_object)
}

fn number(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|f| f.round() as i64))
}

fn parse_rect(value: &Value) -> Option<WindowRect> {
    let parts = value.get("tuple")?.as_array()?;
    let at = |i: usize| parts.get(i).and_then(number);
    Some(WindowRect {
        x: at(0)?,
        y: at(1)?,
        width: at(2)?,
        height: at(3)?,
        screen_width: at(4).unwrap_or(0),
        screen_height: at(5).unwrap_or(0),
    })
}

fn layout_from_settings(root: &Value) -> Vec<WindowState> {
    let Some(windows) = root.get(WINDOWS_KEY).and_then(Value::as_object) else {
        return Vec::new();
    };
    // A window can appear in any of the maps
    fn state<'map>(
        states: &'map mut BTreeMap<String, WindowState>,
        key: &str,
    ) -> &'map mut WindowState {
        states
            .entry(key.to_string())
            .or_insert_with(|| WindowState {
                key: key.to_string(),
                rect: None,
                open: false,
                locked: false,
                minimized: false,
                compact: None,
                light_background: None,
                stack: None,
            })
    }

    let mut states: BTreeMap<String, WindowState> = BTreeMap::new();
    for (key, value) in section(windows, RECTS_KEY).into_iter().flatten() {
        state(&mut states, key).rect = parse_rect(value);
    }
    for flag in Flag::ALL {
        for (key, value) in section(windows, flag.key()).into_iter().flatten() {
            flag.set(state(&mut states, key), value.as_bool().unwrap_or(false));
        }
    }
    for style in Style::ALL {
        for (key, value) in section(windows, style.key()).into_iter().flatten() {
            style.set(state(&mut states, key), value.as_bool());
        }
    }
    for (key, value) in section(windows, STACKS_KEY).into_iter().flatten() {
        if let Some(stack) = value.as_str() {
            state(&mut states, key).stack = Some(stack.to_string());
        }
    }
    states.into_values().collect()
}

// Rewrites a property map, stamping it only when something in it changed.
fn update_section(
    windows: &mut Map<String, Value>,
    key: &str,
    now: &str,
    entries: impl Iterator<Item = (String, Value)>,
) {
    let existing = section(windows, key).cloned().unwrap_or_default();
    let mut updated = existing.clone();
    for (window, value) in entries {
        updated.insert(window, value);
    }
    if updated != existing {
        windows.insert(key.to_string(), json!({"tuple": [now, updated]}));
    }
}

fn layout_into_settings(root: &mut Value, layout: &[WindowState]) -> Result<(), String> {
    let windows = root
        .as_object_mut()
        .ok_or("Settings file has an unexpected structure")?
        .entry(WINDOWS_KEY.to_string())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("Settings 'windows' section has an unexpected structure")?;
    let now = format!("long:{}", filetime_now());

    update_section(
        windows,
        RECTS_KEY,
        &now,
        layout.iter().filter_map(|w| {
            let r = w.rect?;
            Some((
                w.key.clone(),
                json!({"tuple": [r.x, r.y, r.width, r.height, r.screen_width, r.screen_height]}),
            ))
        }),
    );
    // Flags are written for windows that had one or now differ from the
    // client's default (off)
    for flag in Flag::ALL {
        let stored = section(windows, flag.key()).cloned().unwrap_or_default();
        update_section(
            windows,
            flag.key(),
            &now,
            layout
                .iter()
                .filter(|w| flag.get(w) || stored.contains_key(&w.key))
                .map(|w| (w.key.clone(), json!(flag.get(w)))),
        );
    }
    for style in Style::ALL {
        update_section(
            windows,
            style.key(),
            &now,
            layout
                .iter()
                .filter_map(|w| Some((w.key.clone(), json!(style.get(w)?)))),
        );
    }
    Ok(())
}

fn neocom_from_settings(root: &Value) -> Neocom {
    let setting = |key: &str| {
        root.get(UI_KEY)
            .and_then(|ui| ui.get(key))
            .and_then(|v| v.get("tuple"))
            .and_then(|t| t.get(1))
            .and_then(number)
    };
    Neocom {
        side: if setting(NEOCOM_ALIGN_KEY) == Some(ALIGN_RIGHT) {
            NeocomSide::Right
        } else {
            NeocomSide::Left
        },
        width: setting(NEOCOM_WIDTH_KEY).unwrap_or(NEOCOM_DEFAULT_WIDTH),
    }
}

// Rects as stored; use `Neocom::place` for where they show up in game.
pub fn read_window_layout(path: &Path) -> Result<WindowLayout, String> {
    let (json, _) = load_settings_json(path)?;
    let compact_default = json
        .get(UI_KEY)
        .and_then(|ui| ui.get(COMPACT_DEFAULT_KEY))
        .and_then(|v| v.get("tuple"))
        .and_then(|t| t.get(1))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(WindowLayout {
        windows: layout_from_settings(&json),
        neocom: neocom_from_settings(&json),
        compact_default,
    })
}

pub fn write_window_layout(
    path: &Path,
    layout: &[WindowState],
    backup: bool,
) -> Result<(), String> {
    if !path.exists() {
        return Err("Settings file does not exist".into());
    }
    if backup {
        auto_backup(path, "pre-window-layout")?;
    }

    let (mut json, had_crc) = load_settings_json(path)?;
    layout_into_settings(&mut json, layout)?;
    let bytes = encode_settings_json(&json, had_crc)?;
    fs::write(path, bytes).map_err(|e| format!("Failed to write settings file: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Value {
        json!({
            "bytes:windows": {
                "bytes:__version__": {"tuple": ["long:1", 1]},
                "bytes:windowSizesAndPositions_1": {"tuple": ["long:1", {
                    "bytes:overview": {"tuple": [3254, 603, 582, 814, 3836, 2130]},
                    "bytes:548": {"tuple": [415, 1519, 452, 611, 3836, 2130]},
                    "utf8:chatchannel_fleet": {"tuple": [415, 1519, 452, 611, 3836, 2130]},
                }]},
                "bytes:openWindows": {"tuple": ["long:1", {
                    "bytes:overview": true,
                    "bytes:548": true,
                    "utf8:chatchannel_fleet": true,
                }]},
                "bytes:lockedWindows": {"tuple": ["long:2", {"bytes:overview": true}]},
                "bytes:compactWindows": {"tuple": ["long:1", {"bytes:548": false}]},
                "bytes:stacksWindows": {"tuple": ["long:1", {"utf8:chatchannel_fleet": "bytes:548"}]},
            }
        })
    }

    #[test]
    fn reads_windows_and_stacks() {
        let layout = layout_from_settings(&settings());
        assert_eq!(layout.len(), 3);
        let overview = layout.iter().find(|w| w.id() == "overview").unwrap();
        assert!(overview.open && overview.locked && !overview.minimized);
        assert_eq!(overview.rect.unwrap().width, 582);
        assert_eq!(overview.rect.unwrap().screen_height, 2130);

        let fleet = layout
            .iter()
            .find(|w| w.id() == "chatchannel_fleet")
            .unwrap();
        assert_eq!(fleet.key, "utf8:chatchannel_fleet");
        assert_eq!(fleet.stack.as_deref(), Some("bytes:548"));
        let stack = layout.iter().find(|w| w.id() == "548").unwrap();
        assert!(stack.is_stack());
        assert_eq!(stack.compact, Some(false));
        assert_eq!(overview.compact, None);
        assert!(!overview.is_stack());
    }

    #[test]
    fn writes_changes_back_under_the_same_keys() {
        let mut root = settings();
        let mut layout = layout_from_settings(&root);
        for window in &mut layout {
            if window.id() == "overview" {
                window.rect.as_mut().unwrap().x = 100;
                window.locked = false;
                window.minimized = true;
                window.compact = Some(true);
            }
        }
        layout_into_settings(&mut root, &layout).unwrap();

        let windows = root[WINDOWS_KEY].as_object().unwrap();
        let rects = section(windows, RECTS_KEY).unwrap();
        assert_eq!(
            rects["bytes:overview"],
            json!({"tuple": [100, 603, 582, 814, 3836, 2130]})
        );
        assert!(rects.contains_key("utf8:chatchannel_fleet"));
        assert_eq!(
            section(windows, LOCKED_KEY).unwrap()["bytes:overview"],
            json!(false)
        );
        assert_eq!(
            section(windows, MINIMIZED_KEY).unwrap()["bytes:overview"],
            json!(true)
        );
        // Untouched maps keep their timestamp, unrelated keys stay
        assert_eq!(windows[OPEN_KEY]["tuple"][0], json!("long:1"));
        assert_eq!(windows["bytes:__version__"]["tuple"][1], json!(1));
        assert_eq!(layout_from_settings(&root), layout);
    }

    #[test]
    fn neocom_shifts_windows_docked_against_it() {
        let rect = |x, width| WindowRect {
            x,
            y: 0,
            width,
            height: 100,
            screen_width: 1000,
            screen_height: 800,
        };
        let left = Neocom {
            side: NeocomSide::Left,
            width: 45,
        };
        assert_eq!(left.place(rect(0, 200)).x, 45);
        assert_eq!(left.place(rect(10, 200)).x, 10);
        assert_eq!(left.store(rect(45, 200)).x, 0);
        assert_eq!(left.place(rect(800, 200)).x, 800);

        let right = Neocom {
            side: NeocomSide::Right,
            width: 45,
        };
        assert_eq!(right.place(rect(800, 200)).x, 755);
        assert_eq!(right.store(rect(755, 200)).x, 800);
        assert_eq!(right.place(rect(0, 200)).x, 0);

        let root = json!({"bytes:ui": {
            "bytes:neocomWidth": {"tuple": ["long:1", 45]},
            "bytes:neocomAlign": {"tuple": ["long:1", 6]},
        }});
        assert_eq!(neocom_from_settings(&root), right);
    }

    #[test]
    fn round_trips_through_marshal() {
        let dir = std::env::temp_dir().join("eve-wrench-window-layout-test");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core_char_12345.dat");
        fs::write(&path, encode_settings_json(&settings(), false).unwrap()).unwrap();

        let mut layout = read_window_layout(&path).unwrap();
        assert_eq!(layout.neocom, Neocom::default());
        layout.windows[0].open = false;
        write_window_layout(&path, &layout.windows, false).unwrap();
        assert_eq!(read_window_layout(&path).unwrap(), layout);

        fs::remove_dir_all(&dir).ok();
    }
}
