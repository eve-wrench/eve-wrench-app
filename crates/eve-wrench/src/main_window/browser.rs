use std::ops::Range;
use std::rc::Rc;

use eve_wrench_core::{AppData, BackupEntry, ProfileData, ServerData, SettingsEntry, SettingsKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::title_bar::view_action;
use super::{DragPreview, DraggedSettings, MainView, Sort, SortColumn, Source, Tab};
use crate::table::{self, Column};
use crate::ui::{self, StyleExt as _};

// Avatar, name, modified, row actions
const ENTRY_COLUMNS: [Column; 4] = [
    Column::fixed(px(36.)),
    Column::fill(),
    Column::fixed(px(112.)),
    Column::fixed(px(96.)).end(),
];

// Selection, avatar, name, date, menu
const BACKUP_COLUMNS: [Column; 5] = [
    Column::fixed(px(32.)),
    Column::fixed(px(36.)),
    Column::fill(),
    Column::fixed(px(112.)),
    Column::fixed(px(40.)).end(),
];

// Per-render labels, looked up once instead of once per row.
struct RowLabels {
    source: SharedString,
    target: SharedString,
}

impl MainView {
    pub(super) fn render_browser(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Some(data) = self.store.read(cx).data() else {
            return div().flex_1();
        };

        let mut tabs: Vec<Tab> = data
            .servers
            .iter()
            .map(|s| Tab::Server(s.info.id))
            .collect();
        if !data.backups.is_empty() {
            tabs.push(Tab::Backups);
        }
        let active = self
            .tab
            .filter(|t| tabs.contains(t))
            .or(tabs.first().copied());

        let content = match active {
            Some(Tab::Server(server)) => data
                .servers
                .iter()
                .find(|s| s.info.id == server)
                .map(|s| self.render_server(s, &data, window, cx)),
            Some(Tab::Backups) => Some(self.render_backups(&data, cx).into_any_element()),
            None => None,
        };

        v_flex()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .text_sm()
            .child(self.render_tab_strip(&tabs, active, &data, cx))
            .children(content)
    }

    fn render_tab_strip(
        &self,
        tabs: &[Tab],
        active: Option<Tab>,
        data: &AppData,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (fg, muted, border) = (theme.foreground, theme.muted_foreground, theme.border);

        let items: Vec<AnyElement> = tabs
            .iter()
            .map(|tab| {
                let selected = active == Some(*tab);
                let (id, accent, lead, label, count) = match tab {
                    Tab::Server(server) => {
                        let color = ui::server_color(*server);
                        (
                            SharedString::from(format!("tab-{}", server.id())),
                            color,
                            div()
                                .size(px(6.))
                                .rounded_full()
                                .bg(color)
                                .into_any_element(),
                            SharedString::from(server.display_name()),
                            None,
                        )
                    }
                    Tab::Backups => (
                        SharedString::from("tab-backups"),
                        fg,
                        Icon::new(IconName::Archive).xsmall().into_any_element(),
                        SharedString::from(t!("titleBar.backups").to_string()),
                        Some(data.backups.len()),
                    ),
                };
                let tab = *tab;
                h_flex()
                    .id(id)
                    .relative()
                    .h_full()
                    .px_3()
                    .gap_2()
                    .cursor_pointer()
                    .text_size(px(13.))
                    .font_medium()
                    .text_color(if selected { fg } else { muted })
                    .when(!selected, |this| this.hover(move |s| s.text_color(fg)))
                    .child(lead)
                    .child(label)
                    .children(count.map(|n| {
                        div()
                            .text_xs()
                            .text_color(muted.opacity(0.8))
                            .child(n.to_string())
                    }))
                    .when(selected, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left_3()
                                .right_3()
                                .bottom_0()
                                .h(px(2.))
                                .rounded_t(px(2.))
                                .bg(accent),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.tab != Some(tab) {
                            this.tab = Some(tab);
                            this.alias_edit = None;
                            this.browser_scroll.set_offset(point(px(0.), px(0.)));
                            cx.notify();
                        }
                    }))
                    .into_any_element()
            })
            .collect();

        h_flex()
            .flex_none()
            .h(px(40.))
            .px_1()
            .border_b_1()
            .border_color(border)
            .children(items)
    }

    // Sections are laid out with fixed heights, so each header's offset in the
    // scroll content is known up front. On every scroll the view re-renders
    // and pins a copy of the current header to the top; the next header
    // pushes it out as it arrives.
    fn render_server(
        &self,
        server: &ServerData,
        data: &Rc<AppData>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let labels = RowLabels {
            source: t!("actions.source").to_string().into(),
            target: t!("actions.target").to_string().into(),
        };
        let sections: Vec<(&ProfileData, SettingsKind)> = server
            .profiles
            .iter()
            .flat_map(|profile| {
                [SettingsKind::User, SettingsKind::Char]
                    .into_iter()
                    .filter(|kind| !profile.entries(*kind).is_empty())
                    .map(move |kind| (profile, kind))
            })
            .collect();

        let mut tops = Vec::with_capacity(sections.len() + 1);
        let mut y = px(0.);
        for (profile, kind) in &sections {
            tops.push(y);
            y += ui::HEADER_HEIGHT + ui::ROW_HEIGHT * profile.entries(*kind).len() as f32;
        }
        tops.push(y);

        let scroll_top = -self.browser_scroll.offset().y;
        let pinned = tops.iter().rposition(|top| *top < scroll_top).map(|index| {
            let next = tops.get(index + 1).copied();
            let shift = next
                .map(|next| (next - scroll_top - ui::HEADER_HEIGHT).min(px(0.)))
                .unwrap_or(px(0.));
            let header = match sections.get(index) {
                Some((profile, kind)) => self.profile_header(profile, *kind, "pinned", cx),
                None => self.extra_header(cx),
            };
            div().absolute().top(shift).left_0().right_0().child(
                header.bg(cx.theme().background).child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(cx.theme().muted.opacity(0.35)),
                ),
            )
        });

        let section_elements: Vec<AnyElement> = sections
            .iter()
            .map(|(profile, kind)| {
                let mut entries: Vec<&SettingsEntry> = profile.entries(*kind).iter().collect();
                self.sort_entries(&mut entries, *kind);
                let header = self.profile_header(profile, *kind, "inline", cx);
                let rows: Vec<AnyElement> = entries
                    .into_iter()
                    .map(|entry| self.render_entry_row(entry, data, &labels, window, cx))
                    .collect();
                v_flex().child(header).children(rows).into_any_element()
            })
            .collect();

        let server_path = server.info.server_path.clone();
        let muted = cx.theme().muted_foreground;
        let content = v_flex()
            .children(section_elements)
            .child(self.extra_header(cx))
            .child(
                h_flex()
                    .justify_between()
                    .gap_6()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap_0p5()
                            .child(
                                div()
                                    .font_medium()
                                    .child(t!("extra.alwaysShowBracketText").to_string()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted)
                                    .whitespace_normal()
                                    .child(t!("extra.alwaysShowBracketTextDesc").to_string()),
                            ),
                    )
                    .child(
                        div().flex_none().child(
                            Switch::new(SharedString::from(format!(
                                "brackets-{}",
                                server.info.id.id()
                            )))
                            .checked(server.info.brackets_always_show)
                            .on_change(cx.listener(
                                move |this, enabled: &bool, window, cx| {
                                    this.set_brackets_always_show(
                                        server_path.clone(),
                                        *enabled,
                                        window,
                                        cx,
                                    )
                                },
                            )),
                        ),
                    ),
            );

        // Clipped so a header being pushed out never paints over the tabs
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .child(
                div()
                    .id("browser-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.browser_scroll)
                    .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                    .child(content),
            )
            .children(pinned)
            .into_any_element()
    }

    fn sort_entries(&self, entries: &mut [&SettingsEntry], kind: SettingsKind) {
        let sort = match kind {
            SettingsKind::User => self.account_sort,
            SettingsKind::Char => self.character_sort,
        };
        entries.sort_by(|a, b| {
            sort.apply(match sort.column {
                SortColumn::Name => a
                    .sort_name()
                    .to_lowercase()
                    .cmp(&b.sort_name().to_lowercase()),
                SortColumn::Time => a.modified_time.cmp(&b.modified_time),
            })
        });
    }

    fn extra_header(&self, cx: &App) -> Div {
        ui::section_header(cx).child(ui::section_title(t!("extra.title").to_string()))
    }

    // The section header doubles as the column header: the title sorts by
    // name, the date label sorts by date.
    fn sort_control(
        &self,
        id: SharedString,
        label: impl IntoElement,
        column: SortColumn,
        sort: Sort,
        on_sort: impl Fn(&mut Self, SortColumn) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let arrow = (sort.column == column).then(|| {
            Icon::new(if sort.ascending {
                IconName::ChevronUp
            } else {
                IconName::ChevronDown
            })
            .xsmall()
        });
        let hover = cx.theme().foreground;
        h_flex()
            .id(id)
            .gap_1()
            .cursor_pointer()
            .hover(move |s| s.text_color(hover))
            .child(label)
            .children(arrow)
            .on_click(cx.listener(move |this, _, _, cx| {
                on_sort(this, column);
                cx.notify();
            }))
    }

    fn profile_header(
        &self,
        profile: &ProfileData,
        kind: SettingsKind,
        variant: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let sort = match kind {
            SettingsKind::User => self.account_sort,
            SettingsKind::Char => self.character_sort,
        };
        let title = match kind {
            SettingsKind::User => t!("list.accounts"),
            SettingsKind::Char => t!("list.characters"),
        };
        let section_id = format!("{}-{:?}-{}", profile.path, kind, variant);
        let set_sort = move |this: &mut Self, column| match kind {
            SettingsKind::User => this.account_sort.toggle(column, false),
            SettingsKind::Char => this.character_sort.toggle(column, false),
        };
        let can_add_all = self.source.as_ref().is_some_and(|s| s.kind() == kind);
        let show_profile = !profile.name.eq_ignore_ascii_case("default");
        let count = profile.entries(kind).len();

        let name = self.sort_control(
            format!("sort-name-{}", section_id).into(),
            h_flex()
                .gap_2()
                .child(ui::section_title(title.to_string()))
                .child(ui::count_label(count, cx))
                .when(show_profile, |this| {
                    this.child(format!("· {}", profile.name))
                }),
            SortColumn::Name,
            sort,
            set_sort,
            cx,
        );
        let modified = self.sort_control(
            format!("sort-time-{}", section_id).into(),
            ui::section_title(t!("table.modified").to_string()),
            SortColumn::Time,
            sort,
            set_sort,
            cx,
        );
        // Always shown so the action is discoverable; it only becomes active
        // once a source of this kind is selected
        let add_all_hint = match &self.source {
            None => Some(t!("toast.noSourceSelectedDesc")),
            Some(_) if !can_add_all => Some(t!("toast.typeMismatchDesc")),
            Some(_) => None,
        };
        let add_all = {
            let profile = profile.clone();
            Button::new(SharedString::from(format!("add-all-{}", section_id)))
                .ghost()
                .xsmall()
                .icon(IconName::ArrowDownToLine)
                .label(t!("table.addAll").to_string())
                .disabled(!can_add_all)
                .when_some(add_all_hint, |this, hint| this.tooltip(hint.to_string()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.add_all_from_profile(&profile, kind, window, cx)
                }))
                .into_any_element()
        };

        table::cells(
            ui::section_header(cx).gap_0(),
            &ENTRY_COLUMNS,
            [
                table::empty(),
                name.into_any_element(),
                modified.into_any_element(),
                add_all,
            ],
        )
    }

    fn render_entry_row(
        &self,
        entry: &SettingsEntry,
        data: &Rc<AppData>,
        labels: &RowLabels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_source = matches!(&self.source, Some(Source::Entry(e)) if e.path == entry.path);
        let is_target = self.targets.iter().any(|t| t.path == entry.path);
        let can_target = self.source.as_ref().is_some_and(|s| s.kind() == entry.kind);
        let marker = if is_source {
            Some(ui::source_color())
        } else if is_target {
            Some(ui::target_color())
        } else {
            None
        };
        let portrait = self.store.read(cx).portrait(&entry.id);
        let row_id = SharedString::from(entry.path.clone());

        let actions = h_flex()
            .gap_1()
            .child(
                Button::new(SharedString::from(format!("{}:source", row_id)))
                    .ghost()
                    .small()
                    .icon(IconName::ArrowUpFromLine)
                    .disabled(is_source)
                    .tooltip(labels.source.clone())
                    .on_click(cx.listener({
                        let entry = entry.clone();
                        move |this, _, _, cx| this.set_source(Source::Entry(entry.clone()), cx)
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("{}:target", row_id)))
                    .ghost()
                    .small()
                    .icon(IconName::ArrowDownToLine)
                    .disabled(!can_target)
                    .tooltip(labels.target.clone())
                    .on_click(cx.listener({
                        let entry = entry.clone();
                        move |this, _, window, cx| this.add_target(entry.clone(), window, cx)
                    })),
            )
            .child(self.render_entry_menu(entry, data, row_id.clone(), cx));

        let dragged = Source::Entry(entry.clone());
        let drag_portrait = portrait.clone();
        table::cells(
            ui::list_row(ElementId::Name(row_id), marker, cx)
                .h(ui::ROW_HEIGHT)
                .group("entry-row")
                .on_drag(DraggedSettings(dragged), move |dragged, grab, _, cx| {
                    let portrait = drag_portrait.clone();
                    cx.new(|_| DragPreview::new(&dragged.0, grab, portrait))
                }),
            &ENTRY_COLUMNS,
            [
                ui::entry_avatar(entry.kind, portrait, px(24.), cx),
                self.render_entry_name(entry, window, cx),
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(ui::relative_time(entry.modified_time))
                    .into_any_element(),
                actions.into_any_element(),
            ],
        )
        .into_any_element()
    }

    fn render_entry_name(
        &self,
        entry: &SettingsEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(character) = &entry.character {
            return h_flex()
                .gap_2()
                .min_w_0()
                .child(div().truncate().child(character.name.clone()))
                .children(character.corporation.clone().map(|corp| {
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(corp)
                }))
                .into_any_element();
        }

        if let Some(edit) = self.alias_edit.as_ref().filter(|e| e.id == entry.id) {
            let id = entry.id.clone();
            let input = edit.input.clone();
            return h_flex()
                .gap_1()
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "escape" {
                        this.alias_edit = None;
                        cx.notify();
                    }
                }))
                .child(div().w(px(220.)).child(Input::new(&edit.input).small()))
                .child(
                    Button::new("alias-save")
                        .ghost()
                        .small()
                        .icon(IconName::Check)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            let alias = input.read(cx).value().to_string();
                            this.save_alias(id.clone(), alias, window, cx)
                        })),
                )
                .child(
                    Button::new("alias-cancel")
                        .ghost()
                        .small()
                        .icon(IconName::X)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.alias_edit = None;
                            cx.notify();
                        })),
                )
                .into_any_element();
        }

        let entry_for_edit = entry.clone();
        h_flex()
            .gap_2()
            .min_w_0()
            .child(
                div()
                    .truncate()
                    .child(entry.alias.clone().unwrap_or_else(|| entry.id.clone())),
            )
            .when(entry.alias.is_some(), |this| {
                this.child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(entry.id.clone()),
                )
            })
            .child(
                div()
                    .opacity(0.)
                    .group_hover("entry-row", |s| s.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("alias-edit-{}", entry.path)))
                            .ghost()
                            .xsmall()
                            .icon(IconName::Pencil)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_alias_edit(&entry_for_edit, window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    // Menu contents are built only when the menu opens.
    fn render_entry_menu(
        &self,
        entry: &SettingsEntry,
        data: &Rc<AppData>,
        row_id: SharedString,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        let entry = entry.clone();
        let data = data.clone();

        Button::new(SharedString::from(format!("{}:menu", row_id)))
            .ghost()
            .small()
            .icon(IconName::Ellipsis)
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                let menu = menu
                    .when(entry.kind == SettingsKind::User, |menu| {
                        let entry = entry.clone();
                        menu.item(
                            PopupMenuItem::new(t!("actions.editFormations").to_string())
                                .icon(IconName::Radar)
                                .on_click(move |_, _, cx| {
                                    crate::formation_editor::open(
                                        entry.path.clone(),
                                        entry.display_name.clone(),
                                        cx,
                                    )
                                }),
                        )
                    })
                    .item(
                        PopupMenuItem::new(t!("actions.createBackup").to_string())
                            .icon(IconName::Save)
                            .on_click(view_action(&view, {
                                let entry = entry.clone();
                                move |this, window, cx| {
                                    this.create_backup(entry.clone(), window, cx)
                                }
                            })),
                    );

                let backups: Vec<BackupEntry> = data.backups_for(&entry).cloned().collect();
                if backups.is_empty() {
                    return menu.item(
                        PopupMenuItem::new(t!("actions.noBackupsAvailable").to_string())
                            .icon(IconName::RotateCcw)
                            .disabled(true),
                    );
                }

                let view = view.clone();
                let entry = entry.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(IconName::RotateCcw)),
                    t!("actions.restoreFromBackup").to_string(),
                    window,
                    cx,
                    move |menu, _, _| {
                        backups.iter().fold(
                            menu.max_h(px(320.)).scrollable(true),
                            |menu, backup| {
                                let entry = entry.clone();
                                let backup = backup.clone();
                                menu.item(
                                    PopupMenuItem::new(format!(
                                        "{}  ·  {}",
                                        backup.name,
                                        ui::relative_time(backup.timestamp)
                                    ))
                                    .on_click(view_action(
                                        &view,
                                        move |this, window, cx| {
                                            this.restore_backup(
                                                entry.clone(),
                                                backup.clone(),
                                                window,
                                                cx,
                                            )
                                        },
                                    )),
                                )
                            },
                        )
                    },
                )
            })
    }

    fn sorted_backups(&self, data: &AppData) -> Vec<BackupEntry> {
        let sort = self.backup_sort;
        let mut backups = data.backups.clone();
        backups.sort_by(|a, b| {
            sort.apply(match sort.column {
                SortColumn::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortColumn::Time => a.timestamp.cmp(&b.timestamp),
            })
        });
        backups
    }

    fn render_backups(
        &self,
        data: &Rc<AppData>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let sort = self.backup_sort;
        let all_selected = !data.backups.is_empty()
            && data
                .backups
                .iter()
                .all(|b| self.selected_backups.contains(&b.id));
        let selected_count = self.selected_backups.len();
        let all_ids: Vec<String> = data.backups.iter().map(|b| b.id.clone()).collect();
        let set_sort = |this: &mut Self, column| this.backup_sort.toggle(column, true);

        let header = ui::section_header(cx)
            .gap_0()
            .child(table::cell(
                &BACKUP_COLUMNS[0],
                Checkbox::new("backups-select-all")
                    .checked(all_selected)
                    .on_change(cx.listener(move |this, checked: &bool, _, cx| {
                        this.selected_backups = if *checked {
                            all_ids.iter().cloned().collect()
                        } else {
                            Default::default()
                        };
                        cx.notify();
                    })),
            ))
            .child(table::cell(&BACKUP_COLUMNS[1], table::empty()))
            .child(table::cell(
                &BACKUP_COLUMNS[2],
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        self.sort_control(
                            "sort-backup-name".into(),
                            h_flex()
                                .gap_2()
                                .child(ui::section_title(t!("titleBar.backups").to_string()))
                                .child(ui::count_label(data.backups.len(), cx)),
                            SortColumn::Name,
                            sort,
                            set_sort,
                            cx,
                        ),
                    )
                    .when(selected_count > 0, |this| {
                        this.child(
                            h_flex()
                                .gap_2()
                                .pr_4()
                                .child(
                                    t!("backup.selectedCount", count = selected_count).to_string(),
                                )
                                .child(
                                    Button::new("delete-selected-backups")
                                        .danger()
                                        .xsmall()
                                        .icon(IconName::Trash)
                                        .label(t!("backup.deleteSelected").to_string())
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.delete_selected_backups(window, cx)
                                        })),
                                ),
                        )
                    }),
            ))
            .child(table::cell(
                &BACKUP_COLUMNS[3],
                self.sort_control(
                    "sort-backup-time".into(),
                    ui::section_title(t!("backup.date").to_string()),
                    SortColumn::Time,
                    sort,
                    set_sort,
                    cx,
                ),
            ))
            .child(table::cell(&BACKUP_COLUMNS[4], table::empty()));

        // Virtualized: only visible rows are built, so long backup histories stay cheap
        v_flex().flex_1().min_h_0().child(header).child(
            uniform_list(
                "backups",
                data.backups.len(),
                cx.processor(|this, range: Range<usize>, _, cx| this.render_backup_rows(range, cx)),
            )
            .flex_1(),
        )
    }

    fn render_backup_rows(
        &mut self,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(data) = self.store.read(cx).data() else {
            return Vec::new();
        };
        let backups = self.sorted_backups(&data);
        backups
            .get(range)
            .unwrap_or_default()
            .iter()
            .map(|backup| self.render_backup_row(backup, &data, cx))
            .collect()
    }

    fn render_backup_row(
        &self,
        backup: &BackupEntry,
        data: &Rc<AppData>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_source = matches!(&self.source, Some(Source::Backup(b)) if b.id == backup.id);
        let selected = self.selected_backups.contains(&backup.id);
        let marker = if is_source {
            Some(ui::source_color())
        } else if selected {
            Some(cx.theme().muted_foreground.opacity(0.6))
        } else {
            None
        };
        let row_id = SharedString::from(backup.path.clone());
        let view = cx.entity().downgrade();
        let data = data.clone();
        let menu_backup = backup.clone();
        let backup_id = backup.id.clone();
        let muted = cx.theme().muted_foreground;

        ui::list_row(ElementId::Name(row_id.clone()), marker, cx)
            .h(ui::ROW_HEIGHT)
            .w_full()
            .on_drag(
                DraggedSettings(Source::Backup(backup.clone())),
                |dragged, grab, _, cx| cx.new(|_| DragPreview::new(&dragged.0, grab, None)),
            )
            .child(table::cell(
                &BACKUP_COLUMNS[0],
                Checkbox::new(SharedString::from(format!("{}:select", row_id)))
                    .checked(selected)
                    .on_change(cx.listener(move |this, checked: &bool, _, cx| {
                        if *checked {
                            this.selected_backups.insert(backup_id.clone());
                        } else {
                            this.selected_backups.remove(&backup_id);
                        }
                        cx.notify();
                    })),
            ))
            .child(table::cell(
                &BACKUP_COLUMNS[1],
                ui::entry_avatar(backup.kind, None, px(24.), cx),
            ))
            .child(table::cell(
                &BACKUP_COLUMNS[2],
                h_flex()
                    .min_w_0()
                    .gap_2()
                    .child(div().truncate().child(backup.name.clone()))
                    .child(
                        div().truncate().text_xs().text_color(muted).child(
                            backup
                                .original_name
                                .clone()
                                .unwrap_or_else(|| backup.original_id.clone()),
                        ),
                    ),
            ))
            .child(table::cell(
                &BACKUP_COLUMNS[3],
                div()
                    .text_color(muted)
                    .child(ui::relative_time(backup.timestamp)),
            ))
            .child(table::cell(
                &BACKUP_COLUMNS[4],
                h_flex().child(
                    Button::new(SharedString::from(format!("{}:menu", row_id)))
                        .ghost()
                        .small()
                        .icon(IconName::Ellipsis)
                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                            let backup = menu_backup.clone();
                            let menu = menu.item(
                                PopupMenuItem::new(t!("actions.useAsSource").to_string())
                                    .icon(IconName::ArrowUpFromLine)
                                    .disabled(is_source)
                                    .on_click(view_action(&view, {
                                        let backup = backup.clone();
                                        move |this, _, cx| {
                                            this.set_source(Source::Backup(backup.clone()), cx)
                                        }
                                    })),
                            );
                            let targets: Vec<SettingsEntry> = data
                                .entries()
                                .filter(|e| e.kind == backup.kind)
                                .cloned()
                                .collect();
                            let menu = if targets.is_empty() {
                                menu
                            } else {
                                let view = view.clone();
                                let backup = backup.clone();
                                menu.submenu_with_icon(
                                    Some(Icon::new(IconName::ArrowDownToLine)),
                                    t!("actions.applyTo").to_string(),
                                    window,
                                    cx,
                                    move |menu, _, _| {
                                        targets.iter().fold(
                                            menu.max_h(px(256.)).scrollable(true),
                                            |menu, target| {
                                                let target = target.clone();
                                                let backup = backup.clone();
                                                menu.item(
                                                    PopupMenuItem::new(target.display_name.clone())
                                                        .on_click(view_action(
                                                            &view,
                                                            move |this, window, cx| {
                                                                this.apply_backup(
                                                                    backup.clone(),
                                                                    target.clone(),
                                                                    window,
                                                                    cx,
                                                                )
                                                            },
                                                        )),
                                                )
                                            },
                                        )
                                    },
                                )
                            };
                            menu.separator().item(
                                PopupMenuItem::new(t!("actions.delete").to_string())
                                    .icon(IconName::Trash)
                                    .on_click(view_action(&view, move |this, window, cx| {
                                        this.delete_backups(vec![backup.clone()], window, cx)
                                    })),
                            )
                        }),
                ),
            ))
            .into_any_element()
    }
}
