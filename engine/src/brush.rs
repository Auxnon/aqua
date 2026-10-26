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

    /// Feeds one input sample; appends the dabs it produces.
    pub fn add(&mut self, p: InputPoint, out: &mut Vec<(DabKind, Dab)>) {
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
            self.walk_to(InputPoint { pressure: self.smoothed.pressure, ..p }, out);
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
        }
        self.last = target;
    }

    fn radius(&self, pressure: f32) -> f32 {
        let s = &self.settings;
        let p = pressure.clamp(0.0, 1.0);
        (s.size * (1.0 - s.pressure_size + s.pressure_size * p.powf(0.8))).max(0.75)
    }

    fn emit(&mut self, p: InputPoint, step: f32, out: &mut Vec<(DabKind, Dab)>) {
        let s = self.settings;
        let radius = self.radius(p.pressure);
        let flow = 1.0 - s.pressure_flow + s.pressure_flow * p.pressure.clamp(0.0, 1.0);
        // Amount per dab is scaled by spacing so the laid-down density doesn't depend on it.
        let density = self.spacing(radius) / radius;
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
                    (0.15 + water) * 0.9 * flow * density,
                    self.dir[0] * 0.06 * water * density,
                    self.dir[1] * 0.06 * water * density,
                    (1.0 - water * 1.6).clamp(0.0, 1.0) * 0.9,
                ];
                dab.pigment = s.pigment.map(|v| v * amt);
                out.push((DabKind::Wet, dab));
            }
            Tool::Water => {
                let water = s.water.max(0.1) * self.water_res;
                dab.water_push = [water * 1.1 * flow * density, self.dir[0] * 0.08 * water * density, self.dir[1] * 0.08 * water * density, 0.0];
                out.push((DabKind::Wet, dab));
            }
            Tool::Ink => {
                // Ink is dense: several times the concentration of a watercolour load.
                let amt = 2.5 * flow * density * (0.4 + 0.6 * s.load);
                let ink = s.pigment.map(|v| v * amt);
                let bleed = s.bleed.clamp(0.0, 1.0);
                dab.shape[0] = (1.2 / radius).clamp(0.02, 0.5) + bleed * 0.15;
                dab.shape[1] = 0.0;
                if bleed < 0.999 {
                    let mut d = dab;
                    d.ink = ink.map(|v| v * (1.0 - bleed));
                    out.push((DabKind::Fixed, d));
                }
                if bleed > 0.001 {
                    let mut d = dab;
                    d.ink = ink.map(|v| v * bleed);
                    // India ink carries its own water, which is what lets it feather.
                    d.water_push = [bleed * 0.35 * density, 0.0, 0.0, 0.0];
                    out.push((DabKind::Wet, d));
                }
            }
            Tool::Eraser => {
                dab.shape[3] = (s.strength * density * 1.5).clamp(0.0, 1.0);
                out.push((DabKind::Erase, dab));
            }
        }
    }
}
