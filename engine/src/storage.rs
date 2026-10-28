//! Saving and loading paintings.
//!
//! A painting is a set of named files (a directory on Android, an IndexedDB
//! record on the web):
//!   meta.json      - size, paper, layer list
//!   layer_<id>.bin - lz4 of the raw f16 textures (dep, fix, and water/pig/ink if wet)
//!   thumb.png      - small preview for the gallery
//!
//! Reading textures back from the GPU is asynchronous (the browser can't block),
//! so saving is split in two: [`begin_save`] queues the copies while it has the
//! canvas, and [`PendingSave::finish`] awaits them without it. Native code has
//! blocking wrappers that read and write directories.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::canvas::{Canvas, Layer, PaperSpec};
use crate::gpu::{IRect, Readback, write_texture};
use crate::pipelines::Pipelines;

/// A painting's files, by name.
pub type Files = Vec<(String, Vec<u8>)>;

const VERSION: u32 = 1;
pub const THUMB_SIZE: u32 = 512;

#[derive(Serialize, Deserialize)]
struct LayerMeta {
    id: u32,
    name: String,
    opacity: f32,
    visible: bool,
    #[serde(default = "one")]
    drying: f32,
    #[serde(default = "default_flow")]
    flow: f32,
    wet_rect: Option<[i32; 4]>,
}

fn one() -> f32 {
    1.0
}

fn default_flow() -> f32 {
    crate::canvas::DEFAULT_FLOW
}

#[derive(Serialize, Deserialize)]
struct Meta {
    version: u32,
    width: u32,
    height: u32,
    paper: PaperSpec,
    #[serde(default)]
    home: u32,
    active: usize,
    layers: Vec<LayerMeta>,
}

/// Texture copies queued by [`begin_save`].
pub struct PendingSave {
    meta: Meta,
    layers: Vec<(u32, Vec<Readback>)>,
    thumb: PendingImage,
}

/// Queues everything a save needs to read back from the GPU.
pub fn begin_save(canvas: &mut Canvas) -> PendingSave {
    // Make sure the composite is current before taking the thumbnail.
    canvas.update(0);
    let gpu = canvas.gpu().clone();
    let mut layers = Vec::new();
    let mut metas = Vec::new();
    for l in &canvas.layers {
        let mut reads = vec![Readback::texture(&gpu, &l.dep.tex), Readback::texture(&gpu, &l.fix.tex)];
        let wet_rect = l.wet.as_ref().map(|ws| {
            reads.extend([&ws.water, &ws.pig, &ws.ink].map(|t| Readback::texture(&gpu, &t.tex)));
            [ws.rect.x0, ws.rect.y0, ws.rect.x1, ws.rect.y1]
        });
        layers.push((l.id, reads));
        metas.push(LayerMeta { id: l.id, name: l.name.clone(), opacity: l.opacity, visible: l.visible, drying: l.drying, flow: l.flow, wet_rect });
    }
    let meta = Meta { version: VERSION, width: canvas.w, height: canvas.h, paper: canvas.paper_spec, home: canvas.home, active: canvas.active, layers: metas };
    PendingSave { meta, layers, thumb: begin_composite(canvas, THUMB_SIZE) }
}

impl PendingSave {
    /// The painting's files: meta.json, thumb.png and one layer_<id>.bin per layer.
    pub async fn finish(self) -> Result<Files, String> {
        let mut files = Files::new();
        for (id, reads) in self.layers {
            let mut raw = Vec::new();
            for r in reads {
                raw.extend(r.read().await);
            }
            files.push((format!("layer_{id}.bin"), lz4_flex::compress_prepend_size(&raw)));
        }
        files.push(("thumb.png".into(), encode_png(&self.thumb.finish().await)?));
        files.push(("meta.json".into(), serde_json::to_string_pretty(&self.meta).map_err(|e| e.to_string())?.into_bytes()));
        Ok(files)
    }
}

/// Builds a canvas from a painting's files; `read` returns a file by name.
pub fn load_files(gpu: Arc<crate::gpu::Gpu>, pipes: Arc<Pipelines>, read: impl Fn(&str) -> Option<Vec<u8>>) -> Result<Canvas, String> {
    let meta: Meta = serde_json::from_slice(&read("meta.json").ok_or("no meta.json")?).map_err(|e| e.to_string())?;
    let mut canvas = Canvas::new(gpu.clone(), pipes, meta.width, meta.height, meta.paper);
    canvas.home = meta.home % 4;
    let tex_bytes = meta.width as usize * meta.height as usize * 8;
    let mut layers: Vec<Layer> = Vec::new();
    let mut wet = Vec::new();
    for lm in &meta.layers {
        let mut l = canvas.new_layer_object(lm.name.clone());
        l.id = lm.id;
        l.opacity = lm.opacity;
        l.visible = lm.visible;
        l.drying = lm.drying;
        l.flow = lm.flow;
        let raw = read(&format!("layer_{}.bin", lm.id)).and_then(|b| lz4_flex::decompress_size_prepended(&b).ok());
        if let Some(raw) = raw
            && raw.len() >= tex_bytes * 2 {
                write_texture(&gpu, &l.dep.tex, &raw[..tex_bytes]);
                write_texture(&gpu, &l.fix.tex, &raw[tex_bytes..tex_bytes * 2]);
                if let Some(r) = lm.wet_rect
                    && raw.len() >= tex_bytes * 5 {
                        wet.push((layers.len(), IRect::new(r[0], r[1], r[2], r[3]), raw[tex_bytes * 2..tex_bytes * 5].to_vec()));
                    }
            }
        layers.push(l);
    }
    if layers.is_empty() {
        return Err("painting has no layers".into());
    }
    canvas.set_layers(layers, meta.active);
    for (i, rect, raw) in wet {
        canvas.make_wet(i, rect);
        let ws = canvas.layers[i].wet.as_ref().unwrap();
        write_texture(&gpu, &ws.water.tex, &raw[..tex_bytes]);
        write_texture(&gpu, &ws.pig.tex, &raw[tex_bytes..tex_bytes * 2]);
        write_texture(&gpu, &ws.ink.tex, &raw[tex_bytes * 2..]);
    }
    canvas.invalidate();
    Ok(canvas)
}

/// A composite read queued by [`begin_composite`].
pub struct PendingImage {
    read: Readback,
    w: u32,
    h: u32,
    max_side: u32,
}

/// Queues a read of the composite; see [`PendingImage::finish`].
pub fn begin_composite(canvas: &mut Canvas, max_side: u32) -> PendingImage {
    canvas.update(0);
    let gpu = canvas.gpu().clone();
    PendingImage { read: Readback::texture(&gpu, &canvas.composite.tex), w: canvas.w, h: canvas.h, max_side }
}

impl PendingImage {
    /// The composite as 8-bit sRGB RGBA, box-filtered so the longest side is at most `max_side`.
    pub async fn finish(self) -> (u32, u32, Vec<u8>) {
        let raw = self.read.read().await;
        let px: &[half::f16] = bytemuck::cast_slice(&raw);
        let (w, h) = (self.w, self.h);
        let f = w.max(h).div_ceil(self.max_side).max(1);
        let (ow, oh) = (w / f, h / f);
        let mut out = Vec::with_capacity((ow * oh * 4) as usize);
        if px.len() < (w * h * 4) as usize {
            return (0, 0, out);
        }
        for oy in 0..oh {
            for ox in 0..ow {
                let mut acc = [0.0f32; 3];
                for y in oy * f..(oy + 1) * f {
                    for x in ox * f..(ox + 1) * f {
                        let i = ((y * w + x) * 4) as usize;
                        for c in 0..3 {
                            acc[c] += px[i + c].to_f32();
                        }
                    }
                }
                let n = (f * f) as f32;
                for c in acc {
                    out.push((crate::pigment::linear_to_srgb(c / n) * 255.0 + 0.5) as u8);
                }
                out.push(255);
            }
        }
        (ow, oh, out)
    }
}

/// Queues a read of the composite texel under `(x, y)`, if it's on the canvas.
pub fn begin_sample(canvas: &mut Canvas, x: f32, y: f32) -> Option<Readback> {
    if x < 0.0 || y < 0.0 || x >= canvas.w as f32 || y >= canvas.h as f32 {
        return None;
    }
    canvas.update(0);
    let gpu = canvas.gpu().clone();
    Some(Readback::start(&gpu, &canvas.composite.tex, x as u32, y as u32, 1, 1))
}

/// A composite texel read by [`begin_sample`] as sRGB 0..1.
pub fn texel_srgb(px: &[u8]) -> Option<[f32; 3]> {
    let f: &[half::f16] = bytemuck::try_cast_slice(px.get(..8)?).ok()?;
    Some([0, 1, 2].map(|i| crate::pigment::linear_to_srgb(f[i].to_f32())))
}

pub fn encode_png(img: &(u32, u32, Vec<u8>)) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut buf, img.0, img.1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().map_err(|e| e.to_string())?;
        wr.write_image_data(&img.2).map_err(|e| e.to_string())?;
    }
    Ok(buf)
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

/// Blocking, directory-based versions for Android and the desktop tools.
#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    use super::*;

    pub fn save(canvas: &mut Canvas, dir: &Path) -> Result<(), String> {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let files = pollster::block_on(begin_save(canvas).finish())?;
        // Drop files of layers that no longer exist.
        if let Ok(rd) = fs::read_dir(dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with("layer_") && !files.iter().any(|(n, _)| *n == name) {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        // meta.json last, so a painting is never listed before its layers are written.
        for (name, data) in &files {
            write_atomic(&dir.join(name), data)?;
        }
        Ok(())
    }

    pub fn load(gpu: Arc<crate::gpu::Gpu>, pipes: Arc<Pipelines>, dir: &Path) -> Result<Canvas, String> {
        load_files(gpu, pipes, |name| fs::read(dir.join(name)).ok())
    }

    /// The composite as 8-bit sRGB RGBA, box-filtered so the longest side is at most `max_side`.
    pub fn composite_srgb(canvas: &mut Canvas, max_side: u32) -> (u32, u32, Vec<u8>) {
        pollster::block_on(begin_composite(canvas, max_side).finish())
    }

    pub fn export_png(canvas: &mut Canvas, path: &Path) -> Result<(), String> {
        let img = composite_srgb(canvas, u32::MAX);
        write_atomic(path, &encode_png(&img)?)
    }

    fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}
