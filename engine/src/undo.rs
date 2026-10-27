//! Tile-based undo.
//!
//! An entry is opened when a stroke (or a destructive layer action) starts and
//! records the *pre-image* of every 256x256 tile of one layer just before that
//! tile is first written. Simulation only ever touches a layer's active rect, so
//! tiles outside it are unchanged since the entry opened and can be captured
//! lazily. Restoring an entry captures the current contents of the same tiles
//! first, which becomes the redo entry.

use std::collections::HashMap;

use crate::gpu::{IRect, Tex};

pub const TILE: u32 = 256;

pub struct TileImage {
    pub dep: Tex,
    pub fix: Tex,
    /// water, suspended pigment, wet ink - present when the layer was wet
    pub wet: Option<[Tex; 3]>,
}

impl TileImage {
    pub fn bytes(&self) -> u64 {
        self.dep.bytes() + self.fix.bytes() + self.wet.as_ref().map_or(0, |w| w.iter().map(|t| t.bytes()).sum())
    }
}

pub struct StrokeEntry {
    pub layer_id: u32,
    /// Active rect of the layer's wet state when the entry opened, if it was wet.
    pub wet_rect: Option<IRect>,
    pub tiles: HashMap<(u32, u32), TileImage>,
}

impl StrokeEntry {
    pub fn bytes(&self) -> u64 {
        self.tiles.values().map(|t| t.bytes()).sum()
    }
}

pub enum Entry {
    Pixels(StrokeEntry),
    /// Layer was added at `index`; undo removes it.
    LayerAdded { index: usize, id: u32 },
    /// Layer was removed from `index`; undo puts it back.
    LayerRemoved { index: usize, layer: Box<crate::canvas::Layer> },
    LayerMoved { from: usize, to: usize },
    LayerProps { id: u32, opacity: f32, visible: bool, name: String },
}

impl Entry {
    pub fn bytes(&self) -> u64 {
        match self {
            Entry::Pixels(e) => e.bytes(),
            Entry::LayerRemoved { layer, .. } => layer.bytes(),
            _ => 0,
        }
    }
}

pub struct UndoStack {
    pub undo: Vec<Entry>,
    pub redo: Vec<Entry>,
    /// The pixel entry currently collecting tiles (index into `undo`).
    pub open: Option<usize>,
    pub budget: u64,
}

impl UndoStack {
    pub fn new(budget: u64) -> Self {
        Self { undo: Vec::new(), redo: Vec::new(), open: None, budget }
    }

    pub fn push(&mut self, e: Entry) {
        self.redo.clear();
        self.open = None;
        self.undo.push(e);
        if matches!(self.undo.last(), Some(Entry::Pixels(_))) {
            self.open = Some(self.undo.len() - 1);
        }
        self.trim();
    }

    pub fn close(&mut self) {
        self.open = None;
    }

    pub fn open_entry(&mut self, layer_id: u32) -> Option<&mut StrokeEntry> {
        let i = self.open?;
        match self.undo.get_mut(i) {
            Some(Entry::Pixels(e)) if e.layer_id == layer_id => Some(e),
            _ => None,
        }
    }

    /// The newest entry, if it is `layer_id`'s pixel entry (open or closed) and
    /// nothing is waiting to be redone. Water spreading after a stroke belongs to
    /// that stroke, so tiles it reaches can still be captured into it: they have
    /// not changed since the stroke began.
    pub fn latest_pixels(&mut self, layer_id: u32) -> Option<&mut StrokeEntry> {
        if !self.redo.is_empty() {
            return None;
        }
        match self.undo.last_mut() {
            Some(Entry::Pixels(e)) if e.layer_id == layer_id => Some(e),
            _ => None,
        }
    }

    pub fn total_bytes(&self) -> u64 {
        self.undo.iter().chain(self.redo.iter()).map(|e| e.bytes()).sum()
    }

    pub fn trim(&mut self) {
        while self.total_bytes() > self.budget && self.undo.len() > 1 {
            self.undo.remove(0);
            if let Some(o) = self.open.as_mut() {
                *o = o.saturating_sub(1);
            }
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.open = None;
    }
}

/// Tiles (tx, ty) overlapping `r`, clipped to a w x h canvas.
pub fn tiles_in(r: &IRect, w: u32, h: u32) -> impl Iterator<Item = (u32, u32)> {
    let r = r.intersect(&IRect::full(w, h));
    let (tx0, ty0) = ((r.x0.max(0) as u32) / TILE, (r.y0.max(0) as u32) / TILE);
    let (tx1, ty1) = if r.is_empty() { (0, 0) } else { ((r.x1 as u32).div_ceil(TILE), (r.y1 as u32).div_ceil(TILE)) };
    let (tx1, ty1) = if r.is_empty() { (tx0, ty0) } else { (tx1, ty1) };
    (ty0..ty1).flat_map(move |ty| (tx0..tx1).map(move |tx| (tx, ty)))
}

/// Pixel rect of tile (tx, ty) on a w x h canvas.
pub fn tile_rect(t: (u32, u32), w: u32, h: u32) -> (u32, u32, u32, u32) {
    let x = t.0 * TILE;
    let y = t.1 * TILE;
    (x, y, TILE.min(w - x), TILE.min(h - y))
}
