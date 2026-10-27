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

fn vnoise(x: vec2<f32>) -> f32 {
    let i = floor(x);
    let f = fract(x);
    let u = f * f * (3.0 - 2.0 * f);
    let c = vec2<i32>(i);
    return mix(
        mix(hash12(c), hash12(c + vec2<i32>(1, 0)), u.x),
        mix(hash12(c + vec2<i32>(0, 1)), hash12(c + vec2<i32>(1, 1)), u.x),
        u.y,
    );
}

// How strongly the layer's Flow setting acts on water this deep. Ordinary strokes
// are shallower than the onset and behave exactly as with Flow at zero.
fn flood(h: f32) -> f32 {
    return P.chaos * smoothstep(2.0, 2.9, h);
}

// Flooded water doesn't spread evenly: it finds channels. A slowly drifting noise
// field scales how easily water crosses each cell edge, so a flood breaks into
// wandering runs and fingers. Stays within (0.15, 1] to keep the scheme stable.
fn channel(p: vec2<i32>, q: vec2<i32>, amount: f32) -> f32 {
    if (amount <= 0.0) {
        return 1.0;
    }
    let m = vec2<f32>(p + q) * 0.5;
    let t = P.time;
    let n = vnoise(m / 11.0 + vec2<f32>(t * 0.004, -t * 0.003)) * 0.65
        + vnoise(m / 3.5 + vec2<f32>(-t * 0.009, t * 0.007)) * 0.35;
    return mix(1.0, mix(0.15, 1.0, smoothstep(0.25, 0.75, n)), amount);
}

// Soaked paper buckles. Under a flood the buckles become slopes: water runs off
// the crests into the troughs and down them, which is what makes a flood wander
// and break out in runs instead of sitting as a round puddle. Drifts very slowly
// as the paper relaxes.
fn cockle(p: vec2<i32>) -> f32 {
    let x = vec2<f32>(p) + vec2<f32>(3711.0, 1291.0);
    let t = P.time * 0.0004;
    return vnoise(x / 70.0 + vec2<f32>(t, t * 0.7)) * 0.7 + vnoise(x / 26.0 - vec2<f32>(t * 1.3, t)) * 0.3;
}

const COCKLE_HEIGHT: f32 = 16.0;

fn pipe(p: vec2<i32>, q: vec2<i32>, h: f32, H: f32, old: f32) -> f32 {
    if (!inside(q)) {
        return 0.0;
    }
    let wq = textureLoad(water_in, q, 0);
    let Hq = wq.x + P.paper_influence * textureLoad(paper, q, 0).r;
    // Deep water keeps its momentum, so a flood surges out
    // as a moving front instead of creeping like a thin wash. Judged by the deeper
    // side of the edge so both directions see the same pipe (otherwise the
    // cockle slope would pump water one way).
    let deep = flood(max(h, wq.x));
    let g = channel(p, q, deep);
    let damping = mix(P.damping, 0.9, deep);
    // (A stronger pull than P.gravity is unstable in this scheme; momentum is not.)
    let gravity = P.gravity * g;
    let slope = deep * COCKLE_HEIGHT * (cockle(p) - cockle(q));
    var f = max(0.0, old * damping + P.dt * gravity * (H - Hq + slope));
    // Surface tension: a thin film does not creep onto dry paper; a puddle does.
    // Where the channels run, the front breaks through sooner.
    let q_wet = wq.x > EPS_WATER || wq.y > P.cap_threshold;
    if (!q_wet && h * mix(1.0, g * 1.6, deep) < P.tension) {
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
