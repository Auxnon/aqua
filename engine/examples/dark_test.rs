//! Dark paint worked over several times while wet: it should keep bleeding
//! smoothly out towards the dry paper, not break up into blocks.
//! `cargo run --release --example dark_test -- <out_dir>`

use std::path::PathBuf;

use aqua_engine::brush::{BrushSettings, InputPoint, Tool};
use aqua_engine::engine::Engine;
use aqua_engine::gpu::Gpu;
use aqua_engine::pigment::from_srgb;
use aqua_engine::storage;

const DT: f32 = 1.0 / 60.0;

fn frame(e: &mut Engine) {
    let c = e.canvas.as_mut().unwrap();
    c.dwell(DT);
    c.update(2);
}

fn draw(e: &mut Engine, pts: &[InputPoint]) {
    e.stroke(0, &pts[..1]);
    for p in &pts[1..] {
        e.stroke(1, std::slice::from_ref(p));
        frame(e);
    }
    e.stroke(2, &pts[pts.len() - 1..]);
    frame(e);
}

/// Largest and mean of each channel's sum over a wet layer texture.
fn stats(e: &Engine, what: &str) {
    let c = e.canvas.as_ref().unwrap();
    let l = &c.layers[0];
    let read = |t: &wgpu::Texture| -> Vec<f32> {
        let raw = aqua_engine::gpu::read_texture(c.gpu(), t);
        raw.as_chunks::<2>().0.iter().map(|b| half::f16::from_le_bytes(*b).to_f32()).collect()
    };
    let mut line = format!("{what}:");
    let mut texs = vec![("dep", &l.dep.tex)];
    if let Some(ws) = l.wet.as_ref() {
        texs.push(("pig", &ws.pig.tex));
        texs.push(("water", &ws.water.tex));
    }
    for (name, t) in texs {
        let px = read(t);
        let cells = px.as_chunks::<4>().0;
        let (mut mx, mut sum, mut n) = ([0f32; 4], [0f32; 4], 0usize);
        for c in cells {
            if c.iter().any(|v| *v > 1e-4) {
                n += 1;
            }
            for k in 0..4 {
                mx[k] = mx[k].max(c[k]);
                sum[k] += c[k];
            }
        }
        let mean = sum.map(|s| s / n.max(1) as f32);
        line += &format!("  {name} max {:.3?} mean {:.3?}", mx, mean);
    }
    println!("{line}");
}

/// The settled pigment's first channel, log scaled, as a greyscale PGM: shows
/// where it piles up even where the paint looks black.
fn dep_map(e: &Engine, path: &std::path::Path) {
    let c = e.canvas.as_ref().unwrap();
    let raw = aqua_engine::gpu::read_texture(c.gpu(), &c.layers[0].dep.tex);
    let px: Vec<f32> = raw.as_chunks::<2>().0.iter().map(|b| half::f16::from_le_bytes(*b).to_f32()).collect();
    let mut img = format!("P5 {} {} 255\n", c.w, c.h).into_bytes();
    for cell in px.as_chunks::<4>().0 {
        img.push(((cell[0].max(0.0) + 1.0).ln() / 7.0 * 255.0).min(255.0) as u8);
    }
    std::fs::write(path, img).unwrap();
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "dark_out".into()));
    std::fs::create_dir_all(&out).unwrap();
    let gpu = Gpu::headless().expect("gpu");
    gpu.device.on_uncaptured_error(std::sync::Arc::new(|err: wgpu::Error| panic!("wgpu: {err}")));
    let mut e = Engine::new(gpu);
    e.new_canvas(520, 380, 0, 11, 0);

    // A dark indigo, worked over the same patch again and again while it is wet.
    let dark = from_srgb([0.08, 0.1, 0.25]);
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 30.0, pigment: dark, water: 0.8, ..Default::default() };
    for pass in 0..8 {
        let mut pts = Vec::new();
        for row in 0..5 {
            let y = 140.0 + row as f32 * 22.0 + pass as f32 * 2.0;
            for i in 0..30 {
                let t = i as f32 / 29.0;
                let x = if row % 2 == 0 { 150.0 + 220.0 * t } else { 370.0 - 220.0 * t };
                pts.push(InputPoint { x, y, pressure: 0.8, tilt: [0.0; 2] });
            }
        }
        draw(&mut e, &pts);
        for _ in 0..20 {
            frame(&mut e);
        }
        stats(&e, &format!("pass {pass}"));
        if pass == 3 || pass == 7 {
            storage::export_png(e.canvas.as_mut().unwrap(), &out.join(format!("pass{pass}.png"))).unwrap();
        }
    }
    // Let it run on wet for a while, then dry.
    for k in 0..6 {
        for _ in 0..120 {
            frame(&mut e);
        }
        stats(&e, &format!("wet +{}s", (k + 1) * 2));
    }
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("wet.png")).unwrap();
    let mut n = 0;
    while e.canvas.as_ref().unwrap().any_wet() && n < 20000 {
        frame(&mut e);
        n += 1;
    }
    stats(&e, "dry");
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("dry.png")).unwrap();
    dep_map(&e, &out.join("dep.pgm"));
    println!("wrote {}", out.display());
}
