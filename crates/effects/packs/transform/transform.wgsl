// builtin.transform — 2D spatial distortion & placement
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    offset_x: f32,
    offset_y: f32,
    scale_x: f32,
    scale_y: f32,
    rotation: f32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    var p = uv - center;

    // Inverse translation
    let texel = 1.0 / u_resolution;
    p = p - vec2<f32>(params.offset_x, params.offset_y) * texel;

    // Inverse rotation
    let rad = -radians(params.rotation);
    let cos_r = cos(rad);
    let sin_r = sin(rad);
    p = vec2<f32>(
        p.x * cos_r - p.y * sin_r,
        p.x * sin_r + p.y * cos_r
    );

    // Inverse scale
    let sx = select(1.0, params.scale_x, abs(params.scale_x) > 1e-4);
    let sy = select(1.0, params.scale_y, abs(params.scale_y) > 1e-4);
    p = p / vec2<f32>(sx, sy);

    let src_uv = p + center;

    if (src_uv.x < 0.0 || src_uv.x > 1.0 || src_uv.y < 0.0 || src_uv.y > 1.0) {
        return vec4<f32>(0.0);
    }

    return textureSample(u_texture, u_sampler, src_uv);
}
