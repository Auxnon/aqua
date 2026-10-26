// Virtual-pipe shallow water: outflow flux per direction (x=left, y=right, z=up, w=down).

@group(0) @binding(0) var<uniform> P: SimParams;
@group(0) @binding(1) var water_in: texture_2d<f32>;
@group(0) @binding(2) var flux_in: texture_2d<f32>;
@group(0) @binding(3) var paper: texture_2d<f32>;
@group(0) @binding(4) var flux_out: texture_storage_2d<rgba16float, write>;

fn surface(q: vec2<i32>) -> vec2<f32> {
    let w = textureLoad(water_in, q, 0);
    return vec2<f32>(w.x + P.paper_influence * textureLoad(paper, q, 0).r, w.y);
}

fn pipe(p: vec2<i32>, q: vec2<i32>, h: f32, H: f32, old: f32) -> f32 {
    if (!inside(q)) {
        return 0.0;
    }
    let wq = textureLoad(water_in, q, 0);
    let Hq = wq.x + P.paper_influence * textureLoad(paper, q, 0).r;
    var f = max(0.0, old * P.damping + P.dt * P.gravity * (H - Hq));
    // Surface tension: a thin film does not creep onto dry paper; a puddle does.
    let q_wet = wq.x > EPS_WATER || wq.y > P.cap_threshold;
    if (!q_wet && h < P.tension) {
        f = 0.0;
    }
    return f;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_cell(wg, li);
    if (off_canvas(p)) {
        return;
    }
    let h = textureLoad(water_in, p, 0).x;
    var f = vec4<f32>(0.0);
    if (h > EPS_WATER * 0.1) {
        let H = h + P.paper_influence * textureLoad(paper, p, 0).r;
        let old = textureLoad(flux_in, p, 0);
        f = vec4<f32>(
            pipe(p, p + vec2<i32>(-1, 0), h, H, old.x),
            pipe(p, p + vec2<i32>(1, 0), h, H, old.y),
            pipe(p, p + vec2<i32>(0, -1), h, H, old.z),
            pipe(p, p + vec2<i32>(0, 1), h, H, old.w),
        );
        let total = (f.x + f.y + f.z + f.w) * P.dt;
        if (total > h) {
            f *= h / total;
        }
    }
    textureStore(flux_out, p, f);
}
