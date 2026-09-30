use eve_wrench_core::updates::{APP_VERSION, is_preview_version};
use gpui_kit::assets::IconName;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, TitleBar, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::ui::StyleExt as _;
use rust_i18n::t;

use super::MainView;
use crate::ui;

impl MainView {
    pub(super) fn render_title_bar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let loading = self.store.read(cx).is_loading();

        TitleBar::new().child(ui::title_bar_content(
            window,
            h_flex()
                .gap_2()
                .child(
                    Icon::new(IconName::Wrench)
                        .small()
                        .text_color(cx.theme().foreground),
                )
                .child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child("EVE WRENCH"),
                )
                .when(is_preview_version(APP_VERSION), |this| {
                    this.child(preview_badge(cx))
                }),
            h_flex()
                .gap_1()
                .child(self.render_settings_menu())
                .child(
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
                )
                .child(
                    Button::new("refresh")
                        .ghost()
                        .small()
                        .icon(IconName::RefreshCw)
                        .loading(loading)
                        .disabled(loading)
                        .tooltip(t!("common.refresh").to_string())
                        .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                ),
        ))
    }

    fn render_settings_menu(&self) -> impl IntoElement + use<> {
        let panel = self.settings_panel.clone();
        Popover::new("settings-popover")
            .anchor(Anchor::TopRight)
            .p_0()
            .overflow_hidden()
            .trigger(
                Button::new("settings")
                    .ghost()
                    .small()
                    .icon(IconName::Settings)
                    .tooltip(t!("titleBar.settings").to_string()),
            )
            .content(move |_, _, _| panel.clone())
    }
}

fn preview_badge(cx: &App) -> impl IntoElement + use<> {
    let amber = crate::ui::amber(cx);
    div()
        .id("preview-badge")
        .px_2()
        .rounded_full()
        .border_1()
        .border_color(amber.opacity(0.5))
        .bg(amber.opacity(0.1))
        .text_size(px(10.))
        .font_semibold()
        .text_color(amber)
        .child(t!("titleBar.preview").to_string().to_uppercase())
        .tooltip(|window, cx| {
            gpui_kit::component::tooltip::Tooltip::new(format!("v{}", APP_VERSION))
                .build(window, cx)
        })
}

// Adapts a view method into a menu item click handler.
pub(super) fn view_action(
    view: &WeakEntity<MainView>,
    f: impl Fn(&mut MainView, &mut Window, &mut Context<MainView>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, window, cx| {
        view.update(cx, |this, cx| f(this, window, cx)).ok();
    }
}
