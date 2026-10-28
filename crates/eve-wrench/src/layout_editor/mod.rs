mod canvas;
mod inspector;
mod model;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

use eve_wrench_core::scan;
use eve_wrench_core::window_layout::{self, Neocom, WindowLayout, WindowRect, WindowState};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, TitleBar, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use self::canvas::{Drag, Pan, View};
use self::model::{History, Snapshot};
use crate::store::{Store, StoreEvent};
use crate::ui::{self, StyleExt as _};

actions!(layout_editor, [Undo, Redo, Save]);

const KEY_CONTEXT: &str = "LayoutEditor";

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
        window_bounds: Some(WindowBounds::centered(size(px(1360.), px(820.)), cx)),
        window_min_size: Some(size(px(1040.), px(600.))),
        app_id: Some("eve-wrench".into()),
        window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
        ..TitleBar::window_options()
    };
    let editor_path = path.clone();
    let opened = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| LayoutEditor::new(PathBuf::from(editor_path), name, window, cx))
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

// The client uses one layout for both, but shows some windows only docked
// and some only in space.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Location {
    InSpace,
    Docked,
}

// Rects where the client shows them, which is what the editor works on.
fn placed(mut layout: WindowLayout) -> WindowLayout {
    let neocom = layout.neocom;
    for window in &mut layout.windows {
        window.rect = window.rect.map(|r| neocom.place(r));
    }
    layout
}

// The four editable numbers of a rect, in inspector order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RectField {
    X,
    Y,
    Width,
    Height,
}

impl RectField {
    const ALL: [RectField; 4] = [
        RectField::X,
        RectField::Y,
        RectField::Width,
        RectField::Height,
    ];

    fn get(self, r: &WindowRect) -> i64 {
        match self {
            RectField::X => r.x,
            RectField::Y => r.y,
            RectField::Width => r.width,
            RectField::Height => r.height,
        }
    }

    fn set(self, r: &mut WindowRect, value: i64) {
        match self {
            RectField::X => r.x = value,
            RectField::Y => r.y = value,
            RectField::Width => r.width = value.max(model::MIN_WIDTH),
            RectField::Height => r.height = value.max(model::MIN_HEIGHT),
        }
    }

    fn label(self) -> &'static str {
        match self {
            RectField::X => "X",
            RectField::Y => "Y",
            RectField::Width => "W",
            RectField::Height => "H",
        }
    }
}

pub struct LayoutEditor {
    store: Entity<Store>,
    file_path: PathBuf,
    display_name: String,
    state: LoadState,
    // As shown in game, see `placed`
    windows: Vec<WindowState>,
    saved: Vec<WindowState>,
    neocom: Neocom,
    compact_default: bool,
    history: History,
    disk_changed: bool,
    saving: bool,
    focus_handle: FocusHandle,

    selected: Option<String>,
    hovered: Option<String>,
    drag: Option<Drag>,
    pan: Option<Pan>,
    view: View,
    canvas_bounds: Rc<Cell<Bounds<Pixels>>>,
    snap: bool,
    show_closed: bool,
    location: Location,
    rect_inputs: [Entity<InputState>; 4],
    _subscriptions: Vec<Subscription>,
}

impl LayoutEditor {
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

        let rect_inputs = RectField::ALL.map(|field| {
            let input = cx.new(|cx| InputState::new(window, cx));
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, input, event, _, cx| {
                    if let InputEvent::Change = event
                        && let Ok(value) = input.read(cx).value().trim().parse::<i64>()
                    {
                        this.typed_edit(format!("rect-{field:?}"), cx, |this| {
                            this.update_selected_rect(|r| field.set(r, value));
                        });
                    }
                },
            ));
            input
        });

        window.set_window_title(&title(&display_name));
        let mut editor = Self {
            store,
            file_path,
            display_name,
            state: LoadState::Loading,
            windows: Vec::new(),
            saved: Vec::new(),
            neocom: Neocom::default(),
            compact_default: false,
            history: History::default(),
            disk_changed: false,
            saving: false,
            focus_handle: cx.focus_handle(),
            selected: None,
            hovered: None,
            drag: None,
            pan: None,
            view: View::default(),
            canvas_bounds: Rc::new(Cell::new(Bounds::default())),
            snap: true,
            show_closed: false,
            location: Location::InSpace,
            rect_inputs,
            _subscriptions: subscriptions,
        };
        editor.load(window, cx);
        editor
    }

    fn is_dirty(&self) -> bool {
        self.windows != self.saved
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            windows: self.windows.clone(),
            selected: self.selected.clone(),
        }
    }

    fn selected_window(&self) -> Option<&WindowState> {
        let key = self.selected.as_ref()?;
        self.windows.iter().find(|w| &w.key == key)
    }

    fn update_selected_rect(&mut self, f: impl FnOnce(&mut WindowRect)) {
        let Some(window) = self.selected_window() else {
            return;
        };
        let Some(mut rect) = window.rect else {
            return;
        };
        let key = window.key.clone();
        f(&mut rect);
        model::set_rect(&mut self.windows, &key, rect);
    }

    fn update_selected(&mut self, f: impl FnOnce(&mut WindowState)) {
        let Some(key) = self.selected.clone() else {
            return;
        };
        if let Some(window) = self.windows.iter_mut().find(|w| w.key == key) {
            f(window);
        }
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state = LoadState::Loading;
        self.disk_changed = false;
        cx.notify();
        let path = self.file_path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { window_layout::read_window_layout(&path) })
                .await;
            this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(layout) => {
                        let layout = placed(layout);
                        this.neocom = layout.neocom;
                        this.compact_default = layout.compact_default;
                        this.saved = layout.windows.clone();
                        this.windows = layout.windows;
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
                        window_layout::read_window_layout(&path),
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
                let Ok(layout) = disk.map(placed) else {
                    return;
                };
                this.neocom = layout.neocom;
                this.compact_default = layout.compact_default;
                let disk = layout.windows;
                if disk == this.saved || !matches!(this.state, LoadState::Ready) {
                    return;
                }
                if this.is_dirty() {
                    this.disk_changed = true;
                } else {
                    this.saved = disk.clone();
                    this.windows = disk;
                    this.history.clear();
                    this.after_structural_change(window, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // Keeps the selection valid and pushes the model into the text inputs
    // after any change that didn't come from typing.
    fn after_structural_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_window().is_none() {
            self.selected = None;
        }
        self.sync_inputs(window, cx);
        cx.notify();
    }

    fn sync_inputs(&self, window: &mut Window, cx: &mut Context<Self>) {
        let rect = self.selected_window().and_then(|w| w.rect);
        for (field, input) in RectField::ALL.iter().zip(&self.rect_inputs) {
            let text = rect.map(|r| field.get(&r).to_string()).unwrap_or_default();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    // An undoable change made through buttons, toggles or the canvas.
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
        if self.windows != before.windows {
            self.history.record(before, Some(key));
        }
        cx.notify();
    }

    fn select(&mut self, key: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if key != self.selected {
            self.selected = key;
            self.after_structural_change(window, cx);
        }
    }

    fn restore(&mut self, snapshot: Snapshot, window: &mut Window, cx: &mut Context<Self>) {
        self.windows = snapshot.windows;
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

    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.edit(window, cx, |this| this.windows = this.saved.clone());
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = self.windows.clone();
        let neocom = self.neocom;
        let payload: Vec<WindowState> = shown
            .iter()
            .cloned()
            .map(|mut w| {
                w.rect = w.rect.map(|r| neocom.store(r));
                w
            })
            .collect();
        let backup = self.store.read(cx).config().auto_backup;
        let path = self.file_path.clone();
        self.saving = true;
        cx.notify();

        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    window_layout::write_window_layout(&path, &payload, backup)
                })
                .await;
            this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(()) => {
                        this.saved = shown;
                        this.disk_changed = false;
                        ui::notify_success(
                            t!("layoutEditor.saved"),
                            t!("layoutEditor.savedDesc"),
                            window,
                            cx,
                        );
                        this.store.update(cx, |store, cx| store.reload(cx));
                    }
                    Err(e) => ui::notify_error(t!("layoutEditor.saveFailed"), e, window, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

fn title(name: &str) -> String {
    t!("layoutEditor.title", name = name).to_string()
}

impl LayoutEditor {
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
                .child(Icon::new(IconName::PanelsTopLeft).small())
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
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("layoutEditor.closeClient").to_string()),
            )
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
                    .label(t!("layoutEditor.save").to_string())
                    .loading(self.saving)
                    .disabled(self.saving || !dirty)
                    .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
            )
    }
}

impl Render for LayoutEditor {
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
                        .child(t!("layoutEditor.loadFailed").to_string()),
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
                        .child(self.render_canvas(cx))
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
            // Drags and pans keep going when the pointer leaves the canvas
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
