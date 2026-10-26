//! Saving and loading paintings.
//!
//! A painting is a directory:
//!   meta.json      - size, paper, layer list
//!   layer_<id>.bin - lz4 of the raw f16 textures (dep, fix, and water/pig/ink if wet)
//!   thumb.png      - small preview for the gallery

use std::fs;
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::canvas::{Canvas, Layer, PaperSpec};
use crate::gpu::{IRect, read_texture, write_texture};
use crate::pipelines::Pipelines;

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
    wet_rect: Option<[i32; 4]>,
}

fn one() -> f32 {
    1.0
}

#[derive(Serialize, Deserialize)]
struct Meta {
    version: u32,
    width: u32,
    height: u32,
    paper: PaperSpec,
    active: usize,
    layers: Vec<LayerMeta>,
}

pub fn save(canvas: &mut Canvas, dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // Make sure the composite is current before taking the thumbnail.
    canvas.update(0);
    let gpu = canvas.gpu().clone();
    let mut layers = Vec::new();
    for l in &canvas.layers {
        let mut raw = read_texture(&gpu, &l.dep.tex);
        raw.extend(read_texture(&gpu, &l.fix.tex));
        let wet_rect = l.wet.as_ref().map(|ws| {
            raw.extend(read_texture(&gpu, &ws.water.tex));
            raw.extend(read_texture(&gpu, &ws.pig.tex));
            raw.extend(read_texture(&gpu, &ws.ink.tex));
            [ws.rect.x0, ws.rect.y0, ws.rect.x1, ws.rect.y1]
        });
        write_atomic(&dir.join(format!("layer_{}.bin", l.id)), &lz4_flex::compress_prepend_size(&raw))?;
        layers.push(LayerMeta { id: l.id, name: l.name.clone(), opacity: l.opacity, visible: l.visible, drying: l.drying, wet_rect });
    }
    let meta = Meta { version: VERSION, width: canvas.w, height: canvas.h, paper: canvas.paper_spec, active: canvas.active, layers };
    // Drop files of layers that no longer exist.
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(id) = name.strip_prefix("layer_").and_then(|s| s.strip_suffix(".bin"))
                && !meta.layers.iter().any(|l| l.id.to_string() == id) {
                    let _ = fs::remove_file(e.path());
                }
        }
    }
    let thumb = encode_png(&composite_srgb(canvas, THUMB_SIZE))?;
    write_atomic(&dir.join("thumb.png"), &thumb)?;
    write_atomic(&dir.join("meta.json"), serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?.as_bytes())?;
    Ok(())
}

pub fn load(gpu: Arc<crate::gpu::Gpu>, pipes: Arc<Pipelines>, dir: &Path) -> Result<Canvas, String> {
    let meta: Meta = serde_json::from_slice(&fs::read(dir.join("meta.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut canvas = Canvas::new(gpu.clone(), pipes, meta.width, meta.height, meta.paper);
    let tex_bytes = meta.width as usize * meta.height as usize * 8;
    let mut layers: Vec<Layer> = Vec::new();
    let mut wet = Vec::new();
    for lm in &meta.layers {
        let mut l = canvas.new_layer_object(lm.name.clone());
        l.id = lm.id;
        l.opacity = lm.opacity;
        l.visible = lm.visible;
        l.drying = lm.drying;
        let raw = fs::read(dir.join(format!("layer_{}.bin", lm.id))).ok().and_then(|b| lz4_flex::decompress_size_prepended(&b).ok());
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

/// The composite as 8-bit sRGB RGBA, box-filtered so the longest side is at most `max_side`.
pub fn composite_srgb(canvas: &mut Canvas, max_side: u32) -> (u32, u32, Vec<u8>) {
    canvas.update(0);
    let gpu = canvas.gpu().clone();
    let raw = read_texture(&gpu, &canvas.composite.tex);
    let px: &[half::f16] = bytemuck::cast_slice(&raw);
    let (w, h) = (canvas.w, canvas.h);
    let f = w.max(h).div_ceil(max_side).max(1);
    let (ow, oh) = (w / f, h / f);
    let mut out = Vec::with_capacity((ow * oh * 4) as usize);
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

pub fn export_png(canvas: &mut Canvas, path: &Path) -> Result<(), String> {
    let img = composite_srgb(canvas, u32::MAX);
    write_atomic(path, &encode_png(&img)?)
}

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}
