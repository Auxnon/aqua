// Shared declarations prepended to every compute shader that walks a rect.

struct SimParams {
    origin: vec2<i32>,
    size: vec2<i32>,
    canvas: vec2<i32>,
    dt: f32,
    gravity: f32,
    damping: f32,
    paper_influence: f32,
    tension: f32,
    absorb: f32,
    capillary: f32,
    cap_threshold: f32,
    evap_water: f32,
    evap_paper: f32,
    edge_evap: f32,
    deposit: f32,
    lift: f32,
    granulation: f32,
    bloom: f32,
    diffuse: f32,
    ink_bleed: f32,
    ink_fix: f32,
    staining: f32,
    edge_drift: f32,
    // How unruly deep water gets (the layer's Flow setting), and simulation time.
    chaos: f32,
    time: f32,
    // The paper's downhill direction, length 0 (flat) to 1 (steep).
    tilt: vec2<f32>,
}

const EPS_WATER: f32 = 1e-3;
const MAX_WATER: f32 = 3.0;
const DIAG: f32 = 0.70710678;

// Sparse simulation: only 32x32 tiles near wet paint are computed. Each tile is
// 4x4 workgroups of 8x8; dispatch is (16, tile count, 1).
const TILE: i32 = 32;
@group(0) @binding(20) var<storage, read> tiles: array<u32>;
@group(0) @binding(21) var tile_mask: texture_2d<f32>;

fn tile_cell(wg: vec3<u32>, li: vec3<u32>) -> vec2<i32> {
    let t = tiles[wg.y];
    let origin = vec2<i32>(i32(t & 0xffffu), i32(t >> 16u)) * TILE;
    return origin + vec2<i32>(i32(wg.x % 4u) * 8 + i32(li.x), i32(wg.x / 4u) * 8 + i32(li.y));
}

fn off_canvas(p: vec2<i32>) -> bool {
    return any(p >= P.canvas);
}

// Cells outside the active tiles are dry walls.
fn inside(q: vec2<i32>) -> bool {
    if (any(q < vec2<i32>(0)) || any(q >= P.canvas)) {
        return false;
    }
    return textureLoad(tile_mask, q / TILE, 0).r > 0.5;
}

fn cell(gid: vec3<u32>) -> vec2<i32> {
    return P.origin + vec2<i32>(gid.xy);
}

fn out_of_rect(gid: vec3<u32>) -> bool {
    return any(vec2<i32>(gid.xy) >= P.size);
}

// The 8-neighbourhood, unrolled through a lookup function so loops stay simple.
fn nb(i: i32) -> vec2<i32> {
    switch i {
        case 0: { return vec2<i32>(-1, 0); }
        case 1: { return vec2<i32>(1, 0); }
        case 2: { return vec2<i32>(0, -1); }
        case 3: { return vec2<i32>(0, 1); }
        case 4: { return vec2<i32>(-1, -1); }
        case 5: { return vec2<i32>(1, -1); }
        case 6: { return vec2<i32>(-1, 1); }
        default: { return vec2<i32>(1, 1); }
    }
}

fn nb_weight(i: i32) -> f32 {
    return select(1.0, DIAG, i >= 4);
}

// Paper texel: r = height, g = fibre angle (0..1 -> 0..pi), b = fibre strength, a = absorbency.
// Capillary conductance along a direction: water wicks faster along fibres.
fn fibre_conductance(paper: vec4<f32>, d: vec2<i32>) -> f32 {
    let theta = paper.g * 3.14159265;
    let dir = normalize(vec2<f32>(d));
    let align = abs(dot(dir, vec2<f32>(cos(theta), sin(theta))));
    return mix(1.0, 0.15 + 1.85 * align * align, paper.b) * (0.6 + 0.8 * paper.a);
}

// An integer hash (PCG-style) in 0..1. The usual fract(sin(...)) loses its
// randomness for large arguments on phone GPUs and turns into visible patterns.
fn hash12(p: vec2<i32>) -> f32 {
    var v = bitcast<vec2<u32>>(p) * 1664525u + 1013904223u;
    v.x += v.y * 1664525u;
    v.y += v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    v.x += v.y * 1664525u;
    v.y += v.x * 1664525u;
    v = v ^ (v >> vec2<u32>(16u));
    return f32(v.y >> 8u) * (1.0 / 16777216.0);
}

// Smooth value noise: no edges between lattice cells.
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
