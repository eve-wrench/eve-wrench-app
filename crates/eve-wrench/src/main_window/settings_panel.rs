use eve_wrench_core::updates::{APP_VERSION, is_preview_version};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::MainView;
use crate::LOCALES;
use crate::store::Store;
use crate::ui::{self, StyleExt as _};

// Contents of the settings popover in the title bar.
pub struct SettingsPanel {
    store: Entity<Store>,
    main: WeakEntity<MainView>,
    _subscription: Subscription,
}

impl SettingsPanel {
    pub fn new(main: WeakEntity<MainView>, cx: &mut Context<Self>) -> Self {
        let store = Store::global(cx);
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        Self {
            store,
            main,
            _subscription: subscription,
        }
    }

    fn on_main(
        &self,
        f: impl Fn(&mut MainView, &mut Window, &mut Context<MainView>) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        let main = self.main.clone();
        move |_, window, cx| {
            main.update(cx, |this, cx| f(this, window, cx)).ok();
        }
    }

    fn section(title: String, cx: &App) -> Div {
        ui::section_header(cx).child(ui::section_title(title))
    }

    fn row(cx: &App) -> Div {
        h_flex()
            .px_4()
            .py_2p5()
            .gap_3()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.6))
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let config = self.store.read(cx).config().clone();
        let muted = cx.theme().muted_foreground;
        let current_locale = rust_i18n::locale().to_string();
        let locale_index = LOCALES
            .iter()
            .position(|(code, _)| *code == current_locale)
            .unwrap_or(0);
        let main = self.main.clone();

        let folder = match &config.custom_eve_path {
            Some(path) => div().truncate().child(path.clone()),
            None => div()
                .text_color(muted)
                .child(t!("settings.defaultLocation").to_string()),
        };

        let mut version = format!("EVE Wrench {}", APP_VERSION);
        if is_preview_version(APP_VERSION) {
            version = format!("{} · {}", version, t!("titleBar.preview"));
        }

        v_flex()
            .w(px(320.))
            .text_sm()
            .child(Self::section(
                t!("settings.eveSettingsFolder").to_string(),
                cx,
            ))
            .child(
                Self::row(cx)
                    .child(div().flex_1().min_w_0().text_xs().child(folder))
                    .child(
                        Button::new("change-folder")
                            .outline()
                            .xsmall()
                            .icon(IconName::FolderOpen)
                            .label(t!("settings.change").to_string())
                            .on_click(self.on_main(|this, window, cx| {
                                this.select_custom_eve_path(window, cx)
                            })),
                    )
                    .when(config.custom_eve_path.is_some(), |this| {
                        this.child(
                            Button::new("reset-folder")
                                .ghost()
                                .xsmall()
                                .icon(IconName::RotateCcw)
                                .tooltip(t!("settings.resetToDefault").to_string())
                                .on_click(self.on_main(|this, window, cx| {
                                    this.clear_custom_eve_path(window, cx)
                                })),
                        )
                    }),
            )
            .child(Self::section(t!("settings.backups").to_string(), cx))
            .child(
                Self::row(cx)
                    .child(div().flex_1().child(t!("settings.autoBackup").to_string()))
                    .child(
                        Switch::new("auto-backup")
                            .small()
                            .checked(config.auto_backup)
                            .on_change(move |_, _, cx| {
                                main.update(cx, |this, cx| this.toggle_auto_backup(cx)).ok();
                            }),
                    ),
            )
            .child(Self::section(t!("settings.language").to_string(), cx))
            .child(Self::row(cx).child(ui::segmented(
                "language",
                LOCALES.iter().map(|(_, name)| *name),
                Some(locale_index),
                {
                    let main = self.main.clone();
                    move |index, _, cx| {
                        if let Some((code, _)) = LOCALES.get(index) {
                            main.update(cx, |this, cx| this.set_locale(code, cx)).ok();
                        }
                    }
                },
                cx,
            )))
            .child(Self::section(
                format!(
                    "{} / {}",
                    t!("importExport.import"),
                    t!("importExport.export")
                ),
                cx,
            ))
            .child(
                Self::row(cx)
                    .child(
                        Button::new("export-settings")
                            .outline()
                            .small()
                            .flex_1()
                            .icon(IconName::Download)
                            .label(t!("importExport.export").to_string())
                            .on_click(
                                self.on_main(|this, window, cx| this.export_settings(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("import-settings")
                            .outline()
                            .small()
                            .flex_1()
                            .icon(IconName::Upload)
                            .label(t!("importExport.import").to_string())
                            .on_click(
                                self.on_main(|this, window, cx| this.import_settings(window, cx)),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .px_4()
                    .py_2p5()
                    .gap_3()
                    .bg(cx.theme().muted.opacity(0.35))
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(muted)
                            .font_medium()
                            .child(version),
                    )
                    .child(
                        Button::new("check-updates")
                            .ghost()
                            .xsmall()
                            .icon(IconName::RefreshCw)
                            .label(t!("update.checkForUpdates").to_string())
                            .on_click(
                                self.on_main(|this, window, cx| this.check_for_updates(window, cx)),
                            ),
                    ),
            )
    }
}
