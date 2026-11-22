//! What the desktop app is doing: the open painting, the brush, the panels.
//! The UI (ui.rs) and the window's input (main.rs) both act through this.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use aqua_engine::brush::{BrushSettings, InputPoint, Tool};
use aqua_engine::engine::{Engine, View};
use aqua_engine::pigment::{self, Pigment};
use aqua_engine::storage;

use crate::store::{Artwork, Settings, Store, pictures_dir};
use crate::sync::Sync;
use crate::view::{Mat, place};

pub const MAX_BRUSH: f32 = 300.0;
/// The usual warm white paper and black paper, as sRGB (`PaperSpec::white` and `black`).
pub const PAPER_WHITE: [f32; 3] = [0.968, 0.961, 0.937];
pub const PAPER_BLACK: [f32; 3] = [0.11, 0.106, 0.102];
/// Saves this often while painting, as well as when leaving or closing.
const AUTOSAVE: Duration = Duration::from_secs(30);

pub fn size_to_rail(s: f32) -> f32 {
    s.clamp(1.0, MAX_BRUSH).ln() / MAX_BRUSH.ln()
}
pub fn rail_to_size(v: f32) -> f32 {
    MAX_BRUSH.powf(v).clamp(1.0, MAX_BRUSH)
}

/// One tool's brush settings, with the Android and web defaults.
#[derive(Clone, Copy, PartialEq)]
pub struct BrushP {
    pub size: f32,
    pub load: f32,
    pub water: f32,
    pub softness: f32,
    pub bristle: f32,
    pub bleed: f32,
    pub strength: f32,
    pub pressure_size: f32,
    pub pressure_flow: f32,
    pub smoothing: f32,
    pub splatter: f32,
    pub tilt: f32,
}

impl BrushP {
    pub fn new(size: f32) -> Self {
        Self { size, load: 0.6, water: 0.6, softness: 0.18, bristle: 0.5, bleed: 0.0, strength: 0.8, pressure_size: 0.6, pressure_flow: 0.5, smoothing: 0.3, splatter: 0.0, tilt: 0.7 }
    }
    fn array(&self, p: Pigment) -> [f32; 16] {
        [
            self.size, p[0], p[1], p[2], p[3], self.load, self.water, self.softness, self.bristle, self.bleed, self.strength,
            self.pressure_size, self.pressure_flow, self.smoothing, self.splatter, self.tilt,
        ]
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Gallery,
    Canvas,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    None,
    Layers,
    Palette,
    Brush,
    Picker,
    Paper,
}

pub enum Dialog {
    None,
    Sync { test: String },
    New { size: usize, paper: u32, black: bool },
    Delete(PathBuf),
}

pub struct Studio {
    pub engine: Engine,
    pub store: Store,
    pub settings: Settings,
    pub sync: Sync,
    pub screen: Screen,
    pub dir: Option<PathBuf>,
    /// Canvas to window pixels.
    pub view: Mat,
    /// The window in physical pixels, and pixels per egui point.
    pub window: (f32, f32),
    pub ppp: f32,
    /// None: the eyedropper.
    pub tool: Option<Tool>,
    pub brush_tool: Tool,
    pub params: [BrushP; 5],
    pub paint: Pigment,
    pub ink: Pigment,
    pub panel: Panel,
    pub hidden: bool,
    pub dialog: Dialog,
    saved_revision: u64,
    last_save: Instant,
    pub toast: Option<(String, Instant)>,
    pub gallery: Vec<Artwork>,
    pub thumbs: HashMap<PathBuf, (SystemTime, egui::TextureHandle)>,
    pub stroking: bool,
    /// The ⋮ menu is open.
    pub more_open: bool,
}

impl Studio {
    pub fn new(engine: Engine, store: Store, settings: Settings, sync: Sync) -> Self {
        let mut ink_brush = BrushP::new(3.0);
        ink_brush.load = 0.8;
        ink_brush.softness = 0.05;
        ink_brush.bristle = 0.0;
        ink_brush.pressure_size = 0.8;
        ink_brush.smoothing = 0.5;
        ink_brush.splatter = 0.35;
        ink_brush.tilt = 0.25;
        let mut water = BrushP::new(40.0);
        water.water = 0.8;
        water.softness = 0.3;
        let mut eraser = BrushP::new(40.0);
        eraser.softness = 0.4;
        eraser.bristle = 0.0;
        let mut push = BrushP::new(36.0);
        push.softness = 0.4;
        push.bristle = 0.0;
        let mut s = Self {
            engine,
            store,
            settings,
            sync,
            screen: Screen::Gallery,
            dir: None,
            view: Mat::default(),
            window: (1.0, 1.0),
            ppp: 1.0,
            tool: Some(Tool::Watercolor),
            brush_tool: Tool::Watercolor,
            params: [BrushP::new(28.0), water, ink_brush, eraser, push],
            paint: pigment::from_srgb(pigment::PRESETS[4].1),
            ink: pigment::from_srgb([0.067, 0.067, 0.078]),
            panel: Panel::None,
            hidden: false,
            dialog: Dialog::None,
            saved_revision: 0,
            last_save: Instant::now(),
            toast: None,
            gallery: Vec::new(),
            thumbs: HashMap::new(),
            stroking: false,
            more_open: false,
        };
        s.store.prune();
        s.reload_gallery();
        s
    }

    // ------------------------------------------------------------ brush

    pub fn bp(&self) -> BrushP {
        self.params[self.brush_tool as usize]
    }
    pub fn set_bp(&mut self, p: BrushP) {
        self.params[self.brush_tool as usize] = p;
        self.push_brush();
    }
    pub fn color(&self) -> Pigment {
        if self.brush_tool == Tool::Ink { self.ink } else { self.paint }
    }

    pub fn push_brush(&mut self) {
        let t = self.brush_tool;
        if let Some(b) = BrushSettings::from_params(t as u32, &self.params[t as usize].array(self.color())) {
            self.engine.brush = b;
        }
    }

    /// Tapping the selected tool again opens its panel.
    pub fn select_tool(&mut self, t: Option<Tool>) {
        if t == self.tool {
            let own = if t.is_none() { Panel::Picker } else { Panel::Brush };
            self.panel = if self.panel == own { Panel::None } else { own };
            return;
        }
        self.tool = t;
        if let Some(t) = t {
            self.brush_tool = t;
        }
        if matches!(self.panel, Panel::Brush | Panel::Picker) {
            self.panel = if t.is_none() { Panel::None } else { Panel::Brush };
        }
        self.push_brush();
    }

    pub fn set_color(&mut self, p: Pigment) {
        if self.brush_tool == Tool::Ink {
            self.ink = p;
        } else {
            self.paint = p;
        }
        if matches!(self.brush_tool, Tool::Eraser | Tool::Water | Tool::Push) {
            self.paint = p;
            self.brush_tool = Tool::Watercolor;
            self.tool = Some(Tool::Watercolor);
        }
        self.push_brush();
    }

    /// Brushes that run dry and are dipped in the cup, or are always wet.
    pub fn set_dip(&mut self, on: bool, tell: bool) {
        self.settings.dip = on;
        self.engine.set_charge(Tool::Watercolor as u32, if on { 1.0 } else { -1.0 });
        // Dipping is how the brush gets its water, and a dry brush pushes wet
        // paint by itself, so those two tools aren't needed.
        if on && matches!(self.tool, Some(Tool::Water | Tool::Push)) {
            self.select_tool(Some(Tool::Watercolor));
        }
        if tell {
            self.settings.save();
            self.say(if on { "Brush runs dry: dip it in the cup" } else { "Brush always wet" });
        }
    }

    /// Dips the pen in the ink well, or any other brush in the water cup.
    pub fn dip_brush(&mut self) {
        let t = if self.brush_tool == Tool::Ink { Tool::Ink } else { Tool::Watercolor };
        self.engine.set_charge(t as u32, 1.0);
    }

    pub fn charge(&self) -> f32 {
        if self.brush_tool == Tool::Ink { self.engine.ink_charge } else { self.engine.charge.unwrap_or(1.0) }
    }

    pub fn say(&mut self, s: &str) {
        self.toast = Some((s.to_string(), Instant::now()));
    }

    // ------------------------------------------------------------ view

    pub fn set_window(&mut self, w: u32, h: u32, ppp: f32) {
        let was = self.window;
        self.window = (w.max(1) as f32, h.max(1) as f32);
        self.ppp = ppp;
        self.engine.resize(w, h);
        // Keep the painting where it was relative to the middle of the window.
        self.view.translate((self.window.0 - was.0) / 2.0, (self.window.1 - was.1) / 2.0);
        self.push_view();
    }

    pub fn fit(&mut self) {
        let Some(c) = self.engine.canvas.as_ref() else { return };
        let (cw, ch) = (c.w as f32, c.h as f32);
        let q = c.home % 4;
        let (w, h) = if q % 2 == 1 { (ch, cw) } else { (cw, ch) };
        let (ww, wh) = self.window;
        let s = (ww * 0.92 / w).min(wh * 0.84 / h);
        self.view = place(cw, ch, ww / 2.0, wh / 2.0, s, 90.0 * q as f32);
        self.push_view();
    }

    pub fn push_view(&mut self) {
        let inv = self.view.invert();
        self.engine.set_view(View { m: inv.0, scale: self.view.scale() });
    }

    pub fn zoom_at(&mut self, k: f32, x: f32, y: f32) {
        let s = self.view.scale();
        let k = (s * k).clamp(0.02, 64.0) / s;
        self.view.zoom_at(k, x, y);
        self.push_view();
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.view.translate(dx, dy);
        self.push_view();
    }

    /// A pointer at window pixels (x, y) as a stroke point.
    pub fn point(&self, x: f32, y: f32, pressure: f32) -> InputPoint {
        let (cx, cy) = self.view.invert().map(x, y);
        InputPoint { x: cx, y: cy, pressure, tilt: [0.0, 0.0] }
    }

    // ------------------------------------------------------------ painting

    pub fn stroke(&mut self, phase: i32, p: InputPoint) {
        self.engine.stroke(phase, &[p]);
        self.stroking = phase < 2;
    }

    pub fn undo(&mut self) {
        if let Some(c) = self.engine.canvas.as_mut() {
            c.undo();
            self.engine.needs_present = true;
        }
    }
    pub fn redo(&mut self) {
        if let Some(c) = self.engine.canvas.as_mut() {
            c.redo();
            self.engine.needs_present = true;
        }
    }

    /// The eyedropper at window pixels (x, y).
    pub fn sample(&mut self, x: f32, y: f32) {
        let p = self.point(x, y, 0.0);
        let wash = self.settings.pick_wash;
        let Some(c) = self.engine.canvas.as_mut() else { return };
        let picked = if wash {
            storage::begin_sample_pigment(c, p.x, p.y).and_then(|r| pollster::block_on(r.finish()))
        } else {
            storage::begin_sample(c, p.x, p.y)
                .and_then(|r| storage::texel_srgb(&pollster::block_on(r.read())))
                .map(pigment::from_srgb)
        };
        if let Some(pg) = picked {
            self.set_color(pg);
        }
    }

    // ------------------------------------------------------------ files

    pub fn reload_gallery(&mut self) {
        self.gallery = self.store.list();
    }

    pub fn open(&mut self, dir: PathBuf) {
        match storage::load(self.engine.gpu.clone(), self.engine.pipes.clone(), &dir) {
            Ok(c) => {
                self.saved_revision = c.revision;
                self.engine.canvas = Some(c);
                self.engine.needs_present = true;
                self.enter(dir);
            }
            Err(e) => self.say(&format!("Couldn't open it: {e}")),
        }
    }

    pub fn create(&mut self, w: u32, h: u32, paper: u32, black: bool) {
        let seed = 1 + (SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().subsec_nanos() % 0x7fff_fffe);
        self.engine.new_canvas(w, h, paper, seed, 0);
        if black {
            self.engine.set_paper_color(PAPER_BLACK);
        }
        let dir = self.store.new_dir();
        self.enter(dir);
        self.save(false);
    }

    fn enter(&mut self, dir: PathBuf) {
        self.dir = Some(dir);
        self.screen = Screen::Canvas;
        self.panel = Panel::None;
        self.hidden = false;
        self.last_save = Instant::now();
        if self.settings.dip {
            self.engine.set_charge(Tool::Watercolor as u32, 1.0);
        } else {
            self.engine.set_charge(Tool::Watercolor as u32, -1.0);
        }
        self.push_brush();
        self.fit();
    }

    /// Saves, and pushes the save to the sync daemons when sync is on (or `force_sync`).
    pub fn save(&mut self, force_sync: bool) {
        let (Some(dir), Some(c)) = (self.dir.clone(), self.engine.canvas.as_mut()) else { return };
        let rev = c.revision;
        match storage::save(c, &dir) {
            Ok(()) => {
                self.saved_revision = rev;
                self.last_save = Instant::now();
                self.thumbs.remove(&dir);
                self.sync.push(&dir, &self.settings.sync, force_sync);
            }
            Err(e) => self.say(&format!("Save failed: {e}")),
        }
    }

    /// Saves now and then if anything changed.
    pub fn autosave(&mut self) {
        let changed = self.engine.canvas.as_ref().is_some_and(|c| c.revision != self.saved_revision);
        if changed && !self.stroking && self.last_save.elapsed() >= AUTOSAVE {
            self.save(false);
        }
    }

    pub fn leave(&mut self) {
        if self.engine.canvas.as_ref().is_some_and(|c| c.revision != self.saved_revision) {
            self.save(false);
        }
        self.engine.canvas = None;
        self.engine.needs_present = true;
        self.dir = None;
        self.screen = Screen::Gallery;
        self.panel = Panel::None;
        self.hidden = false;
        self.reload_gallery();
    }

    pub fn export(&mut self) {
        let Some(c) = self.engine.canvas.as_mut() else { return };
        let t = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_millis();
        let path = pictures_dir().join(format!("Aqua_{t}.png"));
        match storage::export_png(c, &path) {
            Ok(()) => self.say(&format!("Saved to {}", path.display())),
            Err(e) => self.say(&format!("Export failed: {e}")),
        }
    }
}
