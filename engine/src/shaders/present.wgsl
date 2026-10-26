// Draws the composite onto the screen with the current pan / zoom / rotation.

struct View {
    row0: vec4<f32>,   // canvas_from_screen affine, x row
    row1: vec4<f32>,   // canvas_from_screen affine, y row
    canvas: vec2<f32>,
    scale: f32,        // screen pixels per canvas pixel
    encode_srgb: f32,
    background: vec4<f32>,
}

@group(0) @binding(0) var<uniform> V: View;
@group(0) @binding(1) var composite: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn to_canvas(s: vec2<f32>) -> vec2<f32> {
    return vec2<f32>(dot(V.row0.xy, s) + V.row0.z, dot(V.row1.xy, s) + V.row1.z);
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = 1.055 * pow(c, vec3<f32>(1.0 / 2.4)) - 0.055;
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

@fragment
fn fs_main(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let c = to_canvas(frag.xy);
    // Signed distance (in screen px) to the canvas edge, for AA and a soft shadow.
    let d = max(-c, c - V.canvas);
    let outside = max(d.x, d.y) * V.scale;
    var col = V.background.rgb;
    if (outside > -1.0) {
        let shadow = 1.0 - 0.35 * exp(-max(outside, 0.0) / 14.0);
        col = col * shadow;
    }
    if (outside < 1.0) {
        let inv = 1.0 / V.canvas;
        var paint: vec3<f32>;
        if (V.scale < 0.9) {
            // Zoomed out: a small box filter keeps paper grain from shimmering.
            let o = 0.3 / V.scale;
            paint = 0.25 * (textureSampleLevel(composite, samp, (c + vec2<f32>(-o, -o)) * inv, 0.0).rgb
                + textureSampleLevel(composite, samp, (c + vec2<f32>(o, -o)) * inv, 0.0).rgb
                + textureSampleLevel(composite, samp, (c + vec2<f32>(-o, o)) * inv, 0.0).rgb
                + textureSampleLevel(composite, samp, (c + vec2<f32>(o, o)) * inv, 0.0).rgb);
        } else {
            paint = textureSampleLevel(composite, samp, c * inv, 0.0).rgb;
        }
        col = mix(paint, col, clamp(outside * 0.5 + 0.5, 0.0, 1.0));
    }
    if (V.encode_srgb > 0.5) {
        col = linear_to_srgb(clamp(col, vec3<f32>(0.0), vec3<f32>(1.0)));
    }
    return vec4<f32>(col, 1.0);
}
