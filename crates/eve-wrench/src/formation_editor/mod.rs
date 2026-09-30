mod inspector;
mod list;
mod model;
mod scanner;
mod viewport;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use eve_wrench_core::{ProbeFormation, formations, scan};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, TitleBar, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use self::model::{
    AXES, Axis, Formation, History, MAX_PROBES, Pattern, Snapshot, format_number, parse_distance,
};
use self::scanner::Camera;
use self::viewport::Drag;
use crate::store::{Store, StoreEvent};
use crate::ui::{self, StyleExt as _};

actions!(formation_editor, [Undo, Redo, Save]);

const KEY_CONTEXT: &str = "FormationEditor";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-z", Undo, Some(KEY_CONTEXT)),
        KeyBinding::new("secondary-shift-z", Redo, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-y", Redo, Some(KEY_CONTEXT)),
        KeyBinding::new("secondary-s", Save, Some(KEY_CONTEXT)),
    ]);
}

// One editor window per settings file; opening it again focuses the window.
pub fn open(path: String, name: String, cx: &mut App) {
    let store = Store::global(cx);
    if let Some(window) = store.read(cx).editor_window(&path)
        && window
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1280.), px(800.)), cx)),
        window_min_size: Some(size(px(1040.), px(640.))),
        app_id: Some("eve-wrench".into()),
        window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
        ..TitleBar::window_options()
    };
    let editor_path = path.clone();
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| FormationEditor::new(PathBuf::from(editor_path), name, window, cx))
    });
    if let Ok((window, _)) = opened {
        store.update(cx, |store, _| store.register_editor_window(path, window));
    }
}

enum LoadState {
    Loading,
    Failed(SharedString),
    Ready,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TransformTool {
    Scale,
    Rotate,
    Mirror,
    Move,
}

// Inspector sections that can be collapsed.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Section {
    Formation,
    Probes,
    Transform,
    Pattern,
    Balance,
}

// Where a "Move" nudges the formation: an axis and a direction along it.
#[derive(Clone, Copy, PartialEq)]
struct Direction {
    axis: Axis,
    sign: f64,
}

pub struct FormationEditor {
    store: Entity<Store>,
    file_path: PathBuf,
    display_name: String,
    state: LoadState,
    formations: Vec<Formation>,
    saved: Vec<Formation>,
    selected: usize,
    history: History,
    disk_changed: bool,
    saving: bool,
    focus_handle: FocusHandle,

    selected_probe: Option<usize>,
    hovered_probe: Option<usize>,
    camera: Camera,
    drag: Option<Drag>,
    scanner_bounds: Rc<Cell<Bounds<Pixels>>>,
    _camera_animation: Option<Task<()>>,

    name_input: Entity<InputState>,
    probe_inputs: Vec<[Entity<InputState>; 3]>,
    scale_input: Entity<InputState>,
    rotate_input: Entity<InputState>,
    rotate_axis: Axis,
    move_input: Entity<InputState>,
    move_direction: Direction,
    spread_input: Entity<InputState>,
    transform_tool: TransformTool,
    mirror_axis: Axis,
    collapsed: std::collections::HashSet<Section>,
    pattern: Pattern,
    pattern_axis: Axis,
    pattern_range: f64,
    _subscriptions: Vec<Subscription>,
}

impl FormationEditor {
    fn new(
        file_path: PathBuf,
        display_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let store = Store::global(cx);
        let mut subscriptions =
            vec![
                cx.subscribe_in(&store, window, |this, _, event, window, cx| {
                    if let StoreEvent::DataChanged = event {
                        this.on_disk_changed(window, cx);
                    }
                }),
            ];

        let name_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("formationEditor.name").to_string())
        });
        subscriptions.push(
            cx.subscribe_in(&name_input, window, |this, input, event, _, cx| {
                if let InputEvent::Change = event {
                    let name = input.read(cx).value().to_string();
                    this.typed_edit("name".into(), cx, |this| {
                        if let Some(f) = this.current_mut() {
                            f.name = name;
                        }
                    });
                }
            }),
        );

        let probe_inputs = (0..MAX_PROBES)
            .map(|row| {
                AXES.map(|info| {
                    let input = cx.new(|cx| InputState::new(window, cx));
                    let axis = info.axis;
                    subscriptions.push(cx.subscribe_in(
                        &input,
                        window,
                        move |this, input, event, _, cx| {
                            if let InputEvent::Change = event
                                && let Some(value) = parse_distance(&input.read(cx).value())
                            {
                                this.typed_edit(format!("probe-{}-{:?}", row, axis), cx, |this| {
                                    if let Some(p) =
                                        this.current_mut().and_then(|f| f.probes.get_mut(row))
                                    {
                                        p.set(axis, value);
                                    }
                                });
                            }
                        },
                    ));
                    input
                })
            })
            .collect();

        let number_input = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let value = value.to_string();
            cx.new(|cx| InputState::new(window, cx).default_value(value))
        };
        let scale_input = number_input("2", window, cx);
        let rotate_input = number_input("45", window, cx);
        let move_input = number_input("1000", window, cx);
        let spread_input = number_input("500", window, cx);

        window.set_window_title(&title(&display_name));
        let mut editor = Self {
            store,
            file_path,
            display_name,
            state: LoadState::Loading,
            formations: Vec::new(),
            saved: Vec::new(),
            selected: 0,
            history: History::default(),
            disk_changed: false,
            saving: false,
            focus_handle: cx.focus_handle(),
            selected_probe: None,
            hovered_probe: None,
            camera: Camera::default(),
            drag: None,
            scanner_bounds: Rc::new(Cell::new(Bounds::default())),
            _camera_animation: None,
            name_input,
            probe_inputs,
            scale_input,
            rotate_input,
            rotate_axis: Axis::Y,
            move_input,
            move_direction: Direction {
                axis: Axis::Z,
                sign: 1.,
            },
            spread_input,
            transform_tool: TransformTool::Scale,
            mirror_axis: Axis::X,
            collapsed: [Section::Pattern].into_iter().collect(),
            pattern: Pattern::Pinpoint,
            pattern_axis: Axis::Y,
            pattern_range: 32.,
            _subscriptions: subscriptions,
        };
        editor.load(window, cx);
        editor
    }

    fn current(&self) -> Option<&Formation> {
        self.formations.get(self.selected)
    }

    fn current_mut(&mut self) -> Option<&mut Formation> {
        self.formations.get_mut(self.selected)
    }

    fn is_dirty(&self) -> bool {
        self.formations != self.saved
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            formations: self.formations.clone(),
            selected: self.selected,
        }
    }

    fn read_formations(path: PathBuf) -> Result<Vec<Formation>, String> {
        formations::read_probe_formations(&path)
            .map(|stored| stored.iter().map(Formation::from_stored).collect())
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state = LoadState::Loading;
        self.disk_changed = false;
        cx.notify();
        let path = self.file_path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { Self::read_formations(path) })
                .await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(formations) => {
                        this.saved = formations.clone();
                        this.formations = formations;
                        this.selected = 0;
                        this.history.clear();
                        this.state = LoadState::Ready;
                    }
                    Err(e) => this.state = LoadState::Failed(e.into()),
                }
                this.after_structural_change(window, cx);
            })
            .ok();
        })
        .detach();
    }

    // Another window changed settings: follow the file unless that would
    // discard unsaved edits, in which case offer a reload instead.
    fn on_disk_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path = self.file_path.clone();
        let locations = self.store.read(cx).locations().clone();
        cx.spawn_in(window, async move |this, cx| {
            let (name, disk) = cx
                .background_spawn(async move {
                    (
                        scan::entry_display_name(&locations, &path),
                        Self::read_formations(path),
                    )
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                if let Ok(name) = name
                    && name != this.display_name
                {
                    window.set_window_title(&title(&name));
                    this.display_name = name;
                }
                let Ok(disk) = disk else {
                    return;
                };
                if disk == this.saved || !matches!(this.state, LoadState::Ready) {
                    return;
                }
                if this.is_dirty() {
                    this.disk_changed = true;
                } else {
                    this.saved = disk.clone();
                    this.formations = disk;
                    this.history.clear();
                    this.after_structural_change(window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // Keeps selections in range and pushes the model into the text inputs
    // after any change that didn't come from typing.
    fn after_structural_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = self.selected.min(self.formations.len().saturating_sub(1));
        let probe_count = self.current().map_or(0, |f| f.probes.len());
        self.selected_probe = self.selected_probe.filter(|i| *i < probe_count);
        self.hovered_probe = self.hovered_probe.filter(|i| *i < probe_count);

        let name = self.current().map(|f| f.name.clone()).unwrap_or_default();
        self.name_input
            .update(cx, |input, cx| input.set_value(name, window, cx));
        for row in 0..MAX_PROBES {
            self.sync_probe_row(row, window, cx);
        }
        cx.notify();
    }

    fn sync_probe_row(&self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let probe = self.current().and_then(|f| f.probes.get(row)).copied();
        for (info, input) in AXES.iter().zip(&self.probe_inputs[row]) {
            let text = probe
                .map(|p| format_number(p.get(info.axis)))
                .unwrap_or_default();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    // An undoable change made through buttons, menus or tools.
    fn edit(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        self.history.record(self.snapshot(), None);
        f(self);
        self.after_structural_change(window, cx);
    }

    // An undoable change typed into a field; one field's keystrokes collapse
    // into a single step and the field itself is left alone.
    fn typed_edit(&mut self, key: String, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        let before = self.snapshot();
        f(self);
        if self.formations != before.formations {
            self.history.record(before, Some(key));
        }
        cx.notify();
    }

    fn with_current(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Formation),
    ) {
        self.edit(window, cx, |this| {
            if let Some(formation) = this.current_mut() {
                f(formation);
            }
        });
    }

    fn restore(&mut self, snapshot: Snapshot, window: &mut Window, cx: &mut Context<Self>) {
        self.formations = snapshot.formations;
        self.selected = snapshot.selected;
        self.after_structural_change(window, cx);
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(previous) = self.history.undo(self.snapshot()) {
            self.restore(previous, window, cx);
        }
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.history.redo(self.snapshot()) {
            self.restore(next, window, cx);
        }
    }

    fn save_action(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_dirty() && !self.saving {
            self.save(window, cx);
        }
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index != self.selected {
            self.selected = index;
            self.selected_probe = None;
            self.after_structural_change(window, cx);
        }
    }

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(window, cx, |this| this.formations = this.saved.clone());
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let payload: Vec<ProbeFormation> = self
            .formations
            .iter()
            .enumerate()
            .map(|(i, f)| f.to_stored(i as i64))
            .collect();
        let backup = self.store.read(cx).config().auto_backup;
        let path = self.file_path.clone();
        self.saving = true;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let normalized: Vec<String> = payload.iter().map(|f| f.name.clone()).collect();
            let result = cx
                .background_spawn(async move {
                    formations::write_probe_formations(&path, &payload, backup)
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(()) => {
                        // Match what a re-read returns, so the saved state compares equal
                        for (formation, name) in this.formations.iter_mut().zip(normalized) {
                            formation.name = name;
                        }
                        this.saved = this.formations.clone();
                        this.disk_changed = false;
                        this.after_structural_change(window, cx);
                        ui::notify_success(
                            t!("formationEditor.saved"),
                            t!("formationEditor.savedDesc"),
                            window,
                            cx,
                        );
                        this.store.update(cx, |store, cx| store.reload(cx));
                    }
                    Err(e) => ui::notify_error(t!("formationEditor.saveFailed"), e, window, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn title(name: &str) -> String {
    t!("formationEditor.title", name = name).to_string()
}

pub(super) fn editor_action(
    view: &WeakEntity<FormationEditor>,
    f: impl Fn(&mut FormationEditor, &mut Window, &mut Context<FormationEditor>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, window, cx| {
        view.update(cx, |this, cx| f(this, window, cx)).ok();
    }
}

impl FormationEditor {
    fn render_title_bar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        TitleBar::new().child(ui::title_bar_content(
            window,
            h_flex()
                .gap_2()
                .min_w_0()
                .child(Icon::new(IconName::Radar).small())
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .font_semibold()
                        .child(title(&self.display_name)),
                ),
            Button::new("toggle-theme")
                .ghost()
                .small()
                .icon(if cx.theme().is_dark() {
                    IconName::Sun
                } else {
                    IconName::Moon
                })
                .tooltip(t!("titleBar.toggleTheme").to_string())
                .on_click(|_, window, cx| crate::toggle_theme(window, cx)),
        ))
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let dirty = self.is_dirty();
        h_flex()
            .flex_none()
            .h(px(52.))
            .gap_2()
            .px_4()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                Button::new("undo")
                    .ghost()
                    .small()
                    .icon(IconName::Undo)
                    .tooltip(t!("formationEditor.undo").to_string())
                    .disabled(!self.history.can_undo())
                    .on_click(cx.listener(|this, _, window, cx| this.undo(&Undo, window, cx))),
            )
            .child(
                Button::new("redo")
                    .ghost()
                    .small()
                    .icon(IconName::Redo)
                    .tooltip(t!("formationEditor.redo").to_string())
                    .disabled(!self.history.can_redo())
                    .on_click(cx.listener(|this, _, window, cx| this.redo(&Redo, window, cx))),
            )
            .when(dirty, |this| {
                this.child(
                    h_flex()
                        .ml_2()
                        .gap_1p5()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(div().size_1p5().rounded_full().bg(ui::amber(cx)))
                        .child(t!("formationEditor.unsaved").to_string()),
                )
            })
            .child(div().flex_1())
            .child(
                Button::new("reset")
                    .outline()
                    .small()
                    .label(t!("formationEditor.reset").to_string())
                    .disabled(self.saving || !dirty)
                    .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
            )
            .child(
                Button::new("save")
                    .primary()
                    .small()
                    .min_w(px(120.))
                    .label(t!("formationEditor.save").to_string())
                    .loading(self.saving)
                    .disabled(self.saving || !dirty)
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            )
    }
}

impl Render for FormationEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match &self.state {
            LoadState::Loading => div()
                .flex()
                .flex_1()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child(t!("formationEditor.loading").to_string())
                .into_any_element(),
            LoadState::Failed(error) => v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_3()
                .p_8()
                .text_center()
                .child(
                    div()
                        .font_medium()
                        .child(t!("formationEditor.loadFailed").to_string()),
                )
                .child(
                    div()
                        .max_w(px(512.))
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(error.clone()),
                )
                .child(
                    Button::new("retry")
                        .outline()
                        .small()
                        .label(t!("common.refresh").to_string())
                        .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
                )
                .into_any_element(),
            LoadState::Ready => v_flex()
                .flex_1()
                .min_h_0()
                .child(
                    h_flex()
                        .flex_1()
                        .min_h_0()
                        .items_stretch()
                        .child(self.render_list(cx))
                        .child(self.render_viewport(cx))
                        .child(self.render_inspector(cx)),
                )
                .child(self.render_footer(cx))
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::save_action))
            // Drags keep going when the pointer leaves the pane they started in
            .on_mouse_move(cx.listener(Self::on_drag_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_drag_end))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_drag_end))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(window, cx))
            .when(self.disk_changed, |this| {
                let amber = ui::amber(cx);
                this.child(
                    h_flex()
                        .justify_between()
                        .gap_2()
                        .px_4()
                        .py_1p5()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .bg(amber.opacity(0.1))
                        .child(
                            div()
                                .text_xs()
                                .text_color(amber)
                                .child(t!("formationEditor.fileChanged").to_string()),
                        )
                        .child(
                            Button::new("reload")
                                .outline()
                                .xsmall()
                                .label(t!("formationEditor.reload").to_string())
                                .on_click(cx.listener(|this, _, window, cx| this.load(window, cx))),
                        ),
                )
            })
            .child(body)
    }
}
