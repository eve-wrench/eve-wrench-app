use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use eve_wrench_core::config::ThemePreference;
use eve_wrench_core::{Server, SettingsKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{
    ActiveTheme as _, Icon, Sizable as _, ThemeMode, WindowExt as _, h_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

pub trait StyleExt: Styled + Sized {
    fn font_medium(self) -> Self {
        self.font_weight(FontWeight::MEDIUM)
    }

    fn font_semibold(self) -> Self {
        self.font_weight(FontWeight::SEMIBOLD)
    }
}

impl<T: Styled> StyleExt for T {}

pub fn theme_mode(preference: ThemePreference) -> ThemeMode {
    match preference {
        ThemePreference::Light => ThemeMode::Light,
        ThemePreference::Dark => ThemeMode::Dark,
    }
}

pub fn theme_preference(mode: ThemeMode) -> ThemePreference {
    if mode.is_dark() {
        ThemePreference::Dark
    } else {
        ThemePreference::Light
    }
}

pub fn server_color(server: Server) -> Hsla {
    match server {
        Server::Tranquility => hsla(160. / 360., 1.0, 0.4, 1.0),
        Server::Singularity => hsla(280. / 360., 0.8, 0.6, 1.0),
        Server::Thunderdome => hsla(35. / 360., 1.0, 0.5, 1.0),
        Server::Serenity => hsla(200. / 360., 0.8, 0.5, 1.0),
    }
}

pub fn amber(cx: &App) -> Hsla {
    if cx.theme().is_dark() {
        hsla(43. / 360., 0.96, 0.56, 1.0)
    } else {
        hsla(32. / 360., 0.95, 0.44, 1.0)
    }
}

// Row accents: where settings are copied from, and where they go.
pub fn source_color() -> Hsla {
    hsla(212. / 360., 0.9, 0.6, 1.0)
}

pub fn target_color() -> Hsla {
    hsla(152. / 360., 0.62, 0.48, 1.0)
}

pub fn emerald() -> Hsla {
    hsla(160. / 360., 0.84, 0.39, 1.0)
}

pub fn relative_time(timestamp: u64) -> SharedString {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let diff = now.saturating_sub(timestamp);
    let text = match diff {
        0..60 => t!("time.justNow"),
        60..3600 => t!("time.minutesAgo", n = diff / 60),
        3600..86400 => t!("time.hoursAgo", n = diff / 3600),
        _ => t!("time.daysAgo", n = diff / 86400),
    };
    text.to_string().into()
}

pub fn kind_icon(kind: SettingsKind) -> IconName {
    match kind {
        SettingsKind::User => IconName::User,
        SettingsKind::Char => IconName::Rocket,
    }
}

// Portrait for ESI-resolved characters, otherwise the account or character icon.
pub fn entry_avatar(
    kind: SettingsKind,
    portrait: Option<Arc<Image>>,
    size: Pixels,
    cx: &App,
) -> AnyElement {
    let muted = cx.theme().muted_foreground;
    let icon = move || {
        Icon::new(kind_icon(kind))
            .small()
            .text_color(muted)
            .into_any_element()
    };

    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(size)
        .overflow_hidden()
        .rounded(px(4.))
        .map(|this| match portrait {
            Some(image) => this.child(
                img(image)
                    .size_full()
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(icon),
            ),
            None => this.child(icon()),
        })
        .into_any_element()
}

pub fn server_badge(server: Server) -> impl IntoElement + use<> {
    let color = server_color(server);
    div()
        .flex_none()
        .px_1()
        .rounded(px(4.))
        .border_1()
        .border_color(color)
        .text_color(color)
        .text_size(px(10.))
        .child(server.short_name())
}

// Space the title bar keeps clear at its leading edge: the traffic lights on
// macOS, a small inset elsewhere (the window controls sit at the far end).
#[cfg(target_os = "macos")]
const TITLE_BAR_INSET: Pixels = px(80.);
#[cfg(not(target_os = "macos"))]
const TITLE_BAR_INSET: Pixels = px(12.);

// Title bar contents with `center` centered on the whole window rather than
// on the space left over after the traffic lights or window controls.
pub fn title_bar_content(
    window: &Window,
    center: impl IntoElement,
    trailing: impl IntoElement,
) -> Div {
    h_flex()
        .relative()
        .w_full()
        .h_full()
        .justify_end()
        .pr_2()
        .child(
            h_flex()
                .absolute()
                .top_0()
                .bottom_0()
                .left(-TITLE_BAR_INSET)
                .w(window.viewport_size().width)
                .justify_center()
                .child(center),
        )
        .child(trailing)
}

pub const ROW_HEIGHT: Pixels = px(44.);
pub const HEADER_HEIGHT: Pixels = px(36.);

// Full-width bar that titles a section; callers append their own columns.
pub fn section_header(cx: &App) -> Div {
    h_flex()
        .h(HEADER_HEIGHT)
        .flex_none()
        .px_4()
        .gap_2()
        .bg(cx.theme().muted.opacity(0.35))
        .border_b_1()
        .border_color(cx.theme().border)
        .text_xs()
        .font_medium()
        .text_color(cx.theme().muted_foreground)
}

// Segmented control. The selected option is a solid pill in the primary
// color so the current choice is obvious at a glance.
pub fn segmented<L: Into<SharedString>>(
    id: impl Into<ElementId>,
    labels: impl IntoIterator<Item = L>,
    selected: Option<usize>,
    on_select: impl Fn(usize, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Stateful<Div> {
    let on_select = Rc::new(on_select);
    let theme = cx.theme();
    let (active_bg, active_fg) = (theme.primary, theme.primary_foreground);
    let (idle_fg, hover_fg) = (theme.muted_foreground, theme.foreground);

    h_flex()
        .id(id)
        .flex_1()
        .p(px(2.))
        .gap(px(2.))
        .rounded(px(6.))
        .bg(theme.muted.opacity(0.7))
        .children(labels.into_iter().enumerate().map(|(i, label)| {
            let on_select = on_select.clone();
            let active = selected == Some(i);
            div()
                .id(i)
                .flex_1()
                .h(px(22.))
                .px_2()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(4.))
                .text_xs()
                .font_medium()
                .cursor_pointer()
                .map(|this| {
                    if active {
                        this.bg(active_bg).text_color(active_fg).shadow_xs()
                    } else {
                        this.text_color(idle_fg)
                            .hover(move |s| s.text_color(hover_fg))
                    }
                })
                .on_click(move |_, window, cx| on_select(i, window, cx))
                .child(label.into())
        }))
}

pub fn section_title(text: impl Into<SharedString>) -> Div {
    div().child(text.into().to_uppercase())
}

// Edge-to-edge list row with a hover wash. Source and target rows are marked
// with a leading accent bar instead of a card highlight.
pub fn list_row(id: impl Into<ElementId>, marker: Option<Hsla>, cx: &App) -> Stateful<Div> {
    let hover = cx.theme().muted.opacity(0.6);
    h_flex()
        .id(id)
        .relative()
        .flex_none()
        .min_h(ROW_HEIGHT)
        .px_4()
        .border_b_1()
        .border_color(cx.theme().border.opacity(0.6))
        .hover(move |style| style.bg(hover))
        .when_some(marker, |this, color| {
            this.bg(color.opacity(0.1)).child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(3.))
                    .bg(color),
            )
        })
}

pub fn count_label(count: usize, cx: &App) -> impl IntoElement + use<> {
    div()
        .text_color(cx.theme().muted_foreground.opacity(0.7))
        .child(count.to_string())
}

pub fn notify_success(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    window.push_notification(Notification::success(message).title(title), cx);
}

pub fn notify_error(
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) {
    window.push_notification(Notification::error(message).title(title), cx);
}

pub fn format_distance(km: f64) -> String {
    const AU_KM: f64 = 149_597_870.7;
    if km >= AU_KM / 100. {
        format!("{:.2} AU", km / AU_KM)
    } else {
        format!("{} km", group_thousands(km.round() as i64))
    }
}

fn group_thousands(value: i64) -> String {
    let digits = value.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if value < 0 {
        out.insert(0, '-');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::format_distance;

    #[test]
    fn distances_use_km_until_a_hundredth_of_an_au() {
        assert_eq!(format_distance(250.), "250 km");
        assert_eq!(format_distance(1_234_567.4), "1,234,567 km");
        assert_eq!(format_distance(149_597_870.7), "1.00 AU");
    }
}
