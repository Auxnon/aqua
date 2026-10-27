//! Turns stylus samples into brush dabs: smoothing, spacing, pressure response
//! and a paint/water reservoir that runs dry along a stroke.

use crate::pigment::Pigment;

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Watercolor = 0,
    Water = 1,
    Ink = 2,
    Eraser = 3,
}

impl Tool {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Tool::Water,
            2 => Tool::Ink,
            3 => Tool::Eraser,
            _ => Tool::Watercolor,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BrushSettings {
    pub tool: Tool,
    /// Radius in canvas pixels at full pressure.
    pub size: f32,
    /// Pigment colour at unit concentration.
    pub pigment: Pigment,
    /// How much pigment the brush carries (0..1).
    pub load: f32,
    /// How much water the brush carries (0..1).
    pub water: f32,
    pub softness: f32,
    pub bristle: f32,
    /// Ink only: 0 = technical pen, 1 = India ink that feathers into wet paper.
    pub bleed: f32,
    /// Eraser strength.
    pub strength: f32,
    /// Ink only: droplets thrown off by quick pressure flicks, like a quill.
    pub splatter: f32,
    pub pressure_size: f32,
    pub pressure_flow: f32,
    pub smoothing: f32,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            tool: Tool::Watercolor,
            size: 28.0,
            pigment: crate::pigment::from_srgb([0.16, 0.20, 0.62]),
            load: 0.6,
            water: 0.6,
            softness: 0.18,
            bristle: 0.5,
            bleed: 0.0,
            strength: 0.8,
            splatter: 0.0,
            pressure_size: 0.6,
            pressure_flow: 0.5,
            smoothing: 0.3,
        }
    }
}

/// GPU instance data for one dab (matches `DabIn` in dab.wgsl).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Dab {
    /// x, y, radius, angle
    pub pos_radius: [f32; 4],
    /// water, push x, push y, dryness
    pub water_push: [f32; 4],
    pub pigment: [f32; 4],
    pub ink: [f32; 4],
    /// softness, bristle, seed, strength
    pub shape: [f32; 4],
}

impl Dab {
    pub fn bounds(&self) -> crate::gpu::IRect {
        let r = self.pos_radius[2] * 1.15 * std::f32::consts::SQRT_2 + 2.0;
        let (x, y) = (self.pos_radius[0], self.pos_radius[1]);
        crate::gpu::IRect::new((x - r).floor() as i32, (y - r).floor() as i32, (x + r).ceil() as i32 + 1, (y + r).ceil() as i32 + 1)
    }
}

/// Which pipeline (and therefore which textures) a dab writes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DabKind {
    /// water + suspended pigment + wet ink + push flux
    Wet,
    /// permanent ink
    Fixed,
    Erase,
}

#[derive(Clone, Copy, Debug)]
pub struct InputPoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

/// How long the brush must rest before it starts soaking paint into the paper.
const DWELL_DELAY: f32 = 0.08;

pub struct Stroke {
    settings: BrushSettings,
    seed: f32,
    smoothed: InputPoint,
    last: InputPoint,
    /// distance travelled since the last dab
    carry: f32,
    water_res: f32,
    paint_res: f32,
    dir: [f32; 2],
    started: bool,
    /// Seconds since the brush last moved far enough to lay a dab.
    still: f32,
    /// Seconds spent soaking at the current spot; the blot grows with it.
    dwell: f32,
    prev_raw: Option<InputPoint>,
    /// Splatter energy built up by pressure jumps and fast flicks.
    flick: f32,
    rng: u32,
}

impl Stroke {
    pub fn new(settings: BrushSettings, seed: u32) -> Self {
        let p = InputPoint { x: 0.0, y: 0.0, pressure: 0.0 };
        Self {
            settings,
            seed: (seed % 997) as f32 * 0.61803,
            smoothed: p,
            last: p,
            carry: 0.0,
            water_res: 1.0,
            paint_res: 1.0,
            dir: [1.0, 0.0],
            started: false,
            still: 0.0,
            dwell: 0.0,
            prev_raw: None,
            flick: 0.0,
            rng: seed.wrapping_mul(747796405).wrapping_add(2891336453) | 1,
        }
    }

    fn spacing(&self, radius: f32) -> f32 {
        let frac = match self.settings.tool {
            Tool::Ink => 0.08,
            Tool::Eraser => 0.15,
            _ => 0.12,
        };
        (radius * frac).max(0.5)
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Feeds one input sample; appends the dabs it produces.
    pub fn add(&mut self, p: InputPoint, out: &mut Vec<(DabKind, Dab)>) {
        self.track_flick(p, out);
        if !self.started {
            self.started = true;
            self.smoothed = p;
            self.last = p;
            self.emit(p, 0.0, out);
            return;
        }
        // Streamline smoothing: the brush trails the pen a little.
        let k = 1.0 - self.settings.smoothing.clamp(0.0, 0.95) * 0.85;
        let target = InputPoint {
            x: self.smoothed.x + (p.x - self.smoothed.x) * k,
            y: self.smoothed.y + (p.y - self.smoothed.y) * k,
            pressure: self.smoothed.pressure + (p.pressure - self.smoothed.pressure) * k.max(0.5),
        };
        self.smoothed = target;
        self.walk_to(target, out);
    }

    /// Lets the smoothed brush catch up with the pen when it lifts.
    pub fn finish(&mut self, p: InputPoint, out: &mut Vec<(DabKind, Dab)>) {
        if self.started {
            // Flicking the pen off the paper throws ink ahead of the stroke.
            if let Some(q) = self.prev_raw {
                let r = self.radius(q.pressure).max(1.5);
                let speed = ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt() / r;
                self.flick += (speed - 4.0).max(0.0) * 0.12;
                if self.flick > 0.3 {
                    let e = self.flick;
                    self.splatter(q, e, 0.6, out);
                }
            }
            self.walk_to(InputPoint { pressure: self.smoothed.pressure, ..p }, out);
        }
    }

    /// Called every frame while the pen is down. A resting brush keeps soaking
    /// paint (or ink, or water) into the paper, faster the harder it is pressed,
    /// so the mark slowly blooms outwards.
    pub fn dwell(&mut self, dt: f32, out: &mut Vec<(DabKind, Dab)>) {
        if !self.started || self.settings.tool == Tool::Eraser {
            return;
        }
        self.still += dt;
        if self.still < DWELL_DELAY {
            return;
        }
        self.dwell += dt;
        let p = InputPoint { pressure: self.smoothed.pressure, ..self.last };
        let pr = p.pressure.clamp(0.0, 1.0);
        let rate = 0.25 + 1.75 * pr * pr;
        // The blot widens a little; most of the spread is the extra water running
        // outwards in the simulation, which gives it a ragged, wet edge.
        let grow = 1.0 + (self.dwell * (0.1 + 0.3 * pr)).min(0.35);
        self.water_res *= (-dt * 0.12).exp();
        self.paint_res *= (-dt * 0.09).exp();
        self.emit_with(p, 0.0, dt * rate, grow, 2.5, out);
    }

    /// Builds up splatter energy from sudden pressure changes and very fast moves.
    fn track_flick(&mut self, p: InputPoint, out: &mut Vec<(DabKind, Dab)>) {
        if self.settings.tool != Tool::Ink || self.settings.splatter <= 0.0 {
            return;
        }
        if let Some(q) = self.prev_raw {
            let dp = (p.pressure - q.pressure).abs();
            let r = self.radius(p.pressure).max(1.5);
            let speed = ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt() / r;
            self.flick = self.flick * 0.85 + (dp - 0.04).max(0.0) * 8.0 + (speed - 6.0).max(0.0) * 0.1;
            if self.flick > 0.35 {
                let e = self.flick;
                // A jab with little motion throws drops all round; a fast flick throws them forwards.
                let spread = if speed < 1.0 { std::f32::consts::PI } else { 0.9 };
                self.splatter(p, e, spread, out);
                self.flick = 0.0;
            }
        }
        self.prev_raw = Some(p);
    }

    /// Throws ink droplets around `p`, mostly along the stroke direction.
    fn splatter(&mut self, p: InputPoint, energy: f32, spread: f32, out: &mut Vec<(DabKind, Dab)>) {
        let s = self.settings;
        let n = (energy * 6.0 * s.splatter).ceil().min(12.0) as u32;
        let r = self.radius(p.pressure).max(1.5);
        let base = self.dir[1].atan2(self.dir[0]);
        let reach = (0.5 + energy).min(2.5);
        for _ in 0..n {
            let a = base + (self.rand() * 2.0 - 1.0) * spread;
            let d = r * (1.5 + self.rand() * self.rand() * 14.0) * reach;
            let radius = r * (0.12 + 0.5 * self.rand() * self.rand());
            let amt = 2.0 * (0.4 + 0.6 * s.load) * (0.6 + 0.6 * self.rand());
            let dab = Dab {
                pos_radius: [p.x + a.cos() * d, p.y + a.sin() * d, radius.max(0.6), self.rand() * std::f32::consts::TAU],
                water_push: [0.0; 4],
                pigment: [0.0; 4],
                ink: [0.0; 4],
                shape: [(1.0 / radius).clamp(0.08, 0.5), 0.0, self.seed, 0.0],
            };
            push_ink(dab, s.pigment.map(|v| v * amt), s.bleed, 1.0, out);
        }
    }

    fn walk_to(&mut self, target: InputPoint, out: &mut Vec<(DabKind, Dab)>) {
        let (dx, dy) = (target.x - self.last.x, target.y - self.last.y);
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < 1e-3 {
            return;
        }
        let d = [dx / dist, dy / dist];
        // Turn gradually so the bristle streaks follow the stroke smoothly.
        self.dir = [self.dir[0] * 0.6 + d[0] * 0.4, self.dir[1] * 0.6 + d[1] * 0.4];
        let n = (self.dir[0] * self.dir[0] + self.dir[1] * self.dir[1]).sqrt().max(1e-6);
        self.dir = [self.dir[0] / n, self.dir[1] / n];

        let mut travelled = 0.0;
        loop {
            let t = travelled / dist;
            let pr = self.last.pressure + (target.pressure - self.last.pressure) * t;
            let step = self.spacing(self.radius(pr)) - self.carry;
            if travelled + step > dist {
                self.carry += dist - travelled;
                break;
            }
            travelled += step;
            self.carry = 0.0;
            let t = travelled / dist;
            let pt = InputPoint {
                x: self.last.x + dx * t,
                y: self.last.y + dy * t,
                pressure: self.last.pressure + (target.pressure - self.last.pressure) * t,
            };
            self.emit(pt, step, out);
            self.still = 0.0;
            self.dwell = 0.0;
        }
        self.last = target;
    }

    fn radius(&self, pressure: f32) -> f32 {
        let s = &self.settings;
        let p = pressure.clamp(0.0, 1.0);
        (s.size * (1.0 - s.pressure_size + s.pressure_size * p.powf(0.8))).max(0.75)
    }

    fn emit(&mut self, p: InputPoint, step: f32, out: &mut Vec<(DabKind, Dab)>) {
        // Amount per dab is scaled by spacing so the laid-down density doesn't depend on it.
        let radius = self.radius(p.pressure);
        let density = self.spacing(radius) / radius;
        self.emit_with(p, step, density, 1.0, 1.0, out);
    }

    /// Lays one dab. `density` is how much of a full coat it deposits; `grow`
    /// widens it and `soak` multiplies the water released (both for a resting
    /// brush's spreading blot: a brush held to the paper drains into it).
    fn emit_with(&mut self, p: InputPoint, step: f32, density: f32, grow: f32, soak: f32, out: &mut Vec<(DabKind, Dab)>) {
        let s = self.settings;
        let radius = self.radius(p.pressure) * grow;
        let flow = 1.0 - s.pressure_flow + s.pressure_flow * p.pressure.clamp(0.0, 1.0);
        let angle = self.dir[1].atan2(self.dir[0]);

        // The reservoir empties with distance; more water lets a stroke go further.
        let reach = s.size * (25.0 + 90.0 * s.water);
        let decay = (-step / reach).exp();
        self.water_res *= decay;
        self.paint_res *= (-step / (reach * 1.4)).exp();

        let mut dab = Dab {
            pos_radius: [p.x, p.y, radius, angle],
            water_push: [0.0; 4],
            pigment: [0.0; 4],
            ink: [0.0; 4],
            shape: [s.softness.clamp(0.02, 1.0), s.bristle, self.seed, s.strength],
        };
        match s.tool {
            Tool::Watercolor => {
                let water = s.water * self.water_res;
                let amt = s.load * self.paint_res * flow * density * 1.2;
                dab.water_push = [
                    (0.15 + water) * 0.9 * flow * density * soak,
                    self.dir[0] * 0.06 * water * density,
                    self.dir[1] * 0.06 * water * density,
                    (1.0 - water * 1.6).clamp(0.0, 1.0) * 0.9,
                ];
                dab.pigment = s.pigment.map(|v| v * amt);
                out.push((DabKind::Wet, dab));
            }
            Tool::Water => {
                let water = s.water.max(0.1) * self.water_res;
                dab.water_push = [water * 1.1 * flow * density * soak, self.dir[0] * 0.08 * water * density, self.dir[1] * 0.08 * water * density, 0.0];
                out.push((DabKind::Wet, dab));
            }
            Tool::Ink => {
                // Ink is dense: several times the concentration of a watercolour load.
                let amt = 2.5 * flow * density * (0.4 + 0.6 * s.load);
                // A pen blot spreads less than wet ink feathers.
                dab.pos_radius[2] = self.radius(p.pressure) * grow.sqrt();
                dab.shape[0] = (1.2 / radius).clamp(0.02, 0.5) + s.bleed.clamp(0.0, 1.0) * 0.15;
                dab.shape[1] = 0.0;
                push_ink(dab, s.pigment.map(|v| v * amt), s.bleed, density * soak, out);
            }
            Tool::Eraser => {
                dab.shape[3] = (s.strength * density * 1.5).clamp(0.0, 1.0);
                out.push((DabKind::Erase, dab));
            }
        }
    }
}

/// Splits ink between permanent (pen) and wet (India ink, carrying its own water
/// so it can feather) according to `bleed`. `water` scales the ink's own water.
fn push_ink(dab: Dab, ink: Pigment, bleed: f32, water: f32, out: &mut Vec<(DabKind, Dab)>) {
    let bleed = bleed.clamp(0.0, 1.0);
    if bleed < 0.999 {
        let mut d = dab;
        d.ink = ink.map(|v| v * (1.0 - bleed));
        out.push((DabKind::Fixed, d));
    }
    if bleed > 0.001 {
        let mut d = dab;
        d.ink = ink.map(|v| v * bleed);
        d.water_push = [bleed * 0.35 * water, 0.0, 0.0, 0.0];
        out.push((DabKind::Wet, d));
    }
}
