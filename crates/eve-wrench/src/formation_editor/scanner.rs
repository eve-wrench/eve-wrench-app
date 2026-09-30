use std::cell::Cell;
use std::f64::consts::{FRAC_1_SQRT_2, FRAC_PI_2, PI, TAU};
use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

use super::model::{AXES, Axis, Formation};
use crate::ui;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub yaw: f64,
    pub pitch: f64,
    pub zoom: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            yaw: 0.6,
            pitch: 0.4,
            zoom: 1.,
        }
    }
}

// Fixed viewpoints, each keeping EVE's compass the way a pilot reads it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum View {
    // Looking down: North up, West left
    Top,
    // From the south looking north: Up up, East right
    Front,
    // From the east looking west: Up up, North right
    Side,
}

impl View {
    pub fn camera(self, zoom: f64) -> Camera {
        let (yaw, pitch) = match self {
            View::Top => (PI, FRAC_PI_2),
            View::Front => (PI, 0.),
            View::Side => (FRAC_PI_2, 0.),
        };
        Camera { yaw, pitch, zoom }
    }
}

impl Camera {
    pub fn orbit(&mut self, dx: f64, dy: f64) {
        self.yaw += dx * 0.01;
        self.pitch = (self.pitch + dy * 0.01).clamp(-FRAC_PI_2, FRAC_PI_2);
    }

    pub fn zoom_by(&mut self, factor: f64) {
        self.zoom = (self.zoom * factor).clamp(0.2, 4.);
    }

    // Eases between two cameras, turning the short way around.
    pub fn lerp(self, to: Camera, t: f64) -> Camera {
        let mut dyaw = (to.yaw - self.yaw) % TAU;
        if dyaw > PI {
            dyaw -= TAU;
        } else if dyaw < -PI {
            dyaw += TAU;
        }
        Camera {
            yaw: self.yaw + dyaw * t,
            pitch: self.pitch + (to.pitch - self.pitch) * t,
            zoom: self.zoom + (to.zoom - self.zoom) * t,
        }
    }

    // Returns screen-space (x, y) in scene units and depth toward the viewer.
    fn project(&self, [x, y, z]: [f64; 3]) -> (f64, f64, f64) {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let x1 = x * cos_yaw + z * sin_yaw;
        let z1 = -x * sin_yaw + z * cos_yaw;
        let y1 = y * cos_pitch - z1 * sin_pitch;
        let depth = y * sin_pitch + z1 * cos_pitch;
        (x1, -y1, depth)
    }
}

// Round ring spacing to 1/2/2.5/5 × 10^n so the labels are nice numbers
fn nice_step(target: f64) -> f64 {
    let pow = 10f64.powf(target.max(1.).log10().floor());
    [1., 2., 2.5, 5., 10.]
        .into_iter()
        .map(|mult| pow * mult)
        .find(|step| *step >= target)
        .unwrap_or(pow * 10.)
}

fn axis_dir(axis: Axis) -> [f64; 3] {
    match axis {
        Axis::X => [1., 0., 0.],
        Axis::Y => [0., 1., 0.],
        Axis::Z => [0., 0., 1.],
    }
}

fn scaled(v: [f64; 3], s: f64) -> [f64; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

// Screen geometry of the scanner for one frame. Painting and mouse handling
// share it, so what gets hit is exactly what was drawn. Scene units follow the
// original 400×400 SVG viewBox.
pub struct Scene {
    camera: Camera,
    center: Point<Pixels>,
    unit: f32,
    scale: f64,
    extent: f64,
}

impl Scene {
    pub fn new(bounds: Bounds<Pixels>, camera: Camera, formation: Option<&Formation>) -> Self {
        let extent = formation.map(Formation::extent).unwrap_or(1.);
        Self {
            camera,
            center: bounds.center(),
            unit: f32::from(bounds.size.width.min(bounds.size.height)) / 400.,
            scale: 160. / extent * camera.zoom,
            extent,
        }
    }

    fn units(&self, v: f32) -> Pixels {
        px(v * self.unit)
    }

    // World km to screen, with depth toward the viewer.
    fn world(&self, v: [f64; 3]) -> (Point<Pixels>, f64) {
        let (sx, sy, depth) = self.camera.project(v);
        (
            point(
                self.center.x + px((sx * self.scale) as f32 * self.unit),
                self.center.y + px((sy * self.scale) as f32 * self.unit),
            ),
            depth,
        )
    }

    fn probe_radius(&self, depth: f64) -> Pixels {
        self.units((4.5 + depth / self.extent).max(1.5) as f32)
    }

    // The front-most probe under the pointer, with a little slack for small dots.
    pub fn hit_probe(&self, formation: &Formation, at: Point<Pixels>) -> Option<usize> {
        formation
            .probes
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let (center, depth) = self.world([p.x, p.y, p.z]);
                let reach = self.probe_radius(depth) + px(5.);
                let d = center - at;
                let (dx, dy, r) = (f32::from(d.x), f32::from(d.y), f32::from(reach));
                (dx * dx + dy * dy <= r * r).then_some((i, depth))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    fn line(
        &self,
        from: Point<Pixels>,
        to: Point<Pixels>,
        width: f32,
        dash: Option<[f32; 2]>,
        color: Hsla,
        window: &mut Window,
    ) {
        let mut builder = PathBuilder::stroke(self.units(width));
        if let Some([on, off]) = dash {
            builder = builder.dash_array(&[self.units(on), self.units(off)]);
        }
        builder.move_to(from);
        builder.line_to(to);
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }

    fn polygon(
        &self,
        points: &[Point<Pixels>],
        width: f32,
        dash: Option<[f32; 2]>,
        color: Hsla,
        window: &mut Window,
    ) {
        let mut builder = PathBuilder::stroke(self.units(width));
        if let Some([on, off]) = dash {
            builder = builder.dash_array(&[self.units(on), self.units(off)]);
        }
        builder.add_polygon(points, true);
        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }

    fn ring(
        &self,
        center: Point<Pixels>,
        radius: Pixels,
        width: f32,
        color: Hsla,
        window: &mut Window,
    ) {
        let points: Vec<_> = (0..32)
            .map(|i| {
                let a = i as f32 / 32. * std::f32::consts::TAU;
                point(center.x + radius * a.cos(), center.y + radius * a.sin())
            })
            .collect();
        self.polygon(&points, width, None, color, window);
    }

    fn disc(&self, center: Point<Pixels>, radius: Pixels, color: Hsla, window: &mut Window) {
        let bounds = Bounds::centered_at(center, size(radius * 2., radius * 2.));
        window.paint_quad(fill(bounds, color).corner_radii(radius));
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &self,
        text: impl Into<SharedString>,
        at: Point<Pixels>,
        font_size: f32,
        bold: bool,
        centered: bool,
        color: Hsla,
        mono: &SharedString,
        window: &mut Window,
        cx: &mut App,
    ) {
        let text: SharedString = text.into();
        let mut font = font(mono.clone());
        if bold {
            font.weight = FontWeight::BOLD;
        }
        let size = self.units(font_size);
        let run = TextRun {
            len: text.len(),
            font,
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window.text_system().shape_line(text, size, &[run], None);
        let line_height = size * 1.2;
        let origin = if centered {
            point(at.x - line.width / 2., at.y - line_height / 2.)
        } else {
            point(at.x, at.y - line_height / 2.)
        };
        line.paint(origin, line_height, TextAlign::Left, None, window, cx)
            .ok();
    }
}

pub struct ScannerProps {
    pub formation: Option<Formation>,
    pub camera: Camera,
    pub selected: Option<usize>,
    pub hovered: Option<usize>,
    pub launch_label: SharedString,
    // Receives the canvas bounds each frame, for mapping pointer events
    pub bounds: Rc<Cell<Bounds<Pixels>>>,
}

pub fn scanner(props: ScannerProps) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            props.bounds.set(bounds);
            let fg = cx.theme().foreground;
            let amber = ui::amber(cx);
            let accent = ui::source_color();
            let mono = cx.theme().mono_font_family.clone();
            let scene = Scene::new(bounds, props.camera, props.formation.as_ref());
            let extent = scene.extent;
            let origin = scene.center;

            // Scale rings and faint spokes in the horizontal (equatorial) plane
            let step = nice_step(extent / 3.);
            for i in 0..8 {
                let angle = f64::from(i) / 8. * TAU;
                let r = step * 3.;
                let (end, _) = scene.world([r * angle.cos(), 0., r * angle.sin()]);
                scene.line(origin, end, 0.5, None, fg.opacity(0.12), window);
            }
            for radius in [step, step * 2., step * 3.] {
                let points: Vec<_> = (0..72)
                    .map(|i| {
                        let angle = f64::from(i) / 72. * TAU;
                        scene
                            .world([radius * angle.cos(), 0., radius * angle.sin()])
                            .0
                    })
                    .collect();
                scene.polygon(&points, 1., Some([5., 5.]), fg.opacity(0.28), window);
                let (label_at, _) =
                    scene.world([radius * FRAC_1_SQRT_2, 0., -radius * FRAC_1_SQRT_2]);
                scene.text(
                    ui::format_distance(radius),
                    point(label_at.x + scene.units(3.), label_at.y - scene.units(4.)),
                    7.,
                    false,
                    false,
                    fg.opacity(0.75),
                    &mono,
                    window,
                    cx,
                );
            }

            // Compass axes, subtle so the probes stay the focus
            for info in &AXES {
                let dir = axis_dir(info.axis);
                let north = info.axis == Axis::Z;
                let (from, _) = scene.world(scaled(dir, -extent));
                let (to, _) = scene.world(scaled(dir, extent));
                scene.line(from, to, 0.75, None, fg.opacity(0.4), window);
                let (pos, _) = scene.world(scaled(dir, extent * 1.16));
                let (neg, _) = scene.world(scaled(dir, -extent * 1.16));
                let (size, opacity) = if north { (13., 1.) } else { (9., 0.85) };
                scene.text(
                    info.positive,
                    pos,
                    size,
                    north,
                    true,
                    fg.opacity(opacity),
                    &mono,
                    window,
                    cx,
                );
                scene.text(
                    info.negative,
                    neg,
                    9.,
                    false,
                    true,
                    fg.opacity(0.55),
                    &mono,
                    window,
                    cx,
                );
            }

            // Center reticle
            let r = scene.units(4.);
            scene.line(
                point(origin.x - r, origin.y),
                point(origin.x + r, origin.y),
                0.75,
                None,
                fg.opacity(0.5),
                window,
            );
            scene.line(
                point(origin.x, origin.y - r),
                point(origin.x, origin.y + r),
                0.75,
                None,
                fg.opacity(0.5),
                window,
            );

            let Some(formation) = props.formation.as_ref() else {
                return;
            };

            // Launch center: the point EVE pulls onto the ship
            if !formation.is_balanced() {
                let (c, _) = scene.world(formation.centroid());
                scene.line(c, origin, 0.75, Some([3., 3.]), amber.opacity(0.6), window);
                let d = scene.units(4.);
                let diamond = [
                    point(c.x, c.y - d),
                    point(c.x + d, c.y),
                    point(c.x, c.y + d),
                    point(c.x - d, c.y),
                ];
                scene.polygon(&diamond, 1., None, amber, window);
                scene.text(
                    props.launch_label.clone(),
                    point(c.x + scene.units(7.), c.y + scene.units(1.)),
                    7.,
                    false,
                    false,
                    amber,
                    &mono,
                    window,
                    cx,
                );
            }

            // Probes, far to near: a tether down to the equatorial plane so
            // height reads at a glance, then a glowing node
            let mut probes: Vec<_> = formation
                .probes
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let (top, depth) = scene.world([p.x, p.y, p.z]);
                    let (base, _) = scene.world([p.x, 0., p.z]);
                    (i, top, base, depth)
                })
                .collect();
            probes.sort_by(|a, b| a.3.total_cmp(&b.3));

            let shadow = hsla(220. / 360., 0.09, 0.46, 1.);
            for (i, top, base, depth) in probes {
                let selected = props.selected == Some(i);
                let hovered = props.hovered == Some(i);
                scene.line(top, base, 0.75, None, fg.opacity(0.35), window);
                scene.ring(base, scene.units(1.5), 0.75, fg.opacity(0.5), window);
                let radius = scene.probe_radius(depth);
                if selected || hovered {
                    let color = if selected { accent } else { fg.opacity(0.6) };
                    scene.ring(top, radius + scene.units(3.5), 1.2, color, window);
                }
                scene.disc(top, radius + scene.units(2.5), shadow.opacity(0.18), window);
                scene.disc(top, radius + scene.units(1.2), shadow.opacity(0.35), window);
                scene.disc(
                    top,
                    radius,
                    if selected { accent } else { gpui_kit::white() },
                    window,
                );

                if selected || hovered {
                    let p = formation.probes[i];
                    scene.text(
                        format!(
                            "{:02}  {} / {} / {} km",
                            i + 1,
                            super::model::format_number(p.z),
                            super::model::format_number(p.x),
                            super::model::format_number(p.y)
                        ),
                        point(top.x + radius + scene.units(7.), top.y),
                        7.5,
                        selected,
                        false,
                        if selected { accent } else { fg.opacity(0.85) },
                        &mono,
                        window,
                        cx,
                    );
                }
            }
        },
    )
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::{Camera, View, nice_step};

    #[test]
    fn ring_steps_are_round_numbers() {
        assert_eq!(nice_step(0.5), 1.);
        assert_eq!(nice_step(83.3), 100.);
        assert_eq!(nice_step(180.), 200.);
        assert_eq!(nice_step(2200.), 2500.);
        assert_eq!(nice_step(4000.), 5000.);
    }

    #[test]
    fn top_view_puts_north_up_and_west_left() {
        let camera = View::Top.camera(1.);
        let (nx, ny, _) = camera.project([0., 0., 1.]);
        let (wx, wy, _) = camera.project([1., 0., 0.]);
        assert!(ny < -0.99 && nx.abs() < 1e-9);
        assert!(wx < -0.99 && wy.abs() < 1e-9);
    }

    #[test]
    fn camera_turns_the_short_way() {
        let from = Camera {
            yaw: 0.1,
            pitch: 0.,
            zoom: 1.,
        };
        let to = Camera {
            yaw: std::f64::consts::TAU - 0.1,
            ..from
        };
        assert!((from.lerp(to, 0.5).yaw - 0.).abs() < 1e-9);
    }
}
