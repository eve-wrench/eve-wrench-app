mod actions;
mod browser;
mod copy_panel;
mod dialogs;
mod settings_panel;
mod title_bar;

use std::collections::{HashMap, HashSet};

use eve_wrench_core::copy::COPY_GROUPS;
use eve_wrench_core::{BackupEntry, Server, SettingsEntry, SettingsKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::InputState;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, TitleBar, h_flex, v_flex};
use gpui_kit::*;

use crate::ui::StyleExt as _;
use rust_i18n::t;

use crate::store::{Store, StoreEvent};
use crate::ui;

pub fn open(cx: &mut App) {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::centered(size(px(1000.), px(700.)), cx)),
        window_min_size: Some(size(px(1000.), px(600.))),
        app_id: Some("eve-wrench".into()),
        window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
        ..TitleBar::window_options()
    };
    gpui_kit::open_window(options, cx, |window, cx| {
        window.set_window_title("EVE Wrench");
        cx.new(|cx| MainView::new(window, cx))
    })
    .expect("failed to open the main window");
}

#[derive(Clone, PartialEq)]
pub enum Source {
    Entry(SettingsEntry),
    Backup(BackupEntry),
}

impl Source {
    pub fn kind(&self) -> SettingsKind {
        match self {
            Source::Entry(e) => e.kind,
            Source::Backup(b) => b.kind,
        }
    }

    pub fn path(&self) -> &str {
        match self {
            Source::Entry(e) => &e.path,
            Source::Backup(b) => &b.path,
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            Source::Entry(e) => &e.display_name,
            Source::Backup(b) => &b.name,
        }
    }
}

// Payload while a row is dragged onto the copy panel.
#[derive(Clone)]
pub struct DraggedSettings(pub Source);

pub struct DragPreview {
    // Where the drag started inside the row; GPUI anchors the preview there,
    // so it's shifted by this much to sit at the pointer instead
    grab: Point<Pixels>,
    kind: SettingsKind,
    name: SharedString,
    portrait: Option<std::sync::Arc<Image>>,
}

impl DragPreview {
    pub fn new(
        source: &Source,
        grab: Point<Pixels>,
        portrait: Option<std::sync::Arc<Image>>,
    ) -> Self {
        Self {
            grab,
            kind: source.kind(),
            name: source.display_name().to_string().into(),
            portrait,
        }
    }
}

impl Render for DragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pill = h_flex()
            .gap_2()
            .pl_1p5()
            .pr_3()
            .py_1()
            .rounded(px(6.))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md()
            .text_sm()
            .text_color(cx.theme().popover_foreground)
            .child(ui::entry_avatar(
                self.kind,
                self.portrait.clone(),
                px(20.),
                cx,
            ))
            .child(self.name.clone());
        div()
            .pl(self.grab.x + px(8.))
            .pt(self.grab.y + px(8.))
            .child(pill)
    }
}

#[derive(Clone, Copy, PartialEq)]
enum SortColumn {
    Name,
    Time,
}

#[derive(Clone, Copy)]
struct Sort {
    column: SortColumn,
    ascending: bool,
}

impl Sort {
    // Clicking the active column flips direction; a new column starts
    // ascending, except dates when `newest_first` is set.
    fn toggle(&mut self, column: SortColumn, newest_first: bool) {
        if self.column == column {
            self.ascending = !self.ascending;
        } else {
            self.column = column;
            self.ascending = !(newest_first && column == SortColumn::Time);
        }
    }

    fn apply(self, ordering: std::cmp::Ordering) -> std::cmp::Ordering {
        if self.ascending {
            ordering
        } else {
            ordering.reverse()
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Server(Server),
    Backups,
}

struct AliasEdit {
    id: String,
    input: Entity<InputState>,
    _subscription: Subscription,
}

pub struct MainView {
    store: Entity<Store>,
    tab: Option<Tab>,
    source: Option<Source>,
    targets: Vec<SettingsEntry>,
    groups: HashMap<&'static str, bool>,
    copying: bool,
    account_sort: Sort,
    character_sort: Sort,
    backup_sort: Sort,
    selected_backups: HashSet<String>,
    alias_edit: Option<AliasEdit>,
    announce_refresh: bool,
    browser_scroll: ScrollHandle,
    settings_panel: Entity<settings_panel::SettingsPanel>,
    _dialog_subscription: Option<Subscription>,
    _subscriptions: Vec<Subscription>,
}

impl MainView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let store = Store::global(cx);
        let this = cx.weak_entity();
        let settings_panel = cx.new(|cx| settings_panel::SettingsPanel::new(this, cx));
        let subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(&store, window, |this, _, event, window, cx| match event {
                StoreEvent::DataChanged => this.on_data_changed(window, cx),
                StoreEvent::LoadFailed(error) => {
                    this.announce_refresh = false;
                    ui::notify_error(t!("toast.loadDataFailed"), error.clone(), window, cx);
                }
            }),
        ];

        cx.spawn_in(window, async move |this, cx| {
            let update = cx.background_spawn(async { crate::updater::check() }).await;
            if let Ok(Some(info)) = update {
                this.update_in(cx, |this, window, cx| {
                    this.show_update_dialog(info, window, cx)
                })
                .ok();
            }
        })
        .detach();

        let name_sort = Sort {
            column: SortColumn::Name,
            ascending: true,
        };
        Self {
            store,
            tab: None,
            source: None,
            targets: Vec::new(),
            groups: COPY_GROUPS.iter().map(|g| (g.id, g.default_on)).collect(),
            copying: false,
            account_sort: name_sort,
            character_sort: name_sort,
            backup_sort: Sort {
                column: SortColumn::Time,
                ascending: false,
            },
            selected_backups: HashSet::new(),
            alias_edit: None,
            browser_scroll: ScrollHandle::new(),
            settings_panel,
            announce_refresh: false,
            _dialog_subscription: None,
            _subscriptions: subscriptions,
        }
    }

    // Keeps the selection pointing at current data: entries are refreshed
    // (new names, portraits) and anything deleted on disk is dropped.
    fn on_data_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(data) = self.store.read(cx).data() else {
            return;
        };

        self.source = match self.source.take() {
            Some(Source::Entry(e)) => data.find_entry(&e.path).cloned().map(Source::Entry),
            Some(Source::Backup(b)) => data
                .backups
                .iter()
                .find(|x| x.path == b.path)
                .cloned()
                .map(Source::Backup),
            None => None,
        };
        self.targets = self
            .targets
            .iter()
            .filter_map(|t| data.find_entry(&t.path).cloned())
            .collect();
        let backup_ids: HashSet<&str> = data.backups.iter().map(|b| b.id.as_str()).collect();
        self.selected_backups
            .retain(|id| backup_ids.contains(id.as_str()));

        let tab_exists = match self.tab {
            Some(Tab::Server(server)) => data.servers.iter().any(|s| s.info.id == server),
            Some(Tab::Backups) => !data.backups.is_empty(),
            None => false,
        };
        if !tab_exists {
            self.tab = data.servers.first().map(|s| Tab::Server(s.info.id));
        }

        if std::mem::take(&mut self.announce_refresh) && !data.is_empty() {
            let message = t!(
                "toast.dataRefreshedDesc",
                servers = data.servers.len(),
                backups = data.backups.len()
            );
            ui::notify_success(t!("toast.dataRefreshed"), message, window, cx);
        }
        cx.notify();
    }

    fn render_loading(&self, cx: &App) -> impl IntoElement + use<> {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .child(Spinner::new().large())
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("common.loading").to_string()),
            )
    }

    fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .text_center()
            .child(
                Icon::new(IconName::Rocket)
                    .size(px(48.))
                    .text_color(cx.theme().muted_foreground),
            )
            .child(
                div()
                    .font_semibold()
                    .child(t!("empty.noEveInstallations").to_string()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("empty.noEveInstallationsDesc").to_string()),
            )
            .child(
                Button::new("set-custom-path")
                    .outline()
                    .small()
                    .mt_2()
                    .icon(IconName::FolderOpen)
                    .label(t!("settings.setCustomPath").to_string())
                    .on_click(
                        cx.listener(|this, _, window, cx| this.select_custom_eve_path(window, cx)),
                    ),
            )
    }
}

impl Render for MainView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match self.store.read(cx).data().map(|d| d.is_empty()) {
            None => self.render_loading(cx).into_any_element(),
            Some(true) => self.render_empty(cx).into_any_element(),
            Some(false) => h_flex()
                .flex_1()
                .items_stretch()
                .overflow_hidden()
                .child(self.render_browser(window, cx))
                .child(self.render_copy_panel(cx))
                .into_any_element(),
        };

        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(window, cx))
            .child(body)
    }
}
