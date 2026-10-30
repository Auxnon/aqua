//! The mixing palette: a small CPU-side tray where pigments are dabbed and
//! smudged together with a finger. Mixing happens in K/S space, so the colours
//! you get are the ones the paint on the canvas would make.

use crate::pigment::{self, Pigment};

pub struct Palette {
    pub w: usize,
    pub h: usize,
    cells: Vec<Pigment>,
    /// paint carried by the finger during a smudge, one entry per footprint offset
    patch: Vec<Pigment>,
    patch_r: i32,
}

const TRAY: [f32; 3] = [0.88, 0.875, 0.86];

impl Palette {
    pub fn new(w: usize, h: usize) -> Self {
        Self { w, h, cells: vec![[0.0; 4]; w * h], patch: Vec::new(), patch_r: 0 }
    }

    pub fn clear(&mut self) {
        self.cells.fill([0.0; 4]);
    }

    fn falloff(d2: f32, r: f32) -> f32 {
        let t = 1.0 - (d2 / (r * r)).min(1.0);
        t * t
    }

    /// Lays down `amount` of `pig` in a soft disc.
    pub fn dab(&mut self, x: f32, y: f32, r: f32, pig: Pigment, amount: f32) {
        let ri = r.ceil() as i32;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let (px, py) = (x as i32 + dx, y as i32 + dy);
                if px < 0 || py < 0 || px >= self.w as i32 || py >= self.h as i32 {
                    continue;
                }
                let c = Self::falloff((dx * dx + dy * dy) as f32, r);
                if c <= 0.0 {
                    continue;
                }
                let cell = &mut self.cells[py as usize * self.w + px as usize];
                // Thick paint saturates: adding more mostly replaces rather than piles up.
                let room = (1.0 / (1.0 + cell[3] * 0.5)).max(0.2);
                for i in 0..4 {
                    cell[i] += pig[i] * amount * c * room;
                }
            }
        }
    }

    /// Starts a smudge: the finger picks up what is under it.
    pub fn smudge_begin(&mut self, x: f32, y: f32, r: f32) {
        self.patch_r = r.ceil() as i32;
        let n = (2 * self.patch_r + 1) as usize;
        self.patch = vec![[0.0; 4]; n * n];
        for dy in -self.patch_r..=self.patch_r {
            for dx in -self.patch_r..=self.patch_r {
                let i = ((dy + self.patch_r) as usize) * n + (dx + self.patch_r) as usize;
                self.patch[i] = self.get(x as i32 + dx, y as i32 + dy);
            }
        }
    }

    fn get(&self, x: i32, y: i32) -> Pigment {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return [0.0; 4];
        }
        self.cells[y as usize * self.w + x as usize]
    }

    /// Drags the carried paint from (x0,y0) to (x1,y1), exchanging it with the tray.
    pub fn smudge(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, strength: f32) {
        if self.patch.is_empty() {
            self.smudge_begin(x0, y0, 12.0);
        }
        let r = self.patch_r;
        let n = (2 * r + 1) as usize;
        let dist = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        let steps = (dist / (r as f32 * 0.2).max(1.0)).ceil().max(1.0) as i32;
        for s in 1..=steps {
            let t = s as f32 / steps as f32;
            let (cx, cy) = ((x0 + (x1 - x0) * t) as i32, (y0 + (y1 - y0) * t) as i32);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (px, py) = (cx + dx, cy + dy);
                    if px < 0 || py < 0 || px >= self.w as i32 || py >= self.h as i32 {
                        continue;
                    }
                    let c = Self::falloff((dx * dx + dy * dy) as f32, r as f32);
                    if c <= 0.0 {
                        continue;
                    }
                    let pi = ((dy + r) as usize) * n + (dx + r) as usize;
                    let ci = py as usize * self.w + px as usize;
                    let cell = self.cells[ci];
                    let carried = self.patch[pi];
                    let put = strength * c * 0.5;
                    let take = strength * c * 0.35;
                    self.cells[ci] = pigment::mix(cell, carried, put);
                    self.patch[pi] = pigment::mix(carried, cell, take);
                }
            }
        }
    }

    pub fn smudge_end(&mut self) {
        self.patch.clear();
    }

    /// Average pigment around (x, y), as thick as it lies in the tray: a wash
    /// spread thin over the tray picks up as a thinner, lighter paint (see
    /// [`pigment::strength`]); anything thicker than unit concentration picks
    /// up at unit concentration.
    pub fn pick(&self, x: f32, y: f32, r: f32) -> Option<Pigment> {
        let ri = r.ceil() as i32;
        let mut acc = [0.0f32; 4];
        let mut weight = 0.0;
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let c = Self::falloff((dx * dx + dy * dy) as f32, r);
                let p = self.get(x as i32 + dx, y as i32 + dy);
                weight += c;
                for i in 0..4 {
                    acc[i] += p[i] * c;
                }
            }
        }
        if weight <= 0.0 {
            return None;
        }
        pigment::picked(acc.map(|v| v / weight))
    }

    /// Renders the tray as ARGB_8888 pixels.
    pub fn render(&self, out: &mut [i32]) {
        for (i, cell) in self.cells.iter().enumerate().take(out.len()) {
            let lin = if cell[3] < 1e-4 { TRAY } else { pigment::film_over(*cell, TRAY) };
            let px = |v: f32| (pigment::linear_to_srgb(v) * 255.0 + 0.5) as u32;
            out[i] = (0xFF00_0000u32 | (px(lin[0]) << 16) | (px(lin[1]) << 8) | px(lin[2])) as i32;
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(8 + self.cells.len() * 16);
        v.extend((self.w as u32).to_le_bytes());
        v.extend((self.h as u32).to_le_bytes());
        v.extend_from_slice(bytemuck::cast_slice(&self.cells));
        lz4_flex::compress_prepend_size(&v)
    }

    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let v = lz4_flex::decompress_size_prepended(b).ok()?;
        let w = u32::from_le_bytes(v.get(0..4)?.try_into().ok()?) as usize;
        let h = u32::from_le_bytes(v.get(4..8)?.try_into().ok()?) as usize;
        let body = v.get(8..)?;
        if body.len() != w * h * 16 {
            return None;
        }
        let mut p = Self::new(w, h);
        p.cells = bytemuck::pod_collect_to_vec(body);
        Some(p)
    }
}
