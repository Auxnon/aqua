//! Headless test of a resting brush, ink splatter, flooding water and erasing wet
//! paint. Writes PNGs.
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
    let bad = px.iter().filter(|v| !v.is_finite()).count();
    let neg = px.as_chunks::<4>().0.iter().filter(|c| c[0] < -1e-3).count();
    if bad + neg > 0 {
        println!("{what}: {bad} non-finite values, {neg} cells of negative water");
    }
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

/// Undoes every stroke and checks the canvas is back to blank paper.
fn undo_to_blank(e: &mut Engine, name: &str) {
    let c = e.canvas.as_mut().unwrap();
    let (w, h) = (c.w, c.h);
    while c.can_undo() {
        c.undo();
    }
    let (_, _, undone) = storage::composite_srgb(c, 800);
    let mut blank = Engine::new(c.gpu().clone());
    blank.new_canvas(w, h, 0, 7, 0);
    let (_, _, fresh) = storage::composite_srgb(blank.canvas.as_mut().unwrap(), 800);
    let differ = undone.iter().zip(&fresh).filter(|(a, b)| a.abs_diff(**b) > 2).count();
    println!("{name}: after undoing everything, {differ} bytes differ from blank paper");
    assert_eq!(differ, 0, "undo left marks behind");
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "brush_out".into()));
    std::fs::create_dir_all(&out).unwrap();
    let gpu = Gpu::headless().expect("gpu");
    // A validation error drops the whole frame silently; fail loudly instead.
    gpu.device.on_uncaptured_error(std::sync::Arc::new(|err: wgpu::Error| panic!("wgpu: {err}")));
    let mut e = Engine::new(gpu);
    e.new_canvas(1000, 700, 0, 7, 0);

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

    // The same with a big brush: drops should fly about as far, with a rougher rim.
    e.brush.size = 14.0;
    let jabs: Vec<InputPoint> = jabs.iter().map(|p| InputPoint { x: p.x + 500.0, ..*p }).collect();
    draw(&mut e, &jabs);
    let flick: Vec<InputPoint> = flick.iter().map(|p| InputPoint { x: p.x + 500.0, y: p.y + 40.0, ..*p }).collect();
    draw(&mut e, &flick);
    dry(&mut e);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("brush.png")).unwrap();

    // Flooding: a watercolour wash scrubbed over and over with clear water, calm vs wild.
    for (name, flow) in [("flood_calm", 0.0), ("flood_wild", 1.0)] {
        e.new_canvas(800, 800, 0, 7, 0);
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

        // Water that spread beyond the strokes is part of their undo.
        undo_to_blank(&mut e, name);
    }

    // Eraser across a wash: while water is standing it lifts and smears, once the
    // surface has soaked in (still damp) and when dry it erases.
    e.new_canvas(600, 400, 0, 7, 0);
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 40.0, pigment: from_srgb([0.2, 0.3, 0.7]), water: 1.0, ..Default::default() };
    for row in 0..3 {
        let y = 120.0 + row as f32 * 60.0;
        draw(&mut e, &(0..60).map(|i| InputPoint { x: 80.0 + i as f32 * 7.0, y, pressure: 0.8 }).collect::<Vec<_>>());
    }
    let line = |x: f32| (0..40).map(|i| InputPoint { x, y: 60.0 + i as f32 * 7.0, pressure: 0.8 }).collect::<Vec<_>>();
    e.brush = BrushSettings { tool: Tool::Eraser, size: 20.0, strength: 0.8, softness: 0.4, bristle: 0.0, ..Default::default() };
    draw(&mut e, &line(200.0));
    for _ in 0..200 {
        frame(&mut e);
    }
    assert!(e.canvas.as_ref().unwrap().any_wet());
    draw(&mut e, &line(330.0));
    dry(&mut e);
    draw(&mut e, &line(440.0));
    frame(&mut e);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("eraser.png")).unwrap();
    undo_to_blank(&mut e, "eraser");

    // Tipped paper: a wet wash, a puddle of clear water dropped into a wash, and
    // India ink in water, painted with the paper tipped towards the bottom edge.
    // Beads gather along the lower edges and run as drips.
    e.new_canvas(700, 600, 0, 7, 0);
    e.canvas.as_mut().unwrap().set_tilt(0.0, 1.0);
    let row = |x: f32, y: f32| (0..40).map(|i| InputPoint { x: x + i as f32 * 6.0, y, pressure: 0.9 }).collect::<Vec<_>>();
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 30.0, pigment: from_srgb([0.2, 0.35, 0.7]), water: 1.0, ..Default::default() };
    for r in 0..3 {
        draw(&mut e, &row(60.0, 80.0 + r as f32 * 40.0));
    }
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 30.0, pigment: from_srgb([0.75, 0.2, 0.3]), water: 0.8, ..Default::default() };
    draw(&mut e, &row(380.0, 80.0));
    e.brush = BrushSettings { tool: Tool::Water, size: 30.0, water: 1.0, softness: 0.3, ..Default::default() };
    for _ in 0..4 {
        draw(&mut e, &row(380.0, 85.0));
    }
    e.brush = BrushSettings { tool: Tool::Water, size: 30.0, water: 1.0, softness: 0.3, ..Default::default() };
    draw(&mut e, &row(380.0, 220.0));
    e.brush = BrushSettings { tool: Tool::Ink, size: 5.0, pigment: black, load: 0.9, bleed: 1.0, softness: 0.05, bristle: 0.0, pressure_size: 0.8, ..Default::default() };
    draw(&mut e, &row(380.0, 220.0));
    // A puddle scrubbed into a green wash.
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 30.0, pigment: from_srgb([0.2, 0.55, 0.3]), water: 1.0, ..Default::default() };
    draw(&mut e, &row(60.0, 300.0));
    e.brush = BrushSettings { tool: Tool::Water, size: 30.0, water: 1.0, softness: 0.3, ..Default::default() };
    for _ in 0..8 {
        draw(&mut e, &row(60.0, 300.0));
    }
    water_stats(&e, "tilt +0 frames");
    for k in 1..=480 {
        frame(&mut e);
        if k % 60 == 0 {
            water_stats(&e, &format!("tilt +{k} frames"));
        }
        if k == 90 {
            storage::export_png(e.canvas.as_mut().unwrap(), &out.join("tilt_wet.png")).unwrap();
        }
    }
    dry(&mut e);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("tilt.png")).unwrap();
    undo_to_blank(&mut e, "tilt");

    // Push: two identical wet washes; the right one is pushed outwards to the
    // right and upwards with the dry brush, and a dry wash below is pushed too
    // (it should not change).
    e.new_canvas(800, 600, 0, 7, 0);
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 34.0, pigment: from_srgb([0.75, 0.25, 0.2]), water: 0.9, ..Default::default() };
    for x in [100.0, 450.0] {
        for r in 0..3 {
            draw(&mut e, &(0..30).map(|i| InputPoint { x: x + i as f32 * 6.0, y: 100.0 + r as f32 * 40.0, pressure: 0.9 }).collect::<Vec<_>>());
        }
    }
    draw(&mut e, &(0..30).map(|i| InputPoint { x: 450.0 + i as f32 * 6.0, y: 450.0, pressure: 0.9 }).collect::<Vec<_>>());
    e.canvas.as_mut().unwrap().dry_layer(0);
    for r in 0..3 {
        draw(&mut e, &(0..30).map(|i| InputPoint { x: 100.0 + i as f32 * 6.0, y: 100.0 + r as f32 * 40.0, pressure: 0.9 }).collect::<Vec<_>>());
        draw(&mut e, &(0..30).map(|i| InputPoint { x: 450.0 + i as f32 * 6.0, y: 100.0 + r as f32 * 40.0, pressure: 0.9 }).collect::<Vec<_>>());
    }
    e.brush = BrushSettings { tool: Tool::Push, size: 30.0, strength: 0.9, softness: 0.4, bristle: 0.0, ..Default::default() };
    for k in 0..6 {
        let y = 100.0 + k as f32 * 16.0;
        draw(&mut e, &(0..25).map(|i| InputPoint { x: 560.0 + i as f32 * 8.0, y: y - i as f32 * 3.0, pressure: 0.9 }).collect::<Vec<_>>());
    }
    draw(&mut e, &(0..25).map(|i| InputPoint { x: 500.0 + i as f32 * 8.0, y: 450.0, pressure: 0.9 }).collect::<Vec<_>>());
    frame(&mut e);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("push_wet.png")).unwrap();
    // Scrubbing back and forth fast over a fresh wash: many dabs per frame over the same spot.
    e.brush = BrushSettings { tool: Tool::Watercolor, size: 34.0, pigment: from_srgb([0.2, 0.3, 0.7]), water: 1.0, ..Default::default() };
    for r in 0..3 {
        draw(&mut e, &(0..30).map(|i| InputPoint { x: 450.0 + i as f32 * 6.0, y: 140.0 + r as f32 * 20.0, pressure: 1.0 }).collect::<Vec<_>>());
    }
    water_stats(&e, "push scrub before");
    e.brush = BrushSettings { tool: Tool::Push, size: 30.0, strength: 1.0, softness: 0.4, bristle: 0.0, ..Default::default() };
    let scrub: Vec<InputPoint> = (0..120).map(|i| InputPoint { x: 450.0 + ((i as f32 * 0.9).sin() * 0.5 + 0.5) * 180.0, y: 150.0 + (i % 7) as f32 * 4.0, pressure: 1.0 }).collect();
    e.stroke(0, &scrub[..1]);
    for c in scrub[1..].chunks(6) {
        e.stroke(1, c);
        frame(&mut e);
    }
    e.stroke(2, &scrub[scrub.len() - 1..]);
    for k in 0..90 {
        frame(&mut e);
        if k % 30 == 0 {
            water_stats(&e, &format!("push scrub +{k}"));
        }
    }
    dry(&mut e);
    storage::export_png(e.canvas.as_mut().unwrap(), &out.join("push.png")).unwrap();
    undo_to_blank(&mut e, "push");
    println!("wrote {}", out.display());
}
