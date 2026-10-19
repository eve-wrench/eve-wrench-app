use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use eve_wrench_core::SettingsEntry;
use eve_wrench_core::drone_groups::{
    self, DEFAULT_PATTERN, DroneCategory, DroneGroupPreset, PATTERN_STYLES, PATTERN_TOKENS,
    PatternError,
};
use eve_wrench_core::esi;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::MainView;
use crate::ui::{self, StyleExt as _};

impl MainView {
    // Reads which presets the account already has, then opens the picker
    // with those selected.
    pub(super) fn edit_drone_groups(
        &mut self,
        entry: SettingsEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = PathBuf::from(&entry.path);
        cx.spawn_in(window, async move |this, cx| {
            let present = cx
                .background_spawn({
                    let path = path.clone();
                    async move { drone_groups::read_drone_groups(&path) }
                })
                .await;
            this.update_in(cx, |this, window, cx| match present {
                Ok(present) => this.show_drone_groups_dialog(entry, present, window, cx),
                Err(e) => ui::notify_error(t!("toast.droneGroupsLoadFailed"), e, window, cx),
            })
            .ok();
        })
        .detach();
    }

    fn show_drone_groups_dialog(
        &mut self,
        entry: SettingsEntry,
        present: HashSet<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = t!("droneGroups.title", name = entry.display_name).to_string();
        let main = cx.entity().downgrade();
        let pattern = (self.store.read(cx).config().drone_group_pattern.clone())
            .unwrap_or_else(|| DEFAULT_PATTERN.to_string());
        let pattern_input = cx.new(|cx| InputState::new(window, cx).default_value(pattern));
        let dialog_view = cx.new(|cx| DroneGroupsDialog {
            path: PathBuf::from(entry.path),
            step: Step::Groups,
            selected: present.clone(),
            present,
            main,
            icons: HashMap::new(),
            _subscription: cx.subscribe(&pattern_input, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            pattern_input,
        });

        // Icons fill in once downloaded; cached ones arrive almost at once
        let type_ids: Vec<i64> = drone_groups::catalog()
            .iter()
            .filter_map(|p| p.type_id)
            .collect();
        let cache_dir = self.store.read(cx).locations().data_dir().join("icons");
        let view = dialog_view.downgrade();
        cx.spawn(async move |_, cx| {
            let icons = cx
                .background_spawn(async move { esi::type_icons(&type_ids, &cache_dir) })
                .await;
            view.update(cx, |dialog, cx| {
                dialog.icons = icons
                    .into_iter()
                    .map(|(id, png)| {
                        (
                            id,
                            Arc::new(Image::from_bytes(ImageFormat::Png, png.to_vec())),
                        )
                    })
                    .collect();
                cx.notify();
            })
            .ok();
        })
        .detach();

        window.open_dialog(cx, move |dialog, _, _| {
            dialog
                .title(title.clone())
                .w(px(900.))
                .child(dialog_view.clone())
        });
    }

    fn save_drone_groups(
        &mut self,
        path: PathBuf,
        selected: HashSet<String>,
        pattern: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let backup = self.store.read(cx).config().auto_backup;
        let count = selected.len();
        let stored = (pattern != DEFAULT_PATTERN).then(|| pattern.clone());
        if let Err(e) = self.store.update(cx, |store, cx| {
            store.update_config(cx, |config| config.drone_group_pattern = stored)
        }) {
            ui::notify_error(t!("toast.updateSettingFailed"), e, window, cx);
        }
        self.run_job(
            window,
            cx,
            move |_| drone_groups::write_drone_groups(&path, &selected, &pattern, backup),
            move |_, result, window, cx| match result {
                Ok(()) => ui::notify_success(
                    t!("toast.droneGroupsSaved"),
                    t!("toast.droneGroupsSavedDesc", count = count),
                    window,
                    cx,
                ),
                Err(e) => ui::notify_error(t!("toast.droneGroupsSaveFailed"), e, window, cx),
            },
        );
    }
}

const CATEGORIES: [DroneCategory; 8] = [
    DroneCategory::Light,
    DroneCategory::Medium,
    DroneCategory::Heavy,
    DroneCategory::Sentry,
    DroneCategory::Special,
    DroneCategory::Ewar,
    DroneCategory::Logistics,
    DroneCategory::Industry,
];

// Examples for the naming step: a drone, a sentry and an EWAR group
const EXAMPLE_IDS: [&str; 3] = ["hobgoblin", "garde", "light-ecm"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Groups,
    Naming,
    Review,
}

impl Step {
    const ALL: [Step; 3] = [Step::Groups, Step::Naming, Step::Review];

    fn id(self) -> &'static str {
        match self {
            Step::Groups => "groups",
            Step::Naming => "naming",
            Step::Review => "review",
        }
    }
}

struct DroneGroupsDialog {
    path: PathBuf,
    step: Step,
    // Presets the account has now, and the ones it will have
    present: HashSet<String>,
    selected: HashSet<String>,
    main: WeakEntity<MainView>,
    pattern_input: Entity<InputState>,
    icons: HashMap<i64, Arc<Image>>,
    _subscription: Subscription,
}

impl DroneGroupsDialog {
    fn pattern(&self, cx: &App) -> String {
        self.pattern_input.read(cx).value().to_string()
    }

    fn icon(&self, preset: &DroneGroupPreset, size: Pixels, cx: &App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let placeholder = move || {
            Icon::new(IconName::Drone)
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
            .rounded(px(4.))
            .overflow_hidden()
            .bg(cx.theme().muted)
            .map(
                |this| match preset.type_id.and_then(|id| self.icons.get(&id)) {
                    Some(image) => this.child(
                        img(image.clone())
                            .size_full()
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(placeholder),
                    ),
                    None => this.child(placeholder()),
                },
            )
            .into_any_element()
    }

    fn render_steps(&self, invalid: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let labels = Step::ALL.into_iter().enumerate().map(|(i, step)| {
            format!(
                "{}. {}",
                i + 1,
                t!(format!("droneGroups.steps.{}", step.id()))
            )
        });
        let current = Step::ALL.iter().position(|s| *s == self.step);
        let view = cx.entity().downgrade();
        h_flex().child(
            ui::segmented(
                "drone-steps",
                labels,
                current,
                move |i, _, cx| {
                    let step = Step::ALL[i];
                    if step == Step::Review && invalid {
                        return;
                    }
                    view.update(cx, |this, cx| {
                        this.step = step;
                        cx.notify();
                    })
                    .ok();
                },
                cx,
            )
            .flex_none()
            .w(px(360.)),
        )
    }

    // Uppercase title, an optional count and a hairline running to the edge,
    // in the style of the main window's section headers.
    fn section_header(title: String, count: Option<String>, cx: &App) -> Div {
        let theme = cx.theme();
        h_flex()
            .gap_2()
            .py_1()
            .text_xs()
            .font_medium()
            .text_color(theme.muted_foreground)
            .child(ui::section_title(title))
            .when_some(count, |this, count| {
                this.child(
                    div()
                        .text_color(theme.muted_foreground.opacity(0.6))
                        .child(count),
                )
            })
            .child(div().flex_1().h(px(1.)).bg(theme.border.opacity(0.6)))
    }

    fn render_groups(&self, presets: &[DroneGroupPreset], cx: &mut Context<Self>) -> Div {
        v_flex()
            .gap_5()
            .children(CATEGORIES.into_iter().map(|category| {
                let section: Vec<&DroneGroupPreset> =
                    presets.iter().filter(|p| p.category == category).collect();
                let ids: Vec<String> = section.iter().map(|p| p.id.clone()).collect();
                let chosen = ids.iter().filter(|id| self.selected.contains(*id)).count();
                let all_selected = chosen == ids.len();
                let hover = cx.theme().foreground;
                // Long names (maintenance bots, big mining drones) get three columns
                let columns = if section.iter().any(|p| p.name.len() > 18) {
                    3
                } else {
                    4
                };

                v_flex()
                    .gap_2()
                    .child(
                        Self::section_header(
                            t!(format!("droneGroups.category.{}", category.id())).to_string(),
                            Some(format!("{chosen}/{}", ids.len())),
                            cx,
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!(
                                    "drone-toggle-{}",
                                    category.id()
                                )))
                                .cursor_pointer()
                                .hover(move |s| s.text_color(hover))
                                .child(if all_selected {
                                    t!("importDialog.deselectAll").to_string()
                                } else {
                                    t!("importDialog.selectAll").to_string()
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    for id in &ids {
                                        if all_selected {
                                            this.selected.remove(id);
                                        } else {
                                            this.selected.insert(id.clone());
                                        }
                                    }
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        h_flex().flex_wrap().gap_y_2().children(
                            section
                                .into_iter()
                                .map(|preset| self.render_tile(preset, columns, cx)),
                        ),
                    )
            }))
    }

    fn render_tile(
        &self,
        preset: &DroneGroupPreset,
        columns: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = preset.id.clone();
        let selected = self.selected.contains(&preset.id);
        let theme = cx.theme();
        let accent = ui::target_color();
        let (border, hover, fg, muted) = (
            theme.border.opacity(0.5),
            theme.muted.opacity(0.5),
            theme.foreground,
            theme.muted_foreground,
        );
        let detail = preset
            .stats
            .damage
            .clone()
            .or_else(|| preset.stats.effect.clone())
            .unwrap_or_default();

        let cell = if columns == 3 {
            div().w_1_3()
        } else {
            div().w_1_4()
        };
        cell.pr_2().child(
            h_flex()
                .id(SharedString::from(format!("drone-tile-{}", preset.id)))
                .relative()
                .gap_2()
                .p_1p5()
                .rounded_lg()
                .border_1()
                .cursor_pointer()
                .map(|this| {
                    if selected {
                        this.bg(accent.opacity(0.07))
                            .border_color(accent.opacity(0.22))
                    } else {
                        this.border_color(border).hover(move |s| s.bg(hover))
                    }
                })
                .child(
                    div()
                        .when(!selected, |this| this.opacity(0.35))
                        .child(self.icon(preset, px(32.), cx)),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .truncate()
                                .text_color(if selected { fg } else { muted })
                                .child(preset.name.clone()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .truncate()
                                .text_color(muted.opacity(if selected { 1.0 } else { 0.6 }))
                                .child(detail),
                        ),
                )
                .child(check_mark(selected, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.selected.remove(&id) {
                        this.selected.insert(id.clone());
                    }
                    cx.notify();
                })),
        )
    }

    fn render_naming(
        &self,
        presets: &[DroneGroupPreset],
        error: Option<PatternError>,
        cx: &mut Context<Self>,
    ) -> Div {
        let current = self.pattern(cx);
        let theme = cx.theme();
        let accent = ui::target_color();
        let (border, hover, muted, danger, panel) = (
            theme.border.opacity(0.5),
            theme.muted.opacity(0.5),
            theme.muted_foreground,
            theme.danger,
            theme.muted.opacity(0.35),
        );
        let examples: Vec<&DroneGroupPreset> = presets
            .iter()
            .filter(|p| EXAMPLE_IDS.contains(&p.id.as_str()))
            .collect();
        let custom = !PATTERN_STYLES.iter().any(|(_, p)| *p == current);

        let styles: Vec<Stateful<Div>> = PATTERN_STYLES
            .iter()
            .map(|(id, pattern)| {
                let selected = current == *pattern;
                v_flex()
                    .id(SharedString::from(format!("drone-style-{id}")))
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .p_3()
                    .rounded_lg()
                    .border_1()
                    .cursor_pointer()
                    .map(|this| {
                        if selected {
                            this.bg(accent.opacity(0.07))
                                .border_color(accent.opacity(0.22))
                        } else {
                            this.border_color(border).hover(move |s| s.bg(hover))
                        }
                    })
                    .child(
                        h_flex()
                            .pb_1()
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .font_medium()
                                    .child(t!(format!("droneGroups.style.{id}")).to_string()),
                            )
                            .child(check_mark(selected, cx)),
                    )
                    .children(examples.iter().map(|preset| {
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(muted)
                            .child(preset.label(pattern))
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.pattern_input.update(cx, |input, cx| {
                            input.set_value(pattern.to_string(), window, cx)
                        });
                        cx.notify();
                    }))
            })
            .collect();

        let tokens: Vec<Div> = PATTERN_TOKENS
            .iter()
            .map(|token| {
                h_flex()
                    .w_1_2()
                    .gap_2()
                    .py_0p5()
                    .text_xs()
                    .child(
                        div()
                            .w(px(64.))
                            .flex_none()
                            .font_medium()
                            .child(format!("{{{token}}}")),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(muted)
                            .child(t!(format!("droneGroups.tokens.{token}")).to_string()),
                    )
            })
            .collect();

        let invalid = error.is_some();
        let status = match error {
            Some(PatternError::MustStartWithName) => t!("droneGroups.patternStart").to_string(),
            Some(PatternError::UnknownToken(token)) => {
                t!("droneGroups.patternToken", token = token).to_string()
            }
            Some(PatternError::Unbalanced) => t!("droneGroups.patternBrackets").to_string(),
            None => examples
                .iter()
                .map(|p| p.label(&current))
                .collect::<Vec<_>>()
                .join("   ·   "),
        };

        v_flex()
            .gap_5()
            .child(
                v_flex()
                    .gap_2()
                    .child(Self::section_header(
                        t!("droneGroups.styles").to_string(),
                        None,
                        cx,
                    ))
                    .child(h_flex().gap_2().children(styles)),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        Self::section_header(t!("droneGroups.custom").to_string(), None, cx)
                            .child(check_mark(custom, cx)),
                    )
                    .child(Input::new(&self.pattern_input).small())
                    .child(
                        div()
                            .text_xs()
                            .truncate()
                            .text_color(if invalid { danger } else { muted })
                            .child(status),
                    ),
            )
            .child(
                v_flex()
                    .gap_2()
                    .p_3()
                    .rounded_lg()
                    .bg(panel)
                    .child(h_flex().flex_wrap().children(tokens))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(t!("droneGroups.optionalHint").to_string()),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(t!("droneGroups.namingHint").to_string()),
            )
    }

    fn render_review(&self, presets: &[DroneGroupPreset], cx: &mut Context<Self>) -> Div {
        let pattern = self.pattern(cx);
        let theme = cx.theme();
        let accent = ui::target_color();
        let (muted, pill) = (theme.muted_foreground, theme.muted.opacity(0.7));
        let removed: Vec<&DroneGroupPreset> = presets
            .iter()
            .filter(|p| self.present.contains(&p.id) && !self.selected.contains(&p.id))
            .collect();

        let tag = |text: String, color: Hsla, bg: Hsla| {
            div()
                .flex_none()
                .px_1p5()
                .rounded(px(4.))
                .bg(bg)
                .text_color(color)
                .text_size(px(10.))
                .font_medium()
                .child(text)
        };
        let row = |preset: &DroneGroupPreset, badge: Option<Div>, struck: bool| {
            h_flex()
                .gap_2()
                .py_1()
                .px_1()
                .border_b_1()
                .border_color(theme.border.opacity(0.4))
                .child(
                    div()
                        .when(struck, |this| this.opacity(0.4))
                        .child(self.icon(preset, px(20.), cx)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_sm()
                        .truncate()
                        .when(struck, |this| this.line_through().text_color(muted))
                        .child(if struck {
                            preset.name.clone()
                        } else {
                            preset.label(&pattern)
                        }),
                )
                .children(badge)
        };

        v_flex()
            .gap_5()
            .child(
                div()
                    .text_xs()
                    .text_color(muted)
                    .child(t!("droneGroups.reviewHint", count = self.selected.len()).to_string()),
            )
            .children(CATEGORIES.into_iter().filter_map(|category| {
                let section: Vec<&DroneGroupPreset> = presets
                    .iter()
                    .filter(|p| p.category == category && self.selected.contains(&p.id))
                    .collect();
                (!section.is_empty()).then(|| {
                    let title = t!(format!("droneGroups.category.{}", category.id())).to_string();
                    v_flex()
                        .child(Self::section_header(
                            title,
                            Some(section.len().to_string()),
                            cx,
                        ))
                        .children(section.into_iter().map(|preset| {
                            let badge = if self.present.contains(&preset.id) {
                                tag(t!("droneGroups.updated").to_string(), muted, pill)
                            } else {
                                tag(
                                    t!("droneGroups.new").to_string(),
                                    accent,
                                    accent.opacity(0.12),
                                )
                            };
                            row(preset, Some(badge), false)
                        }))
                })
            }))
            .when(!removed.is_empty(), |this| {
                this.child(
                    v_flex()
                        .child(Self::section_header(
                            t!("droneGroups.removed").to_string(),
                            Some(removed.len().to_string()),
                            cx,
                        ))
                        .children(removed.into_iter().map(|preset| row(preset, None, true)))
                        .child(
                            div()
                                .pt_1()
                                .text_xs()
                                .text_color(muted)
                                .child(t!("droneGroups.removedHint").to_string()),
                        ),
                )
            })
    }

    fn render_footer(&self, invalid: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let step = self.step;
        let back = match step {
            Step::Groups => None,
            Step::Naming => Some(Step::Groups),
            Step::Review => Some(Step::Naming),
        };
        let next = match step {
            Step::Groups => Some(Step::Naming),
            Step::Naming => Some(Step::Review),
            Step::Review => None,
        };

        h_flex()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        t!(
                            "droneGroups.selectedCount",
                            selected = self.selected.len(),
                            total = drone_groups::catalog().len()
                        )
                        .to_string(),
                    ),
            )
            .child(
                Button::new("drone-groups-cancel")
                    .ghost()
                    .label(t!("common.cancel").to_string())
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .when_some(back, |this, back| {
                this.child(
                    Button::new("drone-groups-back")
                        .outline()
                        .icon(IconName::ChevronLeft)
                        .label(t!("droneGroups.back").to_string())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.step = back;
                            cx.notify();
                        })),
                )
            })
            .map(|this| match next {
                Some(next) => this.child(
                    Button::new("drone-groups-next")
                        .primary()
                        .label(t!("droneGroups.next").to_string())
                        .disabled(step == Step::Naming && invalid)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.step = next;
                            cx.notify();
                        })),
                ),
                None => this.child(
                    Button::new("drone-groups-save")
                        .primary()
                        .label(t!("droneGroups.save", count = self.selected.len()).to_string())
                        .disabled(invalid)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let path = this.path.clone();
                            let selected = this.selected.clone();
                            let pattern = this.pattern(cx);
                            this.main
                                .update(cx, |main, cx| {
                                    main.save_drone_groups(path, selected, pattern, window, cx)
                                })
                                .ok();
                            window.close_dialog(cx);
                        })),
                ),
            })
    }
}

// A filled accent dot with a check when selected, an empty ring otherwise.
fn check_mark(selected: bool, cx: &App) -> Div {
    let accent = ui::target_color();
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(16.))
        .rounded_full()
        .map(|this| {
            if selected {
                this.bg(accent).child(
                    Icon::new(IconName::Check)
                        .xsmall()
                        .text_color(cx.theme().background),
                )
            } else {
                this.border_1().border_color(cx.theme().border)
            }
        })
}

impl Render for DroneGroupsDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let presets = drone_groups::catalog();
        let error = drone_groups::validate_pattern(&self.pattern(cx)).err();
        let invalid = error.is_some();
        let body = match self.step {
            Step::Groups => self.render_groups(&presets, cx),
            Step::Naming => self.render_naming(&presets, error, cx),
            Step::Review => self.render_review(&presets, cx),
        };

        v_flex()
            .gap_3()
            .child(self.render_steps(invalid, cx))
            .child(
                div()
                    .id("drone-groups-body")
                    // Fill the window, leaving room for the title, steps and footer
                    .h((window.viewport_size().height - px(300.)).clamp(px(280.), px(720.)))
                    .overflow_y_scroll()
                    .pr_1()
                    .child(body),
            )
            .child(self.render_footer(invalid, cx))
    }
}
