use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

use eve_wrench_core::SettingsEntry;
use eve_wrench_core::archive::ImportAnalysis;
use eve_wrench_core::updates::APP_VERSION;

use crate::ui;
use crate::updater::AvailableUpdate;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::dialog::{DialogAction, DialogClose, DialogFooter};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::ui::StyleExt as _;
use rust_i18n::t;

use super::MainView;

pub(super) struct ConfirmOptions {
    pub title: SharedString,
    pub description: SharedString,
    pub ok_text: SharedString,
    pub destructive: bool,
}

pub(super) fn confirm(
    options: ConfirmOptions,
    window: &mut Window,
    cx: &mut App,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
) {
    let on_confirm = Rc::new(on_confirm);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_confirm = on_confirm.clone();
        alert
            .confirm()
            .title(options.title.clone())
            .description(options.description.clone())
            .ok_text(options.ok_text.clone())
            .cancel_text(t!("dialog.cancel").to_string())
            .when(options.destructive, |alert| {
                alert.ok_variant(ButtonVariant::Danger)
            })
            .on_ok(move |_, window, cx| {
                on_confirm(window, cx);
                true
            })
    });
}

impl MainView {
    pub(super) fn create_backup(
        &mut self,
        entry: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(t!("dialog.backupName").to_string())
                .default_value(entry.display_name.clone())
        });
        let view = cx.entity().downgrade();
        let description = t!("dialog.createBackupDesc", name = entry.display_name).to_string();

        let submit = Rc::new({
            let input = input.clone();
            let view = view.clone();
            move |window: &mut Window, cx: &mut App| -> bool {
                let name = input.read(cx).value().trim().to_string();
                if name.is_empty() {
                    return false;
                }
                let entry = entry.clone();
                view.update(cx, |this, cx| {
                    this.do_create_backup(entry, name, window, cx)
                })
                .ok();
                true
            }
        });

        self._dialog_subscription = Some(cx.subscribe_in(&input, window, {
            let submit = submit.clone();
            move |_, _, event, window, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) && submit(window, cx) {
                    window.close_dialog(cx);
                }
            }
        }));

        window.open_dialog(cx, {
            let input = input.clone();
            move |dialog, _, _| {
                let submit = submit.clone();
                dialog
                    .title(t!("dialog.createBackup").to_string())
                    .child(
                        v_flex()
                            .gap_3()
                            .child(div().text_sm().child(description.clone()))
                            .child(Input::new(&input)),
                    )
                    .footer(dialog_buttons(t!("dialog.create").to_string()))
                    .on_ok(move |_, window, cx| submit(window, cx))
            }
        });
        input.update(cx, |input, cx| input.focus(window, cx));
    }

    pub(super) fn show_import_dialog(
        &mut self,
        path: PathBuf,
        analysis: ImportAnalysis,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        let dialog_view = cx.new(|_| ImportDialog {
            selected: analysis.conflicts.iter().cloned().collect(),
            analysis,
            path,
            main: view,
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(t!("importDialog.title").to_string())
                .w(px(520.))
                .child(dialog_view.clone())
        });
    }

    pub(super) fn show_update_dialog(
        &mut self,
        update: AvailableUpdate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dialog_view = cx.new(|_| UpdateDialog {
            update,
            installing: false,
        });
        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .w(px(448.))
                .title(t!("update.updateAvailable").to_string())
                .child(dialog_view.clone())
        });
    }

    // Manual check from the settings menu; unlike the startup check it
    // reports "up to date" and failures.
    pub(super) fn check_for_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async { crate::updater::check() }).await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(Some(update)) => this.show_update_dialog(update, window, cx),
                Ok(None) => ui::notify_success(
                    t!("update.upToDate"),
                    t!("update.upToDateDesc", version = APP_VERSION),
                    window,
                    cx,
                ),
                Err(e) => ui::notify_error(t!("update.checkFailed"), e, window, cx),
            })
            .ok();
        })
        .detach();
    }
}

fn dialog_buttons(ok_label: String) -> DialogFooter {
    DialogFooter::new()
        .child(
            DialogClose::new().child(
                Button::new("dialog-cancel")
                    .outline()
                    .label(t!("dialog.cancel").to_string()),
            ),
        )
        .child(DialogAction::new().child(Button::new("dialog-ok").primary().label(ok_label)))
}

struct ImportDialog {
    analysis: ImportAnalysis,
    path: PathBuf,
    selected: HashSet<String>,
    main: WeakEntity<MainView>,
}

// Archive paths look like "<server folder>/settings_<profile>/<file>".
fn describe_path(relative_path: &str) -> (String, String) {
    let parts: Vec<&str> = relative_path.split(['/', '\\']).collect();
    let file = parts.last().copied().unwrap_or(relative_path).to_string();
    let server = parts.iter().find_map(|part| {
        eve_wrench_core::Server::from_folder_name(part).map(|s| s.display_name().to_string())
    });
    let profile = parts
        .iter()
        .find_map(|part| part.strip_prefix("settings_").map(str::to_string));
    let context = [server, profile]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" / ");
    (file, context)
}

impl ImportDialog {
    fn render_section(
        &self,
        icon: IconName,
        icon_color: Hsla,
        title: String,
        files: &[String],
        with_switches: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let all_selected = files.iter().all(|f| self.selected.contains(f));
        let all_files = files.to_vec();

        v_flex()
            .gap_1p5()
            .child(
                h_flex()
                    .gap_2()
                    .child(Icon::new(icon).small().text_color(icon_color))
                    .child(div().text_sm().font_medium().child(title))
                    .child(crate::ui::count_label(files.len(), cx))
                    .when(with_switches, |this| {
                        this.child(div().flex_1()).child(
                            Button::new("toggle-all-conflicts")
                                .ghost()
                                .xsmall()
                                .label(if all_selected {
                                    t!("importDialog.deselectAll").to_string()
                                } else {
                                    t!("importDialog.selectAll").to_string()
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if all_selected {
                                        this.selected.clear();
                                    } else {
                                        this.selected.extend(all_files.iter().cloned());
                                    }
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .children(files.iter().map(|file| {
                let (name, context) = describe_path(file);
                let key = file.clone();
                h_flex()
                    .pl_6()
                    .gap_3()
                    .text_xs()
                    .when(with_switches, |this| {
                        this.child(
                            Switch::new(SharedString::from(format!("conflict-{}", file)))
                                .small()
                                .checked(self.selected.contains(file))
                                .on_change(cx.listener(move |this, checked: &bool, _, cx| {
                                    if *checked {
                                        this.selected.insert(key.clone());
                                    } else {
                                        this.selected.remove(&key);
                                    }
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_medium()
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(cx.theme().muted_foreground.opacity(0.6))
                            .child(context),
                    )
            }))
    }
}

impl Render for ImportDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let analysis = self.analysis.clone();
        let muted_foreground = cx.theme().muted_foreground;
        let import_label = if !analysis.conflicts.is_empty() && !self.selected.is_empty() {
            format!(
                "{} ({} {})",
                t!("importExport.import"),
                self.selected.len(),
                t!("importDialog.overwrite")
            )
        } else {
            t!("importExport.import").to_string()
        };

        v_flex()
            .gap_4()
            .child(
                div()
                    .text_sm()
                    .text_color(muted_foreground)
                    .child(t!("importDialog.foundFiles", count = analysis.total_files).to_string()),
            )
            .child(
                v_flex()
                    .id("import-files")
                    .max_h_80()
                    .overflow_y_scroll()
                    .gap_4()
                    .when(!analysis.new_files.is_empty(), |this| {
                        this.child(self.render_section(
                            IconName::FilePlus,
                            crate::ui::emerald(),
                            t!("importDialog.newFiles").to_string(),
                            &analysis.new_files,
                            false,
                            cx,
                        ))
                    })
                    .when(!analysis.conflicts.is_empty(), |this| {
                        this.child(self.render_section(
                            IconName::TriangleAlert,
                            crate::ui::amber(cx),
                            t!("importDialog.conflicts").to_string(),
                            &analysis.conflicts,
                            true,
                            cx,
                        ))
                    })
                    .when(!analysis.unchanged.is_empty(), |this| {
                        this.child(self.render_section(
                            IconName::CircleCheck,
                            muted_foreground,
                            t!("importDialog.unchanged").to_string(),
                            &analysis.unchanged,
                            false,
                            cx,
                        ))
                    }),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("import-cancel")
                            .outline()
                            .label(t!("common.cancel").to_string())
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(
                        Button::new("import-confirm")
                            .primary()
                            .label(import_label)
                            .on_click(cx.listener(|this, _, window, cx| {
                                let path = this.path.clone();
                                let overwrite: Vec<String> =
                                    this.selected.iter().cloned().collect();
                                this.main
                                    .update(cx, |main, cx| {
                                        main.execute_import(path, overwrite, window, cx)
                                    })
                                    .ok();
                                window.close_dialog(cx);
                            })),
                    ),
            )
    }
}

struct UpdateDialog {
    update: AvailableUpdate,
    installing: bool,
}

impl UpdateDialog {
    fn install(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(installer) = self.update.installer.clone() else {
            return;
        };
        self.installing = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { crate::updater::install(&installer) })
                .await;
            this.update_in(cx, |this, window, cx| match result {
                Ok(()) => cx.restart(),
                Err(e) => {
                    this.installing = false;
                    ui::notify_error(t!("update.installFailed"), e, window, cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

impl Render for UpdateDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let info = &self.update.info;
        let notes: String = if info.release_notes.chars().count() > 300 {
            info.release_notes
                .chars()
                .take(300)
                .chain("...".chars())
                .collect()
        } else {
            info.release_notes.clone()
        };
        let muted = cx.theme().muted_foreground;
        let url = info.release_url.clone();

        let primary = if self.update.installer.is_some() {
            Button::new("update-install")
                .primary()
                .icon(IconName::Download)
                .label(if self.installing {
                    t!("update.installing").to_string()
                } else {
                    t!("update.install").to_string()
                })
                .loading(self.installing)
                .disabled(self.installing)
                .on_click(cx.listener(|this, _, window, cx| this.install(window, cx)))
        } else {
            Button::new("update-download")
                .primary()
                .icon(IconName::Download)
                .label(t!("update.download").to_string())
                .on_click(move |_, window, cx| {
                    cx.open_url(&url);
                    window.close_dialog(cx);
                })
        };

        v_flex()
            .gap_4()
            .child(div().text_sm().text_color(muted).child(format!(
                "v{} → v{}",
                info.current_version, info.latest_version
            )))
            .child(
                div()
                    .text_sm()
                    .text_color(muted)
                    .child(t!("update.newVersionAvailable").to_string()),
            )
            .when(!notes.is_empty(), |this| {
                this.child(
                    div()
                        .id("release-notes")
                        .max_h_32()
                        .overflow_y_scroll()
                        .p_3()
                        .rounded(px(6.))
                        .bg(cx.theme().muted)
                        .text_xs()
                        .text_color(muted)
                        .child(notes),
                )
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("update-later")
                            .outline()
                            .label(t!("update.later").to_string())
                            .disabled(self.installing)
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child(primary),
            )
    }
}
