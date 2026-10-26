//! The painting: layers, the wet simulation, dabs, compositing and undo.
//!
//! Every layer owns two textures holding pigment as Kubelka-Munk (K rgb, S):
//! `dep` (settled watercolour, can be re-lifted by water) and `fix` (permanent
//! ink). While a layer is wet it also owns a [`WetState`] with surface water,
//! paper saturation, pipe fluxes and suspended pigment / ink. The simulation
//! only runs inside the wet state's `rect`, which grows with the strokes and is
//! released once the layer has dried.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use bytemuck::{Pod, Zeroable};

use crate::brush::{BrushSettings, Dab, DabKind, InputPoint, Stroke};
use crate::gpu::{IRect, Gpu, Tex, UniformRing, copy_full, copy_rect};
use crate::pipelines::Pipelines;
use crate::undo::{self, Entry, StrokeEntry, TileImage, UndoStack};

/// Simulation steps per second of wall time.
pub const STEPS_PER_SECOND: f32 = 120.0;
/// How far beyond a stroke water and blooms may travel, in pixels.
const WET_MARGIN: i32 = 64;
const MAX_LAYERS: usize = 24;
/// Below this maximum wetness (4 * surface water + paper saturation) paper counts as dry.
const DRY: f32 = 0.02;
/// Dry tiles stay simulated while wet paint is within this many tiles, so water
/// spreading between two probes never runs into a retired tile.
const RETIRE_DISTANCE: i32 = 2;
/// Simulation steps between wetness probes.
const PROBE_STEPS: u32 = 20;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SimParams {
    origin: [i32; 2],
    size: [i32; 2],
    canvas: [i32; 2],
    dt: f32,
    gravity: f32,
    damping: f32,
    paper_influence: f32,
    tension: f32,
    absorb: f32,
    capillary: f32,
    cap_threshold: f32,
    evap_water: f32,
    evap_paper: f32,
    edge_evap: f32,
    deposit: f32,
    lift: f32,
    granulation: f32,
    bloom: f32,
    diffuse: f32,
    ink_bleed: f32,
    ink_fix: f32,
    staining: f32,
    edge_drift: f32,
    _pad: [f32; 2],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LayerU {
    origin: [i32; 2],
    size: [i32; 2],
    opacity: f32,
    wet: u32,
    first: u32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PaperParams {
    size: [i32; 2],
    seed: u32,
    roughness: f32,
    grain: f32,
    fibre: f32,
    sizing: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DabGlobals {
    canvas: [f32; 2],
    _pad: [f32; 2],
}

/// Physical constants of the simulation, per step.
#[derive(Clone, Copy, Debug)]
pub struct Tuning {
    pub gravity: f32,
    pub damping: f32,
    pub paper_influence: f32,
    pub tension: f32,
    pub absorb: f32,
    pub capillary: f32,
    pub cap_threshold: f32,
    pub evap_water: f32,
    pub evap_paper: f32,
    pub edge_evap: f32,
    pub deposit: f32,
    pub lift: f32,
    pub granulation: f32,
    pub bloom: f32,
    pub diffuse: f32,
    pub ink_bleed: f32,
    pub ink_fix: f32,
    pub staining: f32,
    pub edge_drift: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            gravity: 0.25,
            damping: 0.7,
            paper_influence: 0.12,
            tension: 0.35,
            absorb: 0.006,
            capillary: 0.025,
            cap_threshold: 0.3,
            evap_water: 0.0012,
            evap_paper: 0.0012,
            edge_evap: 4.0,
            deposit: 0.02,
            lift: 0.006,
            granulation: 0.6,
            bloom: 0.05,
            diffuse: 0.03,
            ink_bleed: 0.04,
            ink_fix: 0.01,
            staining: 0.2,
            edge_drift: 0.8,
        }
    }
}

/// Paper look and feel, chosen when a painting is created.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct PaperSpec {
    pub seed: u32,
    pub roughness: f32,
    pub grain: f32,
    pub fibre: f32,
    pub sizing: f32,
}

impl PaperSpec {
    /// 0 = cold press, 1 = hot press, 2 = rough
    pub fn preset(kind: u32, seed: u32) -> Self {
        match kind {
            1 => Self { seed, roughness: 0.15, grain: 3.0, fibre: 0.35, sizing: 0.2 },
            2 => Self { seed, roughness: 0.9, grain: 7.0, fibre: 0.6, sizing: 0.45 },
            _ => Self { seed, roughness: 0.5, grain: 4.5, fibre: 0.5, sizing: 0.3 },
        }
    }
}

/// Side of a simulation tile in pixels (must match TILE in common.wgsl).
pub const SIM_TILE: u32 = 32;

/// The set of tiles a wet layer simulates, mirrored to the GPU as a list (for
/// dispatch) and a mask texture (for neighbour lookups).
struct TileSet {
    tw: u32,
    th: u32,
    active: Vec<bool>,
    list: Vec<u32>,
    buf: wgpu::Buffer,
    mask: Tex,
    dirty: bool,
}

impl TileSet {
    fn new(device: &wgpu::Device, w: u32, h: u32) -> Self {
        let (tw, th) = (w.div_ceil(SIM_TILE), h.div_ceil(SIM_TILE));
        Self {
            tw,
            th,
            active: vec![false; (tw * th) as usize],
            list: Vec::new(),
            buf: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("sim tiles"),
                size: (tw * th * 4).max(4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            mask: Tex::new(device, tw, th, wgpu::TextureFormat::Rgba8Unorm, "tile mask"),
            dirty: false,
        }
    }

    /// Grid indices of the tiles overlapping `r`.
    fn indices(&self, r: &IRect) -> impl Iterator<Item = (u32, u32)> + use<> {
        let r = r.intersect(&IRect::full(self.tw * SIM_TILE, self.th * SIM_TILE));
        let (x0, y0, x1, y1) = if r.is_empty() {
            (0, 0, 0, 0)
        } else {
            (r.x0 as u32 / SIM_TILE, r.y0 as u32 / SIM_TILE, (r.x1 as u32).div_ceil(SIM_TILE).min(self.tw), (r.y1 as u32).div_ceil(SIM_TILE).min(self.th))
        };
        (y0..y1).flat_map(move |ty| (x0..x1).map(move |tx| (tx, ty)))
    }

    fn activate(&mut self, r: &IRect) {
        for (tx, ty) in self.indices(r) {
            self.activate_tile(tx, ty);
        }
    }

    fn activate_tile(&mut self, tx: u32, ty: u32) {
        let i = (ty * self.tw + tx) as usize;
        if !self.active[i] {
            self.active[i] = true;
            self.list.push(tx | (ty << 16));
            self.dirty = true;
        }
    }

    /// Unmarks the given tiles (packed `tx | ty << 16`, as in `list`).
    fn remove(&mut self, tiles: &[u32]) {
        for &t in tiles {
            self.active[((t >> 16) * self.tw + (t & 0xffff)) as usize] = false;
        }
        let active = &self.active;
        let tw = self.tw;
        self.list.retain(|&t| active[((t >> 16) * tw + (t & 0xffff)) as usize]);
        self.dirty = true;
    }

    /// Unmarks everything (used for the per-frame dirty set).
    fn clear(&mut self) {
        for &t in &self.list {
            self.active[((t >> 16) * self.tw + (t & 0xffff)) as usize] = false;
        }
        self.list.clear();
        self.dirty = true;
    }

    fn extend(&mut self, other: &TileSet) {
        for &t in &other.list {
            let i = ((t >> 16) * self.tw + (t & 0xffff)) as usize;
            if !self.active[i] {
                self.active[i] = true;
                self.list.push(t);
                self.dirty = true;
            }
        }
    }

    fn flush(&mut self, gpu: &Gpu) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        gpu.queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&self.list));
        let px: Vec<u8> = self.active.iter().flat_map(|&a| if a { [255u8; 4] } else { [0u8; 4] }).collect();
        crate::gpu::write_texture(gpu, &self.mask.tex, &px);
    }
}

pub struct WetState {
    /// Bounding box of every tile this wet layer may simulate (`reach`).
    pub rect: IRect,
    /// Tiles simulated right now: the part of `reach` near wet paint. Outside them
    /// the wet textures are zero and each ping-pong pair holds the same image.
    tiles: TileSet,
    /// Every tile painted into (plus margin) since the layer got wet. Retired
    /// tiles may come back from here if water spreads towards them again.
    reach: Vec<bool>,
    /// Frame each tile was last activated in, so a probe never retires a tile
    /// painted into after the probe was taken.
    stamp: Vec<u32>,
    frame: u32,
    probe_frame: u32,
    steps_since_probe: u32,
    pub water: Tex,
    water_alt: Tex,
    flux: Tex,
    flux_alt: Tex,
    pub pig: Tex,
    pig_alt: Tex,
    pub ink: Tex,
    ink_alt: Tex,
    dep_alt: Tex,
    fix_alt: Tex,
    probe: Probe,
    /// Frames since paint last touched this layer.
    idle_frames: u32,
    pub wetness: f32,
}

impl WetState {
    fn new(gpu: &Gpu, layer: &Layer, enc: &mut wgpu::CommandEncoder) -> Self {
        let d = &gpu.device;
        let (w, h) = (layer.dep.width(), layer.dep.height());
        let tiles = TileSet::new(d, w, h);
        let n = (tiles.tw * tiles.th) as usize;
        let s = Self {
            rect: IRect::EMPTY,
            reach: vec![false; n],
            stamp: vec![0; n],
            frame: 0,
            probe_frame: 0,
            steps_since_probe: 0,
            probe: Probe::new(&gpu.device, n),
            tiles,
            water: Tex::sim(d, w, h, "water"),
            water_alt: Tex::sim(d, w, h, "water'"),
            flux: Tex::sim(d, w, h, "flux"),
            flux_alt: Tex::sim(d, w, h, "flux'"),
            pig: Tex::sim(d, w, h, "pig"),
            pig_alt: Tex::sim(d, w, h, "pig'"),
            ink: Tex::sim(d, w, h, "ink"),
            ink_alt: Tex::sim(d, w, h, "ink'"),
            dep_alt: Tex::sim(d, w, h, "dep'"),
            fix_alt: Tex::sim(d, w, h, "fix'"),
            idle_frames: 0,
            wetness: 1.0,
        };
        copy_full(enc, &layer.dep, &s.dep_alt);
        copy_full(enc, &layer.fix, &s.fix_alt);
        s
    }

    fn bytes(&self) -> u64 {
        self.water.bytes() * 10
    }

    /// Marks `r` as simulated. Anything written into the layer outside the
    /// simulation must be activated, so the ping-pong copies get resynchronised.
    pub fn activate(&mut self, r: &IRect, full: &IRect) {
        let r = r.intersect(full);
        if r.is_empty() {
            return;
        }
        self.rect = self.rect.union(&r);
        let indices: Vec<_> = self.tiles.indices(&r).collect();
        for (tx, ty) in indices {
            let i = (ty * self.tiles.tw + tx) as usize;
            self.reach[i] = true;
            self.stamp[i] = self.frame;
            self.tiles.activate_tile(tx, ty);
        }
    }

    /// Decides from a per-tile wetness probe which tiles to simulate: the reach
    /// within `RETIRE_DISTANCE` of wet tiles, plus anything painted since the probe.
    /// Returns (tiles to retire, tiles to bring back).
    fn plan_tiles(&self, wet_tiles: &[f32]) -> (Vec<u32>, Vec<u32>) {
        let (tw, th) = (self.tiles.tw as i32, self.tiles.th as i32);
        let mut near = vec![false; wet_tiles.len()];
        for ty in 0..th {
            for tx in 0..tw {
                if wet_tiles[(ty * tw + tx) as usize] < DRY {
                    continue;
                }
                for y in (ty - RETIRE_DISTANCE).max(0)..(ty + RETIRE_DISTANCE + 1).min(th) {
                    for x in (tx - RETIRE_DISTANCE).max(0)..(tx + RETIRE_DISTANCE + 1).min(tw) {
                        near[(y * tw + x) as usize] = true;
                    }
                }
            }
        }
        let (mut retire, mut revive) = (Vec::new(), Vec::new());
        for (i, &near) in near.iter().enumerate() {
            let t = (i as u32 % self.tiles.tw) | ((i as u32 / self.tiles.tw) << 16);
            if self.tiles.active[i] && !near && self.stamp[i] < self.probe_frame {
                retire.push(t);
            } else if !self.tiles.active[i] && near && self.reach[i] {
                revive.push(t);
            }
        }
        (retire, revive)
    }
}

/// Asynchronous read-back of a layer's maximum wetness.
struct Probe {
    storage: wgpu::Buffer,
    readback: wgpu::Buffer,
    state: Arc<AtomicU8>,
}

const PROBE_IDLE: u8 = 0;
const PROBE_PENDING: u8 = 1;
const PROBE_READY: u8 = 2;
const PROBE_MAPPING: u8 = 3;

impl Probe {
    fn new(device: &wgpu::Device, tiles: usize) -> Self {
        let size = (tiles * 4) as u64;
        Self {
            storage: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wet probe"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            readback: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("wet probe readback"),
                size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            state: Arc::new(AtomicU8::new(PROBE_IDLE)),
        }
    }
}

pub struct Layer {
    pub id: u32,
    pub name: String,
    pub opacity: f32,
    pub visible: bool,
    /// Evaporation multiplier for this layer.
    pub drying: f32,
    pub dep: Tex,
    pub fix: Tex,
    pub wet: Option<WetState>,
}

impl Layer {
    fn new(device: &wgpu::Device, w: u32, h: u32, id: u32, name: String) -> Self {
        Self { id, name, opacity: 1.0, visible: true, drying: 1.0, dep: Tex::sim(device, w, h, "dep"), fix: Tex::sim(device, w, h, "fix"), wet: None }
    }

    pub fn bytes(&self) -> u64 {
        self.dep.bytes() * 2 + self.wet.as_ref().map_or(0, |w| w.bytes())
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct LayerInfo {
    pub id: u32,
    pub name: String,
    pub opacity: f32,
    pub visible: bool,
    pub wet: bool,
    pub wetness: f32,
    pub drying: f32,
}

pub struct Canvas {
    gpu: Arc<Gpu>,
    pipes: Arc<Pipelines>,
    pub w: u32,
    pub h: u32,
    pub paper_spec: PaperSpec,
    pub paper: Tex,
    pub layers: Vec<Layer>,
    pub active: usize,
    next_id: u32,
    pub tuning: Tuning,
    below: [Tex; 2],
    below_idx: usize,
    above_r: [Tex; 2],
    above_t: [Tex; 2],
    above_idx: Option<usize>,
    pub composite: Tex,
    /// Tiles whose composite must be refreshed this frame.
    dirty_tiles: TileSet,
    all_tiles: TileSet,
    zero: Tex,
    zero_tile: Tex,
    structure_dirty: bool,
    /// Area whose composite must be refreshed even if nothing is wet (e.g. after undo).
    pending_dirty: IRect,
    ring: UniformRing,
    dab_buf: wgpu::Buffer,
    pending: Vec<(DabKind, Dab)>,
    stroke: Option<Stroke>,
    stroke_seed: u32,
    pub undo: UndoStack,
    /// Bumped whenever pixels change; the UI uses it to know when to save.
    pub revision: u64,
}

impl Canvas {
    pub fn new(gpu: Arc<Gpu>, pipes: Arc<Pipelines>, w: u32, h: u32, paper_spec: PaperSpec) -> Self {
        let d = &gpu.device;
        let mut c = Self {
            paper: Tex::new(d, w, h, wgpu::TextureFormat::Rgba8Unorm, "paper"),
            layers: Vec::new(),
            active: 0,
            next_id: 1,
            tuning: Tuning::default(),
            below: [Tex::sim(d, w, h, "below0"), Tex::sim(d, w, h, "below1")],
            below_idx: 0,
            above_r: [Tex::sim(d, w, h, "above_r0"), Tex::sim(d, w, h, "above_r1")],
            above_t: [Tex::sim(d, w, h, "above_t0"), Tex::sim(d, w, h, "above_t1")],
            above_idx: None,
            composite: Tex::sim(d, w, h, "composite"),
            dirty_tiles: TileSet::new(d, w, h),
            all_tiles: {
                let mut t = TileSet::new(d, w, h);
                t.activate(&IRect::full(w, h));
                t
            },
            zero: Tex::sim(d, 1, 1, "zero"),
            zero_tile: Tex::sim(d, undo::TILE, undo::TILE, "zero tile"),
            structure_dirty: true,
            pending_dirty: IRect::EMPTY,
            ring: UniformRing::new(d),
            dab_buf: d.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dabs"),
                size: 4096 * size_of::<Dab>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            pending: Vec::new(),
            stroke: None,
            stroke_seed: 1,
            undo: UndoStack::new(Self::undo_budget(w, h)),
            revision: 0,
            w,
            h,
            paper_spec,
            gpu,
            pipes,
        };
        c.generate_paper();
        c.push_layer_raw(None);
        c
    }

    fn undo_budget(w: u32, h: u32) -> u64 {
        // Enough for a few dozen typical strokes, never less than ~4 full-layer snapshots.
        let layer = w as u64 * h as u64 * 8 * 5;
        (layer * 4).max(512 << 20)
    }

    pub fn gpu(&self) -> &Arc<Gpu> {
        &self.gpu
    }

    fn generate_paper(&mut self) {
        let p = PaperParams {
            size: [self.w as i32, self.h as i32],
            seed: self.paper_spec.seed,
            roughness: self.paper_spec.roughness,
            grain: self.paper_spec.grain,
            fibre: self.paper_spec.fibre,
            sizing: self.paper_spec.sizing,
            _pad: 0.0,
        };
        let u = self.ring.push(&p);
        let bg = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipes.paper_gen.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.ring.res(u) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&self.paper.view) },
            ],
        });
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        {
            let mut pass = enc.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipes.paper_gen);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(self.w.div_ceil(8), self.h.div_ceil(8), 1);
        }
        self.ring.flush(&self.gpu.queue);
        self.gpu.queue.submit([enc.finish()]);
        self.structure_dirty = true;
    }

    // ------------------------------------------------------------------ layers

    fn push_layer_raw(&mut self, index: Option<usize>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let layer = Layer::new(&self.gpu.device, self.w, self.h, id, format!("Layer {id}"));
        let i = index.unwrap_or(self.layers.len()).min(self.layers.len());
        self.layers.insert(i, layer);
        self.active = i;
        self.structure_dirty = true;
        i
    }

    /// Adds an empty layer above the active one.
    pub fn add_layer(&mut self) -> bool {
        if self.layers.len() >= MAX_LAYERS {
            return false;
        }
        let at = if self.layers.is_empty() { 0 } else { self.active + 1 };
        let i = self.push_layer_raw(Some(at));
        let id = self.layers[i].id;
        self.undo.push(Entry::LayerAdded { index: i, id });
        true
    }

    pub fn remove_layer(&mut self, index: usize) {
        if self.layers.len() <= 1 || index >= self.layers.len() {
            return;
        }
        let layer = self.layers.remove(index);
        self.active = self.active.min(self.layers.len() - 1);
        self.structure_dirty = true;
        self.undo.push(Entry::LayerRemoved { index, layer: Box::new(layer) });
    }

    pub fn move_layer(&mut self, from: usize, to: usize) {
        if from >= self.layers.len() || to >= self.layers.len() || from == to {
            return;
        }
        let l = self.layers.remove(from);
        self.layers.insert(to, l);
        self.active = to;
        self.structure_dirty = true;
        self.undo.push(Entry::LayerMoved { from, to });
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.layers.len() && index != self.active {
            self.active = index;
            self.structure_dirty = true;
        }
    }

    pub fn set_layer_props(&mut self, index: usize, opacity: f32, visible: bool, name: Option<String>) {
        let Some(l) = self.layers.get_mut(index) else { return };
        let old = Entry::LayerProps { id: l.id, opacity: l.opacity, visible: l.visible, name: l.name.clone() };
        // Dragging the opacity slider shouldn't flood the stack: coalesce with the last props entry.
        let coalesce = matches!(self.undo.undo.last(), Some(Entry::LayerProps { id, .. }) if *id == l.id) && name.is_none();
        l.opacity = opacity.clamp(0.0, 1.0);
        l.visible = visible;
        if let Some(n) = name {
            l.name = n;
        }
        if !coalesce {
            self.undo.push(old);
        }
        self.structure_dirty = true;
    }

    pub fn set_layer_drying(&mut self, index: usize, drying: f32) {
        if let Some(l) = self.layers.get_mut(index) {
            l.drying = drying.clamp(0.1, 8.0);
        }
    }

    pub fn layer_infos(&self) -> Vec<LayerInfo> {
        self.layers
            .iter()
            .map(|l| LayerInfo {
                id: l.id,
                name: l.name.clone(),
                opacity: l.opacity,
                visible: l.visible,
                wet: l.wet.is_some(),
                wetness: l.wet.as_ref().map_or(0.0, |w| w.wetness),
                drying: l.drying,
            })
            .collect()
    }

    /// Number of 32x32 tiles simulated across all wet layers.
    pub fn sim_tiles(&self) -> usize {
        self.layers.iter().filter_map(|l| l.wet.as_ref()).map(|w| w.tiles.list.len()).sum()
    }

    pub fn any_wet(&self) -> bool {
        self.layers.iter().any(|l| l.wet.is_some())
    }

    /// Instantly dries a layer: suspended pigment settles, wet ink becomes permanent.
    pub fn dry_layer(&mut self, index: usize) {
        if index >= self.layers.len() || self.layers[index].wet.is_none() {
            return;
        }
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        self.begin_pixel_entry(index, &mut enc);
        self.bake(index, &mut enc);
        self.undo.close();
        self.ring.flush(&self.gpu.queue);
        self.gpu.queue.submit([enc.finish()]);
        self.revision += 1;
    }

    pub fn dry_all(&mut self) {
        for i in 0..self.layers.len() {
            if self.layers[i].wet.is_some() {
                let mut enc = self.gpu.device.create_command_encoder(&Default::default());
                self.bake(i, &mut enc);
                self.ring.flush(&self.gpu.queue);
                self.gpu.queue.submit([enc.finish()]);
            }
        }
    }

    fn bake(&mut self, index: usize, enc: &mut wgpu::CommandEncoder) {
        let Some(wet) = self.layers[index].wet.take() else { return };
        let r = wet.rect.intersect(&IRect::full(self.w, self.h));
        let params = self.sim_params(&r, 1.0);
        let layer = &mut self.layers[index];
        if !r.is_empty() {
            let u = self.ring.push(&params);
            let bg = bind(&self.gpu.device, &self.pipes.bake, &[
                (0, self.ring.res(u)),
                (1, tv(&wet.pig)),
                (2, tv(&wet.ink)),
                (3, tv(&layer.dep)),
                (4, tv(&layer.fix)),
                (5, tv(&wet.dep_alt)),
                (6, tv(&wet.fix_alt)),
            ]);
            dispatch(enc, &self.pipes.bake, &bg, &r);
            // Only the rect changed; bring the results back into the layer's own textures.
            copy_rect(enc, &wet.dep_alt.tex, &layer.dep.tex, (r.x0 as u32, r.y0 as u32), (r.x0 as u32, r.y0 as u32), r.w(), r.h());
            copy_rect(enc, &wet.fix_alt.tex, &layer.fix.tex, (r.x0 as u32, r.y0 as u32), (r.x0 as u32, r.y0 as u32), r.w(), r.h());
        }
        self.pending_dirty = self.pending_dirty.union(&r);
        if index != self.active {
            self.structure_dirty = true;
        }
    }

    pub fn clear_layer(&mut self, index: usize) {
        if index >= self.layers.len() {
            return;
        }
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        self.begin_pixel_entry(index, &mut enc);
        self.capture(index, &IRect::full(self.w, self.h), &mut enc);
        self.layers[index].wet = None;
        let l = &self.layers[index];
        clear(&mut enc, &[&l.dep, &l.fix]);
        self.undo.close();
        self.ring.flush(&self.gpu.queue);
        self.gpu.queue.submit([enc.finish()]);
        self.pending_dirty = IRect::full(self.w, self.h);
        self.structure_dirty |= index != self.active;
        self.revision += 1;
    }

    // ------------------------------------------------------------------ strokes

    pub fn begin_stroke(&mut self, settings: BrushSettings, p: InputPoint) {
        self.stroke_seed = self.stroke_seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut s = Stroke::new(settings, self.stroke_seed >> 8);
        s.add(p, &mut self.pending);
        self.stroke = Some(s);
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        self.begin_pixel_entry(self.active, &mut enc);
        self.ring.flush(&self.gpu.queue);
        self.gpu.queue.submit([enc.finish()]);
    }

    pub fn stroke_to(&mut self, p: InputPoint) {
        if let Some(s) = self.stroke.as_mut() {
            s.add(p, &mut self.pending);
        }
    }

    pub fn end_stroke(&mut self, p: Option<InputPoint>) {
        if let Some(mut s) = self.stroke.take()
            && let Some(p) = p {
                s.finish(p, &mut self.pending);
            }
        self.revision += 1;
    }

    /// Abandons the current stroke as if it never happened.
    pub fn cancel_stroke(&mut self) {
        self.pending.clear();
        if self.stroke.take().is_some() {
            self.undo();
            self.undo.redo.clear();
        }
    }

    pub fn stroking(&self) -> bool {
        self.stroke.is_some()
    }

    // ------------------------------------------------------------------ undo

    fn begin_pixel_entry(&mut self, index: usize, enc: &mut wgpu::CommandEncoder) {
        let layer = &self.layers[index];
        let wet_rect = layer.wet.as_ref().map(|w| w.rect);
        self.undo.push(Entry::Pixels(StrokeEntry { layer_id: layer.id, wet_rect, tiles: Default::default() }));
        if let Some(r) = wet_rect {
            // The simulation will keep changing the wet area, so grab it now.
            self.capture(index, &r, enc);
        }
    }

    /// Records the pre-image of every not-yet-captured tile of `rect` in the open entry.
    fn capture(&mut self, index: usize, rect: &IRect, enc: &mut wgpu::CommandEncoder) {
        let (w, h) = (self.w, self.h);
        let layer = &self.layers[index];
        let Some(entry) = self.undo.open_entry(layer.id) else { return };
        let with_wet = entry.wet_rect.is_some();
        for t in undo::tiles_in(rect, w, h) {
            if entry.tiles.contains_key(&t) {
                continue;
            }
            let img = snapshot_tile(&self.gpu.device, enc, layer, t, w, h, with_wet);
            entry.tiles.insert(t, img);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.undo.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if self.stroke.is_some() {
            return;
        }
        self.undo.close();
        if let Some(e) = self.undo.undo.pop() {
            let inv = self.apply(e);
            self.undo.redo.push(inv);
            self.revision += 1;
        }
    }

    pub fn redo(&mut self) {
        if self.stroke.is_some() {
            return;
        }
        self.undo.close();
        if let Some(e) = self.undo.redo.pop() {
            let inv = self.apply(e);
            self.undo.undo.push(inv);
            self.revision += 1;
        }
    }

    /// Applies an entry and returns its inverse.
    fn apply(&mut self, e: Entry) -> Entry {
        self.structure_dirty = true;
        match e {
            Entry::LayerAdded { index, id } => {
                let i = self.layers.iter().position(|l| l.id == id).unwrap_or(index);
                let layer = self.layers.remove(i);
                self.active = self.active.min(self.layers.len().saturating_sub(1));
                Entry::LayerRemoved { index: i, layer: Box::new(layer) }
            }
            Entry::LayerRemoved { index, layer } => {
                let id = layer.id;
                let i = index.min(self.layers.len());
                self.layers.insert(i, *layer);
                self.active = i;
                Entry::LayerAdded { index: i, id }
            }
            Entry::LayerMoved { from, to } => {
                let l = self.layers.remove(to);
                self.layers.insert(from, l);
                self.active = from;
                Entry::LayerMoved { from: to, to: from }
            }
            Entry::LayerProps { id, opacity, visible, name } => {
                let Some(l) = self.layers.iter_mut().find(|l| l.id == id) else {
                    return Entry::LayerProps { id, opacity, visible, name };
                };
                let inv = Entry::LayerProps { id, opacity: l.opacity, visible: l.visible, name: l.name.clone() };
                l.opacity = opacity;
                l.visible = visible;
                l.name = name;
                inv
            }
            Entry::Pixels(entry) => Entry::Pixels(self.restore_pixels(entry)),
        }
    }

    fn restore_pixels(&mut self, entry: StrokeEntry) -> StrokeEntry {
        let (w, h) = (self.w, self.h);
        let Some(index) = self.layers.iter().position(|l| l.id == entry.layer_id) else { return entry };
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());

        // Capture the present state of the same tiles: that is the redo image.
        let cur_wet = self.layers[index].wet.as_ref().map(|w| w.rect);
        let mut inverse = StrokeEntry { layer_id: entry.layer_id, wet_rect: cur_wet, tiles: Default::default() };
        for &t in entry.tiles.keys() {
            let img = snapshot_tile(&self.gpu.device, &mut enc, &self.layers[index], t, w, h, cur_wet.is_some());
            inverse.tiles.insert(t, img);
        }

        let mut created_wet = false;
        match entry.wet_rect {
            None => self.layers[index].wet = None,
            Some(r) => {
                if self.layers[index].wet.is_none() {
                    let ws = WetState::new(&self.gpu, &self.layers[index], &mut enc);
                    self.layers[index].wet = Some(ws);
                    created_wet = true;
                }
                let ws = self.layers[index].wet.as_mut().unwrap();
                ws.activate(&r, &IRect::full(w, h));
                ws.idle_frames = 0;
            }
        }

        let layer = &self.layers[index];
        let mut dirty = IRect::EMPTY;
        for (&t, img) in &entry.tiles {
            let (x, y, tw, th) = undo::tile_rect(t, w, h);
            dirty = dirty.union(&IRect::new(x as i32, y as i32, (x + tw) as i32, (y + th) as i32));
            copy_rect(&mut enc, &img.dep.tex, &layer.dep.tex, (0, 0), (x, y), tw, th);
            copy_rect(&mut enc, &img.fix.tex, &layer.fix.tex, (0, 0), (x, y), tw, th);
            if let Some(ws) = &layer.wet {
                let dst = [&ws.water, &ws.pig, &ws.ink];
                for (k, d) in dst.iter().enumerate() {
                    let src = img.wet.as_ref().map_or(&self.zero_tile, |wt| &wt[k]);
                    copy_rect(&mut enc, &src.tex, &d.tex, (0, 0), (x, y), tw, th);
                }
                copy_rect(&mut enc, &self.zero_tile.tex, &ws.flux.tex, (0, 0), (x, y), tw, th);
            }
        }
        if created_wet {
            let ws = layer.wet.as_ref().unwrap();
            copy_full(&mut enc, &layer.dep, &ws.dep_alt);
            copy_full(&mut enc, &layer.fix, &ws.fix_alt);
        }
        if let Some(ws) = self.layers[index].wet.as_mut() {
            ws.activate(&dirty, &IRect::full(w, h));
        }
        self.ring.flush(&self.gpu.queue);
        self.gpu.queue.submit([enc.finish()]);
        self.pending_dirty = self.pending_dirty.union(&dirty);
        inverse
    }

    // ------------------------------------------------------------------ frame

    fn sim_params(&self, r: &IRect, drying: f32) -> SimParams {
        let t = &self.tuning;
        SimParams {
            origin: [r.x0, r.y0],
            size: [r.w() as i32, r.h() as i32],
            canvas: [self.w as i32, self.h as i32],
            dt: 1.0,
            gravity: t.gravity,
            damping: t.damping,
            paper_influence: t.paper_influence,
            tension: t.tension,
            absorb: t.absorb,
            capillary: t.capillary,
            cap_threshold: t.cap_threshold,
            evap_water: t.evap_water * drying,
            evap_paper: t.evap_paper * drying,
            edge_evap: t.edge_evap,
            deposit: t.deposit,
            lift: t.lift,
            granulation: t.granulation * (0.5 + self.paper_spec.roughness),
            bloom: t.bloom,
            diffuse: t.diffuse,
            ink_bleed: t.ink_bleed,
            ink_fix: t.ink_fix,
            staining: t.staining,
            edge_drift: t.edge_drift,
            _pad: [0.0; 2],
        }
    }

    /// Advances the painting by `steps` simulation steps and refreshes the composite.
    /// Returns true while something is still changing (wet paint or pending dabs).
    pub fn update(&mut self, steps: u32) -> bool {
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        self.dirty_tiles.clear();
        let pending = std::mem::replace(&mut self.pending_dirty, IRect::EMPTY);
        self.dirty_tiles.activate(&pending);

        // 1. Rasterise the dabs that arrived since last frame.
        let dabs = std::mem::take(&mut self.pending);
        if !dabs.is_empty() {
            self.draw_dabs(&dabs, &mut enc);
        }

        // 2. Step every wet layer; whatever it simulates needs recompositing.
        let mut other_wet = false;
        for i in 0..self.layers.len() {
            let Some(ws) = self.layers[i].wet.as_ref() else { continue };
            if ws.tiles.list.is_empty() {
                continue;
            }
            let rect = ws.rect;
            for _ in 0..steps {
                self.step(i, &rect, &mut enc);
            }
            self.dirty_tiles.extend(&self.layers[i].wet.as_ref().unwrap().tiles);
            other_wet |= i != self.active;
        }
        self.probe_wetness(steps, &mut enc);

        // 3. Composite.
        if self.structure_dirty {
            self.all_tiles.flush(&self.gpu);
            let t = (self.all_tiles.buf.clone(), self.all_tiles.list.len() as u32);
            self.rebuild_caches(&t, &mut enc);
            self.final_composite(&t, &mut enc);
            self.structure_dirty = false;
        } else if !self.dirty_tiles.list.is_empty() {
            self.dirty_tiles.flush(&self.gpu);
            let t = (self.dirty_tiles.buf.clone(), self.dirty_tiles.list.len() as u32);
            if other_wet {
                self.rebuild_caches(&t, &mut enc);
            }
            self.final_composite(&t, &mut enc);
        }

        self.ring.flush(&self.gpu.queue);
        self.gpu.queue.submit([enc.finish()]);
        self.after_submit();
        self.needs_update()
    }

    pub fn needs_update(&self) -> bool {
        self.any_wet() || !self.pending.is_empty() || self.stroke.is_some() || self.structure_dirty || !self.pending_dirty.is_empty()
    }

    fn draw_dabs(&mut self, dabs: &[(DabKind, Dab)], enc: &mut wgpu::CommandEncoder) {
        let full = IRect::full(self.w, self.h);
        let idx = self.active;
        let needed = (dabs.len() * size_of::<Dab>()) as u64;
        if needed > self.dab_buf.size() {
            self.dab_buf = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("dabs"),
                size: needed.next_power_of_two(),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        let raw: Vec<Dab> = dabs.iter().map(|d| d.1).collect();
        self.gpu.queue.write_buffer(&self.dab_buf, 0, bytemuck::cast_slice(&raw));

        let globals = self.ring.push(&DabGlobals { canvas: [self.w as f32, self.h as f32], _pad: [0.0; 2] });
        let mut start = 0;
        while start < dabs.len() {
            let kind = dabs[start].0;
            let mut end = start;
            while end < dabs.len() && dabs[end].0 == kind {
                end += 1;
            }
            if kind == DabKind::Wet && self.layers[idx].wet.is_none() {
                let ws = WetState::new(&self.gpu, &self.layers[idx], enc);
                self.layers[idx].wet = Some(ws);
            }
            // Activate, capture for undo and mark dirty per dab, so a fast stroke
            // across the sheet only touches the tiles it actually crosses.
            for (_, dab) in &dabs[start..end] {
                let b = dab.bounds().intersect(&full);
                if b.is_empty() {
                    continue;
                }
                self.dirty_tiles.activate(&b);
                let touched = if kind == DabKind::Wet { b.pad(WET_MARGIN).intersect(&full) } else { b };
                if let Some(ws) = self.layers[idx].wet.as_mut() {
                    ws.activate(&touched, &full);
                    if kind == DabKind::Wet {
                        ws.idle_frames = 0;
                    }
                }
                self.capture(idx, &touched, enc);
            }
            self.draw_run(kind, start as u32..end as u32, globals, enc);
            start = end;
        }
    }

    fn draw_run(&self, kind: DabKind, range: std::ops::Range<u32>, globals: crate::gpu::UBind, enc: &mut wgpu::CommandEncoder) {
        let layer = &self.layers[self.active];
        let (pipe, targets): (&wgpu::RenderPipeline, Vec<&Tex>) = match kind {
            DabKind::Wet => {
                let ws = layer.wet.as_ref().unwrap();
                (&self.pipes.dab_wet, vec![&ws.water, &ws.pig, &ws.ink, &ws.flux])
            }
            DabKind::Fixed => (&self.pipes.dab_fix, vec![&layer.fix]),
            DabKind::Erase => (&self.pipes.dab_erase2, vec![&layer.dep, &layer.fix]),
        };
        let bg = bind(&self.gpu.device, pipe, &[(0, self.ring.res(globals)), (1, tv(&self.paper))]);
        let mut passes = vec![(pipe, targets)];
        if kind == DabKind::Erase
            && let Some(ws) = &layer.wet {
                passes.push((&self.pipes.dab_erase3, vec![&ws.water, &ws.pig, &ws.ink]));
            }
        for (pipe, targets) in passes {
            let atts: Vec<Option<wgpu::RenderPassColorAttachment>> = targets
                .iter()
                .map(|t| {
                    Some(wgpu::RenderPassColorAttachment {
                        view: &t.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                    })
                })
                .collect();
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("dabs"),
                color_attachments: &atts,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bg, &[]);
            pass.set_vertex_buffer(0, self.dab_buf.slice(..));
            pass.draw(0..6, range.clone());
        }
    }

    fn step(&mut self, i: usize, r: &IRect, enc: &mut wgpu::CommandEncoder) {
        let drying = self.layers[i].drying;
        let u = self.ring.push(&self.sim_params(r, drying));
        let d = &self.gpu.device;
        let p = &self.pipes;
        let layer = &mut self.layers[i];
        let ws = layer.wet.as_mut().unwrap();
        ws.tiles.flush(&self.gpu);
        let n = ws.tiles.list.len() as u32;
        let tiles = || [(20u32, ws.tiles.buf.as_entire_binding()), (21u32, tv(&ws.tiles.mask))];

        let mut e = vec![(0, self.ring.res(u)), (1, tv(&ws.water)), (2, tv(&ws.flux)), (3, tv(&self.paper)), (4, tv(&ws.flux_alt))];
        e.extend(tiles());
        dispatch_tiles(enc, &p.flux, &bind(d, &p.flux, &e), n);

        let mut e = vec![
            (0, self.ring.res(u)),
            (1, tv(&ws.water)),
            (2, tv(&ws.flux_alt)),
            (3, tv(&ws.pig)),
            (4, tv(&ws.ink)),
            (5, tv(&ws.water_alt)),
            (6, tv(&ws.pig_alt)),
            (7, tv(&ws.ink_alt)),
        ];
        e.extend(tiles());
        dispatch_tiles(enc, &p.transport, &bind(d, &p.transport, &e), n);

        let mut e = vec![(0, self.ring.res(u)), (1, tv(&ws.water_alt)), (2, tv(&self.paper)), (3, tv(&ws.water))];
        e.extend(tiles());
        dispatch_tiles(enc, &p.settle_water, &bind(d, &p.settle_water, &e), n);

        let mut e = vec![
            (0, self.ring.res(u)),
            (1, tv(&ws.water)),
            (2, tv(&self.paper)),
            (3, tv(&ws.pig_alt)),
            (4, tv(&ws.ink_alt)),
            (5, tv(&layer.dep)),
            (6, tv(&layer.fix)),
            (7, tv(&ws.pig)),
            (8, tv(&ws.ink)),
            (9, tv(&ws.dep_alt)),
            (10, tv(&ws.fix_alt)),
        ];
        e.extend(tiles());
        dispatch_tiles(enc, &p.settle_pigment, &bind(d, &p.settle_pigment, &e), n);
        std::mem::swap(&mut ws.flux, &mut ws.flux_alt);
        std::mem::swap(&mut layer.dep, &mut ws.dep_alt);
        std::mem::swap(&mut layer.fix, &mut ws.fix_alt);
    }

    fn probe_wetness(&mut self, steps: u32, enc: &mut wgpu::CommandEncoder) {
        for i in 0..self.layers.len() {
            let Some(ws) = self.layers[i].wet.as_mut() else { continue };
            ws.frame += 1;
            ws.idle_frames += 1;
            ws.steps_since_probe += steps;
            // Counted in steps, not frames, so water travels the same distance
            // between probes whatever the frame rate.
            if ws.steps_since_probe < PROBE_STEPS || ws.probe.state.load(Ordering::Acquire) != PROBE_IDLE {
                continue;
            }
            ws.steps_since_probe = 0;
            ws.probe_frame = ws.frame;
            ws.tiles.flush(&self.gpu);
            let u = self.ring.push(&self.sim_params(&IRect::full(self.w, self.h), 1.0));
            let ws = self.layers[i].wet.as_mut().unwrap();
            enc.clear_buffer(&ws.probe.storage, 0, None);
            let bg = bind(&self.gpu.device, &self.pipes.wet_tiles, &[
                (0, self.ring.res(u)),
                (1, tv(&ws.water)),
                (2, ws.probe.storage.as_entire_binding()),
                (20, ws.tiles.buf.as_entire_binding()),
            ]);
            dispatch_tiles(enc, &self.pipes.wet_tiles, &bg, ws.tiles.list.len() as u32);
            enc.copy_buffer_to_buffer(&ws.probe.storage, 0, &ws.probe.readback, 0, ws.probe.storage.size());
            ws.probe.state.store(PROBE_PENDING, Ordering::Release);
        }
    }

    fn after_submit(&mut self) {
        let mut to_dry = Vec::new();
        let mut plans = Vec::new();
        for (i, l) in self.layers.iter_mut().enumerate() {
            let Some(ws) = l.wet.as_mut() else { continue };
            match ws.probe.state.load(Ordering::Acquire) {
                PROBE_PENDING => {
                    // Mapping is requested right after the submit that fills the buffer.
                    ws.probe.state.store(PROBE_MAPPING, Ordering::Release);
                    let st = ws.probe.state.clone();
                    ws.probe.readback.slice(..).map_async(wgpu::MapMode::Read, move |res| {
                        st.store(if res.is_ok() { PROBE_READY } else { PROBE_IDLE }, Ordering::Release);
                    });
                }
                PROBE_READY => {
                    let wet_tiles: Option<Vec<f32>> = {
                        let view = ws.probe.readback.slice(..).get_mapped_range().ok();
                        view.map(|b| b.as_chunks::<4>().0.iter().map(|c| f32::from_bits(u32::from_le_bytes(*c))).collect())
                    };
                    ws.probe.readback.unmap();
                    ws.probe.state.store(PROBE_IDLE, Ordering::Release);
                    if let Some(wet_tiles) = wet_tiles {
                        ws.wetness = wet_tiles.iter().fold(0.0, |a, &b| a.max(b));
                        // Dry enough and nobody is painting into it: release the wet state.
                        if ws.wetness < DRY && ws.idle_frames > 30 {
                            to_dry.push(i);
                        } else {
                            let (retire, revive) = ws.plan_tiles(&wet_tiles);
                            if !revive.is_empty() {
                                for &t in &revive {
                                    ws.tiles.activate_tile(t & 0xffff, t >> 16);
                                }
                            }
                            if !retire.is_empty() {
                                plans.push((i, retire));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        if (to_dry.is_empty() || self.stroke.is_some()) && plans.is_empty() {
            return;
        }
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        for (i, retire) in plans {
            self.retire_tiles(i, &retire, &mut enc);
        }
        if self.stroke.is_none() {
            for i in to_dry {
                // Drying on its own is part of the stroke that made it wet: same undo entry.
                if let Some(r) = self.layers[i].wet.as_ref().map(|w| w.rect) {
                    self.capture(i, &r, &mut enc);
                }
                self.bake(i, &mut enc);
            }
        }
        self.ring.flush(&self.gpu.queue);
        self.gpu.queue.submit([enc.finish()]);
    }

    /// Stops simulating tiles that have dried: settles what they still hold into
    /// the layer, then zeroes their wet state so they look like never-wet paper.
    fn retire_tiles(&mut self, index: usize, retire: &[u32], enc: &mut wgpu::CommandEncoder) {
        let full = IRect::full(self.w, self.h);
        let rects: Vec<IRect> = retire
            .iter()
            .map(|&t| {
                let (x, y) = ((t & 0xffff) as i32 * SIM_TILE as i32, (t >> 16) as i32 * SIM_TILE as i32);
                IRect::new(x, y, x + SIM_TILE as i32, y + SIM_TILE as i32).intersect(&full)
            })
            .collect();
        for r in &rects {
            self.capture(index, r, enc);
        }
        let list = self.gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("retired tiles"),
            size: (retire.len() * 4) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.gpu.queue.write_buffer(&list, 0, bytemuck::cast_slice(retire));
        let u = self.ring.push(&self.sim_params(&full, 1.0));
        let layer = &mut self.layers[index];
        let ws = layer.wet.as_mut().unwrap();
        let bg = bind(&self.gpu.device, &self.pipes.retire, &[
            (0, self.ring.res(u)),
            (1, tv(&ws.pig)),
            (2, tv(&ws.ink)),
            (3, tv(&layer.dep)),
            (4, tv(&layer.fix)),
            (5, tv(&ws.dep_alt)),
            (6, tv(&ws.fix_alt)),
            (20, list.as_entire_binding()),
        ]);
        dispatch_tiles(enc, &self.pipes.retire, &bg, retire.len() as u32);
        for r in &rects {
            let (o, w, h) = ((r.x0 as u32, r.y0 as u32), r.w(), r.h());
            copy_rect(enc, &ws.dep_alt.tex, &layer.dep.tex, o, o, w, h);
            copy_rect(enc, &ws.fix_alt.tex, &layer.fix.tex, o, o, w, h);
            for t in [&ws.water, &ws.water_alt, &ws.flux, &ws.flux_alt, &ws.pig, &ws.pig_alt, &ws.ink, &ws.ink_alt] {
                copy_rect(enc, &self.zero_tile.tex, &t.tex, (0, 0), o, w, h);
            }
            self.pending_dirty = self.pending_dirty.union(r);
        }
        ws.tiles.remove(retire);
    }

    fn layer_uniform(&self, r: &IRect, l: Option<&Layer>, first: bool) -> LayerU {
        LayerU {
            origin: [r.x0, r.y0],
            size: [r.w() as i32, r.h() as i32],
            opacity: l.map_or(0.0, |l| if l.visible { l.opacity } else { 0.0 }),
            wet: l.map_or(0, |l| l.wet.is_some() as u32),
            first: first as u32,
            _pad: 0.0,
        }
    }

    fn layer_bindings<'a>(&'a self, l: &'a Layer) -> [(u32, wgpu::BindingResource<'a>); 5] {
        let (pig, ink, water) = match &l.wet {
            Some(ws) => (&ws.pig, &ws.ink, &ws.water),
            None => (&self.zero, &self.zero, &self.zero),
        };
        [(1, tv(&l.dep)), (2, tv(&l.fix)), (3, tv(pig)), (4, tv(ink)), (5, tv(water))]
    }

    fn rebuild_caches(&mut self, tiles: &(wgpu::Buffer, u32), enc: &mut wgpu::CommandEncoder) {
        let d = &self.gpu.device;
        let r = &IRect::full(self.w, self.h);
        let tb = || (20u32, tiles.0.as_entire_binding());
        // Below: paper, then every visible layer under the active one.
        let u = self.ring.push(&self.layer_uniform(r, None, false));
        let bg = bind(d, &self.pipes.paper_init, &[(0, self.ring.res(u)), (6, tv(&self.paper)), (9, tv(&self.below[0])), tb()]);
        dispatch_tiles(enc, &self.pipes.paper_init, &bg, tiles.1);
        let mut cur = 0;
        for i in 0..self.active {
            let l = &self.layers[i];
            if !l.visible || l.opacity <= 0.0 {
                continue;
            }
            let u = self.ring.push(&self.layer_uniform(r, Some(l), false));
            let mut entries: Vec<(u32, wgpu::BindingResource)> = vec![(0, self.ring.res(u))];
            entries.extend(self.layer_bindings(l));
            entries.push((7, tv(&self.below[cur])));
            entries.push((9, tv(&self.below[1 - cur])));
            entries.push(tb());
            let bg = bind(d, &self.pipes.layer_over, &entries);
            dispatch_tiles(enc, &self.pipes.layer_over, &bg, tiles.1);
            cur = 1 - cur;
        }
        self.below_idx = cur;

        // Above: top-down into a single (R, T) pair.
        let mut acc: Option<usize> = None;
        for i in (self.active + 1..self.layers.len()).rev() {
            let l = &self.layers[i];
            if !l.visible || l.opacity <= 0.0 {
                continue;
            }
            let (src, dst) = match acc {
                None => (0, 0),
                Some(a) => (a, 1 - a),
            };
            let u = self.ring.push(&self.layer_uniform(r, Some(l), acc.is_none()));
            let mut entries: Vec<(u32, wgpu::BindingResource)> = vec![(0, self.ring.res(u))];
            entries.extend(self.layer_bindings(l));
            // On the first layer the inputs are ignored; bind the other pair member to avoid aliasing.
            let (src_r, src_t) = if acc.is_none() { (&self.above_r[1], &self.above_t[1]) } else { (&self.above_r[src], &self.above_t[src]) };
            entries.push((7, tv(src_r)));
            entries.push((8, tv(src_t)));
            entries.push((9, tv(&self.above_r[dst])));
            entries.push((10, tv(&self.above_t[dst])));
            entries.push(tb());
            let bg = bind(d, &self.pipes.layer_under_stack, &entries);
            dispatch_tiles(enc, &self.pipes.layer_under_stack, &bg, tiles.1);
            acc = Some(dst);
        }
        self.above_idx = acc;
    }

    fn final_composite(&mut self, tiles: &(wgpu::Buffer, u32), enc: &mut wgpu::CommandEncoder) {
        let r = &IRect::full(self.w, self.h);
        let l = &self.layers[self.active];
        let u = self.ring.push(&self.layer_uniform(r, Some(l), self.above_idx.is_none()));
        let a = self.above_idx.unwrap_or(0);
        let mut entries: Vec<(u32, wgpu::BindingResource)> = vec![(0, self.ring.res(u))];
        entries.extend(self.layer_bindings(l));
        entries.push((7, tv(&self.below[self.below_idx])));
        entries.push((9, tv(&self.composite)));
        entries.push((11, tv(&self.above_r[a])));
        entries.push((12, tv(&self.above_t[a])));
        entries.push((20, tiles.0.as_entire_binding()));
        let bg = bind(&self.gpu.device, &self.pipes.final_composite, &entries);
        dispatch_tiles(enc, &self.pipes.final_composite, &bg, tiles.1);
    }

    /// Recomposites everything on the next update (after loading, resizing, ...).
    pub fn invalidate(&mut self) {
        self.structure_dirty = true;
    }

    pub fn next_layer_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Replaces all layers (used when loading). The first layer becomes active unless `active` says otherwise.
    pub fn set_layers(&mut self, layers: Vec<Layer>, active: usize) {
        self.next_id = layers.iter().map(|l| l.id + 1).max().unwrap_or(1);
        self.layers = layers;
        self.active = active.min(self.layers.len().saturating_sub(1));
        self.undo.clear();
        self.structure_dirty = true;
    }

    pub fn new_layer_object(&mut self, name: String) -> Layer {
        let id = self.next_layer_id();
        Layer::new(&self.gpu.device, self.w, self.h, id, name)
    }

    /// Gives a loaded layer a wet state with the given rect (its textures are filled by the caller).
    pub fn make_wet(&mut self, index: usize, rect: IRect) {
        let mut enc = self.gpu.device.create_command_encoder(&Default::default());
        let mut ws = WetState::new(&self.gpu, &self.layers[index], &mut enc);
        ws.activate(&rect, &IRect::full(self.w, self.h));
        self.layers[index].wet = Some(ws);
        self.gpu.queue.submit([enc.finish()]);
    }
}

fn snapshot_tile(device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, layer: &Layer, t: (u32, u32), w: u32, h: u32, with_wet: bool) -> TileImage {
    let (x, y, tw, th) = undo::tile_rect(t, w, h);
    let grab = |src: &Tex| {
        let dst = Tex::sim(device, undo::TILE, undo::TILE, "undo tile");
        copy_rect(enc, &src.tex, &dst.tex, (x, y), (0, 0), tw, th);
        dst
    };
    let mut grab = grab;
    let dep = grab(&layer.dep);
    let fix = grab(&layer.fix);
    let wet = match (&layer.wet, with_wet) {
        (Some(ws), true) => Some([grab(&ws.water), grab(&ws.pig), grab(&ws.ink)]),
        _ => None,
    };
    TileImage { dep, fix, wet }
}

pub fn tv(t: &Tex) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::TextureView(&t.view)
}

pub fn bind<P: GetLayout>(device: &wgpu::Device, pipe: &P, entries: &[(u32, wgpu::BindingResource)]) -> wgpu::BindGroup {
    let e: Vec<wgpu::BindGroupEntry> = entries.iter().map(|(b, r)| wgpu::BindGroupEntry { binding: *b, resource: r.clone() }).collect();
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &pipe.layout0(), entries: &e })
}

pub trait GetLayout {
    fn layout0(&self) -> wgpu::BindGroupLayout;
}

impl GetLayout for wgpu::ComputePipeline {
    fn layout0(&self) -> wgpu::BindGroupLayout {
        self.get_bind_group_layout(0)
    }
}

impl GetLayout for wgpu::RenderPipeline {
    fn layout0(&self) -> wgpu::BindGroupLayout {
        self.get_bind_group_layout(0)
    }
}

pub fn dispatch(enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, bg: &wgpu::BindGroup, r: &IRect) {
    if r.is_empty() {
        return;
    }
    let mut pass = enc.begin_compute_pass(&Default::default());
    pass.set_pipeline(pipe);
    pass.set_bind_group(0, bg, &[]);
    pass.dispatch_workgroups(r.w().div_ceil(8), r.h().div_ceil(8), 1);
}

/// One 8x8 workgroup per 8x8 block of each active 32x32 tile.
pub fn dispatch_tiles(enc: &mut wgpu::CommandEncoder, pipe: &wgpu::ComputePipeline, bg: &wgpu::BindGroup, tiles: u32) {
    if tiles == 0 {
        return;
    }
    let mut pass = enc.begin_compute_pass(&Default::default());
    pass.set_pipeline(pipe);
    pass.set_bind_group(0, bg, &[]);
    pass.dispatch_workgroups(16, tiles, 1);
}

/// Clears whole textures to zero with an empty render pass.
pub fn clear(enc: &mut wgpu::CommandEncoder, texs: &[&Tex]) {
    for t in texs {
        let _ = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &t.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
}
