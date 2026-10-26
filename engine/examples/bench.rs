//! Times the wet simulation on a large canvas covered in wet paint.
//! `cargo run --release --example bench`

use aqua_engine::brush::{BrushSettings, InputPoint};
use aqua_engine::engine::Engine;
use aqua_engine::gpu::{Gpu, read_pixel};

fn main() {
    let gpu = Gpu::headless().expect("gpu");
    let mut e = Engine::new(gpu.clone());
    e.new_canvas(2048, 2048, 0, 3);
    e.brush = BrushSettings { size: 120.0, water: 1.0, ..Default::default() };
    // Zig-zag wash over the whole sheet.
    for row in 0..9 {
        let y = 100.0 + row as f32 * 220.0;
        let pts: Vec<InputPoint> = (0..=40).map(|i| InputPoint { x: 60.0 + i as f32 * 48.0, y, pressure: 1.0 }).collect();
        e.stroke(0, &pts[..1]);
        e.stroke(1, &pts[1..]);
        e.stroke(2, &pts[40..]);
        e.canvas.as_mut().unwrap().update(2);
    }
    run(&gpu, &mut e, "fully wet");

    // A typical gesture: one long diagonal stroke and a cross stroke.
    e.new_canvas(2048, 2048, 0, 3);
    e.brush = BrushSettings { size: 40.0, water: 0.9, ..Default::default() };
    for (a, b) in [((80.0, 80.0), (1960.0, 1960.0)), ((1960.0, 80.0), (80.0, 1960.0))] {
        let pts: Vec<InputPoint> = (0..=80)
            .map(|i: i32| {
                let t = i as f32 / 80.0;
                InputPoint { x: a.0 + (b.0 - a.0) * t, y: a.1 + (b.1 - a.1) * t, pressure: 1.0 }
            })
            .collect();
        e.stroke(0, &pts[..1]);
        e.stroke(1, &pts[1..]);
        e.stroke(2, &pts[80..]);
        e.canvas.as_mut().unwrap().update(2);
    }
    run(&gpu, &mut e, "two diagonal strokes");
}

fn run(gpu: &Gpu, e: &mut Engine, what: &str) {
    let sync = |gpu: &Gpu, e: &Engine| drop(read_pixel(gpu, &e.canvas.as_ref().unwrap().composite.tex, 0, 0));
    sync(gpu, e);
    for steps in [1u32, 2] {
        let t = std::time::Instant::now();
        let frames = 60;
        for _ in 0..frames {
            e.canvas.as_mut().unwrap().update(steps);
        }
        sync(gpu, e);
        let ms = t.elapsed().as_secs_f64() * 1000.0 / frames as f64;
        println!("2048x2048 {what}, {steps} step(s)/frame: {ms:.2} ms/frame");
    }
}
