use eve_wrench_core::window_layout::WindowState;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::model;
use super::{LayoutEditor, RectField};
use crate::ui::{self, StyleExt as _};

impl LayoutEditor {
    fn render_list_row(&self, window: &WindowState, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.selected.as_deref() == Some(window.key.as_str());
        let theme = cx.theme();
        let accent = ui::target_color();
        let hover = theme.muted.opacity(0.6);
        let key = window.key.clone();
        let open = window.open;
        let (fg, muted) = (theme.foreground, theme.muted_foreground);
        h_flex()
            .id(SharedString::from(format!("list-{}", window.key)))
            .gap_2()
            .px_3()
            .py_1()
            .text_sm()
            .cursor_pointer()
            .map(|this| {
                if selected {
                    this.bg(accent.opacity(0.1))
                } else {
                    this.hover(move |s| s.bg(hover))
                }
            })
            .child(
                Icon::new(model::icon(window))
                    .small()
                    .text_color(if selected { accent } else { muted }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_color(if open { fg } else { muted.opacity(0.7) })
                    .child(model::display_name(window, &self.windows)),
            )
            .when(window.locked, |this| {
                this.child(Icon::new(IconName::Lock).xsmall().text_color(muted))
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                // Selecting a closed window shows the closed ones
                if !open {
                    this.show_closed = true;
                }
                this.select(Some(key.clone()), window, cx);
            }))
    }

    // Windows grouped by what they're for, open ones first in each group.
    pub(super) fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let listed: Vec<&WindowState> = self
            .windows
            .iter()
            .filter(|w| model::on_canvas(w, &self.windows) && self.in_location(w))
            .collect();

        let mut sections: Vec<Div> = Vec::new();
        for category in model::Category::ALL {
            let mut list: Vec<&WindowState> = listed
                .iter()
                .copied()
                .filter(|w| model::category(w) == category)
                .collect();
            if list.is_empty() {
                continue;
            }
            list.sort_by_cached_key(|w| {
                (
                    !w.open,
                    model::display_name(w, &self.windows).to_lowercase(),
                )
            });
            let open = list.iter().filter(|w| w.open).count();
            let title = t!(format!("layoutEditor.category.{}", category.id())).to_string();
            sections.push(
                v_flex()
                    .child(div().px_3().pt_3().pb_0p5().child(ui::section_divider(
                        title,
                        Some(format!("{open}/{}", list.len())),
                        cx,
                    )))
                    .children(list.into_iter().map(|w| self.render_list_row(w, cx))),
            );
        }

        v_flex()
            .id("window-list")
            .w(px(240.))
            .flex_none()
            .pb_3()
            .border_r_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll()
            .children(sections)
    }

    fn render_flag(
        &self,
        id: &'static str,
        label: String,
        checked: bool,
        set: fn(&mut WindowState, bool),
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        h_flex()
            .px_4()
            .py_1p5()
            .child(div().flex_1().text_sm().child(label))
            .child(
                Switch::new(id)
                    .small()
                    .checked(checked)
                    .on_click(cx.listener(move |this, checked: &bool, window, cx| {
                        let checked = *checked;
                        this.edit(window, cx, |this| {
                            this.update_selected(|w| set(w, checked));
                        });
                    })),
            )
    }

    pub(super) fn render_inspector(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (muted, mono, border) = (
            theme.muted_foreground,
            theme.mono_font_family.clone(),
            theme.border,
        );
        let content = match self.selected_window().cloned() {
            None => div()
                .px_4()
                .py_3()
                .text_xs()
                .text_color(muted)
                .child(t!("layoutEditor.noSelection").to_string())
                .into_any_element(),
            Some(window) => {
                let screen = model::screen(&self.windows);
                let saved_on = window
                    .rect
                    .filter(|r| r.screen_width != screen.width || r.screen_height != screen.height);
                let tabs: Vec<String> = model::members(&self.windows, &window.key)
                    .iter()
                    .map(|w| model::window_name(w.id()))
                    .collect();

                v_flex()
                    .child(
                        v_flex()
                            .px_4()
                            .py_3()
                            .gap_0p5()
                            .child(
                                div()
                                    .font_medium()
                                    .child(model::display_name(&window, &self.windows)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_family(mono)
                                    .text_color(muted)
                                    .child(window.id().to_string()),
                            ),
                    )
                    .when(!tabs.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .px_4()
                                .pb_3()
                                .gap_1()
                                .child(ui::section_divider(
                                    t!("layoutEditor.tabs").to_string(),
                                    Some(tabs.len().to_string()),
                                    cx,
                                ))
                                .children(
                                    tabs.into_iter()
                                        .map(|tab| div().text_sm().text_color(muted).child(tab)),
                                ),
                        )
                    })
                    .child(
                        v_flex()
                            .px_4()
                            .gap_2()
                            .child(ui::section_divider(
                                t!("layoutEditor.geometry").to_string(),
                                None,
                                cx,
                            ))
                            .child(h_flex().flex_wrap().gap_y_2().children(
                                RectField::ALL.iter().zip(&self.rect_inputs).map(
                                    |(field, input)| {
                                        h_flex()
                                            .w_1_2()
                                            .pr_2()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .w(px(14.))
                                                    .text_xs()
                                                    .text_color(muted)
                                                    .child(field.label()),
                                            )
                                            .child(div().flex_1().child(Input::new(input).small()))
                                    },
                                ),
                            ))
                            .when_some(saved_on, |this, rect| {
                                this.child(
                                    div().text_xs().text_color(ui::amber(cx)).child(
                                        t!(
                                            "layoutEditor.otherScreen",
                                            width = rect.screen_width,
                                            height = rect.screen_height
                                        )
                                        .to_string(),
                                    ),
                                )
                            }),
                    )
                    .child(div().px_4().pt_4().child(ui::section_divider(
                        t!("layoutEditor.state").to_string(),
                        None,
                        cx,
                    )))
                    .child(self.render_flag(
                        "flag-open",
                        t!("layoutEditor.openFlag").to_string(),
                        window.open,
                        |w, on| w.open = on,
                        cx,
                    ))
                    .child(self.render_flag(
                        "flag-locked",
                        t!("layoutEditor.locked").to_string(),
                        window.locked,
                        |w, on| w.locked = on,
                        cx,
                    ))
                    .child(self.render_flag(
                        "flag-minimized",
                        t!("layoutEditor.minimized").to_string(),
                        window.minimized,
                        |w, on| w.minimized = on,
                        cx,
                    ))
                    .child(self.render_flag(
                        "flag-compact",
                        t!("layoutEditor.compact").to_string(),
                        window.compact.unwrap_or(self.compact_default),
                        |w, on| w.compact = Some(on),
                        cx,
                    ))
                    .child(self.render_flag(
                        "flag-light",
                        t!("layoutEditor.lightBackground").to_string(),
                        window.light_background.unwrap_or(false),
                        |w, on| w.light_background = Some(on),
                        cx,
                    ))
                    .into_any_element()
            }
        };

        v_flex()
            .id("inspector")
            .w(px(280.))
            .flex_none()
            .border_l_1()
            .border_color(border)
            .overflow_y_scroll()
            .child(content)
    }
}
