use std::collections::HashMap;

use eve_wrench_core::window_layout::{Neocom, NeocomSide, WindowRect, WindowState};
use gpui_kit::assets::IconName;
use rust_i18n::t;

// Smallest window the editor lets a resize produce, in screen pixels
pub const MIN_WIDTH: i64 = 100;
pub const MIN_HEIGHT: i64 = 40;

// Used when no window has a rect to take the screen size from
const FALLBACK_SCREEN: Screen = Screen {
    width: 1920,
    height: 1080,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Screen {
    pub width: i64,
    pub height: i64,
}

// The client area most windows were saved on. Rects saved on another one
// (an older resolution) are shown as stored.
pub fn screen(windows: &[WindowState]) -> Screen {
    let mut counts: HashMap<(i64, i64), usize> = HashMap::new();
    for rect in windows.iter().filter_map(|w| w.rect) {
        if rect.screen_width > 0 && rect.screen_height > 0 {
            *counts
                .entry((rect.screen_width, rect.screen_height))
                .or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(size, count)| (count, size))
        .map(|((width, height), _)| Screen { width, height })
        .unwrap_or(FALLBACK_SCREEN)
}

// The screen and the Neocom along one side of it, which windows stay out of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Desktop {
    pub screen: Screen,
    pub neocom: Neocom,
}

impl Desktop {
    // Horizontal range windows can use
    pub fn left(&self) -> i64 {
        match self.neocom.side {
            NeocomSide::Left => self.neocom.width,
            NeocomSide::Right => 0,
        }
    }

    pub fn right(&self) -> i64 {
        match self.neocom.side {
            NeocomSide::Left => self.screen.width,
            NeocomSide::Right => self.screen.width - self.neocom.width,
        }
    }
}

// Windows that only exist while docked.
const DOCKED_ONLY: [&str; 6] = [
    "lobbyWnd",
    "repairshop",
    "tradeWnd",
    "InventoryStructure",
    "StationItems",
    "StationShips",
];

// Windows from the client's inflight UI, which only show in space.
const SPACE_ONLY: [&str; 5] = [
    "droneview",
    "selecteditemview",
    "probeScannerWindow",
    "directionalScannerWindow",
    "MoonScanner",
];

pub fn docked_only(window: &WindowState) -> bool {
    let id = window.id();
    DOCKED_ONLY.contains(&id) || id.starts_with("InventoryStation")
}

pub fn space_only(window: &WindowState) -> bool {
    let id = window.id();
    SPACE_ONLY.contains(&id) || id == "overview" || id.starts_with("overview_")
}

pub fn members<'a>(windows: &'a [WindowState], stack: &str) -> Vec<&'a WindowState> {
    windows
        .iter()
        .filter(|w| w.stack.as_deref() == Some(stack))
        .collect()
}

// Windows drawn on the canvas: everything with a rect that isn't a tab inside
// a stack. Stacks without tabs are leftovers the client no longer shows.
pub fn on_canvas(window: &WindowState, windows: &[WindowState]) -> bool {
    window.rect.is_some()
        && window.stack.is_none()
        && (!window.is_stack() || !members(windows, &window.key).is_empty())
}

// Groups for the window list, in list order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Space,
    Navigation,
    Social,
    Inventory,
    Business,
    Character,
    Other,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::Space,
        Category::Navigation,
        Category::Social,
        Category::Inventory,
        Category::Business,
        Category::Character,
        Category::Other,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Category::Space => "space",
            Category::Navigation => "navigation",
            Category::Social => "social",
            Category::Inventory => "inventory",
            Category::Business => "business",
            Category::Character => "character",
            Category::Other => "other",
        }
    }
}

pub fn category(window: &WindowState) -> Category {
    let id = window.id();
    if window.is_stack() || id.starts_with("chatchannel_") {
        return Category::Social;
    }
    if id.starts_with("overview") {
        return Category::Space;
    }
    if id.starts_with("Inventory") || id.starts_with("Station") {
        return Category::Inventory;
    }
    match id {
        "selecteditemview"
        | "droneview"
        | "directionalScannerWindow"
        | "probeScannerWindow"
        | "MoonScanner" => Category::Space,
        "MapCmdWindow"
        | "mapbrowser"
        | "solar_system_map_panel"
        | "locations"
        | "standaloneBookmarkWnd"
        | "bookmarkLocationWindow" => Category::Navigation,
        "fleetwindow" | "watchlistpanel" | "XmppChatChannels" => Category::Social,
        "ActiveShipCargo" | "fittingWnd" | "lobbyWnd" | "repairshop" => Category::Inventory,
        "market" | "MultiBuy" | "walletWindow" | "contractdetails" | "tradeWnd" | "redeem" => {
            Category::Business
        }
        "charactersheet" | "SkillPlanner" | "shipInfoWindow" | "careerPortal" | "rewardsWnd"
        | "KillReportWnd" => Category::Character,
        _ => Category::Other,
    }
}

pub fn icon(window: &WindowState) -> IconName {
    let id = window.id();
    if window.is_stack() || id.starts_with("chatchannel_") || id == "XmppChatChannels" {
        return IconName::MessageSquare;
    }
    if id.starts_with("overview") {
        return IconName::ListFilter;
    }
    if id.starts_with("Inventory") || id.starts_with("Station") {
        return IconName::Warehouse;
    }
    match id {
        "selecteditemview" => IconName::Crosshair,
        "droneview" => IconName::Drone,
        "directionalScannerWindow" => IconName::Radar,
        "probeScannerWindow" => IconName::ScanSearch,
        "MoonScanner" => IconName::Moon,
        "MapCmdWindow" | "mapbrowser" | "solar_system_map_panel" => IconName::Map,
        "locations" | "standaloneBookmarkWnd" => IconName::Bookmark,
        "bookmarkLocationWindow" => IconName::MapPin,
        "fleetwindow" => IconName::Users,
        "watchlistpanel" => IconName::Eye,
        "ActiveShipCargo" => IconName::Package,
        "fittingWnd" => IconName::Wrench,
        "lobbyWnd" | "repairshop" => IconName::Building2,
        "market" => IconName::Store,
        "MultiBuy" => IconName::ShoppingCart,
        "walletWindow" => IconName::Wallet,
        "contractdetails" => IconName::FileText,
        "tradeWnd" => IconName::ArrowLeftRight,
        "charactersheet" => IconName::User,
        "SkillPlanner" => IconName::GraduationCap,
        "shipInfoWindow" => IconName::Info,
        "KillReportWnd" => IconName::Skull,
        "message" => IconName::MessageCircle,
        "overviewsettings" | "NotificationSettings" => IconName::Settings,
        "fpsMonitor2" => IconName::Gauge,
        _ => IconName::AppWindow,
    }
}

// Window ids with a translated name; the rest are spelled out from the id.
const KNOWN: [&str; 32] = [
    "overview",
    "droneview",
    "fleetwindow",
    "selecteditemview",
    "directionalScannerWindow",
    "probeScannerWindow",
    "watchlistpanel",
    "locations",
    "standaloneBookmarkWnd",
    "bookmarkLocationWindow",
    "ActiveShipCargo",
    "InventoryStructure",
    "lobbyWnd",
    "fittingWnd",
    "market",
    "walletWindow",
    "charactersheet",
    "shipInfoWindow",
    "SkillPlanner",
    "MoonScanner",
    "contractdetails",
    "KillReportWnd",
    "MultiBuy",
    "tradeWnd",
    "message",
    "help",
    "mapbrowser",
    "overviewsettings",
    "XmppChatChannels",
    "ChatWindowStack",
    "fpsMonitor2",
    "MapCmdWindow",
];

pub fn window_name(id: &str) -> String {
    if KNOWN.contains(&id) {
        return t!(format!("layoutEditor.windows.{id}")).to_string();
    }
    // overview_1 is the second overview tab set
    if let Some(n) = id
        .strip_prefix("overview_")
        .and_then(|n| n.parse::<u32>().ok())
    {
        return t!("layoutEditor.overviewN", n = n + 1).to_string();
    }
    if let Some(channel) = id.strip_prefix("chatchannel_") {
        return match channel {
            "local" | "corp" | "alliance" | "fleet" => {
                t!(format!("layoutEditor.chat.{channel}")).to_string()
            }
            _ => t!("layoutEditor.chat.player").to_string(),
        };
    }
    spell_out(id)
}

// "InventoryStructure" -> "Inventory structure", dropping Wnd/Window.
fn spell_out(id: &str) -> String {
    let id = id
        .trim_end_matches("Wnd")
        .trim_end_matches("Window")
        .trim_end_matches("_");
    let mut out = String::new();
    for (i, c) in id.chars().enumerate() {
        if c == '_' {
            out.push(' ');
        } else if i > 0 && c.is_uppercase() {
            out.push(' ');
            out.extend(c.to_lowercase());
        } else if i == 0 {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

// A stack is named after its tabs.
pub fn display_name(window: &WindowState, windows: &[WindowState]) -> String {
    if window.is_stack() && window.id() != "ChatWindowStack" {
        let tabs: Vec<String> = members(windows, &window.key)
            .iter()
            .map(|w| window_name(w.id()))
            .collect();
        if !tabs.is_empty() {
            return tabs.join(" · ");
        }
    }
    window_name(window.id())
}

// Which edges a drag moves; none means the whole window moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edges {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl Edges {
    pub const MOVE: Edges = Edges {
        left: false,
        right: false,
        top: false,
        bottom: false,
    };

    pub fn is_move(self) -> bool {
        self == Self::MOVE
    }
}

// Edges other windows' rects and the screen border offer for snapping, as
// x positions and y positions.
pub struct SnapLines {
    xs: Vec<i64>,
    ys: Vec<i64>,
}

impl SnapLines {
    pub fn new(desktop: Desktop, others: impl Iterator<Item = WindowRect>) -> Self {
        let mut xs = vec![desktop.left(), desktop.right()];
        let mut ys = vec![0, desktop.screen.height];
        for r in others {
            xs.extend([r.x, r.x + r.width]);
            ys.extend([r.y, r.y + r.height]);
        }
        Self { xs, ys }
    }

    // How far to shift so one of `edges` lands on a line, if one is within
    // `threshold`.
    fn shift(lines: &[i64], edges: &[i64], threshold: i64) -> i64 {
        edges
            .iter()
            .flat_map(|edge| lines.iter().map(move |line| line - edge))
            .filter(|d| d.abs() <= threshold)
            .min_by_key(|d| d.abs())
            .unwrap_or(0)
    }
}

// The rect after dragging `edges` of `start` by (dx, dy) screen pixels,
// snapped to `snap` lines within `threshold` and kept on the desktop.
pub fn drag_rect(
    start: WindowRect,
    edges: Edges,
    (dx, dy): (i64, i64),
    desktop: Desktop,
    snap: Option<(&SnapLines, i64)>,
) -> WindowRect {
    let (min_x, max_x) = (desktop.left(), desktop.right());
    let screen = desktop.screen;
    let mut r = start;
    if edges.is_move() {
        r.x = start.x + dx;
        r.y = start.y + dy;
        if let Some((lines, threshold)) = snap {
            r.x += SnapLines::shift(&lines.xs, &[r.x, r.x + r.width], threshold);
            r.y += SnapLines::shift(&lines.ys, &[r.y, r.y + r.height], threshold);
        }
        r.x = r.x.clamp(min_x, (max_x - r.width).max(min_x));
        r.y = r.y.clamp(0, (screen.height - r.height).max(0));
        return r;
    }

    let (mut left, mut top) = (start.x, start.y);
    let (mut right, mut bottom) = (start.x + start.width, start.y + start.height);
    let snapped = |value: i64, lines: &[i64]| match snap {
        Some((_, threshold)) => value + SnapLines::shift(lines, &[value], threshold),
        None => value,
    };
    let (xs, ys) = snap.map_or((&[][..], &[][..]), |(l, _)| (&l.xs[..], &l.ys[..]));
    if edges.left {
        left = snapped(start.x + dx, xs).clamp(min_x, right - MIN_WIDTH);
    }
    if edges.right {
        right = snapped(right + dx, xs).clamp(left + MIN_WIDTH, max_x);
    }
    if edges.top {
        top = snapped(start.y + dy, ys).clamp(0, bottom - MIN_HEIGHT);
    }
    if edges.bottom {
        bottom = snapped(bottom + dy, ys).clamp(top + MIN_HEIGHT, screen.height);
    }
    r.x = left;
    r.y = top;
    r.width = right - left;
    r.height = bottom - top;
    r
}

// The whole edit state, for undo/redo.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub windows: Vec<WindowState>,
    pub selected: Option<String>,
}

pub type History = crate::history::History<Snapshot>;

// Sets a window's rect; a stack's tabs move with it, since the client keeps
// them on the stack's rect.
pub fn set_rect(windows: &mut [WindowState], key: &str, rect: WindowRect) {
    for window in windows.iter_mut() {
        if window.key == key || window.stack.as_deref() == Some(key) {
            window.rect = Some(rect);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Screen = Screen {
        width: 1000,
        height: 800,
    };
    const DESKTOP: Desktop = Desktop {
        screen: SCREEN,
        neocom: Neocom {
            side: NeocomSide::Left,
            width: 0,
        },
    };

    fn rect(x: i64, y: i64, width: i64, height: i64) -> WindowRect {
        WindowRect {
            x,
            y,
            width,
            height,
            screen_width: SCREEN.width,
            screen_height: SCREEN.height,
        }
    }

    fn window(key: &str, stack: Option<&str>) -> WindowState {
        WindowState {
            key: key.to_string(),
            rect: Some(rect(10, 10, 200, 100)),
            open: true,
            locked: false,
            minimized: false,
            compact: None,
            light_background: None,
            stack: stack.map(str::to_string),
        }
    }

    #[test]
    fn moves_snap_and_stay_on_screen() {
        let start = rect(100, 100, 200, 100);
        let lines = SnapLines::new(DESKTOP, [rect(400, 0, 100, 100)].into_iter());
        // Right edge lands at 395 and snaps onto the other window at 400
        let moved = drag_rect(start, Edges::MOVE, (95, 0), DESKTOP, Some((&lines, 8)));
        assert_eq!(moved.x, 200);
        let free = drag_rect(start, Edges::MOVE, (95, 0), DESKTOP, None);
        assert_eq!(free.x, 195);
        let off = drag_rect(start, Edges::MOVE, (-500, 5000), DESKTOP, None);
        assert_eq!((off.x, off.y), (0, 700));
    }

    #[test]
    fn windows_stay_out_of_the_neocom() {
        let desktop = Desktop {
            neocom: Neocom {
                side: NeocomSide::Left,
                width: 45,
            },
            ..DESKTOP
        };
        let start = rect(100, 100, 200, 100);
        let moved = drag_rect(start, Edges::MOVE, (-90, 0), desktop, None);
        assert_eq!(moved.x, 45);
        let lines = SnapLines::new(desktop, std::iter::empty());
        let snapped = drag_rect(start, Edges::MOVE, (-50, 0), desktop, Some((&lines, 8)));
        assert_eq!(snapped.x, 45);
    }

    #[test]
    fn resizes_keep_a_minimum_size() {
        let start = rect(100, 100, 200, 100);
        let edges = Edges {
            left: true,
            bottom: true,
            ..Edges::MOVE
        };
        let r = drag_rect(start, edges, (50, 30), DESKTOP, None);
        assert_eq!((r.x, r.y, r.width, r.height), (150, 100, 150, 130));
        let tiny = drag_rect(start, edges, (500, -500), DESKTOP, None);
        assert_eq!((tiny.width, tiny.height), (MIN_WIDTH, MIN_HEIGHT));
        // Right edge snaps to the screen border
        let right = Edges {
            right: true,
            ..Edges::MOVE
        };
        let lines = SnapLines::new(DESKTOP, std::iter::empty());
        let r = drag_rect(start, right, (695, 0), DESKTOP, Some((&lines, 8)));
        assert_eq!(r.x + r.width, 1000);
    }

    #[test]
    fn stacks_without_tabs_stay_off_the_canvas() {
        let windows = vec![
            window("bytes:548", None),
            window("bytes:98", None),
            window("bytes:chatchannel_corp", Some("bytes:548")),
            window("bytes:overview", None),
        ];
        let shown: Vec<&str> = windows
            .iter()
            .filter(|w| on_canvas(w, &windows))
            .map(|w| w.id())
            .collect();
        assert_eq!(shown, ["548", "overview"]);
    }

    #[test]
    fn stack_tabs_follow_the_stack() {
        let mut windows = vec![
            window("bytes:548", None),
            window("bytes:chatchannel_corp", Some("bytes:548")),
        ];
        set_rect(&mut windows, "bytes:548", rect(0, 0, 300, 300));
        assert!(windows.iter().all(|w| w.rect == Some(rect(0, 0, 300, 300))));
    }

    #[test]
    fn picks_the_most_common_screen() {
        let mut windows = vec![window("a", None), window("b", None), window("c", None)];
        windows[0].rect.as_mut().unwrap().screen_width = 1920;
        assert_eq!(screen(&windows), SCREEN);
        assert_eq!(screen(&[]), FALLBACK_SCREEN);
    }

    #[test]
    fn splits_windows_by_location() {
        let space = window("bytes:overview_3", None);
        let docked = window("bytes:lobbyWnd", None);
        let both = window("bytes:overviewsettings", None);
        assert!(space_only(&space) && !docked_only(&space));
        assert!(docked_only(&docked) && !space_only(&docked));
        assert!(!space_only(&both) && !docked_only(&both));
    }

    #[test]
    fn spells_out_unknown_ids() {
        assert_eq!(spell_out("InventoryStructure"), "Inventory structure");
        assert_eq!(spell_out("careerPortal"), "Career portal");
        assert_eq!(
            spell_out("solar_system_map_panel"),
            "Solar system map panel"
        );
    }
}
