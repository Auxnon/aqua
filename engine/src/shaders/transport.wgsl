// Moves surface water and everything suspended in it along the pipe fluxes.
// Conservative: what leaves one cell arrives in its neighbour.

@group(0) @binding(0) var<uniform> P: SimParams;
@group(0) @binding(1) var water_in: texture_2d<f32>;
@group(0) @binding(2) var flux: texture_2d<f32>;
@group(0) @binding(3) var pig_in: texture_2d<f32>;
@group(0) @binding(4) var ink_in: texture_2d<f32>;
@group(0) @binding(5) var water_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(6) var pig_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(7) var ink_out: texture_storage_2d<rgba16float, write>;

struct Acc {
    h: f32,
    pig: vec4<f32>,
    ink: vec4<f32>,
}

fn inflow(acc: ptr<function, Acc>, q: vec2<i32>, comp: i32) {
    if (!inside(q)) {
        return;
    }
    let fq = textureLoad(flux, q, 0)[comp] * P.dt;
    if (fq <= 0.0) {
        return;
    }
    let hq = textureLoad(water_in, q, 0).x;
    let frac = min(fq / max(hq, 1e-6), 1.0);
    (*acc).h += fq;
    (*acc).pig += textureLoad(pig_in, q, 0) * frac;
    (*acc).ink += textureLoad(ink_in, q, 0) * frac;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_cell(wg, li);
    if (off_canvas(p)) {
        return;
    }
    let w = textureLoad(water_in, p, 0);
    let f = textureLoad(flux, p, 0);
    let out = (f.x + f.y + f.z + f.w) * P.dt;
    let keep = select(1.0, clamp(1.0 - out / w.x, 0.0, 1.0), w.x > 1e-6);

    var acc: Acc;
    acc.h = max(w.x - out, 0.0);
    acc.pig = textureLoad(pig_in, p, 0) * keep;
    acc.ink = textureLoad(ink_in, p, 0) * keep;
    // Each neighbour sends the component of its flux that points at us.
    inflow(&acc, p + vec2<i32>(1, 0), 0);
    inflow(&acc, p + vec2<i32>(-1, 0), 1);
    inflow(&acc, p + vec2<i32>(0, 1), 2);
    inflow(&acc, p + vec2<i32>(0, -1), 3);

    textureStore(water_out, p, vec4<f32>(acc.h, w.y, w.z, w.w));
    textureStore(pig_out, p, acc.pig);
    textureStore(ink_out, p, acc.ink);
}
