// File, clipboard and wording helpers for share files, used by the main
// window's share dialogs and the formation editor.

use eve_wrench_core::confirmations;
use eve_wrench_core::share::{self, GroupItems, GroupPreview, SharePack};
use gpui_kit::*;
use rust_i18n::t;

use crate::ui;

// Names listed under a group before the rest is summed up as "+N more".
const NAME_LIMIT: usize = 8;

pub fn copy_to_clipboard(pack: &SharePack, window: &mut Window, cx: &mut App) {
    match pack.to_paste_string() {
        Ok(text) => {
            let length = text.len();
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            ui::notify_success(
                t!("share.copied"),
                t!("share.copiedDesc", length = length),
                window,
                cx,
            );
        }
        Err(e) => ui::notify_error(t!("share.exportFailed"), e, window, cx),
    }
}

// Names the file after `stem`, keeping only characters every file system takes.
pub fn save_to_file(pack: SharePack, stem: &str, window: &Window, cx: &mut App) {
    let directory = dirs::download_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(std::env::temp_dir);
    let stem: String = stem
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let suggested = format!("{}.{}", stem.trim_matches('-'), share::FILE_EXTENSION);
    let destination = cx.prompt_for_new_path(&directory, Some(&suggested));
    window
        .spawn(cx, async move |cx| {
            let Ok(Ok(Some(dest))) = destination.await else {
                return;
            };
            let shown = dest.display().to_string();
            let result = cx
                .background_spawn(async move { pack.write_file(&dest) })
                .await;
            cx.update(|window, cx| match result {
                Ok(()) => ui::notify_success(t!("share.saved"), shown, window, cx),
                Err(e) => ui::notify_error(t!("share.exportFailed"), e, window, cx),
            })
            .ok();
        })
        .detach();
}

pub fn read_clipboard(cx: &App) -> Result<SharePack, String> {
    let text = cx
        .read_from_clipboard()
        .and_then(|item| item.text())
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| t!("share.clipboardEmpty").to_string())?;
    SharePack::parse(&text)
}

pub fn open_file(
    window: &Window,
    cx: &mut App,
    on_loaded: impl FnOnce(Result<SharePack, String>, &mut Window, &mut App) + 'static,
) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some(t!("share.openFile").to_string().into()),
    });
    window
        .spawn(cx, async move |cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = cx
                .background_spawn(async move { SharePack::read_file(&path) })
                .await;
            cx.update(|window, cx| on_loaded(result, window, cx)).ok();
        })
        .detach();
}

pub fn group_label(id: &str) -> String {
    t!(format!("copyGroups.{id}")).to_string()
}

// One line saying how much a group holds.
pub fn items_summary(items: &GroupItems) -> String {
    match items {
        GroupItems::Formations(formations) => {
            t!("share.formationCount", count = formations.len()).to_string()
        }
        GroupItems::Overview { profiles, tabs } => t!(
            "share.overviewCount",
            profiles = profiles.len(),
            tabs = tabs.len()
        )
        .to_string(),
        GroupItems::Dialogs(keys) => t!("share.dialogCount", count = keys.len()).to_string(),
        GroupItems::Settings(count) => t!("share.settingCount", count = count).to_string(),
    }
}

// Names worth showing for a group, shortened to a single line.
pub fn item_names(items: &GroupItems) -> Option<String> {
    let names: Vec<String> = match items {
        GroupItems::Formations(formations) => {
            formations.iter().map(|(name, _)| name.clone()).collect()
        }
        GroupItems::Overview { tabs, profiles } if !tabs.is_empty() => tabs
            .iter()
            .map(|(tab, profile)| format!("{tab} ({profile})"))
            .chain(
                profiles
                    .iter()
                    .filter(|p| !tabs.iter().any(|(_, used)| used == *p))
                    .cloned(),
            )
            .collect(),
        GroupItems::Overview { profiles, .. } => profiles.clone(),
        GroupItems::Dialogs(keys) => {
            let catalog = confirmations::catalog();
            let language = ui::catalog_language();
            keys.iter()
                .map(|key| {
                    catalog
                        .binary_search_by(|c| c.key.as_str().cmp(key))
                        .ok()
                        .map(|i| catalog[i].title(language).to_string())
                        .unwrap_or_else(|| key.clone())
                })
                .collect()
        }
        GroupItems::Settings(_) => return None,
    };
    if names.is_empty() {
        return None;
    }
    let shown = names.iter().take(NAME_LIMIT).cloned().collect::<Vec<_>>();
    let mut line = shown.join(", ");
    if names.len() > NAME_LIMIT {
        line.push_str(&format!(
            ", {}",
            t!("share.andMore", count = names.len() - NAME_LIMIT)
        ));
    }
    Some(line)
}

// What applying a group would do to the chosen file.
pub fn changes_summary(preview: &GroupPreview) -> Option<String> {
    let changes = preview.changes?;
    if changes.is_empty() {
        return Some(t!("share.noChanges").to_string());
    }
    if preview.merges {
        return Some(if changes.unchanged > 0 {
            t!(
                "share.mergeWithDuplicates",
                added = changes.added,
                duplicates = changes.unchanged
            )
            .to_string()
        } else {
            t!("share.mergeAdded", added = changes.added).to_string()
        });
    }
    let parts: Vec<String> = [
        (changes.added, "share.added"),
        (changes.changed, "share.changed"),
        (changes.removed, "share.removed"),
    ]
    .into_iter()
    .filter(|(count, _)| *count > 0)
    .map(|(count, key)| t!(key, count = count).to_string())
    .collect();
    Some(parts.join(", "))
}
