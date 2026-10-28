//! Browser bindings for the Aqua engine: the same simulation and renderer as the
//! Android app, on WebGPU. Mirrors the Android JNI surface (`Native.kt`), except
//! that the page drives frames from `requestAnimationFrame` instead of an engine
//! thread, and anything that reads back from the GPU returns a promise.

use std::cell::RefCell;
use std::rc::Rc;

use aqua_engine::brush::{BrushSettings, InputPoint};
use aqua_engine::engine::{Engine, View};
use aqua_engine::gpu::Gpu;
use aqua_engine::palette::Palette;
use aqua_engine::{pigment, storage};
use js_sys::{Array, Float32Array, Object, Promise, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::future_to_promise;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
}

/// The engine bound to one `<canvas>`.
#[wasm_bindgen]
pub struct AquaEngine {
    engine: Rc<RefCell<Engine>>,
}

#[wasm_bindgen]
impl AquaEngine {
    /// Opens WebGPU and attaches to `canvas` (sized in device pixels).
    pub async fn create(canvas: HtmlCanvasElement) -> Result<AquaEngine, JsValue> {
        let (w, h) = (canvas.width(), canvas.height());
        let instance = Gpu::new_instance();
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| JsValue::from_str(&format!("WebGPU surface: {e}")))?;
        let gpu = Gpu::request(instance, Some(&surface)).await.map_err(|e| JsValue::from_str(&e))?;
        let mut engine = Engine::new(gpu);
        engine.attach_surface(surface, w, h);
        Ok(AquaEngine { engine: Rc::new(RefCell::new(engine)) })
    }

    pub fn resize(&self, w: u32, h: u32) {
        self.engine.borrow_mut().resize(w, h);
    }

    /// canvasFromScreen affine (a, b, c, d, e, f) followed by the zoom factor.
    pub fn set_view(&self, v: &[f32]) {
        if v.len() >= 7 {
            self.engine.borrow_mut().set_view(View { m: [v[0], v[1], v[2], v[3], v[4], v[5]], scale: v[6] });
        }
    }

    /// Tips the paper: (x, y) is the downhill direction in canvas axes, length 0..1.
    pub fn set_tilt(&self, x: f32, y: f32) {
        if let Some(c) = self.engine.borrow_mut().canvas.as_mut() {
            c.set_tilt(x, y);
        }
    }

    /// See `BrushSettings::from_params` for the layout.
    pub fn set_brush(&self, tool: u32, params: &[f32]) {
        if let Some(b) = BrushSettings::from_params(tool, params) {
            self.engine.borrow_mut().brush = b;
        }
    }

    /// phase: 0 begin, 1 move, 2 end, 3 cancel; points are (x, y, pressure) in canvas pixels.
    pub fn stroke(&self, phase: i32, pts: &[f32]) {
        let p: Vec<InputPoint> = pts.as_chunks::<3>().0.iter().map(|c| InputPoint { x: c[0], y: c[1], pressure: c[2] }).collect();
        self.engine.borrow_mut().stroke(phase, &p);
    }

    pub fn new_canvas(&self, w: u32, h: u32, paper: u32, seed: u32, home: u32) {
        self.engine.borrow_mut().new_canvas(w, h, paper, seed, home);
    }

    /// Opens a painting from its files: an object mapping file names to `Uint8Array`s.
    pub fn load(&self, files: &Object) -> Result<(), JsValue> {
        let read = |name: &str| Reflect::get(files, &JsValue::from_str(name)).ok().filter(|v| !v.is_undefined()).map(|v| Uint8Array::new(&v).to_vec());
        let mut e = self.engine.borrow_mut();
        let c = storage::load_files(e.gpu.clone(), e.pipes.clone(), read).map_err(|e| JsValue::from_str(&e))?;
        e.canvas = Some(c);
        e.needs_present = true;
        Ok(())
    }

    /// Resolves to the painting's files (name -> `Uint8Array`), the same set the Android app saves.
    pub fn save(&self) -> Promise {
        let pending = self.engine.borrow_mut().canvas.as_mut().map(storage::begin_save);
        future_to_promise(async move {
            let files = pending.ok_or("no painting open")?.finish().await.map_err(|e| JsValue::from_str(&e))?;
            let out = Object::new();
            for (name, data) in files {
                Reflect::set(&out, &JsValue::from_str(&name), &Uint8Array::from(data.as_slice()))?;
            }
            Ok(out.into())
        })
    }

    /// Resolves to the full-size painting as PNG bytes.
    pub fn export_png(&self) -> Promise {
        let pending = self.engine.borrow_mut().canvas.as_mut().map(|c| storage::begin_composite(c, u32::MAX));
        future_to_promise(async move {
            let img = pending.ok_or("no painting open")?.finish().await;
            let png = storage::encode_png(&img).map_err(|e| JsValue::from_str(&e))?;
            Ok(Uint8Array::from(png.as_slice()).into())
        })
    }

    /// Resolves to the sRGB colour (0..1) under a canvas point, or undefined off the canvas.
    pub fn sample(&self, x: f32, y: f32) -> Promise {
        let pending = self.engine.borrow_mut().canvas.as_mut().and_then(|c| storage::begin_sample(c, x, y));
        future_to_promise(async move {
            let Some(r) = pending else { return Ok(JsValue::UNDEFINED) };
            Ok(storage::texel_srgb(&r.read().await).map_or(JsValue::UNDEFINED, |c| Float32Array::from(c.as_slice()).into()))
        })
    }

    /// op: 0 add, 1 remove, 2 move to value, 3 select, 4 opacity, 5 visible,
    /// 6 rename, 7 dry, 8 clear, 9 drying speed, 10 flood flow
    pub fn layer_op(&self, op: i32, index: i32, value: f32, name: Option<String>) {
        let mut e = self.engine.borrow_mut();
        let Some(c) = e.canvas.as_mut() else { return };
        let i = index.max(0) as usize;
        match op {
            0 => drop(c.add_layer()),
            1 => c.remove_layer(i),
            2 => c.move_layer(i, value as usize),
            3 => c.set_active(i),
            4 => {
                if let Some(v) = c.layers.get(i).map(|l| l.visible) {
                    c.set_layer_props(i, value, v, None);
                }
            }
            5 => {
                if let Some(o) = c.layers.get(i).map(|l| l.opacity) {
                    c.set_layer_props(i, o, value > 0.5, None);
                }
            }
            6 => {
                if let Some((o, v)) = c.layers.get(i).map(|l| (l.opacity, l.visible)) {
                    c.set_layer_props(i, o, v, name);
                }
            }
            7 => c.dry_layer(i),
            8 => c.clear_layer(i),
            9 => c.set_layer_drying(i, value),
            10 => c.set_layer_flow(i, value),
            _ => {}
        }
        e.needs_present = true;
    }

    pub fn undo(&self) {
        let mut e = self.engine.borrow_mut();
        if let Some(c) = e.canvas.as_mut() {
            c.undo();
            e.needs_present = true;
        }
    }

    pub fn redo(&self) {
        let mut e = self.engine.borrow_mut();
        if let Some(c) = e.canvas.as_mut() {
            c.redo();
            e.needs_present = true;
        }
    }

    /// The painting's state as JSON, in the same shape the Android app reads.
    pub fn query(&self) -> String {
        let e = self.engine.borrow();
        match e.canvas.as_ref() {
            Some(c) => serde_json::json!({
                "width": c.w,
                "height": c.h,
                "home": c.home,
                "active": c.active,
                "canUndo": c.can_undo(),
                "canRedo": c.can_redo(),
                "revision": c.revision,
                "layers": c.layer_infos(),
            })
            .to_string(),
            None => "{}".into(),
        }
    }

    pub fn close(&self) {
        let mut e = self.engine.borrow_mut();
        e.canvas = None;
        e.needs_present = true;
    }

    /// Steps the simulation and presents. Returns true if another frame is wanted.
    pub fn frame(&self) -> bool {
        self.engine.borrow_mut().frame()
    }

    /// Milliseconds to wait before the next frame: 0 while interactive, longer
    /// while paint is only drying.
    pub fn frame_delay_ms(&self) -> f64 {
        self.engine.borrow().frame_delay().as_secs_f64() * 1000.0
    }

    pub fn wants_frame(&self) -> bool {
        self.engine.borrow().wants_frame()
    }
}

/// sRGB 0..1 to a Kubelka-Munk pigment (k r, g, b, s).
#[wasm_bindgen]
pub fn pigment_from_rgb(r: f32, g: f32, b: f32) -> Vec<f32> {
    pigment::from_srgb([r, g, b]).to_vec()
}

/// A pigment's masstone as sRGB 0..1.
#[wasm_bindgen]
pub fn pigment_to_rgb(p: &[f32]) -> Vec<f32> {
    if p.len() < 4 {
        return vec![0.0; 3];
    }
    pigment::to_srgb([p[0], p[1], p[2], p[3]]).to_vec()
}

/// The preset pigments as (name, sRGB) pairs.
#[wasm_bindgen]
pub fn preset_pigments() -> Array {
    pigment::PRESETS
        .iter()
        .map(|(name, rgb)| {
            let a = Array::new();
            a.push(&JsValue::from_str(name));
            a.push(&Float32Array::from(rgb.as_slice()));
            JsValue::from(a)
        })
        .collect()
}

/// The CPU mixing tray.
#[wasm_bindgen]
pub struct AquaPalette {
    p: Palette,
}

#[wasm_bindgen]
impl AquaPalette {
    #[wasm_bindgen(constructor)]
    pub fn new(w: usize, h: usize) -> AquaPalette {
        AquaPalette { p: Palette::new(w.max(1), h.max(1)) }
    }

    pub fn dab(&mut self, x: f32, y: f32, r: f32, pig: &[f32], amount: f32) {
        if pig.len() >= 4 {
            self.p.dab(x, y, r, [pig[0], pig[1], pig[2], pig[3]], amount);
        }
    }

    pub fn smudge_begin(&mut self, x: f32, y: f32, r: f32) {
        self.p.smudge_begin(x, y, r);
    }

    pub fn smudge(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, strength: f32) {
        self.p.smudge(x0, y0, x1, y1, strength);
    }

    pub fn smudge_end(&mut self) {
        self.p.smudge_end();
    }

    pub fn pick(&self, x: f32, y: f32, r: f32) -> Option<Vec<f32>> {
        self.p.pick(x, y, r).map(|p| p.to_vec())
    }

    /// RGBA bytes for an `ImageData`.
    pub fn render(&self) -> Vec<u8> {
        let mut argb = vec![0i32; self.p.w * self.p.h];
        self.p.render(&mut argb);
        argb.iter().flat_map(|&c| {
            let c = c as u32;
            [(c >> 16) as u8, (c >> 8) as u8, c as u8, (c >> 24) as u8]
        })
        .collect()
    }

    pub fn clear(&mut self) {
        self.p.clear();
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.p.to_bytes()
    }

    /// Replaces the tray from saved bytes; false (keeping it) if they don't fit.
    pub fn load(&mut self, b: &[u8]) -> bool {
        match Palette::from_bytes(b) {
            Some(l) if l.w == self.p.w && l.h == self.p.h => {
                self.p = l;
                true
            }
            _ => false,
        }
    }
}
