// Reduces the maximum wetness of a rect into one atomic so the CPU knows when a layer is dry.

@group(0) @binding(0) var<uniform> P: SimParams;
@group(0) @binding(1) var water: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> result: atomic<u32>;

var<workgroup> local_max: atomic<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>, @builtin(local_invocation_index) li: u32) {
    if (li == 0u) {
        atomicStore(&local_max, 0u);
    }
    workgroupBarrier();
    if (!out_of_rect(gid)) {
        let w = textureLoad(water, cell(gid), 0);
        // Non-negative floats order the same as their bit patterns.
        atomicMax(&local_max, bitcast<u32>(max(w.x * 4.0 + w.y, 0.0)));
    }
    workgroupBarrier();
    if (li == 0u) {
        atomicMax(&result, atomicLoad(&local_max));
    }
}
