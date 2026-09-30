use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::model::{
    AXES, Axis, AxisInfo, Formation, MAX_PROBES, PATTERNS, Probe, RANGE_OPTIONS, format_number,
    parse_distance,
};
use super::viewport::step_size;
use super::{Direction, FormationEditor, Section, TransformTool, editor_action};
use crate::table::{self, Column};
use crate::ui;

// Index, N/S, W/E, U/D, range
const PROBE_COLUMNS: [Column; 5] = [
    Column::fixed(px(24.)),
    Column::fill(),
    Column::fill(),
    Column::fill(),
    Column::fixed(px(64.)),
];

const LABEL_WIDTH: Pixels = px(72.);

// Dropdown triggers size to their content, so range pickers get an explicit
// width to fill their column.
const RANGE_BUTTON_WIDTH: Pixels = px(60.);

const TOOLS: [TransformTool; 4] = [
    TransformTool::Scale,
    TransformTool::Rotate,
    TransformTool::Mirror,
    TransformTool::Move,
];

fn axis_label(info: &AxisInfo) -> String {
    format!("{}/{}", info.positive, info.negative)
}

fn axis_index(axis: Axis) -> usize {
    AXES.iter().position(|a| a.axis == axis).unwrap_or(0)
}

// The six compass directions a Move can go, in AXES order.
fn directions() -> Vec<(Direction, &'static str)> {
    AXES.iter()
        .flat_map(|info| {
            [
                (
                    Direction {
                        axis: info.axis,
                        sign: 1.,
                    },
                    info.positive,
                ),
                (
                    Direction {
                        axis: info.axis,
                        sign: -1.,
                    },
                    info.negative,
                ),
            ]
        })
        .collect()
}

impl TransformTool {
    fn label(self) -> String {
        match self {
            TransformTool::Scale => t!("formationEditor.toolScale"),
            TransformTool::Rotate => t!("formationEditor.rotate"),
            TransformTool::Mirror => t!("formationEditor.mirror"),
            TransformTool::Move => t!("formationEditor.move"),
        }
        .to_string()
    }
}

impl FormationEditor {
    fn input_number(input: &Entity<InputState>, cx: &App) -> Option<f64> {
        input
            .read(cx)
            .value()
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
    }

    fn duplicate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(current) = self.current().cloned() else {
            return;
        };
        self.edit(window, cx, |this| {
            this.formations.push(Formation {
                name: format!("{} copy", current.name),
                probes: current.probes,
            });
            this.selected = this.formations.len() - 1;
        });
    }

    fn delete_formation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.current().is_none() {
            return;
        }
        self.edit(window, cx, |this| {
            this.formations.remove(this.selected);
            this.selected_probe = None;
        });
    }

    fn step_probe(
        &mut self,
        row: usize,
        axis: Axis,
        direction: f64,
        modifiers: &Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = step_size(50., modifiers) * direction;
        self.with_current(window, cx, |f| {
            if let Some(p) = f.probes.get_mut(row) {
                p.set(axis, ((p.get(axis) + step) * 1000.).round() / 1000.);
            }
        });
    }

    fn apply_transform(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.transform_tool {
            TransformTool::Scale => {
                if let Some(factor) = Self::input_number(&self.scale_input, cx) {
                    self.with_current(window, cx, |f| f.scale(factor));
                }
            }
            TransformTool::Rotate => {
                if let Some(degrees) = Self::input_number(&self.rotate_input, cx) {
                    let axis = self.rotate_axis;
                    self.with_current(window, cx, |f| f.rotate(axis, degrees));
                }
            }
            TransformTool::Mirror => {
                let axis = self.mirror_axis;
                self.with_current(window, cx, |f| f.mirror(axis));
            }
            TransformTool::Move => {
                if let Some(distance) = parse_distance(&self.move_input.read(cx).value()) {
                    let Direction { axis, sign } = self.move_direction;
                    let mut delta = [0.; 3];
                    delta[match axis {
                        Axis::X => 0,
                        Axis::Y => 1,
                        Axis::Z => 2,
                    }] = distance * sign;
                    self.with_current(window, cx, |f| f.offset(delta));
                }
            }
        }
    }

    fn apply_pattern(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(spread) = parse_distance(&self.spread_input.read(cx).value()).filter(|s| *s > 0.)
        else {
            return;
        };
        let probes = self
            .pattern
            .probes(spread, MAX_PROBES, self.pattern_axis, self.pattern_range);
        self.with_current(window, cx, |f| f.probes = probes);
        self.selected_probe = None;
    }

    // Section header that folds its body away; trailing controls stay clickable.
    fn section_header(
        &self,
        section: Section,
        title: String,
        trailing: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let open = !self.collapsed.contains(&section);
        let hover = cx.theme().foreground;
        ui::section_header(cx)
            .id(SharedString::from(format!("section-{}", section as u8)))
            .justify_between()
            .cursor_pointer()
            .hover(move |s| s.text_color(hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.collapsed.remove(&section) {
                    this.collapsed.insert(section);
                }
                cx.notify();
            }))
            .child(
                h_flex()
                    .gap_1p5()
                    .child(
                        Icon::new(if open {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .xsmall(),
                    )
                    .child(ui::section_title(title)),
            )
            .children(trailing.map(|trailing| {
                div()
                    .id(SharedString::from(format!(
                        "section-trailing-{}",
                        section as u8
                    )))
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(trailing)
            }))
    }

    fn field_row(label: String, cx: &App) -> Div {
        h_flex().px_4().py_1().gap_2().child(
            div()
                .w(LABEL_WIDTH)
                .flex_none()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
    }

    fn unit(text: &'static str, cx: &App) -> Div {
        div()
            .w(px(18.))
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(text)
    }

    fn segmented(
        &self,
        id: &'static str,
        labels: Vec<String>,
        selected: usize,
        on_pick: impl Fn(&mut Self, usize, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let view = cx.entity().downgrade();
        ui::segmented(
            id,
            labels,
            Some(selected),
            move |index, window, cx| {
                view.update(cx, |this, cx| on_pick(this, index, window, cx))
                    .ok();
            },
            cx,
        )
    }

    fn range_menu(
        &self,
        trigger: Button,
        current: Option<f64>,
        on_pick: impl Fn(&mut Self, f64, &mut Window, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity().downgrade();
        let mut options: Vec<f64> = RANGE_OPTIONS.to_vec();
        // Keep unusual values from existing files selectable instead of hiding them
        if let Some(value) = current
            && !options.contains(&value)
        {
            options.push(value);
            options.sort_by(f64::total_cmp);
        }
        trigger
            .dropdown_menu(move |menu, _, _| {
                options
                    .iter()
                    .fold(menu.max_h(px(280.)).scrollable(true), |menu, value| {
                        let value = *value;
                        let on_pick = on_pick.clone();
                        menu.item(
                            PopupMenuItem::new(format!("{} AU", format_number(value)))
                                .checked(current == Some(value))
                                .on_click(editor_action(&view, move |this, window, cx| {
                                    on_pick(this, value, window, cx)
                                })),
                        )
                    })
            })
            .into_any_element()
    }

    pub(super) fn render_inspector(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let content = match self.current().cloned() {
            None => div()
                .px_4()
                .py_3()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(t!("formationEditor.noSelection").to_string())
                .into_any_element(),
            Some(formation) => v_flex()
                .child(self.render_formation_section(cx))
                .child(self.render_probes_section(&formation, cx))
                .child(self.render_transform_section(cx))
                .child(self.render_pattern_section(cx))
                .child(self.render_balance_section(&formation, cx))
                .into_any_element(),
        };

        v_flex()
            .id("inspector")
            .w(px(360.))
            .flex_none()
            .border_l_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll()
            .text_sm()
            .child(content)
    }

    fn render_formation_section(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let actions = h_flex()
            .gap_0p5()
            .child(
                Button::new("duplicate-formation")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Copy)
                    .tooltip(t!("formationEditor.duplicate").to_string())
                    .on_click(cx.listener(|this, _, window, cx| this.duplicate(window, cx))),
            )
            .child(
                Button::new("delete-formation")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Trash)
                    .text_color(cx.theme().danger)
                    .tooltip(t!("formationEditor.deleteFormation").to_string())
                    .on_click(cx.listener(|this, _, window, cx| this.delete_formation(window, cx))),
            )
            .into_any_element();

        v_flex()
            .child(self.section_header(
                Section::Formation,
                t!("formationEditor.formation").to_string(),
                Some(actions),
                cx,
            ))
            .when(!self.collapsed.contains(&Section::Formation), |this| {
                this.child(
                    div()
                        .px_4()
                        .py_2()
                        .child(Input::new(&self.name_input).small()),
                )
            })
    }

    fn render_probes_section(
        &self,
        formation: &Formation,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let muted = cx.theme().muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        let missing = MAX_PROBES.saturating_sub(formation.probes.len());

        let mut section = v_flex().child(self.section_header(
            Section::Probes,
            t!("formationEditor.probes").to_string(),
            None,
            cx,
        ));
        if self.collapsed.contains(&Section::Probes) {
            return section;
        }

        // The AU header sets every probe's range at once
        let all_ranges = self.range_menu(
            Button::new("all-ranges")
                .ghost()
                .xsmall()
                .w(RANGE_BUTTON_WIDTH)
                .label("AU ▾")
                .tooltip(t!("formationEditor.setAllRanges").to_string()),
            None,
            |this, value, window, cx| this.with_current(window, cx, |f| f.set_all_ranges(value)),
            cx,
        );
        let header = table::cells(
            h_flex()
                .px_4()
                .pt_1p5()
                .pb_1()
                .gap_1()
                .text_size(px(10.))
                .text_color(muted),
            &PROBE_COLUMNS,
            [table::empty()]
                .into_iter()
                .chain(AXES.iter().map(|info| {
                    // Inset to line up with the text inside the inputs
                    div()
                        .id(info.tooltip_key)
                        .pl_2()
                        .child(axis_label(info))
                        .tooltip(move |window, cx| {
                            Tooltip::new(t!(info.tooltip_key).to_string()).build(window, cx)
                        })
                        .into_any_element()
                }))
                .chain([all_ranges]),
        );

        let rows: Vec<AnyElement> = formation
            .probes
            .iter()
            .enumerate()
            .map(|(i, probe)| self.render_probe_row(i, probe, &mono, cx))
            .collect();

        section = section.child(header).children(rows);
        if missing > 0 {
            section = section.child(
                h_flex()
                    .mx_4()
                    .mt_2()
                    .px_3()
                    .py_2()
                    .gap_2()
                    .rounded(px(6.))
                    .bg(ui::amber(cx).opacity(0.1))
                    .text_xs()
                    .child(
                        div().flex_1().text_color(ui::amber(cx)).child(
                            t!(
                                "formationEditor.fewerProbes",
                                count = formation.probes.len()
                            )
                            .to_string(),
                        ),
                    )
                    .child(
                        Button::new("fill-probes")
                            .outline()
                            .xsmall()
                            .label(t!("formationEditor.fillProbes", count = missing).to_string())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.with_current(window, cx, Formation::fill_to_max)
                            })),
                    ),
            );
        }
        section.child(
            div()
                .px_4()
                .pt_2()
                .pb_3()
                .text_size(px(10.))
                .text_color(muted.opacity(0.8))
                .child(t!("formationEditor.inputHint").to_string()),
        )
    }

    fn render_probe_row(
        &self,
        i: usize,
        probe: &Probe,
        mono: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected_probe == Some(i);
        let hovered = self.hovered_probe == Some(i);
        let muted = cx.theme().muted_foreground;
        let accent = ui::source_color();

        let index = div()
            .id(("probe-index", i))
            .cursor_pointer()
            .text_size(px(10.))
            .font_family(mono.clone())
            .text_color(if selected { accent } else { muted })
            .child(format!("{:02}", i + 1))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_probe = if this.selected_probe == Some(i) {
                    None
                } else {
                    Some(i)
                };
                cx.notify();
            }))
            .into_any_element();

        let inputs: Vec<AnyElement> = AXES
            .iter()
            .zip(&self.probe_inputs[i])
            .map(|(info, input)| {
                let axis = info.axis;
                div()
                    .w_full()
                    .font_family(mono.clone())
                    // ↑/↓ nudge the value; Alt-drag scrubs it
                    .capture_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        let direction = match event.keystroke.key.as_str() {
                            "up" => 1.,
                            "down" => -1.,
                            _ => return,
                        };
                        cx.stop_propagation();
                        this.step_probe(i, axis, direction, &event.keystroke.modifiers, window, cx);
                    }))
                    .capture_any_mouse_down(cx.listener(
                        move |this, event: &MouseDownEvent, _, cx| {
                            if event.modifiers.alt {
                                cx.stop_propagation();
                                this.start_scrub(i, axis, event.position.x, cx);
                            }
                        },
                    ))
                    .child(Input::new(input).xsmall())
                    .into_any_element()
            })
            .collect();

        let range = self.range_menu(
            Button::new(("range", i))
                .outline()
                .xsmall()
                .w(RANGE_BUTTON_WIDTH)
                .label(format_number(probe.range)),
            Some(probe.range),
            move |this, value, window, cx| {
                this.with_current(window, cx, |f| {
                    if let Some(p) = f.probes.get_mut(i) {
                        p.range = value;
                    }
                })
            },
            cx,
        );

        let hover_bg = cx.theme().muted.opacity(0.6);
        table::cells(
            h_flex()
                .id(("probe-row", i))
                .relative()
                .px_4()
                .py(px(3.))
                .gap_1()
                .hover(move |s| s.bg(hover_bg))
                .when(selected, |this| {
                    this.bg(accent.opacity(0.1)).child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(3.))
                            .bg(accent),
                    )
                })
                .when(hovered && !selected, |this| this.bg(hover_bg))
                .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                    let next = if *hovering { Some(i) } else { None };
                    if this.hovered_probe != next && (*hovering || this.hovered_probe == Some(i)) {
                        this.hovered_probe = next;
                        cx.notify();
                    }
                })),
            &PROBE_COLUMNS,
            [index].into_iter().chain(inputs).chain([range]),
        )
        .into_any_element()
    }

    fn render_transform_section(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let section = v_flex().child(self.section_header(
            Section::Transform,
            t!("formationEditor.transform").to_string(),
            None,
            cx,
        ));
        if self.collapsed.contains(&Section::Transform) {
            return section;
        }

        let tool_index = TOOLS
            .iter()
            .position(|t| *t == self.transform_tool)
            .unwrap_or(0);
        let axis_labels: Vec<String> = AXES.iter().map(axis_label).collect();

        let fields = match self.transform_tool {
            TransformTool::Scale => vec![
                Self::field_row(t!("formationEditor.factor").to_string(), cx)
                    .child(div().flex_1().child(Input::new(&self.scale_input).xsmall()))
                    .child(Self::unit("×", cx)),
            ],
            TransformTool::Rotate => vec![
                Self::field_row(t!("formationEditor.axis").to_string(), cx).child(self.segmented(
                    "rotate-axis",
                    axis_labels,
                    axis_index(self.rotate_axis),
                    |this, index, _, cx| {
                        this.rotate_axis = AXES[index].axis;
                        cx.notify();
                    },
                    cx,
                )),
                Self::field_row(t!("formationEditor.degrees").to_string(), cx)
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.rotate_input).xsmall()),
                    )
                    .child(Self::unit("°", cx)),
            ],
            TransformTool::Mirror => vec![
                Self::field_row(t!("formationEditor.axis").to_string(), cx).child(self.segmented(
                    "mirror-axis",
                    axis_labels,
                    axis_index(self.mirror_axis),
                    |this, index, _, cx| {
                        this.mirror_axis = AXES[index].axis;
                        cx.notify();
                    },
                    cx,
                )),
            ],
            TransformTool::Move => {
                let directions = directions();
                let direction_index = directions
                    .iter()
                    .position(|(d, _)| *d == self.move_direction)
                    .unwrap_or(0);
                let labels = directions
                    .iter()
                    .map(|(_, label)| label.to_string())
                    .collect();
                vec![
                    Self::field_row(t!("formationEditor.direction").to_string(), cx).child(
                        self.segmented(
                            "move-direction",
                            labels,
                            direction_index,
                            move |this, index, _, cx| {
                                this.move_direction = directions[index].0;
                                cx.notify();
                            },
                            cx,
                        ),
                    ),
                    Self::field_row(t!("formationEditor.distance").to_string(), cx)
                        .child(div().flex_1().child(Input::new(&self.move_input).xsmall()))
                        .child(Self::unit("km", cx)),
                ]
            }
        };

        section
            .child(h_flex().px_4().pt_2().pb_1p5().child(self.segmented(
                "transform-tool",
                TOOLS.iter().map(|t| t.label()).collect(),
                tool_index,
                |this, index, _, cx| {
                    this.transform_tool = TOOLS[index];
                    cx.notify();
                },
                cx,
            )))
            .children(fields)
            .child(
                div().px_4().pt_1p5().pb_3().child(
                    Button::new("apply-transform")
                        .outline()
                        .small()
                        .w_full()
                        .label(t!("formationEditor.apply").to_string())
                        .on_click(
                            cx.listener(|this, _, window, cx| this.apply_transform(window, cx)),
                        ),
                ),
            )
    }

    fn render_pattern_section(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let section = v_flex().child(self.section_header(
            Section::Pattern,
            t!("formationEditor.pattern").to_string(),
            None,
            cx,
        ));
        if self.collapsed.contains(&Section::Pattern) {
            return section;
        }

        let pattern_labels: Vec<String> = PATTERNS
            .iter()
            .map(|p| t!(format!("formationEditor.patterns.{}", p.id())).to_string())
            .collect();
        let pattern_index = PATTERNS
            .iter()
            .position(|p| *p == self.pattern)
            .unwrap_or(0);
        let axis_labels: Vec<String> = AXES.iter().map(axis_label).collect();

        section
            .child(h_flex().px_4().pt_2().pb_1p5().child(self.segmented(
                "pattern-kind",
                pattern_labels,
                pattern_index,
                |this, index, _, cx| {
                    this.pattern = PATTERNS[index];
                    cx.notify();
                },
                cx,
            )))
            .child(
                Self::field_row(t!("formationEditor.spread").to_string(), cx)
                    .child(
                        div()
                            .flex_1()
                            .child(Input::new(&self.spread_input).xsmall()),
                    )
                    .child(Self::unit("km", cx)),
            )
            .child(
                Self::field_row(t!("formationEditor.axis").to_string(), cx).child(self.segmented(
                    "pattern-axis",
                    axis_labels,
                    axis_index(self.pattern_axis),
                    |this, index, _, cx| {
                        this.pattern_axis = AXES[index].axis;
                        cx.notify();
                    },
                    cx,
                )),
            )
            .child(
                Self::field_row(t!("formationEditor.range").to_string(), cx)
                    .child(
                        div().flex_1().child(
                            self.range_menu(
                                Button::new("pattern-range")
                                    .outline()
                                    .xsmall()
                                    .w(px(96.))
                                    .label(format!("{} AU", format_number(self.pattern_range))),
                                Some(self.pattern_range),
                                |this, value, _, cx| {
                                    this.pattern_range = value;
                                    cx.notify();
                                },
                                cx,
                            ),
                        ),
                    )
                    .child(Self::unit("", cx)),
            )
            .child(
                v_flex()
                    .px_4()
                    .pt_1p5()
                    .pb_3()
                    .gap_2()
                    .child(
                        Button::new("apply-pattern")
                            .outline()
                            .small()
                            .w_full()
                            .label(t!("formationEditor.applyPattern").to_string())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.apply_pattern(window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(cx.theme().muted_foreground.opacity(0.8))
                            .child(t!("formationEditor.patternHint").to_string()),
                    ),
            )
    }

    fn render_balance_section(
        &self,
        formation: &Formation,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let info = div()
            .id("balance-info")
            .child(Icon::new(IconName::Info).xsmall())
            .tooltip(|window, cx| {
                Tooltip::new(t!("formationEditor.balanceInfo").to_string()).build(window, cx)
            })
            .into_any_element();
        let section = v_flex().child(self.section_header(
            Section::Balance,
            t!("formationEditor.balance").to_string(),
            Some(info),
            cx,
        ));
        if self.collapsed.contains(&Section::Balance) {
            return section;
        }

        let balanced = formation.is_balanced();
        let can_balance = !formation.probes.is_empty() && !balanced;
        let (dot, color, text) = if balanced {
            (
                ui::emerald(),
                cx.theme().muted_foreground,
                t!("formationEditor.balanced").to_string(),
            )
        } else {
            let amber = ui::amber(cx);
            (
                amber,
                amber,
                t!(
                    "formationEditor.offCenter",
                    offset = ui::format_distance(formation.launch_shift())
                )
                .to_string(),
            )
        };

        section.child(
            h_flex()
                .px_4()
                .py_2p5()
                .gap_2()
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1p5()
                        .text_xs()
                        .text_color(color)
                        .child(div().size_1p5().flex_none().rounded_full().bg(dot))
                        .child(div().truncate().child(text)),
                )
                .child(
                    Button::new("balance")
                        .outline()
                        .xsmall()
                        .icon(IconName::Scale)
                        .label(t!("formationEditor.balance").to_string())
                        .disabled(!can_balance)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.with_current(window, cx, Formation::balance)
                        })),
                ),
        )
    }
}
