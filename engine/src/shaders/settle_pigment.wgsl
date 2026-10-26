// Pigment exchange between water and paper:
//  - suspended pigment diffuses within surface water (soft wet-in-wet bleeds)
//  - capillary flow drags pigment towards drier paper (blooms / backruns)
//  - pigment settles faster in thin water and paper valleys (edges, granulation)
//  - water re-lifts unfixed deposited pigment
//  - wet ink feathers along fibres and becomes permanent as it dries

@group(0) @binding(0) var<uniform> P: SimParams;
@group(0) @binding(1) var water: texture_2d<f32>;
@group(0) @binding(2) var paper: texture_2d<f32>;
@group(0) @binding(3) var pig_in: texture_2d<f32>;
@group(0) @binding(4) var ink_in: texture_2d<f32>;
@group(0) @binding(5) var dep_in: texture_2d<f32>;
@group(0) @binding(6) var fix_in: texture_2d<f32>;
@group(0) @binding(7) var pig_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(8) var ink_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(9) var dep_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(10) var fix_out: texture_storage_2d<rgba16float, write>;

// How freely settled-but-damp pigment can still be pushed around by capillary flow.
fn mobility(s: f32) -> f32 {
    return smoothstep(P.cap_threshold, 1.0, s) * (1.0 - P.staining);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_cell(wg, li);
    if (off_canvas(p)) {
        return;
    }
    let w = textureLoad(water, p, 0);
    let h = w.x;
    let s = w.y;
    let pp = textureLoad(paper, p, 0);
    let pig0 = textureLoad(pig_in, p, 0);
    let ink0 = textureLoad(ink_in, p, 0);
    let dep0 = textureLoad(dep_in, p, 0);
    var fix = textureLoad(fix_in, p, 0);
    let mob0 = mobility(s);

    var dpig = vec4<f32>(0.0);
    var dink = vec4<f32>(0.0);
    var ddep = vec4<f32>(0.0);
    for (var i = 0; i < 8; i++) {
        let d = nb(i);
        let q = p + d;
        if (!inside(q)) {
            continue;
        }
        let wq = textureLoad(water, q, 0);
        let pigq = textureLoad(pig_in, q, 0);
        let inkq = textureLoad(ink_in, q, 0);
        let nw = nb_weight(i);

        if (h > EPS_WATER && wq.x > EPS_WATER) {
            dpig += P.diffuse * nw * (pigq - pig0);
            dink += P.diffuse * 1.5 * nw * (inkq - ink0);
            // Coffee-ring flow: evaporation is fastest where the film is thin, so
            // particles are carried from deep water towards the drying rim.
            let to_q = min(P.edge_drift * max(h - wq.x, 0.0), 0.08) * nw;
            let from_q = min(P.edge_drift * max(wq.x - h, 0.0), 0.08) * nw;
            dpig += pigq * from_q - pig0 * to_q;
        }

        let g = 0.5 * (fibre_conductance(pp, d) + fibre_conductance(textureLoad(paper, q, 0), d)) * nw;
        var t_in = 0.0;
        var t_out = 0.0;
        if (wq.y > s && wq.y > P.cap_threshold) {
            t_in = g * (wq.y - s);
        } else if (s > wq.y && s > P.cap_threshold) {
            t_out = g * (s - wq.y);
        }
        dpig += P.bloom * (pigq * t_in - pig0 * t_out);
        let depq = textureLoad(dep_in, q, 0);
        ddep += P.bloom * 0.5 * (depq * mobility(wq.y) * t_in - dep0 * mob0 * t_out);
        dink += P.ink_bleed * (inkq * t_in - ink0 * t_out);
    }
    var pig = max(pig0 + dpig * P.dt, vec4<f32>(0.0));
    var ink = max(ink0 + dink * P.dt, vec4<f32>(0.0));
    var dep = max(dep0 + ddep * P.dt, vec4<f32>(0.0));

    if (h <= EPS_WATER) {
        // No surface water left: everything suspended falls onto the paper.
        dep += pig;
        pig = vec4<f32>(0.0);
    } else {
        let thin = 1.0 / (1.0 + 6.0 * h);
        let valley = 1.0 - pp.r;
        // Heavy pigment particles collect in the paper's valleys and clump (flocculate).
        let floc = mix(1.0, 0.3 + 1.4 * hash12(p / 2), clamp(P.granulation * 0.6, 0.0, 1.0));
        let gran = max(0.05, (1.0 + P.granulation * (2.0 * valley - 0.8)) * floc);
        let rd = clamp(P.deposit * P.dt * thin * gran, 0.0, 1.0);
        let rl = clamp(P.lift * P.dt * (h / (h + 0.4)) * (0.4 + pp.r) * (1.0 - P.staining), 0.0, 1.0);
        let down = pig * rd;
        let up = dep * rl;
        pig += up - down;
        dep += down - up;
    }

    // India ink sets as the paper dries; once in `fix` water can no longer move it.
    let wet = clamp(h * 2.0 + s, 0.0, 1.0);
    if (wet < 0.03) {
        fix += ink;
        ink = vec4<f32>(0.0);
    } else {
        let r = clamp(P.ink_fix * P.dt * (1.0 - wet), 0.0, 1.0);
        fix += ink * r;
        ink -= ink * r;
    }

    textureStore(pig_out, p, pig);
    textureStore(ink_out, p, ink);
    textureStore(dep_out, p, dep);
    textureStore(fix_out, p, fix);
}
