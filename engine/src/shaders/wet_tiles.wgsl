// Reduces each active tile's maximum wetness into one atomic per tile, so the CPU
// can retire tiles that have dried and release the layer once all of it is dry.

@group(0) @binding(0) var<uniform> P: SimParams;
@group(0) @binding(1) var water: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> result: array<atomic<u32>>;

var<workgroup> local_max: atomic<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(workgroup_id) wg: vec3<u32>, @builtin(local_invocation_id) li: vec3<u32>, @builtin(local_invocation_index) lii: u32) {
    if (lii == 0u) {
        atomicStore(&local_max, 0u);
    }
    workgroupBarrier();
    let p = tile_cell(wg, li);
    if (!off_canvas(p)) {
        let w = textureLoad(water, p, 0);
        // Non-negative floats order the same as their bit patterns.
        atomicMax(&local_max, bitcast<u32>(max(w.x * 4.0 + w.y, 0.0)));
    }
    workgroupBarrier();
    if (lii == 0u) {
        let t = tiles[wg.y];
        let tw = u32((P.canvas.x + TILE - 1) / TILE);
        atomicMax(&result[(t >> 16u) * tw + (t & 0xffffu)], atomicLoad(&local_max));
    }
}
