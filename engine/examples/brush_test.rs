//! Headless test of a resting brush, ink splatter and flooding water. Writes PNGs.
//! `cargo run --release --example brush_test -- <out_dir>`

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

/// Touches down at (x, y) and holds still for `secs`.
fn hold(e: &mut Engine, x: f32, y: f32, pressure: f32, secs: f32) {
    let p = InputPoint { x, y, pressure };
    e.stroke(0, &[p]);
    for _ in 0..(secs / DT) as u32 {
        frame(e);
    }
    e.stroke(2, &[p]);
    frame(e);
}

/// Feeds samples one per frame.
fn draw(e: &mut Engine, pts: &[InputPoint]) {
    e.stroke(0, &pts[..1]);
    for p in &pts[1..] {
        e.stroke(1, std::slice::from_ref(p));
        frame(e);
    }
    e.stroke(2, &pts[pts.len() - 1..]);
    frame(e);
}

fn dry(e: &mut Engine) {
    let mut n = 0;
    while e.canvas.as_ref().unwrap().any_wet() && n < 20000 {
        frame(e);
        n += 1;
    }
}

/// Prints the deepest surface water, mean depth and the wet area's radius.
fn water_stats(e: &Engine, what: &str) {
    let c = e.canvas.as_ref().unwrap();
    let Some(ws) = c.layers[0].wet.as_ref() else { return };
    let raw = aqua_engine::gpu::read_texture(c.gpu(), &ws.water.tex);
    let px: Vec<f32> = raw.as_chunks::<2>().0.iter().map(|b| half::f16::from_le_bytes(*b).to_f32()).collect();
    let (mut max_h, mut sum_h, mut wet) = (0.0f32, 0.0f32, 0usize);
    for cell in px.as_chunks::<4>().0 {
        max_h = max_h.max(cell[0]);
        if cell[0] > 1e-3 {
            sum_h += cell[0];
            wet += 1;
        }
    }
    let radius = (wet as f32 / std::f32::consts::PI).sqrt();
    println!("{what}: max depth {max_h:.2}, mean depth {:.2}, wet radius {radius:.0}px, {} tiles", sum_h / wet.max(1) as f32, c.sim_tiles());
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "brush_out".into()));
    std::fs::create_dir_all(&out).unwrap();
    let gpu = Gpu::headless().expect("gpu");
    let mut e = Engine::new(gpu);
    e.new_canvas(1000, 700, 0, 7);

    // Watercolour: a quick tap, then holds at rising pressure.
    let blue = from_srgb([0.15, 0.3, 0.7]);
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 26.0, pigment: blue, water: 0.7, ..Default::default() };
    hold(&mut e, 120.0, 120.0, 0.6, 0.05);
    for (i, p) in [0.25, 0.6, 1.0].into_iter().enumerate() {
        hold(&mut e, 300.0 + i as f32 * 200.0, 120.0, p, 2.0);
    }

    // Ink: pen and India ink resting on dry paper, light then hard.
    let black = from_srgb([0.05, 0.05, 0.07]);
    for (i, bleed) in [0.0, 1.0].into_iter().enumerate() {
        e.brush = BrushSettings { tool: Tool::Ink, size: 4.0, pigment: black, load: 0.8, bleed, softness: 0.05, bristle: 0.0, pressure_size: 0.8, ..Default::default() };
        for (j, p) in [0.3, 1.0].into_iter().enumerate() {
            hold(&mut e, 120.0 + (i * 2 + j) as f32 * 120.0, 300.0, p, 2.0);
        }
    }

    // Splatter: pressure jabs along a line, and a stroke that ends in a fast flick.
    e.brush = BrushSettings { tool: Tool::Ink, size: 4.0, pigment: black, load: 0.8, bleed: 0.3, splatter: 0.7, softness: 0.05, bristle: 0.0, pressure_size: 0.8, ..Default::default() };
    let jabs: Vec<InputPoint> = (0..60)
        .map(|i| InputPoint { x: 80.0 + i as f32 * 6.0, y: 450.0, pressure: if i % 15 == 7 { 1.0 } else { 0.3 } })
        .collect();
    draw(&mut e, &jabs);
    let mut flick: Vec<InputPoint> = (0..40).map(|i| InputPoint { x: 80.0 + i as f32 * 5.0, y: 560.0, pressure: 0.5 }).collect();
    for k in 1..=4 {
        flick.push(InputPoint { x: 280.0 + (k * k) as f32 * 9.0, y: 560.0 - (k * k) as f32 * 3.0, pressure: 0.5 - k as f32 * 0.1 });
    }
    draw(&mut e, &flick);
    dry(&mut e);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("brush.png")).unwrap();

    // Flooding: a watercolour wash scrubbed over and over with clear water, calm vs wild.
    for (name, flow) in [("flood_calm", 0.0), ("flood_wild", 1.0)] {
        e.new_canvas(800, 800, 0, 7);
        e.canvas.as_mut().unwrap().set_layer_flow(0, flow);
        e.brush = BrushSettings { tool: Tool::Watercolor, size: 40.0, pigment: from_srgb([0.75, 0.2, 0.3]), water: 1.0, load: 0.8, ..Default::default() };
        let scrub = |cx: f32, cy: f32, r: f32| -> Vec<InputPoint> {
            (0..80).map(|i| {
                let a = i as f32 * 0.45;
                InputPoint { x: cx + a.cos() * r * (i as f32 / 80.0), y: cy + a.sin() * r * (i as f32 / 80.0), pressure: 1.0 }
            }).collect()
        };
        draw(&mut e, &scrub(400.0, 400.0, 70.0));
        e.brush = BrushSettings { tool: Tool::Water, size: 45.0, water: 1.0, softness: 0.3, ..Default::default() };
        for pass in 0..10 {
            draw(&mut e, &scrub(400.0 + pass as f32 * 3.0, 400.0, 80.0));
        }
        for k in 0..240 {
            frame(&mut e);
            if k % 60 == 0 {
                water_stats(&e, &format!("{name} +{k} frames"));
            }
        }
        storage::export_png(e.canvas.as_mut().unwrap(), &out.join(format!("{name}_wet.png"))).unwrap();
        dry(&mut e);
        storage::export_png(e.canvas.as_mut().unwrap(), &out.join(format!("{name}.png"))).unwrap();

        // Water that spread beyond the strokes is part of their undo: undoing every
        // stroke must give back blank paper.
        let c = e.canvas.as_mut().unwrap();
        while c.can_undo() {
            c.undo();
        }
        let (_, _, undone) = storage::composite_srgb(c, 800);
        let mut blank = Engine::new(c.gpu().clone());
        blank.new_canvas(800, 800, 0, 7);
        let b = blank.canvas.as_mut().unwrap();
        let (_, _, fresh) = storage::composite_srgb(b, 800);
        let differ = undone.iter().zip(&fresh).filter(|(a, b)| a.abs_diff(**b) > 2).count();
        println!("{name}: after undoing everything, {differ} bytes differ from blank paper");
        assert_eq!(differ, 0, "undo left marks behind");
    }
    println!("wrote {}", out.display());
}
