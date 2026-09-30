use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::model::{
    FORMATION_PRESETS, Formation, MAX_PROBES, Preset, PresetIcon, STACK_PRESETS, index_after_move,
};
use super::{FormationEditor, editor_action};
use crate::ui;

fn preset_icon(icon: PresetIcon) -> IconName {
    match icon {
        PresetIcon::Grid => IconName::Grid2x2,
        PresetIcon::Crosshair => IconName::Crosshair,
        PresetIcon::Skull => IconName::Skull,
        PresetIcon::North => IconName::ArrowUp,
        PresetIcon::South => IconName::ArrowDown,
        PresetIcon::West => IconName::ArrowLeft,
        PresetIcon::East => IconName::ArrowRight,
        PresetIcon::Up => IconName::ChevronsUp,
        PresetIcon::Down => IconName::ChevronsDown,
    }
}

fn preset_label(preset: &Preset) -> String {
    t!(format!("formationEditor.presets.{}", preset.id)).to_string()
}

// Payload while a formation row is dragged to a new position.
#[derive(Clone)]
struct DraggedFormation {
    index: usize,
    name: SharedString,
}

struct FormationDragPreview {
    dragged: DraggedFormation,
    // GPUI anchors the preview where the drag started inside the row
    grab: Point<Pixels>,
}

impl Render for FormationDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.grab.x + px(8.))
            .pt(self.grab.y + px(8.))
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded(px(6.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().popover)
                    .shadow_md()
                    .text_sm()
                    .text_color(cx.theme().popover_foreground)
                    .child(self.dragged.name.clone()),
            )
    }
}

impl FormationEditor {
    fn add_preset(&mut self, preset: &Preset, window: &mut Window, cx: &mut Context<Self>) {
        let name = if preset.id == "blank" {
            format!("Formation {}", self.formations.len() + 1)
        } else {
            preset_label(preset)
        };
        let probes = preset.probes();
        self.edit(window, cx, |this| {
            this.formations.push(Formation { name, probes });
            this.selected = this.formations.len() - 1;
            this.selected_probe = None;
        });
    }

    // Moves a formation to another position, keeping the same one selected.
    fn reorder(&mut self, from: usize, to: usize, window: &mut Window, cx: &mut Context<Self>) {
        if from == to || from >= self.formations.len() || to >= self.formations.len() {
            return;
        }
        self.edit(window, cx, |this| {
            let formation = this.formations.remove(from);
            this.formations.insert(to, formation);
            this.selected = index_after_move(this.selected, from, to);
        });
    }

    pub(super) fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        let count = self.formations.len();

        let add_menu = Button::new("add-formation")
            .ghost()
            .xsmall()
            .icon(IconName::Plus)
            .tooltip(t!("formationEditor.addFormation").to_string())
            .dropdown_menu(move |menu, window, cx| {
                let menu = FORMATION_PRESETS
                    .iter()
                    .enumerate()
                    .fold(menu, |menu, (i, preset)| {
                        menu.item(
                            PopupMenuItem::new(preset_label(preset))
                                .icon(preset_icon(preset.icon))
                                .on_click(editor_action(&view, move |this, window, cx| {
                                    this.add_preset(&FORMATION_PRESETS[i], window, cx)
                                })),
                        )
                    });
                let view = view.clone();
                menu.submenu_with_icon(
                    Some(Icon::new(IconName::Compass)),
                    t!("formationEditor.presetDirectional").to_string(),
                    window,
                    cx,
                    move |menu, _, _| {
                        STACK_PRESETS
                            .iter()
                            .enumerate()
                            .fold(menu, |menu, (i, preset)| {
                                menu.item(
                                    PopupMenuItem::new(preset_label(preset))
                                        .icon(preset_icon(preset.icon))
                                        .on_click(editor_action(&view, move |this, window, cx| {
                                            this.add_preset(&STACK_PRESETS[i], window, cx)
                                        })),
                                )
                            })
                    },
                )
            });

        let muted = cx.theme().muted_foreground;
        let mono = cx.theme().mono_font_family.clone();
        let accent = ui::source_color();
        let rows: Vec<AnyElement> = self
            .formations
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let selected = i == self.selected;
                let dragged = DraggedFormation {
                    index: i,
                    name: f.name.clone().into(),
                };
                ui::list_row(("formation", i), selected.then(ui::source_color), cx)
                    .h(px(40.))
                    .gap_1()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| this.select(i, window, cx)))
                    .on_drag(dragged, |dragged, grab, _, cx| {
                        let dragged = dragged.clone();
                        cx.new(|_| FormationDragPreview { dragged, grab })
                    })
                    .drag_over::<DraggedFormation>(move |style, _, _, _| {
                        style.bg(accent.opacity(0.18))
                    })
                    .on_drop(
                        cx.listener(move |this, dragged: &DraggedFormation, window, cx| {
                            this.reorder(dragged.index, i, window, cx)
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .when(!selected, |this| this.text_color(muted))
                            .child(f.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(28.))
                            .text_right()
                            .text_size(px(10.))
                            .font_family(mono.clone())
                            .text_color(muted)
                            .child(format!("{}/{}", f.probes.len(), MAX_PROBES)),
                    )
                    .into_any_element()
            })
            .collect();

        v_flex()
            .w(px(232.))
            .flex_none()
            .border_r_1()
            .border_color(cx.theme().border)
            .text_sm()
            .child(
                ui::section_header(cx)
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(ui::section_title(
                                t!("formationEditor.formations").to_string(),
                            ))
                            .child(ui::count_label(count, cx)),
                    )
                    .child(add_menu),
            )
            .child(
                v_flex()
                    .id("formation-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .when(self.formations.is_empty(), |this| {
                        this.child(
                            div()
                                .px_4()
                                .py_3()
                                .text_xs()
                                .text_color(muted)
                                .child(t!("formationEditor.noFormations").to_string()),
                        )
                    })
                    .children(rows),
            )
    }
}
