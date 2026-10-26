// Procedural watercolor paper: felted cold-press bumps + fine tooth + fibre field.

struct PaperParams {
    size: vec2<i32>,
    seed: u32,
    roughness: f32,
    grain: f32,
    fibre: f32,
    sizing: f32,
    _pad: f32,
}

@group(0) @binding(0) var<uniform> PP: PaperParams;
@group(0) @binding(1) var out_tex: texture_storage_2d<rgba8unorm, write>;

fn hash2(p: vec2<f32>) -> vec2<f32> {
    let s = f32(PP.seed % 9973u) * 0.137;
    var q = vec2<f32>(dot(p, vec2<f32>(127.1, 311.7)) + s, dot(p, vec2<f32>(269.5, 183.3)) - s);
    return fract(sin(q) * 43758.5453);
}

fn vnoise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = hash2(i).x;
    let b = hash2(i + vec2<f32>(1.0, 0.0)).x;
    let c = hash2(i + vec2<f32>(0.0, 1.0)).x;
    let d = hash2(i + vec2<f32>(1.0, 1.0)).x;
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

fn fbm(p: vec2<f32>) -> f32 {
    var v = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 5; i++) {
        v += amp * vnoise(q);
        q = q * 2.03 + vec2<f32>(17.0, 9.0);
        amp *= 0.5;
    }
    return v;
}

// Worley distance gives the soft dimples of mould-made cold press paper.
fn worley(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    var d = 8.0;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let o = vec2<f32>(f32(x), f32(y));
            let r = o + hash2(i + o) - f;
            d = min(d, dot(r, r));
        }
    }
    return sqrt(d);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (any(vec2<i32>(gid.xy) >= PP.size)) {
        return;
    }
    let p = vec2<f32>(gid.xy);
    let g = max(PP.grain, 1.0);
    let bumps = 1.0 - clamp(worley(p / (g * 1.6)), 0.0, 1.0);
    let tooth = fbm(p / (g * 0.45));
    let mottle = fbm(p / (g * 6.0));
    var height = mix(tooth, bumps * 0.7 + tooth * 0.3, 0.6) * 0.8 + mottle * 0.2;
    height = clamp(0.5 + (height - 0.5) * (0.6 + PP.roughness * 1.6), 0.0, 1.0);

    let angle = fbm(p / (g * 9.0) + vec2<f32>(40.0, 3.0));
    let fibre = clamp(PP.fibre * (0.4 + 1.2 * fbm(p / (g * 1.3) + vec2<f32>(7.0, 77.0))), 0.0, 1.0);
    let absorb = clamp(0.5 + (fbm(p / (g * 12.0) + vec2<f32>(91.0, 13.0)) - 0.5) * PP.sizing * 2.0, 0.0, 1.0);
    textureStore(out_tex, vec2<i32>(gid.xy), vec4<f32>(height, fract(angle * 2.0), fibre, absorb));
}
