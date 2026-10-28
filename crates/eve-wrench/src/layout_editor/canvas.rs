use eve_wrench_core::window_layout::{NeocomSide, WindowRect, WindowState};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, h_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use rust_i18n::t;

use super::model::{self, Desktop, Edges, Screen, SnapLines, Snapshot};
use super::{LayoutEditor, Location};
use crate::ui;

// Room around the screen inside the canvas
const PADDING: f32 = 32.;
// Snap distance on the canvas, so it feels the same at any zoom
const SNAP_DISTANCE: f32 = 8.;
// Grab area of the resize handles on the selected window
const EDGE: f32 = 6.;
const CORNER: f32 = 10.;
const TITLE_HEIGHT: f32 = 18.;

// A window being moved or resized. Keeps the state from before it started
// and records it for undo on release, if anything changed.
pub(super) struct Drag {
    key: String,
    edges: Edges,
    origin: Point<Pixels>,
    start: WindowRect,
    before: Snapshot,
}

// Zoom and pan on top of fitting the screen into the canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct View {
    // 1 shows the whole screen
    zoom: f32,
    // Shift from the centered position, in canvas pixels
    pan: Point<Pixels>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            zoom: 1.,
            pan: point(px(0.), px(0.)),
        }
    }
}

const MIN_ZOOM: f32 = 0.5;
const MAX_ZOOM: f32 = 8.;
// Zoom change per toolbar press
const ZOOM_STEP: f32 = 1.25;

// Panning the view by dragging empty canvas. A press without movement is a
// click, which clears the selection.
pub(super) struct Pan {
    last: Point<Pixels>,
    moved: bool,
}

// Screen pixels to canvas pixels: the screen is scaled to fit and centered,
// then zoomed and panned.
#[derive(Clone, Copy)]
struct Mapping {
    scale: f32,
    // Top left of the screen, relative to the canvas
    offset: Point<Pixels>,
}

impl Mapping {
    fn new(bounds: Bounds<Pixels>, screen: Screen, view: View) -> Self {
        let scale = Self::fit(bounds, screen) * view.zoom;
        Self {
            scale,
            offset: Self::centered(bounds, screen, scale) + view.pan,
        }
    }

    fn fit(bounds: Bounds<Pixels>, screen: Screen) -> f32 {
        let available_w = (f32::from(bounds.size.width) - 2. * PADDING).max(1.);
        let available_h = (f32::from(bounds.size.height) - 2. * PADDING).max(1.);
        (available_w / screen.width as f32).min(available_h / screen.height as f32)
    }

    fn centered(bounds: Bounds<Pixels>, screen: Screen, scale: f32) -> Point<Pixels> {
        point(
            px((f32::from(bounds.size.width) - screen.width as f32 * scale) / 2.),
            px((f32::from(bounds.size.height) - screen.height as f32 * scale) / 2.),
        )
    }

    fn rect(self, r: &WindowRect) -> Bounds<Pixels> {
        Bounds::new(
            point(
                self.offset.x + px(r.x as f32 * self.scale),
                self.offset.y + px(r.y as f32 * self.scale),
            ),
            size(
                px(r.width as f32 * self.scale),
                px(r.height as f32 * self.scale),
            ),
        )
    }

    fn screen_delta(self, delta: Point<Pixels>) -> (i64, i64) {
        (
            (f32::from(delta.x) / self.scale).round() as i64,
            (f32::from(delta.y) / self.scale).round() as i64,
        )
    }
}

impl LayoutEditor {
    fn mapping(&self) -> Mapping {
        Mapping::new(
            self.canvas_bounds.get(),
            model::screen(&self.windows),
            self.view,
        )
    }

    // Zooms by `factor`, keeping the point under `anchor` (window
    // coordinates) in place.
    fn zoom_at(&mut self, factor: f32, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        let bounds = self.canvas_bounds.get();
        let screen = model::screen(&self.windows);
        let before = self.mapping();
        let zoom = (self.view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let local = anchor - bounds.origin;
        // Screen position under the anchor stays under it
        let at = point(
            (f32::from(local.x - before.offset.x)) / before.scale,
            (f32::from(local.y - before.offset.y)) / before.scale,
        );
        let scale = Mapping::fit(bounds, screen) * zoom;
        let offset = point(local.x - px(at.x * scale), local.y - px(at.y * scale));
        self.view = View {
            zoom,
            pan: offset - Mapping::centered(bounds, screen, scale),
        };
        cx.notify();
    }

    fn zoom_centered(&mut self, factor: f32, cx: &mut Context<Self>) {
        let center = self.canvas_bounds.get().center();
        self.zoom_at(factor, center, cx);
    }

    fn fit_view(&mut self, cx: &mut Context<Self>) {
        self.view = View::default();
        cx.notify();
    }

    pub(super) fn desktop(&self) -> Desktop {
        Desktop {
            screen: model::screen(&self.windows),
            neocom: self.neocom,
        }
    }

    // Station-only windows are left out in space, inflight ones when docked.
    pub(super) fn in_location(&self, window: &WindowState) -> bool {
        match self.location {
            Location::InSpace => !model::docked_only(window),
            Location::Docked => !model::space_only(window),
        }
    }

    fn start_drag(
        &mut self,
        key: String,
        edges: Edges,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.select(Some(key.clone()), window, cx);
        let Some(start) = self.selected_window().and_then(|w| w.rect) else {
            return;
        };
        self.drag = Some(Drag {
            key,
            edges,
            origin: position,
            start,
            before: self.snapshot(),
        });
        cx.notify();
    }

    pub(super) fn on_drag_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pan) = &mut self.pan {
            let delta = event.position - pan.last;
            pan.last = event.position;
            pan.moved |= delta.x.abs() + delta.y.abs() > px(0.);
            self.view.pan += delta;
            cx.notify();
            return;
        }
        let Some(drag) = &self.drag else {
            return;
        };
        let mapping = self.mapping();
        let desktop = self.desktop();
        let delta = mapping.screen_delta(event.position - drag.origin);
        let lines = self.snap.then(|| {
            SnapLines::new(
                desktop,
                self.visible()
                    .filter(|w| w.key != drag.key)
                    .filter_map(|w| w.rect),
            )
        });
        let threshold = (SNAP_DISTANCE / mapping.scale).round() as i64;
        let rect = model::drag_rect(
            drag.start,
            drag.edges,
            delta,
            desktop,
            lines.as_ref().map(|lines| (lines, threshold)),
        );
        let key = drag.key.clone();
        model::set_rect(&mut self.windows, &key, rect);
        self.sync_inputs(window, cx);
        cx.notify();
    }

    pub(super) fn on_drag_end(
        &mut self,
        _: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pan) = self.pan.take()
            && !pan.moved
        {
            self.select(None, window, cx);
        }
        if let Some(drag) = self.drag.take()
            && drag.before.windows != self.windows
        {
            self.history.record(drag.before, None);
        }
        cx.notify();
    }

    // Open windows on the canvas, plus closed ones when they're shown.
    pub(super) fn visible(&self) -> impl Iterator<Item = &WindowState> {
        self.windows
            .iter()
            .filter(|w| model::on_canvas(w, &self.windows) && (w.open || self.show_closed))
            .filter(|w| self.in_location(w))
    }

    fn render_window(
        &self,
        state: &WindowState,
        mapping: Mapping,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = cx.theme();
        let accent = ui::target_color();
        let selected = self.selected.as_deref() == Some(state.key.as_str());
        let hovered = self.hovered.as_deref() == Some(state.key.as_str());
        let bounds = mapping.rect(&state.rect.expect("windows on the canvas have a rect"));
        let name = model::display_name(state, &self.windows);
        let dims = state
            .rect
            .map(|r| format!("{} × {}", r.width, r.height))
            .unwrap_or_default();
        let border = if selected {
            accent
        } else if hovered {
            theme.foreground.opacity(0.5)
        } else {
            theme.border
        };
        let key = state.key.clone();
        let hover_key = state.key.clone();
        let compact = state.compact.unwrap_or(self.compact_default);

        div()
            .id(SharedString::from(format!("window-{}", state.key)))
            .absolute()
            .left(bounds.origin.x)
            .top(bounds.origin.y)
            .w(bounds.size.width)
            .h(bounds.size.height)
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(3.))
            .border_1()
            .border_color(border)
            .bg(theme.background.opacity(0.85))
            .when(selected, |this| this.bg(accent.opacity(0.08)))
            .when(!state.open || state.minimized, |this| this.opacity(0.45))
            .cursor_move()
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                let key = Some(hover_key.clone());
                if *hovering {
                    this.hovered = key;
                } else if this.hovered == key {
                    this.hovered = None;
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.start_drag(key.clone(), Edges::MOVE, event.position, window, cx);
                }),
            )
            .child(
                h_flex()
                    .flex_none()
                    .h(px(TITLE_HEIGHT))
                    .gap_1()
                    .px_1p5()
                    // Compact windows have no header bar in game
                    .bg(if compact {
                        transparent_black()
                    } else if selected {
                        accent.opacity(0.18)
                    } else {
                        theme.muted.opacity(0.9)
                    })
                    .text_size(px(10.))
                    .text_color(if selected {
                        theme.foreground
                    } else {
                        theme.muted_foreground
                    })
                    .child(Icon::new(model::icon(state)).size(px(10.)))
                    .when(state.locked, |this| {
                        this.child(Icon::new(IconName::Lock).size(px(9.)))
                    })
                    .child(div().min_w_0().truncate().child(name)),
            )
            .when(selected || hovered, |this| {
                this.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(10.))
                        .text_color(theme.muted_foreground)
                        .child(dims),
                )
            })
            .when(selected, |this| {
                this.children(self.resize_handles(&state.key, bounds.size, cx))
            })
    }

    // Grab strips along the edges and squares at the corners.
    fn resize_handles(
        &self,
        key: &str,
        extent: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let (w, h) = (extent.width, extent.height);
        let edge = |left, right, top, bottom| Edges {
            left,
            right,
            top,
            bottom,
        };
        // Edges, position and size of each handle
        let handles: [(Edges, Point<Pixels>, Size<Pixels>); 8] = [
            (
                edge(true, false, false, false),
                point(px(0.), px(CORNER)),
                size(px(EDGE), h - px(2. * CORNER)),
            ),
            (
                edge(false, true, false, false),
                point(w - px(EDGE), px(CORNER)),
                size(px(EDGE), h - px(2. * CORNER)),
            ),
            (
                edge(false, false, true, false),
                point(px(CORNER), px(0.)),
                size(w - px(2. * CORNER), px(EDGE)),
            ),
            (
                edge(false, false, false, true),
                point(px(CORNER), h - px(EDGE)),
                size(w - px(2. * CORNER), px(EDGE)),
            ),
            (
                edge(true, false, true, false),
                point(px(0.), px(0.)),
                size(px(CORNER), px(CORNER)),
            ),
            (
                edge(false, true, true, false),
                point(w - px(CORNER), px(0.)),
                size(px(CORNER), px(CORNER)),
            ),
            (
                edge(true, false, false, true),
                point(px(0.), h - px(CORNER)),
                size(px(CORNER), px(CORNER)),
            ),
            (
                edge(false, true, false, true),
                point(w - px(CORNER), h - px(CORNER)),
                size(px(CORNER), px(CORNER)),
            ),
        ];

        handles
            .into_iter()
            .enumerate()
            .map(|(i, (edges, origin, handle))| {
                let key = key.to_string();
                let corner = (edges.left || edges.right) && (edges.top || edges.bottom);
                div()
                    .id(("resize", i))
                    .absolute()
                    .left(origin.x)
                    .top(origin.y)
                    .w(handle.width)
                    .h(handle.height)
                    .map(|this| {
                        if corner && (edges.left == edges.top) {
                            this.cursor_nwse_resize()
                        } else if corner {
                            this.cursor_nesw_resize()
                        } else if edges.left || edges.right {
                            this.cursor_ew_resize()
                        } else {
                            this.cursor_ns_resize()
                        }
                    })
                    .when(corner, |this| {
                        this.child(
                            div()
                                .absolute()
                                .left(px(CORNER / 2. - 3.))
                                .top(px(CORNER / 2. - 3.))
                                .size(px(6.))
                                .rounded(px(1.))
                                .bg(ui::target_color()),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.start_drag(key.clone(), edges, event.position, window, cx);
                        }),
                    )
            })
            .collect()
    }

    fn render_toolbar(&self, screen: Screen, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let (chrome_bg, chrome_border) = (theme.background.opacity(0.8), theme.border);
        let (mono, muted) = (theme.mono_font_family.clone(), theme.muted_foreground);
        let zoom = self.view.zoom;
        let view_is_fit = self.view == View::default();
        h_flex()
            // Clicks on the toolbar don't pan or deselect
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .flex_wrap()
            .justify_center()
            .gap_1()
            .p_0p5()
            .pl_2()
            .rounded(px(8.))
            .border_1()
            .border_color(chrome_border)
            .bg(chrome_bg)
            .child(
                div()
                    .text_xs()
                    .font_family(theme.mono_font_family.clone())
                    .text_color(theme.muted_foreground)
                    .child(format!("{} × {}", screen.width, screen.height)),
            )
            .child(div().w(px(1.)).h_4().mx_1().bg(chrome_border))
            .child(
                ui::segmented(
                    "location",
                    [
                        t!("layoutEditor.inSpace").to_string(),
                        t!("layoutEditor.docked").to_string(),
                    ],
                    Some(match self.location {
                        Location::InSpace => 0,
                        Location::Docked => 1,
                    }),
                    {
                        let editor = cx.entity().downgrade();
                        move |i, _, cx| {
                            editor
                                .update(cx, |this, cx| {
                                    this.location = if i == 0 {
                                        Location::InSpace
                                    } else {
                                        Location::Docked
                                    };
                                    cx.notify();
                                })
                                .ok();
                        }
                    },
                    cx,
                )
                .flex_none()
                .w(px(150.)),
            )
            .child(div().w(px(1.)).h_4().mx_1().bg(chrome_border))
            .child(
                Button::new("snap")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Magnet)
                    .tooltip(t!("layoutEditor.snap").to_string())
                    .selected(self.snap)
                    .toggled(self.snap)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.snap = !this.snap;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("show-closed")
                    .ghost()
                    .xsmall()
                    .icon(if self.show_closed {
                        IconName::Eye
                    } else {
                        IconName::EyeOff
                    })
                    .tooltip(t!("layoutEditor.showClosed").to_string())
                    .selected(self.show_closed)
                    .toggled(self.show_closed)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_closed = !this.show_closed;
                        cx.notify();
                    })),
            )
            .child(div().w(px(1.)).h_4().mx_1().bg(chrome_border))
            .child(
                Button::new("zoom-out")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Minus)
                    .tooltip(t!("layoutEditor.zoomOut").to_string())
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_centered(1. / ZOOM_STEP, cx))),
            )
            .child(
                div()
                    .w(px(40.))
                    .text_center()
                    .text_xs()
                    .font_family(mono)
                    .text_color(muted)
                    .child(format!("{:.0}%", zoom * 100.)),
            )
            .child(
                Button::new("zoom-in")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip(t!("layoutEditor.zoomIn").to_string())
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_centered(ZOOM_STEP, cx))),
            )
            .child(
                Button::new("zoom-fit")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Maximize)
                    .label(t!("layoutEditor.fit").to_string())
                    .disabled(view_is_fit)
                    .on_click(cx.listener(|this, _, _, cx| this.fit_view(cx))),
            )
    }

    pub(super) fn render_canvas(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let screen = model::screen(&self.windows);
        let mapping = self.mapping();
        let theme = cx.theme();
        let (border, screen_bg, hint) = (
            theme.border,
            theme.muted.opacity(0.25),
            theme.muted_foreground,
        );
        let (top, bottom) = if theme.is_dark() {
            (hsla(0., 0., 0.098, 1.), hsla(0., 0., 0.024, 1.))
        } else {
            (hsla(0., 0., 0.94, 1.), hsla(0., 0., 0.83, 1.))
        };
        let full = |x: i64, width: i64| WindowRect {
            x,
            y: 0,
            width,
            height: screen.height,
            screen_width: screen.width,
            screen_height: screen.height,
        };
        let screen_bounds = mapping.rect(&full(0, screen.width));
        let neocom_x = match self.neocom.side {
            NeocomSide::Left => 0,
            NeocomSide::Right => screen.width - self.neocom.width,
        };
        let neocom_bounds = mapping.rect(&full(neocom_x, self.neocom.width));
        let neocom_bg = theme.muted.opacity(0.9);

        // Closed windows underneath, the selected one on top
        let mut windows: Vec<&WindowState> = self.visible().collect();
        windows.sort_by_key(|w| (w.open, self.selected.as_deref() == Some(w.key.as_str())));
        let elements: Vec<Stateful<Div>> = windows
            .into_iter()
            .map(|w| self.render_window(w, mapping, cx))
            .collect();

        let bounds_cell = self.canvas_bounds.clone();
        div()
            .id("layout-canvas")
            .relative()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .bg(linear_gradient(
                180.,
                linear_color_stop(top, 0.),
                linear_color_stop(bottom, 1.),
            ))
            .when(self.pan.is_some(), |this| this.cursor_grabbing())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus_handle.focus(window, cx);
                    if event.click_count == 2 {
                        this.fit_view(cx);
                        return;
                    }
                    this.pan = Some(Pan {
                        last: event.position,
                        moved: false,
                    });
                }),
            )
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                let dy = f32::from(event.delta.pixel_delta(window.line_height()).y);
                this.zoom_at((dy * 0.004).exp(), event.position, cx);
            }))
            // Measures the canvas; the windows are placed from the last
            // frame's size, so a change redraws once more
            .child(
                canvas(
                    move |bounds, window, _| {
                        if bounds_cell.get() != bounds {
                            bounds_cell.set(bounds);
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .child(
                div()
                    .absolute()
                    .left(screen_bounds.origin.x)
                    .top(screen_bounds.origin.y)
                    .w(screen_bounds.size.width)
                    .h(screen_bounds.size.height)
                    .rounded(px(4.))
                    .border_1()
                    .border_color(border)
                    .bg(screen_bg),
            )
            .child(
                div()
                    .absolute()
                    .left(neocom_bounds.origin.x)
                    .top(neocom_bounds.origin.y)
                    .w(neocom_bounds.size.width)
                    .h(neocom_bounds.size.height)
                    .border_1()
                    .border_color(border)
                    .bg(neocom_bg),
            )
            .children(elements)
            .child(
                h_flex()
                    .absolute()
                    .top_2()
                    .left_2()
                    .right_2()
                    .justify_center()
                    .child(self.render_toolbar(screen, cx)),
            )
            .child(
                h_flex()
                    .absolute()
                    .bottom_1()
                    .left_4()
                    .right_4()
                    .justify_center()
                    .text_size(px(11.))
                    .text_color(hint)
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .child(t!("layoutEditor.canvasHint").to_string()),
                    ),
            )
    }
}
