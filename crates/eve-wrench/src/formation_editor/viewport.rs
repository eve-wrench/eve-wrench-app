use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::FormationEditor;
use super::model::{Axis, Snapshot, format_number};
use super::scanner::{self, Camera, ScannerProps, Scene, View};
use crate::ui;

const VIEWS: [View; 3] = [View::Top, View::Front, View::Side];

// What the pointer is doing while the button is held.
// What the pointer is doing while the button is held. A scrub keeps the
// state from before it started and records it for undo on release, if the
// value actually changed.
pub(super) enum Drag {
    Orbit {
        last: Point<Pixels>,
    },
    // Alt-drag on a coordinate field
    Scrub {
        row: usize,
        axis: Axis,
        origin: Pixels,
        start: f64,
        before: Snapshot,
    },
}

// Distance per arrow press or scrubbed pixel, with the modifier multipliers
// the fields advertise.
pub(super) fn step_size(base: f64, modifiers: &Modifiers) -> f64 {
    if modifiers.shift {
        base * 10.
    } else if modifiers.alt {
        base * 0.1
    } else {
        base
    }
}

impl FormationEditor {
    fn scene(&self) -> Scene {
        Scene::new(self.scanner_bounds.get(), self.camera, self.current())
    }

    fn on_viewport_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        let hit = self
            .current()
            .and_then(|f| self.scene().hit_probe(f, event.position));

        match hit {
            Some(index) => {
                self.selected_probe = if self.selected_probe == Some(index) {
                    None
                } else {
                    Some(index)
                };
            }
            None if event.click_count == 2 => self.animate_camera(Camera::default(), window, cx),
            None => {
                self.drag = Some(Drag::Orbit {
                    last: event.position,
                });
            }
        }
        cx.notify();
    }

    fn on_viewport_hover(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_some() {
            return;
        }
        let hovered = self
            .current()
            .and_then(|f| self.scene().hit_probe(f, event.position));
        if hovered != self.hovered_probe {
            self.hovered_probe = hovered;
            cx.notify();
        }
    }

    pub(super) fn on_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        match drag {
            Drag::Orbit { last } => {
                let delta = event.position - *last;
                *last = event.position;
                self.camera
                    .orbit(f64::from(f32::from(delta.x)), f64::from(f32::from(delta.y)));
            }
            Drag::Scrub {
                row,
                axis,
                origin,
                start,
                ..
            } => {
                let (row, axis) = (*row, *axis);
                let dx = f64::from(f32::from(event.position.x - *origin));
                let value = (*start + dx * step_size(10., &event.modifiers)).round();
                if let Some(p) = self.current_mut().and_then(|f| f.probes.get_mut(row)) {
                    p.set(axis, value);
                }
                self.sync_probe_row(row, window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn on_drag_end(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        let before = match self.drag.take() {
            Some(Drag::Scrub { before, .. }) => Some(before),
            _ => None,
        };
        if let Some(before) = before
            && before.formations != self.formations
        {
            self.history.record(before, None);
        }
        cx.notify();
    }

    pub(super) fn start_scrub(
        &mut self,
        row: usize,
        axis: Axis,
        origin: Pixels,
        cx: &mut Context<Self>,
    ) {
        let Some(start) = self
            .current()
            .and_then(|f| f.probes.get(row))
            .map(|p| p.get(axis))
        else {
            return;
        };
        self.selected_probe = Some(row);
        self.drag = Some(Drag::Scrub {
            row,
            axis,
            origin,
            start,
            before: self.snapshot(),
        });
        cx.notify();
    }

    pub(super) fn animate_camera(
        &mut self,
        target: Camera,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from = self.camera;
        self._camera_animation = Some(cx.spawn_in(window, async move |this, cx| {
            const FRAMES: u32 = 14;
            for frame in 1..=FRAMES {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let t = f64::from(frame) / f64::from(FRAMES);
                let eased = 1. - (1. - t).powi(3);
                let alive = this.update(cx, |this, cx| {
                    this.camera = from.lerp(target, eased);
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
        }));
    }

    pub(super) fn render_viewport(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let current = self.current().cloned();
        let dark = cx.theme().is_dark();
        let (top, bottom) = if dark {
            (hsla(0., 0., 0.098, 1.), hsla(0., 0., 0.024, 1.))
        } else {
            (hsla(0., 0., 0.94, 1.), hsla(0., 0., 0.83, 1.))
        };
        let hud = if dark {
            white().opacity(0.7)
        } else {
            hsla(0., 0., 0.25, 0.8)
        };
        let hud_strong = if dark {
            white()
        } else {
            hsla(0., 0., 0.09, 1.)
        };
        let chrome_bg = if dark {
            black().opacity(0.45)
        } else {
            white().opacity(0.7)
        };
        let chrome_border = if dark {
            white().opacity(0.12)
        } else {
            hsla(0., 0., 0.6, 0.4)
        };
        let dragging = self.drag.is_some();
        let over_probe = self.hovered_probe.is_some();

        let toolbar = h_flex()
            .gap_0p5()
            .p_0p5()
            .rounded(px(8.))
            .border_1()
            .border_color(chrome_border)
            .bg(chrome_bg)
            .child(
                ui::segmented(
                    "views",
                    [
                        t!("formationEditor.viewTop").to_string(),
                        t!("formationEditor.viewFront").to_string(),
                        t!("formationEditor.viewSide").to_string(),
                    ],
                    VIEWS
                        .iter()
                        .position(|view| self.camera == view.camera(self.camera.zoom)),
                    {
                        let editor = cx.entity().downgrade();
                        move |index, window, cx| {
                            editor
                                .update(cx, |this, cx| {
                                    let target = VIEWS[index].camera(this.camera.zoom);
                                    this.animate_camera(target, window, cx)
                                })
                                .ok();
                        }
                    },
                    cx,
                )
                .w(px(180.))
                .flex_none(),
            )
            .child(div().w(px(1.)).h_4().mx_0p5().bg(chrome_border))
            .child(
                Button::new("view-reset")
                    .ghost()
                    .xsmall()
                    .icon(IconName::RotateCcw)
                    .tooltip(t!("formationEditor.resetView").to_string())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.animate_camera(Camera::default(), window, cx)
                    })),
            );

        let probe_count = current.as_ref().map_or(0, |f| f.probes.len());
        div()
            .id("scanner")
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .bg(linear_gradient(
                180.,
                linear_color_stop(top, 0.),
                linear_color_stop(bottom, 1.),
            ))
            .map(|this| {
                if dragging {
                    this.cursor_grabbing()
                } else if over_probe {
                    this.cursor_pointer()
                } else {
                    this.cursor_grab()
                }
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_viewport_down))
            .on_mouse_move(cx.listener(Self::on_viewport_hover))
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                let dy = f64::from(f32::from(event.delta.pixel_delta(window.line_height()).y));
                this.camera.zoom_by((dy * 0.004).exp());
                cx.notify();
            }))
            .child(scanner::scanner(ScannerProps {
                formation: current.clone(),
                camera: self.camera,
                selected: self.selected_probe,
                hovered: self.hovered_probe,
                launch_label: t!("formationEditor.launchCenter").to_string().into(),
                bounds: self.scanner_bounds.clone(),
            }))
            .child(
                h_flex()
                    .absolute()
                    .top_3()
                    .left_0()
                    .right_0()
                    .justify_center()
                    .child(toolbar),
            )
            .child(
                v_flex()
                    .absolute()
                    .top_3()
                    .left_4()
                    .gap_0p5()
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_size(px(10.))
                    .text_color(hud)
                    .child(
                        div()
                            .text_sm()
                            .text_color(hud_strong)
                            .child(current.as_ref().map(|f| f.name.clone()).unwrap_or_default()),
                    )
                    .child(
                        t!("formationEditor.probesCount", count = probe_count)
                            .to_string()
                            .to_uppercase(),
                    )
                    .child(format!(
                        "{} {}%",
                        t!("formationEditor.zoom").to_uppercase(),
                        format_number((self.camera.zoom * 100.).round())
                    )),
            )
            .child(
                h_flex()
                    .absolute()
                    .bottom_3()
                    .left_4()
                    .right_4()
                    .justify_center()
                    .child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded_full()
                            .border_1()
                            .border_color(chrome_border)
                            .bg(chrome_bg)
                            .text_size(px(11.))
                            .text_color(hud)
                            .child(t!("formationEditor.viewportHint").to_string()),
                    ),
            )
    }
}
