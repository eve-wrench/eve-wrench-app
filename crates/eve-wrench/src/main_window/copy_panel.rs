use eve_wrench_core::SettingsKind;
use eve_wrench_core::copy::COPY_GROUPS;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::{DraggedSettings, MainView, Source};
use crate::table::{self, Column};
use crate::ui;

// Avatar, name, server badge, remove. The remove slot is kept on the source
// row too, so badges line up across both lists.
const PANEL_COLUMNS: [Column; 4] = [
    Column::fixed(px(36.)),
    Column::fill(),
    Column::fixed(px(44.)).end(),
    Column::fixed(px(28.)).end(),
];

impl MainView {
    pub(super) fn render_copy_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let can_copy = self.source.is_some() && !self.targets.is_empty() && !self.copying;

        v_flex()
            .w(px(300.))
            .flex_none()
            .border_l_1()
            .border_color(cx.theme().border)
            .text_sm()
            .child(self.render_source(cx))
            .child(self.render_targets(cx))
            .children(
                self.source
                    .as_ref()
                    .map(|source| self.render_group_options(source.kind(), cx)),
            )
            .child(
                div()
                    .flex_none()
                    .p_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("copy-settings")
                            .primary()
                            .w_full()
                            .icon(IconName::Copy)
                            .label(if self.copying {
                                t!("copyPanel.copying").to_string()
                            } else {
                                t!("copyPanel.copySettings").to_string()
                            })
                            .loading(self.copying)
                            .disabled(!can_copy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.execute_copy(window, cx)),
                            ),
                    ),
            )
    }

    fn panel_header(
        &self,
        id: &'static str,
        title: String,
        count: Option<usize>,
        clearable: bool,
        on_clear: fn(&mut Self),
        cx: &mut Context<Self>,
    ) -> Div {
        ui::section_header(cx)
            .justify_between()
            .child(
                h_flex()
                    .gap_2()
                    .child(ui::section_title(title))
                    .children(count.map(|n| ui::count_label(n, cx))),
            )
            .when(clearable, |this| {
                this.child(
                    Button::new(id)
                        .ghost()
                        .xsmall()
                        .label(t!("common.clear").to_string())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            on_clear(this);
                            cx.notify();
                        })),
                )
            })
    }

    fn empty_hint(text: String, cx: &App) -> Div {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .gap_0p5()
            .px_4()
            .py_3()
            .text_xs()
            .text_color(muted)
            .child(text)
            .child(
                div()
                    .text_color(muted.opacity(0.7))
                    .child(t!("copyPanel.dropHint").to_string()),
            )
    }

    fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let header = self.panel_header(
            "clear-source",
            t!("copyPanel.source").to_string(),
            None,
            self.source.is_some(),
            |this| this.source = None,
            cx,
        );

        let body = match &self.source {
            Some(source) => {
                let entry = match source {
                    Source::Entry(e) => Some(e),
                    Source::Backup(_) => None,
                };
                let portrait = entry.and_then(|e| self.store.read(cx).portrait(&e.id));
                let name = v_flex()
                    .min_w_0()
                    .child(div().truncate().child(source.display_name().to_string()))
                    .when(entry.is_none(), |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(t!("titleBar.backups").to_string()),
                        )
                    });
                table::cells(
                    ui::list_row("source-row", Some(ui::source_color()), cx).h(ui::ROW_HEIGHT),
                    &PANEL_COLUMNS,
                    [
                        ui::entry_avatar(source.kind(), portrait, px(24.), cx),
                        name.into_any_element(),
                        entry
                            .map(|e| ui::server_badge(e.server).into_any_element())
                            .unwrap_or_else(table::empty),
                        table::empty(),
                    ],
                )
                .into_any_element()
            }
            None => Self::empty_hint(t!("copyPanel.noSourceSelected").to_string(), cx)
                .into_any_element(),
        };

        let tint = ui::source_color().opacity(0.12);
        v_flex()
            .flex_none()
            .child(header)
            .child(body)
            .drag_over::<DraggedSettings>(move |style, _, _, _| style.bg(tint))
            .on_drop(cx.listener(|this, dragged: &DraggedSettings, _, cx| {
                this.set_source(dragged.0.clone(), cx)
            }))
    }

    fn render_targets(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let has_targets = !self.targets.is_empty();
        let header = self
            .panel_header(
                "clear-targets",
                t!("copyPanel.targets").to_string(),
                has_targets.then_some(self.targets.len()),
                has_targets,
                |this| this.targets.clear(),
                cx,
            )
            .border_t_1();

        let rows: Vec<AnyElement> = self
            .targets
            .iter()
            .map(|target| {
                let path = target.path.clone();
                let portrait = self.store.read(cx).portrait(&target.id);
                let remove = div()
                    .opacity(0.)
                    .group_hover("target-row", |s| s.opacity(1.))
                    .child(
                        Button::new(SharedString::from(format!("remove-{}", target.path)))
                            .ghost()
                            .xsmall()
                            .icon(IconName::X)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.targets.retain(|t| t.path != path);
                                cx.notify();
                            })),
                    );
                table::cells(
                    ui::list_row(
                        SharedString::from(format!("target-{}", target.path)),
                        Some(ui::target_color()),
                        cx,
                    )
                    .h(ui::ROW_HEIGHT)
                    .group("target-row"),
                    &PANEL_COLUMNS,
                    [
                        ui::entry_avatar(target.kind, portrait, px(24.), cx),
                        div()
                            .truncate()
                            .child(target.display_name.clone())
                            .into_any_element(),
                        ui::server_badge(target.server).into_any_element(),
                        remove.into_any_element(),
                    ],
                )
                .into_any_element()
            })
            .collect();

        // Only accounts and characters are targets, and only of the source's
        // kind; with no source yet the drop goes through so it can explain why
        let source_kind = self.source.as_ref().map(Source::kind);
        let tint = ui::target_color().opacity(0.12);
        v_flex()
            .flex_1()
            .min_h_0()
            .drag_over::<DraggedSettings>(move |style, _, _, _| style.bg(tint))
            .can_drop(move |value, _, _| {
                value
                    .downcast_ref::<DraggedSettings>()
                    .is_some_and(|dragged| match &dragged.0 {
                        Source::Entry(entry) => source_kind.is_none_or(|kind| kind == entry.kind),
                        Source::Backup(_) => false,
                    })
            })
            .on_drop(cx.listener(|this, dragged: &DraggedSettings, window, cx| {
                if let Source::Entry(entry) = &dragged.0 {
                    this.add_target(entry.clone(), window, cx);
                }
            }))
            .child(header)
            .child(
                div()
                    .id("targets-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .when(!has_targets, |this| {
                        this.child(Self::empty_hint(
                            t!("copyPanel.noTargetsSelected").to_string(),
                            cx,
                        ))
                    })
                    .children(rows),
            )
    }

    fn render_group_options(
        &self,
        kind: SettingsKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let hover = cx.theme().muted.opacity(0.6);
        let groups: Vec<AnyElement> = COPY_GROUPS
            .iter()
            .filter(|g| g.applies_to(kind))
            .map(|group| {
                let id = group.id;
                let checked = self.groups.get(id).copied().unwrap_or(group.default_on);
                h_flex()
                    .id(SharedString::from(format!("group-row-{}", id)))
                    .px_4()
                    .py_1p5()
                    .hover(move |s| s.bg(hover))
                    .child(
                        Checkbox::new(SharedString::from(format!("group-{}", id)))
                            .small()
                            .label(t!(format!("copyGroups.{}", id)).to_string())
                            .checked(checked)
                            .on_change(cx.listener(move |this, checked: &bool, _, cx| {
                                this.groups.insert(id, *checked);
                                cx.notify();
                            })),
                    )
                    .into_any_element()
            })
            .collect();

        v_flex()
            .flex_none()
            .child(
                self.panel_header(
                    "copy-options",
                    t!("copyPanel.copyOptions").to_string(),
                    None,
                    false,
                    |_| {},
                    cx,
                )
                .border_t_1(),
            )
            .child(
                v_flex()
                    .id("copy-groups")
                    .max_h(px(220.))
                    .overflow_y_scroll()
                    .py_1()
                    .children(groups),
            )
            .child(
                div()
                    .px_4()
                    .pb_3()
                    .pt_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("copyPanel.selectiveHint").to_string()),
            )
    }
}
