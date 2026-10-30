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
    /// A dry brush: lays nothing down, just pushes standing water (and the paint
    /// and wet ink in it) along the stroke.
    Push = 4,
}

impl Tool {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Tool::Water,
            2 => Tool::Ink,
            3 => Tool::Eraser,
            4 => Tool::Push,
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

impl BrushSettings {
    /// From the flat parameter list the apps send: size, pigment (4), load,
    /// water, softness, bristle, bleed, strength, pressure size, pressure flow,
    /// smoothing, splatter.
    pub fn from_params(tool: u32, p: &[f32]) -> Option<Self> {
        if p.len() < 15 {
            return None;
        }
        Some(Self {
            tool: Tool::from_u32(tool),
            size: p[0],
            pigment: [p[1], p[2], p[3], p[4]],
            load: p[5],
            water: p[6],
            softness: p[7],
            bristle: p[8],
            bleed: p[9],
            strength: p[10],
            pressure_size: p[11],
            pressure_flow: p[12],
            smoothing: p[13],
            splatter: p[14],
        })
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
    /// the eraser in standing water: lifts settled pigment and pushes it along
    Smear,
    /// the push brush: drags standing water and what is suspended in it along
    Push,
}

#[derive(Clone, Copy, Debug)]
pub struct InputPoint {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
}

/// How long the brush must rest before it starts soaking paint into the paper.
/// Radius of a fine nib. Splatter throw distance and flick sensitivity are set
/// by this rather than the brush size.
const SPLAT_NIB: f32 = 3.0;
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
            Tool::Eraser | Tool::Push => 0.15,
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
                let r = self.radius(q.pressure).clamp(1.5, SPLAT_NIB);
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
        if !self.started || matches!(self.settings.tool, Tool::Eraser | Tool::Push) {
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
            // Measured against a small nib, so a big brush flicks as easily as a fine one.
            let r = self.radius(p.pressure).clamp(1.5, SPLAT_NIB);
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

    /// Throws ink droplets around `p`, mostly along the stroke direction. Drops fly
    /// about as far as from a fine nib whatever the brush size; a bigger blot
    /// instead gets a ragged, spiky rim, like a heavy drop hitting the paper.
    fn splatter(&mut self, p: InputPoint, energy: f32, spread: f32, out: &mut Vec<(DabKind, Dab)>) {
        let s = self.settings;
        let n = (energy * 6.0 * s.splatter).ceil().min(12.0) as u32;
        let r = self.radius(p.pressure).max(1.5);
        let nib = r.min(SPLAT_NIB);
        let big = r / nib;
        let base = self.dir[1].atan2(self.dir[0]);
        let reach = (0.5 + energy).min(2.5);
        for _ in 0..n {
            let a = base + (self.rand() * 2.0 - 1.0) * spread;
            // Measured from the blot's edge, in nib-sized steps.
            let d = r - nib + nib * (1.5 + self.rand() * self.rand() * 14.0) * reach;
            let radius = nib * (0.12 + 0.5 * self.rand() * self.rand()) * big.powf(0.3);
            // Near drops are often still joined to the blot by a thin neck.
            if d < r + nib * 3.0 && self.rand() < 0.5 {
                self.splat_run(p, a, r * 0.85, d, (radius * 0.45, radius * 0.3), out);
            }
            // Drops thrown further are stretched along their flight into teardrops.
            let stretch = ((d - r) / (nib * 8.0)).clamp(0.0, 1.0);
            self.splat_drop(p, a, d, radius, stretch, out);
        }
        // Spikes and satellite blobs breaking up the rim of a big blot.
        let spikes = ((big - 1.0) * energy * 3.0 * s.splatter).ceil().min(18.0) as u32;
        let rim_spread = if spread > 1.5 { spread } else { spread + 0.6 };
        for _ in 0..spikes {
            let a = base + (self.rand() * 2.0 - 1.0) * rim_spread;
            let len = r * (0.15 + 0.45 * self.rand()) + nib * reach * self.rand();
            let width = r * (0.08 + 0.12 * self.rand());
            self.splat_run(p, a, r * 0.8, r * 0.8 + len, (width, width * 0.4), out);
            if self.rand() < 0.4 {
                let da = (self.rand() - 0.5) * 0.3;
                self.splat_drop(p, a + da, r * 0.8 + len * 1.3 + width, width * 0.8, 0.3, out);
            }
        }
    }

    /// Ink as dark as the stroke's own, for a drop of `radius`, plus the water that
    /// India ink carries with it.
    fn splat_ink(&mut self, p: InputPoint) -> (Pigment, f32) {
        let s = self.settings;
        let flow = 1.0 - s.pressure_flow + s.pressure_flow * p.pressure.clamp(0.0, 1.0);
        // A stroke builds up about this many dabs' worth at any point.
        let amt = 4.0 * flow.max(0.5) * (0.4 + 0.6 * s.load) * (0.8 + 0.4 * self.rand());
        (s.pigment.map(|v| v * amt), 1.6)
    }

    /// One lumpy drop: a few overlapping blobs, trailing back towards the blot
    /// by `stretch` (0 = round).
    fn splat_drop(&mut self, p: InputPoint, a: f32, d: f32, radius: f32, stretch: f32, out: &mut Vec<(DabKind, Dab)>) {
        self.splat_dab(p, a, d, radius, out);
        let lumps = 1 + (self.rand() * 3.0) as u32;
        for _ in 0..lumps {
            let off = radius * (0.3 + 0.5 * self.rand());
            let la = self.rand() * std::f32::consts::TAU;
            let (x, y) = (p.x + a.cos() * d + la.cos() * off, p.y + a.sin() * d + la.sin() * off);
            let lr = radius * (0.35 + 0.4 * self.rand());
            self.splat_at(p, x, y, lr, out);
        }
        // Tail: smaller blobs back along the flight path.
        let tail = (stretch * 4.0) as u32;
        for k in 1..=tail {
            let t = k as f32 / (tail + 1) as f32;
            let jitter = (self.rand() - 0.5) * radius * 0.4;
            let back = d - radius * (0.6 + 2.2 * t) * (0.5 + stretch);
            let (x, y) = (p.x + a.cos() * back - a.sin() * jitter, p.y + a.sin() * back + a.cos() * jitter);
            self.splat_at(p, x, y, radius * (1.0 - 0.75 * t), out);
        }
    }

    /// A wandering run of ink from distance `from` to `to` along `a`, tapering
    /// from width `w0` to `w1`.
    fn splat_run(&mut self, p: InputPoint, a: f32, from: f32, to: f32, (w0, w1): (f32, f32), out: &mut Vec<(DabKind, Dab)>) {
        let steps = (((to - from) / (w0.min(w1) * 0.7).max(0.5)).ceil() as u32).clamp(2, 24);
        let mut ang = a;
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            ang += (self.rand() - 0.5) * 0.12;
            let d = from + (to - from) * t;
            let w = (w0 + (w1 - w0) * t) * (0.8 + 0.4 * self.rand());
            self.splat_at(p, p.x + ang.cos() * d, p.y + ang.sin() * d, w, out);
        }
    }

    fn splat_dab(&mut self, p: InputPoint, a: f32, d: f32, radius: f32, out: &mut Vec<(DabKind, Dab)>) {
        self.splat_at(p, p.x + a.cos() * d, p.y + a.sin() * d, radius, out);
    }

    fn splat_at(&mut self, p: InputPoint, x: f32, y: f32, radius: f32, out: &mut Vec<(DabKind, Dab)>) {
        let radius = radius.max(0.6);
        let (ink, water) = self.splat_ink(p);
        let bleed = self.settings.bleed;
        let dab = Dab {
            pos_radius: [x, y, radius, self.rand() * std::f32::consts::TAU],
            water_push: [0.0; 4],
            pigment: [0.0; 4],
            ink: [0.0; 4],
            // The same crisp edge as the ink stroke.
            shape: [(1.2 / radius).clamp(0.02, 0.5) + bleed.clamp(0.0, 1.0) * 0.15, 0.0, self.seed, 0.0],
        };
        push_ink(dab, ink, bleed, water, out);
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
                // Erases dry paint. Where water is standing, the paint it takes off is
                // lifted into the water instead and pushed along the stroke (a smear).
                dab.shape[3] = (s.strength * density * 1.5).clamp(0.0, 1.0);
                out.push((DabKind::Erase, dab));
                let push = 2.0 * s.strength * density;
                dab.water_push = [0.0, self.dir[0] * push, self.dir[1] * push, 0.0];
                out.push((DabKind::Smear, dab));
            }
            Tool::Push => {
                // Drags what lies half a radius behind the dab under it (water_push.yz,
                // in pixels), mixing by the strength; ink.x is the extra shove given to
                // the water. The first dab has no direction yet.
                if step <= 0.0 {
                    return;
                }
                let reach = radius * 0.5;
                dab.shape[3] = (s.strength * 0.6).clamp(0.0, 1.0);
                dab.water_push = [0.0, self.dir[0] * reach, self.dir[1] * reach, 0.0];
                dab.ink = [1.5 * s.strength * density, 0.0, 0.0, 0.0];
                out.push((DabKind::Push, dab));
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
