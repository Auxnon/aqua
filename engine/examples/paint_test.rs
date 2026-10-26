//! Headless smoke test: paints a few strokes with every tool, lets them dry and
//! writes PNG snapshots. `cargo run --release --example paint_test -- <out_dir>`

use std::path::PathBuf;

use aqua_engine::brush::{BrushSettings, InputPoint, Tool};
use aqua_engine::engine::Engine;
use aqua_engine::gpu::Gpu;
use aqua_engine::pigment::from_srgb;
use aqua_engine::storage;

fn stroke(e: &mut Engine, pts: &[(f32, f32)], pressure: f32) {
    let ip: Vec<InputPoint> = pts.iter().map(|&(x, y)| InputPoint { x, y, pressure }).collect();
    // Feed the samples a few at a time, like touch events arriving per frame.
    for (i, chunk) in ip.chunks(3).enumerate() {
        let phase = if i == 0 { 0 } else { 1 };
        e.stroke(phase, chunk);
        e.canvas.as_mut().unwrap().update(2);
    }
    e.stroke(2, &[*ip.last().unwrap()]);
}

fn line(x0: f32, y0: f32, x1: f32, y1: f32, n: usize) -> Vec<(f32, f32)> {
    (0..=n).map(|i| {
        let t = i as f32 / n as f32;
        (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t + (t * 9.0).sin() * 6.0)
    }).collect()
}

fn run(e: &mut Engine, frames: u32) {
    for _ in 0..frames {
        e.canvas.as_mut().unwrap().update(2);
    }
}

fn main() {
    env_logger::init();
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "paint_out".into()));
    std::fs::create_dir_all(&out).unwrap();
    let gpu = Gpu::headless().expect("gpu");
    let mut e = Engine::new(gpu);
    e.new_canvas(1024, 768, 0, 7);
    let t0 = std::time::Instant::now();

    // 1. A juicy ultramarine wash.
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 40.0, pigment: from_srgb([0.16, 0.2, 0.62]), water: 0.85, load: 0.7, ..Default::default() };
    for k in 0..4 {
        let y = 120.0 + k as f32 * 45.0;
        stroke(&mut e, &line(80.0, y, 520.0, y, 60), 0.8);
    }
    run(&mut e, 20);
    // 2. Yellow dropped into the wet wash: should bleed into green.
    e.brush.pigment = from_srgb([0.98, 0.8, 0.08]);
    e.brush.size = 30.0;
    stroke(&mut e, &line(300.0, 90.0, 360.0, 300.0, 40), 0.9);
    run(&mut e, 60);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("01_wet.png")).unwrap();

    // 3. Dry-ish red stroke on dry paper.
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 26.0, pigment: from_srgb([0.86, 0.16, 0.1]), water: 0.3, load: 0.9, ..Default::default() };
    stroke(&mut e, &line(600.0, 120.0, 950.0, 160.0, 80), 1.0);
    // 4. Technical pen and India ink.
    e.brush = BrushSettings { tool: Tool::Ink, size: 3.0, pigment: from_srgb([0.05, 0.05, 0.06]), bleed: 0.0, ..Default::default() };
    stroke(&mut e, &line(600.0, 300.0, 950.0, 300.0, 80), 1.0);
    e.brush.bleed = 1.0;
    e.brush.size = 4.0;
    stroke(&mut e, &line(600.0, 360.0, 950.0, 360.0, 80), 1.0);
    // India ink across a fresh wet wash: should feather and smear.
    e.brush = BrushSettings { tool: Tool::Water, size: 45.0, water: 0.9, ..Default::default() };
    stroke(&mut e, &line(100.0, 520.0, 500.0, 520.0, 50), 1.0);
    e.brush = BrushSettings { tool: Tool::Ink, size: 4.0, pigment: from_srgb([0.05, 0.05, 0.06]), bleed: 1.0, ..Default::default() };
    stroke(&mut e, &line(120.0, 470.0, 480.0, 570.0, 60), 1.0);
    run(&mut e, 120);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("02_mid.png")).unwrap();

    // 5. A rose wash; clear water dropped in once it is damp: bloom / backrun.
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 45.0, pigment: from_srgb([0.8, 0.12, 0.38]), water: 0.9, load: 0.5, ..Default::default() };
    for k in 0..3 {
        let y = 470.0 + k as f32 * 50.0;
        stroke(&mut e, &line(600.0, y, 960.0, y, 50), 0.8);
    }
    run(&mut e, 300);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("02b_before_bloom.png")).unwrap();
    e.brush = BrushSettings { tool: Tool::Water, size: 22.0, water: 1.0, ..Default::default() };
    stroke(&mut e, &line(740.0, 520.0, 800.0, 530.0, 10), 1.0);
    let c = e.canvas.as_ref().unwrap();
    println!("wet layers before drying: {:?}", c.layer_infos());
    let mut frames = 0;
    while e.canvas.as_ref().unwrap().any_wet() && frames < 6000 {
        run(&mut e, 10);
        frames += 10;
    }
    println!("dried after {frames} more frames; layers {:?}", e.canvas.as_ref().unwrap().layer_infos());
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("03_dry.png")).unwrap();

    // 6. Undo the bloom, then redo it.
    let c = e.canvas.as_mut().unwrap();
    c.undo();
    c.update(0);
    storage::export_png(c, &out.join("04_undo.png")).unwrap();
    c.redo();
    c.update(0);
    // 7. Save / load roundtrip.
    storage::save(c, &out.join("painting")).unwrap();
    let loaded = storage::load(e.gpu.clone(), e.pipes.clone(), &out.join("painting")).unwrap();
    e.canvas = Some(loaded);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("05_loaded.png")).unwrap();
    println!("done in {:?}", t0.elapsed());
}
