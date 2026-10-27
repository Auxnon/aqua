// Brush dabs are instanced quads rasterised straight into the layer's textures with
// additive (paint, water, ink) or multiplicative (eraser) blending. Every dab of a
// frame goes out in a single draw.

struct DabGlobals {
    canvas: vec2<f32>,
    _pad: vec2<f32>,
}

@group(0) @binding(0) var<uniform> G: DabGlobals;
@group(0) @binding(1) var paper: texture_2d<f32>;

struct DabIn {
    @location(0) pos_radius: vec4<f32>,
    @location(1) water_push: vec4<f32>,
    @location(2) pigment: vec4<f32>,
    @location(3) ink: vec4<f32>,
    @location(4) shape: vec4<f32>,
}

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) water_push: vec4<f32>,
    @location(2) @interpolate(flat) pigment: vec4<f32>,
    @location(3) @interpolate(flat) ink: vec4<f32>,
    @location(4) @interpolate(flat) shape: vec4<f32>,
}

const MARGIN: f32 = 1.15;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32, d: DabIn) -> VOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[vi] * MARGIN;
    let ang = d.pos_radius.w;
    let rot = mat2x2<f32>(cos(ang), sin(ang), -sin(ang), cos(ang));
    let world = d.pos_radius.xy + rot * (c * d.pos_radius.z);
    var o: VOut;
    o.clip = vec4<f32>(world.x / G.canvas.x * 2.0 - 1.0, 1.0 - world.y / G.canvas.y * 2.0, 0.0, 1.0);
    o.local = c;
    o.water_push = d.water_push;
    o.pigment = d.pigment;
    o.ink = d.ink;
    o.shape = d.shape;
    return o;
}

fn h1(x: f32) -> f32 {
    return fract(sin(x * 127.1) * 43758.5453);
}

fn n1(x: f32) -> f32 {
    let i = floor(x);
    let f = fract(x);
    return mix(h1(i), h1(i + 1.0), f * f * (3.0 - 2.0 * f));
}

// shape = (softness, bristle, seed, strength); water_push.w = dryness of the brush.
fn coverage(v: VOut, frag: vec4<f32>) -> f32 {
    let local = v.local;
    let r = length(local);
    let seed = v.shape.z;
    let bristle = v.shape.y;
    let ang = atan2(local.y, local.x);
    let rim = 1.0 - 0.08 * bristle * n1(ang * 2.5 + seed * 7.0);
    let soft = max(v.shape.x, 0.01);
    var c = 1.0 - smoothstep(rim - soft, rim, r);
    // Hair streaks run along the stroke (local x), so they vary across it (local y).
    let streak = n1(local.y * 9.0 + seed * 31.0) * 0.6 + n1(local.y * 23.0 + seed * 11.0) * 0.4;
    c *= mix(1.0, 0.45 + 1.1 * streak, bristle * 0.55);
    let dryness = v.water_push.w;
    if (dryness > 0.0) {
        let h = textureLoad(paper, vec2<i32>(frag.xy), 0).r;
        let catchv = smoothstep(0.85 - dryness * 0.6, 0.95 - dryness * 0.35, h);
        c *= mix(1.0, catchv, dryness);
    }
    return clamp(c, 0.0, 1.5);
}

struct WetOut {
    @location(0) water: vec4<f32>,
    @location(1) pigment: vec4<f32>,
    @location(2) ink: vec4<f32>,
    @location(3) flux: vec4<f32>,
}

@fragment
fn fs_wet(v: VOut) -> WetOut {
    let c = coverage(v, v.clip);
    let push = v.water_push.yz * c;
    var o: WetOut;
    // z marks freshly delivered water; settle_water uses it to stop a brush
    // flooding paper that is already soaked.
    o.water = vec4<f32>(v.water_push.x * c, 0.0, v.water_push.x * c, 0.0);
    o.pigment = v.pigment * c;
    o.ink = v.ink * c;
    o.flux = vec4<f32>(max(-push.x, 0.0), max(push.x, 0.0), max(-push.y, 0.0), max(push.y, 0.0));
    return o;
}

@fragment
fn fs_fix(v: VOut) -> @location(0) vec4<f32> {
    return v.ink * coverage(v, v.clip);
}

struct Erase2 {
    @location(0) a: vec4<f32>,
    @location(1) b: vec4<f32>,
}

@fragment
fn fs_erase2(v: VOut) -> Erase2 {
    let e = vec4<f32>(0.0, 0.0, 0.0, clamp(coverage(v, v.clip) * v.shape.w, 0.0, 1.0));
    return Erase2(e, e);
}

struct Erase3 {
    @location(0) a: vec4<f32>,
    @location(1) b: vec4<f32>,
    @location(2) c: vec4<f32>,
}

@fragment
fn fs_erase3(v: VOut) -> Erase3 {
    let e = vec4<f32>(0.0, 0.0, 0.0, clamp(coverage(v, v.clip) * v.shape.w, 0.0, 1.0));
    return Erase3(e, e, e);
}
