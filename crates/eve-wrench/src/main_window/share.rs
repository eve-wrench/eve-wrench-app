use std::collections::HashSet;
use std::path::PathBuf;

use eve_wrench_core::share::{self, GroupPreview, SharePack};
use eve_wrench_core::{SettingsEntry, SettingsKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::MainView;
use crate::sharing;
use crate::ui;

impl MainView {
    pub(super) fn share_settings(
        &mut self,
        entry: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let note = cx.new(|cx| {
            InputState::new(window, cx).placeholder(t!("share.notePlaceholder").to_string())
        });
        let dialog_view = cx.new(|_| ExportDialog {
            entry: entry.clone(),
            state: State::Loading,
            selected: HashSet::new(),
            note,
        });

        // Everything shareable is read once; ticking groups only filters it
        let path = PathBuf::from(&entry.path);
        let kind = entry.kind;
        let view = dialog_view.downgrade();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_spawn(async move {
                    let ids: Vec<&str> = share::shareable_groups(kind).map(|g| g.id).collect();
                    let pack = SharePack::export(&path, kind, &ids, None)?;
                    let previews = share::preview(&pack, None)?;
                    Ok::<_, String>((pack, previews))
                })
                .await;
            view.update(cx, |dialog, cx| {
                dialog.state = match result {
                    Ok((pack, previews)) => {
                        dialog.selected = default_selection(&previews);
                        State::Ready { pack, previews }
                    }
                    Err(e) => State::Failed(e),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();

        let title = t!("share.exportTitle", name = entry.display_name).to_string();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(title.clone())
                .w(px(560.))
                .child(dialog_view.clone())
        });
    }

    pub(super) fn import_share(
        &mut self,
        entry: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let main = cx.entity().downgrade();
        let dialog_view = cx.new(|_| ImportDialog {
            entry: entry.clone(),
            main,
            state: State::Empty,
            selected: HashSet::new(),
        });
        let title = t!("share.importTitle", name = entry.display_name).to_string();
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(title.clone())
                .w(px(560.))
                .child(dialog_view.clone())
        });
    }

    fn apply_share(
        &mut self,
        entry: SettingsEntry,
        pack: SharePack,
        groups: Vec<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let backup = self.store.read(cx).config().auto_backup;
        let count = groups.len();
        let name = entry.display_name.clone();
        self.run_job(
            window,
            cx,
            move |_| {
                share::apply(
                    &pack,
                    std::path::Path::new(&entry.path),
                    entry.kind,
                    &groups,
                    backup,
                )
            },
            move |_, result, window, cx| match result {
                Ok(()) => ui::notify_success(
                    t!("share.imported"),
                    t!("share.importedDesc", count = count, name = name),
                    window,
                    cx,
                ),
                Err(e) => ui::notify_error(t!("share.importFailed"), e, window, cx),
            },
        );
    }
}

fn default_selection(previews: &[GroupPreview]) -> HashSet<&'static str> {
    previews
        .iter()
        .filter(|p| {
            eve_wrench_core::copy::COPY_GROUPS
                .iter()
                .any(|g| g.id == p.id && g.share == Some(true))
        })
        .map(|p| p.id)
        .collect()
}

enum State {
    // Import only: waiting for a file or paste
    Empty,
    Loading,
    Failed(String),
    Ready {
        pack: SharePack,
        previews: Vec<GroupPreview>,
    },
}

fn group_row<V: Render + Selectable>(
    preview: &GroupPreview,
    checked: bool,
    status: Option<String>,
    cx: &mut Context<V>,
) -> AnyElement {
    let id = preview.id;
    let muted = cx.theme().muted_foreground;
    let names = sharing::item_names(&preview.items);
    let hover = cx.theme().muted.opacity(0.6);
    v_flex()
        .id(SharedString::from(format!("share-group-{id}")))
        .px_3()
        .py_2()
        .gap_0p5()
        .rounded(px(6.))
        .hover(move |s| s.bg(hover))
        .child(
            h_flex()
                .gap_3()
                .child(
                    Checkbox::new(SharedString::from(format!("share-check-{id}")))
                        .small()
                        .label(sharing::group_label(id))
                        .checked(checked)
                        .on_change(cx.listener(move |this, checked: &bool, _, cx| {
                            let selected = this.selected();
                            if *checked {
                                selected.insert(id);
                            } else {
                                selected.remove(id);
                            }
                            cx.notify();
                        })),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(muted)
                        .child(status.unwrap_or_else(|| sharing::items_summary(&preview.items))),
                ),
        )
        .when_some(names, |this, names| {
            this.child(
                div()
                    .pl_6()
                    .text_xs()
                    .text_color(muted.opacity(0.8))
                    .line_clamp(2)
                    .child(names),
            )
        })
        .into_any_element()
}

// Both dialogs keep the ticked group ids in a set.
trait Selectable {
    fn selected(&mut self) -> &mut HashSet<&'static str>;
}

fn render_status(state: &State, cx: &App) -> Option<AnyElement> {
    let text = match state {
        State::Loading => t!("common.loading").to_string(),
        State::Failed(e) => e.clone(),
        _ => return None,
    };
    let failed = matches!(state, State::Failed(_));
    Some(
        h_flex()
            .gap_2()
            .px_3()
            .py_3()
            .text_sm()
            .text_color(if failed {
                cx.theme().danger
            } else {
                cx.theme().muted_foreground
            })
            .when(failed, |this| {
                this.child(Icon::new(IconName::TriangleAlert).small())
            })
            .child(text)
            .into_any_element(),
    )
}

struct ExportDialog {
    entry: SettingsEntry,
    state: State,
    selected: HashSet<&'static str>,
    note: Entity<InputState>,
}

impl Selectable for ExportDialog {
    fn selected(&mut self) -> &mut HashSet<&'static str> {
        &mut self.selected
    }
}

impl ExportDialog {
    // A share of only what's ticked, or None when nothing is.
    fn selection(&self, cx: &App) -> Option<(SharePack, String)> {
        let State::Ready { pack, .. } = &self.state else {
            return None;
        };
        let ids: Vec<&str> = self.selected.iter().copied().collect();
        let note = self.note.read(cx).value().to_string();
        let pack = pack.select(&ids, Some(note)).ok()?;
        let topic = match ids.as_slice() {
            [only] => only.replace('_', "-"),
            _ => "settings".to_string(),
        };
        Some((pack, format!("{}-{}", self.entry.display_name, topic)))
    }
}

impl Render for ExportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let kind = self.entry.kind;

        let rows: Vec<AnyElement> = match &self.state {
            State::Ready { previews, .. } => share::shareable_groups(kind)
                .map(|group| match previews.iter().find(|p| p.id == group.id) {
                    Some(preview) => {
                        let checked = self.selected.contains(group.id);
                        group_row(preview, checked, None, cx)
                    }
                    // Nothing stored for this group, so nothing to share
                    None => h_flex()
                        .px_3()
                        .py_2()
                        .gap_3()
                        .text_sm()
                        .text_color(muted.opacity(0.6))
                        .child(
                            Checkbox::new(SharedString::from(format!("share-check-{}", group.id)))
                                .small()
                                .label(sharing::group_label(group.id))
                                .disabled(true),
                        )
                        .child(div().flex_1())
                        .child(div().text_xs().child(t!("share.notSet").to_string()))
                        .into_any_element(),
                })
                .collect(),
            _ => Vec::new(),
        };

        let ready = matches!(self.state, State::Ready { .. }) && !self.selected.is_empty();
        v_flex()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(t!("share.exportDesc").to_string()),
            )
            .children(render_status(&self.state, cx))
            .child(
                v_flex()
                    .id("share-groups")
                    .max_h(px(360.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .child(Input::new(&self.note).small())
            .child(
                h_flex()
                    .gap_2()
                    .text_xs()
                    .text_color(muted)
                    .child(Icon::new(IconName::Info).xsmall())
                    .child(t!("share.privacyHint").to_string()),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("share-cancel")
                            .outline()
                            .label(t!("common.cancel").to_string())
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("share-copy")
                            .outline()
                            .icon(IconName::Copy)
                            .label(t!("share.copyText").to_string())
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some((pack, _)) = this.selection(cx) {
                                    sharing::copy_to_clipboard(&pack, window, cx);
                                    window.close_dialog(cx);
                                }
                            })),
                    )
                    .child(
                        Button::new("share-save")
                            .primary()
                            .icon(IconName::Download)
                            .label(t!("share.saveFile").to_string())
                            .disabled(!ready)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some((pack, stem)) = this.selection(cx) {
                                    window.close_dialog(cx);
                                    sharing::save_to_file(pack, &stem, window, cx);
                                }
                            })),
                    ),
            )
    }
}

struct ImportDialog {
    entry: SettingsEntry,
    main: WeakEntity<MainView>,
    state: State,
    selected: HashSet<&'static str>,
}

impl Selectable for ImportDialog {
    fn selected(&mut self) -> &mut HashSet<&'static str> {
        &mut self.selected
    }
}

impl ImportDialog {
    // Checks the share fits this file, then works out what it would change.
    fn load(&mut self, loaded: Result<SharePack, String>, cx: &mut Context<Self>) {
        let pack = match loaded {
            Ok(pack) if pack.kind != self.entry.kind => {
                self.state = State::Failed(match pack.kind {
                    SettingsKind::User => t!("share.wrongKindAccount").to_string(),
                    SettingsKind::Char => t!("share.wrongKindCharacter").to_string(),
                });
                cx.notify();
                return;
            }
            Ok(pack) => pack,
            Err(e) => {
                self.state = State::Failed(e);
                cx.notify();
                return;
            }
        };

        self.state = State::Loading;
        cx.notify();
        let target = PathBuf::from(&self.entry.path);
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    share::preview(&pack, Some(&target)).map(|previews| (pack, previews))
                })
                .await;
            this.update(cx, |this, cx| {
                this.state = match result {
                    Ok((pack, previews)) => {
                        this.selected = previews.iter().map(|p| p.id).collect();
                        State::Ready { pack, previews }
                    }
                    Err(e) => State::Failed(e),
                };
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn render_sources(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .gap_2()
            .child(
                Button::new("share-open-file")
                    .outline()
                    .flex_1()
                    .icon(IconName::FolderOpen)
                    .label(t!("share.openFile").to_string())
                    .on_click(cx.listener(|_, _, window, cx| {
                        let view = cx.entity().downgrade();
                        sharing::open_file(window, cx, move |result, _, cx| {
                            view.update(cx, |this, cx| this.load(result, cx)).ok();
                        });
                    })),
            )
            .child(
                Button::new("share-paste")
                    .outline()
                    .flex_1()
                    .icon(IconName::ClipboardPaste)
                    .label(t!("share.paste").to_string())
                    .on_click(cx.listener(|this, _, _, cx| {
                        let result = sharing::read_clipboard(cx);
                        this.load(result, cx);
                    })),
            )
    }

    fn render_header(&self, pack: &SharePack, cx: &App) -> impl IntoElement + use<> {
        let meta = t!(
            "share.sharedMeta",
            time = ui::relative_time(pack.created),
            version = pack.app_version.as_str()
        )
        .to_string();
        v_flex()
            .gap_1()
            .px_3()
            .py_2()
            .rounded(px(6.))
            .bg(cx.theme().muted.opacity(0.5))
            .when_some(pack.note.clone(), |this, note| {
                this.child(div().text_sm().child(note))
            })
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(meta),
            )
    }
}

impl Render for ImportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        let (header, rows) = match &self.state {
            State::Ready { pack, previews } => {
                let header = self.render_header(pack, cx).into_any_element();
                let rows: Vec<AnyElement> = previews
                    .iter()
                    .map(|preview| {
                        let checked = self.selected.contains(preview.id);
                        group_row(preview, checked, sharing::changes_summary(preview), cx)
                    })
                    .collect();
                (Some(header), rows)
            }
            _ => (None, Vec::new()),
        };
        let can_import = matches!(self.state, State::Ready { .. }) && !self.selected.is_empty();

        v_flex()
            .gap_3()
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(t!("share.importDesc").to_string()),
            )
            .child(self.render_sources(cx))
            .children(render_status(&self.state, cx))
            .children(header)
            .when(!rows.is_empty(), |this| {
                this.child(
                    v_flex()
                        .id("share-import-groups")
                        .max_h(px(360.))
                        .overflow_y_scroll()
                        .children(rows),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(muted)
                        .child(t!("share.importHint").to_string()),
                )
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("share-import-cancel")
                            .outline()
                            .label(t!("common.cancel").to_string())
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("share-import-confirm")
                            .primary()
                            .icon(IconName::Upload)
                            .label(t!("importExport.import").to_string())
                            .disabled(!can_import)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let State::Ready { pack, .. } = &this.state else {
                                    return;
                                };
                                let groups: Vec<&'static str> =
                                    this.selected.iter().copied().collect();
                                let (entry, pack) = (this.entry.clone(), pack.clone());
                                this.main
                                    .update(cx, |main, cx| {
                                        main.apply_share(entry, pack, groups, window, cx)
                                    })
                                    .ok();
                                window.close_dialog(cx);
                            })),
                    ),
            )
    }
}
