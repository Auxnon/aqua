// Paper absorbs surface water, capillary action wicks it along fibres, both evaporate.
// Cells on the rim of a wet patch evaporate faster, which pulls water (and pigment)
// outward and produces watercolor's dark dried edges.

@group(0) @binding(0) var<uniform> P: SimParams;
@group(0) @binding(1) var water_in: texture_2d<f32>;
@group(0) @binding(2) var paper: texture_2d<f32>;
@group(0) @binding(3) var water_out: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>) {
    let p = tile_cell(wg, li);
    if (off_canvas(p)) {
        return;
    }
    let w = textureLoad(water_in, p, 0);
    let pp = textureLoad(paper, p, 0);
    // A brush can't flood paper past MAX_WATER: once the surface is that wet it
    // stops releasing water. Only water the brush just delivered (w.z, written by
    // the dabs) is held back; water flowing in from elsewhere is untouched. This
    // also keeps depths where 16-bit storage still resolves a step's flow and
    // evaporation; much deeper water would freeze in place.
    var h = w.x - clamp(w.x - MAX_WATER, 0.0, w.z);
    let s0 = w.y;
    var s = s0;

    // Absorption into the paper's fibres.
    let a = min(h, P.absorb * P.dt * max(0.0, 1.0 - s0) * (0.5 + pp.a));
    h -= a;
    s += a;

    // Capillary wicking (symmetric conductance keeps it conservative).
    var ds = 0.0;
    var dry_neighbours = 0.0;
    for (var i = 0; i < 8; i++) {
        let d = nb(i);
        let q = p + d;
        if (!inside(q)) {
            if (i < 4) { dry_neighbours += 1.0; }
            continue;
        }
        let wq = textureLoad(water_in, q, 0);
        if (i < 4 && wq.x <= EPS_WATER) {
            dry_neighbours += 1.0;
        }
        let g = 0.5 * (fibre_conductance(pp, d) + fibre_conductance(textureLoad(paper, q, 0), d)) * nb_weight(i);
        let sq = wq.y;
        if (sq > s0 && sq > P.cap_threshold) {
            ds += g * (sq - s0);
        } else if (s0 > sq && s0 > P.cap_threshold) {
            ds -= g * (s0 - sq);
        }
    }
    s += ds * P.capillary * P.dt;

    // Evaporation, faster at the rim of a wet area.
    var edge = 0.0;
    if (w.x > EPS_WATER) {
        edge = dry_neighbours * 0.25;
    }
    h = max(0.0, h - P.evap_water * P.dt * (1.0 + P.edge_evap * edge));
    let paper_evap = select(0.3, 1.0, h <= EPS_WATER);
    s = clamp(s - P.evap_paper * P.dt * paper_evap, 0.0, 1.5);
    if (h < 1e-5) {
        h = 0.0;
    }
    textureStore(water_out, p, vec4<f32>(h, s, 0.0, 0.0));
}
