// Edit state uses km for positions and AU for ranges; the settings file
// stores meters. Axes follow EVE's solar-system convention: +x West, +y Up,
// +z North.

use eve_wrench_core::{FormationProbe, ProbeFormation};

const KM: f64 = 1000.;
const AU: f64 = 149_597_870_700.;

// EVE probe launchers hold 8 probes, so formations are capped at 8
pub const MAX_PROBES: usize = 8;

// Valid probe scan ranges in EVE: powers of two from 0.25 to 32 AU
pub const RANGE_OPTIONS: [f64; 8] = [0.25, 0.5, 1., 2., 4., 8., 16., 32.];

const DEFAULT_RANGE: f64 = 32.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

pub struct AxisInfo {
    pub axis: Axis,
    pub positive: &'static str,
    pub negative: &'static str,
    pub tooltip_key: &'static str,
}

// Column order in the probe table and the scanner: N/S, W/E, U/D.
pub const AXES: [AxisInfo; 3] = [
    AxisInfo {
        axis: Axis::Z,
        positive: "N",
        negative: "S",
        tooltip_key: "formationEditor.axisNS",
    },
    AxisInfo {
        axis: Axis::X,
        positive: "W",
        negative: "E",
        tooltip_key: "formationEditor.axisWE",
    },
    AxisInfo {
        axis: Axis::Y,
        positive: "U",
        negative: "D",
        tooltip_key: "formationEditor.axisUD",
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Probe {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub range: f64,
}

impl Probe {
    pub const fn new(x: f64, y: f64, z: f64) -> Self {
        Self {
            x,
            y,
            z,
            range: DEFAULT_RANGE,
        }
    }

    pub fn get(&self, axis: Axis) -> f64 {
        match axis {
            Axis::X => self.x,
            Axis::Y => self.y,
            Axis::Z => self.z,
        }
    }

    pub fn set(&mut self, axis: Axis, value: f64) {
        match axis {
            Axis::X => self.x = value,
            Axis::Y => self.y = value,
            Axis::Z => self.z = value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Formation {
    pub name: String,
    pub probes: Vec<Probe>,
}

fn round3(v: f64) -> f64 {
    (v * 1000.).round() / 1000.
}

impl Formation {
    pub fn from_stored(stored: &ProbeFormation) -> Self {
        Self {
            name: stored.name.clone(),
            probes: stored
                .probes
                .iter()
                .map(|p| Probe {
                    x: p.x / KM,
                    y: p.y / KM,
                    z: p.z / KM,
                    range: p.range / AU,
                })
                .collect(),
        }
    }

    pub fn to_stored(&self, id: i64) -> ProbeFormation {
        let name = self.name.trim();
        ProbeFormation {
            id,
            name: if name.is_empty() {
                format!("Formation {}", id + 1)
            } else {
                name.to_string()
            },
            probes: self
                .probes
                .iter()
                .map(|p| FormationProbe {
                    x: p.x * KM,
                    y: p.y * KM,
                    z: p.z * KM,
                    range: p.range * AU,
                })
                .collect(),
        }
    }

    // Negative factors shrink: -2 divides by 2
    pub fn scale(&mut self, factor: f64) {
        if !factor.is_finite() || factor == 0. {
            return;
        }
        let factor = if factor < 0. {
            1. / factor.abs()
        } else {
            factor
        };
        for p in &mut self.probes {
            p.x *= factor;
            p.y *= factor;
            p.z *= factor;
        }
    }

    // Rotates every probe around the ship along one compass axis. Right-handed
    // about the chosen axis, so positive degrees turn N→W around U/D. Rotation
    // is linear, so a balanced formation stays balanced.
    pub fn rotate(&mut self, axis: Axis, degrees: f64) {
        if !degrees.is_finite() || degrees % 360. == 0. {
            return;
        }
        let (sin, cos) = degrees.to_radians().sin_cos();
        // Cyclic pairs keep the rotation right-handed: x:(y,z) y:(z,x) z:(x,y)
        let (a, b) = match axis {
            Axis::X => (Axis::Y, Axis::Z),
            Axis::Y => (Axis::Z, Axis::X),
            Axis::Z => (Axis::X, Axis::Y),
        };
        for p in &mut self.probes {
            let (u, v) = (p.get(a), p.get(b));
            p.set(a, round3(u * cos - v * sin));
            p.set(b, round3(u * sin + v * cos));
        }
    }

    pub fn set_all_ranges(&mut self, range: f64) {
        for p in &mut self.probes {
            p.range = range;
        }
    }

    // Average probe position. EVE re-centers a launched formation on this
    // point, so only zero-centroid formations launch exactly as drawn.
    pub fn centroid(&self) -> [f64; 3] {
        if self.probes.is_empty() {
            return [0.; 3];
        }
        let n = self.probes.len() as f64;
        let sum = self
            .probes
            .iter()
            .fold([0.; 3], |s, p| [s[0] + p.x, s[1] + p.y, s[2] + p.z]);
        [sum[0] / n, sum[1] / n, sum[2] / n]
    }

    // How far every probe drifts at launch when EVE pulls the centroid onto the ship
    pub fn launch_shift(&self) -> f64 {
        let [x, y, z] = self.centroid();
        (x * x + y * y + z * z).sqrt()
    }

    // Sub-km drift is invisible at probe scales
    pub fn is_balanced(&self) -> bool {
        self.launch_shift() < 0.5
    }

    // Zeroes the centroid with one counterweight probe: appended if the
    // launcher has room, otherwise the last probe is repurposed.
    pub fn balance(&mut self) {
        if self.probes.is_empty() || self.is_balanced() {
            return;
        }
        let full = self.probes.len() >= MAX_PROBES;
        let rest = if full {
            &self.probes[..self.probes.len() - 1]
        } else {
            &self.probes[..]
        };
        let sum = rest
            .iter()
            .fold([0.; 3], |s, p| [s[0] + p.x, s[1] + p.y, s[2] + p.z]);
        let range = rest.first().map(|p| p.range).unwrap_or(DEFAULT_RANGE);
        let counterweight = Probe {
            x: round3(-sum[0]),
            y: round3(-sum[1]),
            z: round3(-sum[2]),
            range,
        };
        if full {
            let last = self.probes.last_mut().expect("full formation has probes");
            *last = Probe {
                range: last.range,
                ..counterweight
            };
        } else {
            self.probes.push(counterweight);
        }
    }

    // Custom formations always launch all 8 probes. Missing ones are added on
    // the ship, which leaves the launch center where it was for a balanced
    // formation.
    pub fn fill_to_max(&mut self) {
        let range = self
            .probes
            .first()
            .map(|p| p.range)
            .unwrap_or(DEFAULT_RANGE);
        while self.probes.len() < MAX_PROBES {
            self.probes.push(Probe {
                range,
                ..Probe::new(0., 0., 0.)
            });
        }
    }

    pub fn mirror(&mut self, axis: Axis) {
        for p in &mut self.probes {
            p.set(axis, -p.get(axis));
        }
    }

    pub fn offset(&mut self, delta: [f64; 3]) {
        for p in &mut self.probes {
            p.x = round3(p.x + delta[0]);
            p.y = round3(p.y + delta[1]);
            p.z = round3(p.z + delta[2]);
        }
    }

    pub fn extent(&self) -> f64 {
        self.probes
            .iter()
            .flat_map(|p| [p.x.abs(), p.y.abs(), p.z.abs()])
            .fold(1., f64::max)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PresetIcon {
    Grid,
    Crosshair,
    Skull,
    North,
    South,
    West,
    East,
    Up,
    Down,
}

pub struct Preset {
    // i18n key under formationEditor.presets; "blank" keeps a generic name
    pub id: &'static str,
    pub icon: PresetIcon,
    probes: fn() -> Vec<Probe>,
}

impl Preset {
    pub fn probes(&self) -> Vec<Probe> {
        (self.probes)()
    }
}

// 8-probe placeholder spread at 250 km, the neutral starting point
fn blank_spread() -> Vec<Probe> {
    vec![
        Probe::new(250., 0., 0.),
        Probe::new(-250., 0., 0.),
        Probe::new(0., 0., 250.),
        Probe::new(0., 0., -250.),
        Probe::new(0., 250., 0.),
        Probe::new(0., -250., 0.),
        Probe::new(0., 500., 0.),
        Probe::new(0., -500., 0.),
    ]
}

// 500 km sphere with an extended 1000 km vertical pair
fn pinpoint() -> Vec<Probe> {
    vec![
        Probe::new(500., 0., 0.),
        Probe::new(-500., 0., 0.),
        Probe::new(0., 0., 500.),
        Probe::new(0., 0., -500.),
        Probe::new(0., 500., 0.),
        Probe::new(0., -500., 0.),
        Probe::new(0., 1000., 0.),
        Probe::new(0., -1000., 0.),
    ]
}

// One probe placed behind the Drifter (11,000 km West, 3,400 km Up), mirrored
// by a counterweight so the centroid is zero and the layout launches as
// drawn; the rest are 250 km placeholders to reposition
fn drifter() -> Vec<Probe> {
    vec![
        Probe::new(11000., 3400., 0.),
        Probe::new(-11000., -3400., 0.),
        Probe::new(250., 0., 0.),
        Probe::new(-250., 0., 0.),
        Probe::new(0., 0., 250.),
        Probe::new(0., 0., -250.),
        Probe::new(0., 250., 0.),
        Probe::new(0., -250., 0.),
    ]
}

pub const FORMATION_PRESETS: [Preset; 3] = [
    Preset {
        id: "blank",
        icon: PresetIcon::Grid,
        probes: blank_spread,
    },
    Preset {
        id: "pinpoint",
        icon: PresetIcon::Crosshair,
        probes: pinpoint,
    },
    Preset {
        id: "drifter",
        icon: PresetIcon::Skull,
        probes: drifter,
    },
];

// Directional stacks: 7 probes layered along one axis at 200 km intervals plus
// one counterweight probe far on the opposite side. EVE re-centers launched
// formations on their centroid, so a one-sided line would come out centered on
// the player; the counterweight zeroes the centroid so the line launches as
// drawn, at the cost of one sacrificial probe.
fn stack(axis: Axis, sign: f64) -> Vec<Probe> {
    let line: Vec<f64> = (1..=7).map(|i| f64::from(i) * 200.).collect();
    let counterweight = -line.iter().sum::<f64>();
    line.into_iter()
        .chain([counterweight])
        .map(|offset| {
            let mut probe = Probe::new(0., 0., 0.);
            probe.set(axis, offset * sign);
            probe
        })
        .collect()
}

pub const STACK_PRESETS: [Preset; 6] = [
    Preset {
        id: "north",
        icon: PresetIcon::North,
        probes: || stack(Axis::Z, 1.),
    },
    Preset {
        id: "south",
        icon: PresetIcon::South,
        probes: || stack(Axis::Z, -1.),
    },
    Preset {
        id: "west",
        icon: PresetIcon::West,
        probes: || stack(Axis::X, 1.),
    },
    Preset {
        id: "east",
        icon: PresetIcon::East,
        probes: || stack(Axis::X, -1.),
    },
    Preset {
        id: "up",
        icon: PresetIcon::Up,
        probes: || stack(Axis::Y, 1.),
    },
    Preset {
        id: "down",
        icon: PresetIcon::Down,
        probes: || stack(Axis::Y, -1.),
    },
];

// Parametric layouts for the pattern generator. Every pattern is point
// symmetric around the ship, so it launches exactly as drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pattern {
    // A cross in the plane across the axis plus a stretched pair along it
    Pinpoint,
    // Evenly spaced around the axis
    Ring,
    // Evenly spaced along the axis, centered on the ship
    Line,
    // Spread over a sphere
    Sphere,
}

pub const PATTERNS: [Pattern; 4] = [
    Pattern::Pinpoint,
    Pattern::Ring,
    Pattern::Line,
    Pattern::Sphere,
];

// The two axes across `axis`, in right-handed order.
fn cross_axes(axis: Axis) -> (Axis, Axis) {
    match axis {
        Axis::X => (Axis::Y, Axis::Z),
        Axis::Y => (Axis::Z, Axis::X),
        Axis::Z => (Axis::X, Axis::Y),
    }
}

fn probe_at(coords: [(Axis, f64); 3], range: f64) -> Probe {
    let mut probe = Probe::new(0., 0., 0.);
    for (axis, value) in coords {
        probe.set(axis, round3(value));
    }
    probe.range = range;
    probe
}

impl Pattern {
    pub fn id(self) -> &'static str {
        match self {
            Pattern::Pinpoint => "pinpoint",
            Pattern::Ring => "ring",
            Pattern::Line => "line",
            Pattern::Sphere => "sphere",
        }
    }

    // A single probe can't form a ring around the ship
    pub fn min_count(self) -> usize {
        match self {
            Pattern::Ring => 2,
            _ => 1,
        }
    }

    pub fn probes(self, spread: f64, count: usize, axis: Axis, range: f64) -> Vec<Probe> {
        let count = count.clamp(self.min_count(), MAX_PROBES);
        let (u, v) = cross_axes(axis);
        match self {
            Pattern::Pinpoint => [
                (spread, 0., 0.),
                (-spread, 0., 0.),
                (0., spread, 0.),
                (0., -spread, 0.),
                (0., 0., spread),
                (0., 0., -spread),
                (0., 0., spread * 2.),
                (0., 0., -spread * 2.),
            ]
            .into_iter()
            .map(|(a, b, c)| probe_at([(u, a), (v, b), (axis, c)], range))
            .collect(),
            Pattern::Ring => (0..count)
                .map(|i| {
                    let angle = i as f64 / count as f64 * std::f64::consts::TAU;
                    probe_at(
                        [
                            (u, spread * angle.cos()),
                            (v, spread * angle.sin()),
                            (axis, 0.),
                        ],
                        range,
                    )
                })
                .collect(),
            Pattern::Line => (0..count)
                .map(|i| {
                    let offset = (i as f64 - (count as f64 - 1.) / 2.) * spread;
                    probe_at([(u, 0.), (v, 0.), (axis, offset)], range)
                })
                .collect(),
            Pattern::Sphere => sphere_points(count)
                .into_iter()
                .map(|[a, b, c]| {
                    probe_at(
                        [(u, a * spread), (v, b * spread), (axis, c * spread)],
                        range,
                    )
                })
                .collect(),
        }
    }
}

// Unit-sphere points: the symmetric solids where one exists, otherwise a
// Fibonacci spiral shifted so its centroid sits on the ship.
fn sphere_points(count: usize) -> Vec<[f64; 3]> {
    let s = 1. / 3f64.sqrt();
    match count {
        1 => vec![[0., 0., 0.]],
        2 => vec![[0., 0., 1.], [0., 0., -1.]],
        4 => vec![[s, s, s], [-s, -s, s], [-s, s, -s], [s, -s, -s]],
        6 => vec![
            [1., 0., 0.],
            [-1., 0., 0.],
            [0., 1., 0.],
            [0., -1., 0.],
            [0., 0., 1.],
            [0., 0., -1.],
        ],
        8 => [-s, s]
            .into_iter()
            .flat_map(|x| {
                [-s, s]
                    .into_iter()
                    .flat_map(move |y| [-s, s].map(|z| [x, y, z]))
            })
            .collect(),
        n => {
            let golden = std::f64::consts::PI * (3. - 5f64.sqrt());
            let mut points: Vec<[f64; 3]> = (0..n)
                .map(|i| {
                    let z = 1. - 2. * (i as f64 + 0.5) / n as f64;
                    let r = (1. - z * z).sqrt();
                    let theta = golden * i as f64;
                    [r * theta.cos(), r * theta.sin(), z]
                })
                .collect();
            let c = points
                .iter()
                .fold([0.; 3], |a, p| [a[0] + p[0], a[1] + p[1], a[2] + p[2]]);
            for p in &mut points {
                for k in 0..3 {
                    p[k] -= c[k] / n as f64;
                }
            }
            points
        }
    }
}

// Reads a distance typed into a field as km. Accepts plain numbers (km),
// "km", "m" and "au" suffixes, and thousands separators.
pub fn parse_distance(text: &str) -> Option<f64> {
    const AU_KM: f64 = 149_597_870.7;
    let cleaned: String = text
        .chars()
        .filter(|c| !matches!(c, ',' | '_' | ' '))
        .collect::<String>()
        .to_lowercase();
    let (number, factor) = if let Some(n) = cleaned.strip_suffix("au") {
        (n, AU_KM)
    } else if let Some(n) = cleaned.strip_suffix("km") {
        (n, 1.)
    } else if let Some(n) = cleaned.strip_suffix('m') {
        (n, 0.001)
    } else {
        (cleaned.as_str(), 1.)
    };
    number
        .parse::<f64>()
        .ok()
        .map(|v| v * factor)
        .filter(|v| v.is_finite())
}

// Undo/redo over the whole edit state. Consecutive edits with the same key
// (typing into one field, one drag) collapse into a single step.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub formations: Vec<Formation>,
    pub selected: usize,
}

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_key: Option<String>,
}

impl History {
    const LIMIT: usize = 200;

    pub fn record(&mut self, before: Snapshot, key: Option<String>) {
        if key.is_some() && key == self.last_key {
            return;
        }
        if self.undo.last() != Some(&before) {
            self.undo.push(before);
            if self.undo.len() > Self::LIMIT {
                self.undo.remove(0);
            }
        }
        self.redo.clear();
        self.last_key = key;
    }

    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        self.last_key = None;
        Some(previous)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.last_key = None;
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

// Where an item at `index` ends up after the item at `from` moves to `to`.
pub fn index_after_move(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        to
    } else if from < index && index <= to {
        index - 1
    } else if to <= index && index < from {
        index + 1
    } else {
        index
    }
}

// Compact number text for inputs: integers without decimals, otherwise up
// to three decimals with trailing zeros removed.
pub fn format_number(value: f64) -> String {
    if value.fract() == 0. && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        let text = format!("{:.3}", value);
        text.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formation(probes: Vec<Probe>) -> Formation {
        Formation {
            name: "test".into(),
            probes,
        }
    }

    #[test]
    fn presets_launch_as_drawn() {
        for preset in FORMATION_PRESETS.iter().chain(&STACK_PRESETS) {
            let f = formation(preset.probes());
            assert_eq!(f.probes.len(), MAX_PROBES, "{}", preset.id);
            assert!(f.is_balanced(), "{} is off-center", preset.id);
        }
    }

    #[test]
    fn balance_appends_a_counterweight_when_there_is_room() {
        let mut f = formation(vec![Probe::new(100., 0., 0.), Probe::new(0., 50., 0.)]);
        assert!(!f.is_balanced());
        f.balance();
        assert_eq!(f.probes.len(), 3);
        assert_eq!(f.probes[2], Probe::new(-100., -50., 0.));
        assert!(f.is_balanced());
    }

    #[test]
    fn balance_repurposes_the_last_probe_when_full() {
        let mut probes: Vec<Probe> = (0..8)
            .map(|i| Probe::new(f64::from(i) * 10., 0., 0.))
            .collect();
        probes[7].range = 4.;
        let mut f = formation(probes);
        f.balance();
        assert_eq!(f.probes.len(), 8);
        assert!(f.is_balanced());
        assert_eq!(f.probes[7].range, 4.);
    }

    #[test]
    fn rotating_north_by_90_about_up_points_west() {
        let mut f = formation(vec![Probe::new(0., 0., 100.)]);
        f.rotate(Axis::Y, 90.);
        assert_eq!(f.probes[0].x, 100.);
        assert_eq!(f.probes[0].z, 0.);
    }

    #[test]
    fn negative_scale_shrinks() {
        let mut f = formation(vec![Probe::new(100., -50., 20.)]);
        f.scale(-2.);
        assert_eq!(f.probes[0], Probe::new(50., -25., 10.));
        f.scale(0.);
        assert_eq!(f.probes[0], Probe::new(50., -25., 10.));
    }

    #[test]
    fn stored_units_round_trip() {
        let f = formation(vec![Probe {
            x: 250.,
            y: -500.,
            z: 0.,
            range: 0.25,
        }]);
        let stored = f.to_stored(3);
        assert_eq!(stored.probes[0].x, 250_000.);
        assert_eq!(Formation::from_stored(&stored), f);
    }

    #[test]
    fn blank_names_get_a_default() {
        let f = Formation {
            name: "  ".into(),
            probes: vec![],
        };
        assert_eq!(f.to_stored(1).name, "Formation 2");
    }

    #[test]
    fn generated_patterns_launch_as_drawn() {
        for pattern in PATTERNS {
            for count in pattern.min_count()..=MAX_PROBES {
                for axis in [Axis::X, Axis::Y, Axis::Z] {
                    let f = formation(pattern.probes(500., count, axis, 8.));
                    assert!(
                        f.is_balanced(),
                        "{:?} with {} probes is off-center",
                        pattern,
                        count
                    );
                    assert!(f.probes.iter().all(|p| p.range == 8.));
                }
            }
        }
    }

    #[test]
    fn pinpoint_about_up_matches_the_classic_layout() {
        let f = formation(Pattern::Pinpoint.probes(500., 8, Axis::Y, 32.));
        let mut generated = f.probes.clone();
        let mut classic = pinpoint();
        let key = |p: &Probe| (p.x as i64, p.y as i64, p.z as i64);
        generated.sort_by_key(key);
        classic.sort_by_key(key);
        assert_eq!(generated, classic);
    }

    #[test]
    fn ring_and_line_sit_at_the_spread() {
        let ring = Pattern::Ring.probes(300., 4, Axis::Y, 32.);
        assert!(
            ring.iter()
                .all(|p| (p.x.hypot(p.z) - 300.).abs() < 0.01 && p.y == 0.)
        );
        let line = Pattern::Line.probes(200., 3, Axis::Z, 32.);
        assert_eq!(
            line.iter().map(|p| p.z).collect::<Vec<_>>(),
            vec![-200., 0., 200.]
        );
    }

    #[test]
    fn filling_to_eight_keeps_a_balanced_formation_balanced() {
        let mut f = formation(Pattern::Line.probes(200., 4, Axis::Z, 4.));
        f.fill_to_max();
        assert_eq!(f.probes.len(), MAX_PROBES);
        assert!(f.is_balanced());
        assert!(f.probes.iter().all(|p| p.range == 4.));
    }

    #[test]
    fn indices_follow_a_move() {
        let order = |from: usize, to: usize| {
            let mut items: Vec<usize> = (0..5).collect();
            let moved = items.remove(from);
            items.insert(to, moved);
            items
        };
        for from in 0..5 {
            for to in 0..5 {
                let items = order(from, to);
                for original in 0..5 {
                    assert_eq!(items[index_after_move(original, from, to)], original);
                }
            }
        }
    }

    #[test]
    fn mirror_and_offset() {
        let mut f = formation(vec![Probe::new(100., 50., -20.)]);
        f.mirror(Axis::X);
        assert_eq!(f.probes[0], Probe::new(-100., 50., -20.));
        f.offset([10., 0., 20.]);
        assert_eq!(f.probes[0], Probe::new(-90., 50., 0.));
    }

    #[test]
    fn distances_parse_with_units() {
        assert_eq!(parse_distance("250"), Some(250.));
        assert_eq!(parse_distance("1,500 km"), Some(1500.));
        assert_eq!(parse_distance("2000m"), Some(2.));
        assert_eq!(parse_distance("0.5AU"), Some(74_798_935.35));
        assert_eq!(parse_distance("-40"), Some(-40.));
        assert_eq!(parse_distance("abc"), None);
    }

    #[test]
    fn history_coalesces_edits_with_the_same_key() {
        let snap = |n: f64| Snapshot {
            formations: vec![formation(vec![Probe::new(n, 0., 0.)])],
            selected: 0,
        };
        let mut history = History::default();
        history.record(snap(0.), Some("row0".into()));
        history.record(snap(1.), Some("row0".into()));
        history.record(snap(2.), None);
        let back = history.undo(snap(3.)).unwrap();
        assert_eq!(back, snap(2.));
        let back = history.undo(back).unwrap();
        assert_eq!(back, snap(0.));
        assert!(!history.can_undo());
        assert_eq!(history.redo(back).unwrap(), snap(2.));
    }

    #[test]
    fn numbers_format_compactly() {
        assert_eq!(format_number(250.), "250");
        assert_eq!(format_number(-12.5), "-12.5");
        assert_eq!(format_number(0.123456), "0.123");
    }
}
