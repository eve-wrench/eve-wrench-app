#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod assets;
mod formation_editor;
mod main_window;
mod store;
mod table;
mod ui;
mod updater;

use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode};
use gpui_kit::*;

use crate::store::Store;

rust_i18n::i18n!("locales", fallback = "en");

actions!(eve_wrench, [Quit]);

pub const LOCALES: [(&str, &str); 2] = [("en", "English"), ("zh-CN", "中文")];

fn main() {
    gpui_kit::application()
        .with_assets(assets::AppAssets)
        .run(|cx| {
            gpui_kit::init(cx);

            cx.bind_keys([KeyBinding::new("secondary-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            cx.set_menus(vec![
                Menu::new("EVE Wrench").items([MenuItem::action("Quit EVE Wrench", Quit)]),
            ]);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            formation_editor::init(cx);
            let store = Store::init(cx);
            let config = store.read(cx).config().clone();
            if let Some(locale) = config.locale.as_deref() {
                rust_i18n::set_locale(locale);
            }
            match config.theme {
                Some(theme) => Theme::change(ui::theme_mode(theme), None, cx),
                None => Theme::sync_system_appearance(None, cx),
            }

            main_window::open(cx);
            cx.activate(true);
        });
}

pub fn toggle_theme(window: &mut Window, cx: &mut App) {
    let next = if cx.theme().is_dark() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    Theme::change(next, Some(window), cx);
    // Losing the theme choice on a failed write is harmless; it still applies now
    let _ = Store::global(cx).update(cx, |store, cx| {
        store.update_config(cx, |config| config.theme = Some(ui::theme_preference(next)))
    });
    cx.refresh_windows();
}
