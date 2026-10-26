// Kubelka-Munk optics. Each layer's pigments are stored as absorption K (rgb) and
// scattering S (a), already multiplied by the amount of pigment, so a texel's K/S
// is a physical mixture and thickness is implicit. A layer becomes (reflectance,
// transmittance) and layers stack with the KM compositing equations.

struct LayerU {
    origin: vec2<i32>,
    size: vec2<i32>,
    opacity: f32,
    wet: u32,
    first: u32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> L: LayerU;
@group(0) @binding(1) var dep: texture_2d<f32>;
@group(0) @binding(2) var fixed_ink: texture_2d<f32>;
@group(0) @binding(3) var pig: texture_2d<f32>;
@group(0) @binding(4) var ink: texture_2d<f32>;
@group(0) @binding(5) var water: texture_2d<f32>;
@group(0) @binding(6) var paper: texture_2d<f32>;
@group(0) @binding(7) var acc_in: texture_2d<f32>;
@group(0) @binding(8) var acc_t_in: texture_2d<f32>;
@group(0) @binding(9) var out_tex: texture_storage_2d<rgba16float, write>;
@group(0) @binding(10) var out_t: texture_storage_2d<rgba16float, write>;
@group(0) @binding(11) var above_r: texture_2d<f32>;
@group(0) @binding(12) var above_t: texture_2d<f32>;

// Work is dispatched over a list of 32x32 tiles: (16, tile count, 1) workgroups.
@group(0) @binding(20) var<storage, read> tiles: array<u32>;

fn tile_px(wg: vec3<u32>, li: vec3<u32>) -> vec2<i32> {
    let t = tiles[wg.y];
    let origin = vec2<i32>(i32(t & 0xffffu), i32(t >> 16u)) * 32;
    return origin + vec2<i32>(i32(wg.x % 4u) * 8 + i32(li.x), i32(wg.x / 4u) * 8 + i32(li.y));
}

struct RT {
    r: vec3<f32>,
    t: vec3<f32>,
}

fn km(K: vec3<f32>, S: f32) -> RT {
    let s = max(S, 1e-4);
    let a = 1.0 + K / s;
    let b = max(sqrt(max(a * a - 1.0, vec3<f32>(0.0))), vec3<f32>(1e-3));
    let bs = min(b * s, vec3<f32>(20.0));
    let e = exp(bs);
    let ei = 1.0 / e;
    let sh = 0.5 * (e - ei);
    let ch = 0.5 * (e + ei);
    let denom = a * sh + b * ch;
    return RT(sh / denom, b / denom);
}

fn layer_rt(p: vec2<i32>, wet_layer: bool) -> RT {
    var ks = textureLoad(dep, p, 0) + textureLoad(fixed_ink, p, 0);
    var film = 1.0;
    if (wet_layer) {
        ks += textureLoad(pig, p, 0) + textureLoad(ink, p, 0);
        let w = textureLoad(water, p, 0);
        // A water film darkens and saturates what is under it.
        film = 1.0 - 0.10 * clamp(w.x * 2.0, 0.0, 1.0) - 0.04 * clamp(w.y, 0.0, 1.0);
    }
    var rt = km(ks.rgb, ks.a);
    let op = L.opacity;
    rt.r = rt.r * op;
    rt.t = (1.0 - op + op * rt.t) * film;
    return rt;
}

// Layer (R1,T1) laid over a surface of reflectance R2.
fn over(l: RT, r2: vec3<f32>) -> vec3<f32> {
    return l.r + l.t * l.t * r2 / max(1.0 - l.r * r2, vec3<f32>(1e-4));
}

// L.size holds the canvas size.
fn off_canvas(p: vec2<i32>) -> bool {
    return any(p >= L.size);
}

@compute @workgroup_size(8, 8)
fn paper_init(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_px(wg, li);
    if (off_canvas(p)) {
        return;
    }
    let dims = vec2<i32>(textureDimensions(paper));
    let h = textureLoad(paper, p, 0).r;
    let hx = textureLoad(paper, min(p + vec2<i32>(1, 0), dims - 1), 0).r;
    let hy = textureLoad(paper, min(p + vec2<i32>(0, 1), dims - 1), 0).r;
    // Raking light from the upper left.
    let shade = 1.0 + 0.9 * ((h - hx) + (h - hy)) + 0.05 * (h - 0.5);
    let base = vec3<f32>(0.93, 0.915, 0.87);
    textureStore(out_tex, p, vec4<f32>(clamp(base * shade, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0));
}

@compute @workgroup_size(8, 8)
fn layer_over(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_px(wg, li);
    if (off_canvas(p)) {
        return;
    }
    let r = over(layer_rt(p, L.wet != 0u), textureLoad(acc_in, p, 0).rgb);
    textureStore(out_tex, p, vec4<f32>(r, 1.0));
}

// Accumulates the stack of layers above the active one, top-down, as one (R, T) pair.
@compute @workgroup_size(8, 8)
fn layer_under_stack(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_px(wg, li);
    if (off_canvas(p)) {
        return;
    }
    let lower = layer_rt(p, L.wet != 0u);
    var ur = vec3<f32>(0.0);
    var ut = vec3<f32>(1.0);
    if (L.first == 0u) {
        ur = textureLoad(acc_in, p, 0).rgb;
        ut = textureLoad(acc_t_in, p, 0).rgb;
    }
    let denom = max(1.0 - ur * lower.r, vec3<f32>(1e-4));
    let r = ur + ut * ut * lower.r / denom;
    let t = ut * lower.t / denom;
    textureStore(out_tex, p, vec4<f32>(r, 1.0));
    textureStore(out_t, p, vec4<f32>(t, 1.0));
}

@compute @workgroup_size(8, 8)
fn final_composite(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_px(wg, li);
    if (off_canvas(p)) {
        return;
    }
    var r = textureLoad(acc_in, p, 0).rgb;
    if (L.opacity > 0.0) {
        r = over(layer_rt(p, L.wet != 0u), r);
    }
    if (L.first == 0u) {
        let ar = textureLoad(above_r, p, 0).rgb;
        let at = textureLoad(above_t, p, 0).rgb;
        r = ar + at * at * r / max(1.0 - ar * r, vec3<f32>(1e-4));
    }
    textureStore(out_tex, p, vec4<f32>(r, 1.0));
}
