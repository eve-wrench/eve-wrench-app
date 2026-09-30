use std::path::{Path, PathBuf};

use eve_wrench_core::copy::COPY_GROUPS;
use eve_wrench_core::{
    BackupEntry, Locations, ProfileData, SettingsEntry, SettingsKind, archive, backups, copy, scan,
};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use rust_i18n::t;

use super::dialogs::{ConfirmOptions, confirm};
use super::{AliasEdit, MainView, Source};
use crate::ui;

impl MainView {
    // Runs blocking file work off the UI thread, hands the result back to
    // the view, then rescans so every window sees the change.
    pub(super) fn run_job<T: Send + 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        job: impl FnOnce(Locations) -> Result<T, String> + Send + 'static,
        done: impl FnOnce(&mut Self, Result<T, String>, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let locations = self.store.read(cx).locations().clone();
        let store = self.store.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { job(locations) }).await;
            this.update_in(cx, |this, window, cx| {
                done(this, result, window, cx);
                store.update(cx, |store, cx| store.reload(cx));
            })
            .ok();
        })
        .detach();
    }

    fn auto_backup(&self, cx: &App) -> bool {
        self.store.read(cx).config().auto_backup
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.announce_refresh = true;
        self.store.update(cx, |store, cx| store.reload(cx));
    }

    // Switching to a source of the other kind drops targets that no longer fit.
    pub(super) fn set_source(&mut self, source: Source, cx: &mut Context<Self>) {
        let kind = source.kind();
        self.targets.retain(|t| t.kind == kind);
        self.source = Some(source);
        cx.notify();
    }

    fn check_target(&self, entry: &SettingsEntry, window: &mut Window, cx: &mut App) -> bool {
        let Some(source) = &self.source else {
            ui::notify_error(
                t!("toast.noSourceSelected"),
                t!("toast.noSourceSelectedDesc"),
                window,
                cx,
            );
            return false;
        };
        if entry.kind != source.kind() {
            ui::notify_error(
                t!("toast.typeMismatch"),
                t!("toast.typeMismatchDesc"),
                window,
                cx,
            );
            return false;
        }
        if matches!(source, Source::Entry(e) if e.path == entry.path) {
            ui::notify_error(
                t!("toast.invalidTarget"),
                t!("toast.invalidTargetDesc"),
                window,
                cx,
            );
            return false;
        }
        true
    }

    pub(super) fn add_target(
        &mut self,
        entry: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.check_target(&entry, window, cx)
            && !self.targets.iter().any(|t| t.path == entry.path)
        {
            self.targets.push(entry);
            cx.notify();
        }
    }

    pub(super) fn add_all_from_profile(
        &mut self,
        profile: &ProfileData,
        kind: SettingsKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let source_path = match &self.source {
            None => {
                ui::notify_error(
                    t!("toast.noSourceSelected"),
                    t!("toast.noSourceSelectedDesc"),
                    window,
                    cx,
                );
                return;
            }
            Some(source) if source.kind() != kind => return,
            Some(Source::Entry(e)) => Some(e.path.clone()),
            Some(Source::Backup(_)) => None,
        };
        for entry in profile.entries(kind) {
            let is_source = source_path.as_deref() == Some(entry.path.as_str());
            if !is_source && !self.targets.iter().any(|t| t.path == entry.path) {
                self.targets.push(entry.clone());
            }
        }
        cx.notify();
    }

    pub(super) fn execute_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        if self.targets.is_empty() {
            return;
        }

        let view = cx.entity().downgrade();
        confirm(
            ConfirmOptions {
                title: t!("dialog.copySettings").into(),
                description: t!(
                    "dialog.copySettingsDesc",
                    source = source.display_name(),
                    count = self.targets.len()
                )
                .into(),
                ok_text: t!("dialog.copy").into(),
                destructive: false,
            },
            window,
            cx,
            move |window, cx| {
                view.update(cx, |this, cx| this.copy_confirmed(window, cx))
                    .ok();
            },
        );
    }

    fn copy_confirmed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let kind = source.kind();
        let excluded: Vec<&'static str> = COPY_GROUPS
            .iter()
            .filter(|g| {
                g.applies_to(kind) && !self.groups.get(g.id).copied().unwrap_or(g.default_on)
            })
            .map(|g| g.id)
            .collect();
        let targets: Vec<String> = self.targets.iter().map(|t| t.path.clone()).collect();
        let source_path = PathBuf::from(source.path());
        let backup = self.auto_backup(cx);

        self.copying = true;
        cx.notify();
        self.run_job(
            window,
            cx,
            move |_| copy::copy_settings_selective(&source_path, &targets, &excluded, backup),
            |this, result, window, cx| {
                this.copying = false;
                match result {
                    Ok(count) => {
                        this.targets.clear();
                        ui::notify_success(
                            t!("toast.settingsCopied"),
                            t!("toast.settingsCopiedDesc", count = count),
                            window,
                            cx,
                        );
                    }
                    Err(e) => ui::notify_error(t!("toast.copyFailed"), e, window, cx),
                }
            },
        );
    }

    pub(super) fn do_create_backup(
        &mut self,
        entry: SettingsEntry,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&entry.path);
        let job_name = name.clone();
        self.run_job(
            window,
            cx,
            move |_| backups::create_backup(&path, &job_name),
            move |_, result, window, cx| match result {
                Ok(_) => ui::notify_success(
                    t!("toast.backupCreated"),
                    t!("toast.backupCreatedDesc", name = name),
                    window,
                    cx,
                ),
                Err(e) => ui::notify_error(t!("toast.backupFailed"), e, window, cx),
            },
        );
    }

    pub(super) fn restore_backup(
        &mut self,
        entry: SettingsEntry,
        backup: BackupEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        confirm(
            ConfirmOptions {
                title: t!("dialog.restoreBackup").into(),
                description: t!(
                    "dialog.restoreBackupDesc",
                    backup = backup.name,
                    target = entry.display_name
                )
                .into(),
                ok_text: t!("dialog.restore").into(),
                destructive: true,
            },
            window,
            cx,
            move |window, cx| {
                let (entry, backup) = (entry.clone(), backup.clone());
                view.update(cx, |this, cx| {
                    let name = backup.name.clone();
                    this.copy_backup_to(
                        backup,
                        entry,
                        window,
                        cx,
                        move |window, cx| {
                            ui::notify_success(
                                t!("toast.backupRestored"),
                                t!("toast.backupRestoredDesc", name = name),
                                window,
                                cx,
                            )
                        },
                        t!("toast.restoreFailed").into(),
                    )
                })
                .ok();
            },
        );
    }

    pub(super) fn apply_backup(
        &mut self,
        backup: BackupEntry,
        target: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let view = cx.entity().downgrade();
        confirm(
            ConfirmOptions {
                title: t!("dialog.applyBackup").into(),
                description: t!(
                    "dialog.applyBackupDesc",
                    backup = backup.name,
                    target = target.display_name
                )
                .into(),
                ok_text: t!("dialog.apply").into(),
                destructive: true,
            },
            window,
            cx,
            move |window, cx| {
                let (target, backup) = (target.clone(), backup.clone());
                view.update(cx, |this, cx| {
                    let message = t!(
                        "toast.backupAppliedDesc",
                        backup = backup.name,
                        target = target.display_name
                    )
                    .to_string();
                    this.copy_backup_to(
                        backup,
                        target,
                        window,
                        cx,
                        move |window, cx| {
                            ui::notify_success(t!("toast.backupApplied"), message, window, cx)
                        },
                        t!("toast.applyFailed").into(),
                    )
                })
                .ok();
            },
        );
    }

    fn copy_backup_to(
        &mut self,
        backup: BackupEntry,
        target: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_success: impl FnOnce(&mut Window, &mut App) + 'static,
        failure_title: SharedString,
    ) {
        self.run_job(
            window,
            cx,
            move |_| backups::copy_settings(Path::new(&backup.path), &[target.path]),
            move |_, result, window, cx| match result {
                Ok(_) => on_success(window, cx),
                Err(e) => ui::notify_error(failure_title, e, window, cx),
            },
        );
    }

    pub(super) fn delete_selected_backups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(data) = self.store.read(cx).data() else {
            return;
        };
        let selected: Vec<BackupEntry> = data
            .backups
            .iter()
            .filter(|b| self.selected_backups.contains(&b.id))
            .cloned()
            .collect();
        self.delete_backups(selected, window, cx);
    }

    pub(super) fn delete_backups(
        &mut self,
        backups: Vec<BackupEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let [first, ..] = backups.as_slice() else {
            return;
        };
        let options = if backups.len() == 1 {
            ConfirmOptions {
                title: t!("dialog.deleteBackup").into(),
                description: t!("dialog.deleteBackupDesc", name = first.name).into(),
                ok_text: t!("dialog.delete").into(),
                destructive: true,
            }
        } else {
            ConfirmOptions {
                title: t!("dialog.deleteBackups").into(),
                description: t!("dialog.deleteBackupsDesc", count = backups.len()).into(),
                ok_text: t!("dialog.delete").into(),
                destructive: true,
            }
        };

        let view = cx.entity().downgrade();
        confirm(options, window, cx, move |window, cx| {
            let backups = backups.clone();
            view.update(cx, |this, cx| {
                this.backups_deletion_confirmed(backups, window, cx)
            })
            .ok();
        });
    }

    fn backups_deletion_confirmed(
        &mut self,
        backups: Vec<BackupEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let paths: Vec<String> = backups.iter().map(|b| b.path.clone()).collect();
        let single_name = (backups.len() == 1).then(|| backups[0].name.clone());
        self.run_job(
            window,
            cx,
            move |_| Ok(backups::delete_backups(&paths)),
            move |this, result: Result<usize, String>, window, cx| {
                if matches!(&this.source, Some(Source::Backup(b)) if backups.iter().any(|x| x.id == b.id)) {
                    this.source = None;
                }
                for backup in &backups {
                    this.selected_backups.remove(&backup.id);
                }
                match (result, single_name) {
                    (Ok(1), Some(name)) => ui::notify_success(
                        t!("toast.backupDeleted"),
                        t!("toast.backupDeletedDesc", name = name),
                        window,
                        cx,
                    ),
                    (Ok(count), _) => ui::notify_success(
                        t!("toast.backupsDeleted"),
                        t!("toast.backupsDeletedDesc", count = count),
                        window,
                        cx,
                    ),
                    (Err(e), _) => ui::notify_error(t!("toast.deleteFailed"), e, window, cx),
                }
            },
        );
    }

    pub(super) fn start_alias_edit(
        &mut self,
        entry: &SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let alias = entry.alias.clone().unwrap_or_default();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(entry.id.clone())
                .default_value(alias)
        });
        let id = entry.id.clone();
        let subscription =
            cx.subscribe_in(&input, window, move |this, input, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let alias = input.read(cx).value().to_string();
                    this.save_alias(id.clone(), alias, window, cx);
                }
            });
        input.update(cx, |input, cx| input.focus(window, cx));
        self.alias_edit = Some(AliasEdit {
            id: entry.id.clone(),
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    pub(super) fn save_alias(
        &mut self,
        id: String,
        alias: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.alias_edit = None;
        cx.notify();
        self.run_job(
            window,
            cx,
            move |locations| scan::set_alias(&locations, &id, Some(&alias)),
            |_, result, window, cx| {
                if let Err(e) = result {
                    ui::notify_error(t!("toast.updateSettingFailed"), e, window, cx);
                }
            },
        );
    }

    pub(super) fn set_brackets_always_show(
        &mut self,
        server_path: String,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_job(
            window,
            cx,
            move |_| scan::set_brackets_always_show(Path::new(&server_path), enabled),
            move |_, result, window, cx| match result {
                Ok(()) => {
                    let status = if enabled {
                        t!("common.enabled")
                    } else {
                        t!("common.disabled")
                    };
                    ui::notify_success(
                        t!("toast.settingUpdated"),
                        t!("toast.settingUpdatedDesc", status = status),
                        window,
                        cx,
                    )
                }
                Err(e) => ui::notify_error(t!("toast.updateSettingFailed"), e, window, cx),
            },
        );
    }

    pub(super) fn toggle_auto_backup(&mut self, cx: &mut Context<Self>) {
        self.store
            .update(cx, |store, cx| {
                store.update_config(cx, |config| config.auto_backup = !config.auto_backup)
            })
            .ok();
    }

    pub(super) fn set_locale(&mut self, locale: &str, cx: &mut Context<Self>) {
        rust_i18n::set_locale(locale);
        let locale = locale.to_string();
        self.store
            .update(cx, |store, cx| {
                store.update_config(cx, |config| config.locale = Some(locale))
            })
            .ok();
        cx.refresh_windows();
    }

    pub(super) fn select_custom_eve_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(t!("dialog.selectEveFolder").to_string().into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let path_string = path.to_string_lossy().into_owned();
                let result = this.store.update(cx, |store, cx| {
                    store.update_config(cx, |config| {
                        config.custom_eve_path = Some(path_string.clone())
                    })
                });
                match result {
                    Ok(()) => {
                        ui::notify_success(t!("toast.customPathSet"), path_string, window, cx)
                    }
                    Err(e) => ui::notify_error(t!("toast.setPathFailed"), e, window, cx),
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn clear_custom_eve_path(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.store.update(cx, |store, cx| {
            store.update_config(cx, |config| config.custom_eve_path = None)
        });
        match result {
            Ok(()) => {
                ui::notify_success(t!("toast.pathReset"), t!("toast.pathResetDesc"), window, cx)
            }
            Err(e) => ui::notify_error(t!("toast.resetPathFailed"), e, window, cx),
        }
    }

    pub(super) fn export_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let directory = dirs::download_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(std::env::temp_dir);
        let suggested = format!(
            "eve-wrench-export-{}.zip",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let destination = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(dest))) = destination.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                let shown = dest.to_string_lossy().into_owned();
                this.run_job(
                    window,
                    cx,
                    move |locations| archive::export_settings(&locations, &dest),
                    move |_, result, window, cx| match result {
                        Ok(count) => ui::notify_success(
                            t!("toast.settingsExported"),
                            t!("toast.settingsExportedDesc", count = count, path = shown),
                            window,
                            cx,
                        ),
                        Err(e) => ui::notify_error(t!("toast.exportFailed"), e, window, cx),
                    },
                );
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn import_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t!("dialog.importSettings").to_string().into()),
        });
        let locations = self.store.read(cx).locations().clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let analysis = cx
                .background_spawn({
                    let path = path.clone();
                    async move { archive::analyze_import(&locations, &path) }
                })
                .await;
            this.update_in(cx, |this, window, cx| match analysis {
                Ok(analysis) => this.show_import_dialog(path, analysis, window, cx),
                Err(e) => ui::notify_error(t!("toast.importAnalysisFailed"), e, window, cx),
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn execute_import(
        &mut self,
        path: PathBuf,
        overwrite: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_job(
            window,
            cx,
            move |locations| archive::execute_import(&locations, &path, &overwrite),
            |_, result, window, cx| match result {
                Ok(r) => ui::notify_success(
                    t!("toast.settingsImported"),
                    t!(
                        "toast.settingsImportedDesc",
                        imported = r.imported_count,
                        skipped = r.skipped_count,
                        backedUp = r.backed_up_count
                    ),
                    window,
                    cx,
                ),
                Err(e) => ui::notify_error(t!("toast.importFailed"), e, window, cx),
            },
        );
    }
}
