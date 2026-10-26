// "Dry layer now": everything suspended settles, wet ink becomes permanent.

@group(0) @binding(0) var<uniform> P: SimParams;
@group(0) @binding(1) var pig_in: texture_2d<f32>;
@group(0) @binding(2) var ink_in: texture_2d<f32>;
@group(0) @binding(3) var dep_in: texture_2d<f32>;
@group(0) @binding(4) var fix_in: texture_2d<f32>;
@group(0) @binding(5) var dep_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(6) var fix_out: texture_storage_2d<rgba16float, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (out_of_rect(gid)) {
        return;
    }
    let p = cell(gid);
    textureStore(dep_out, p, textureLoad(dep_in, p, 0) + textureLoad(pig_in, p, 0));
    textureStore(fix_out, p, textureLoad(fix_in, p, 0) + textureLoad(ink_in, p, 0));
}
