// builtin.tint — Dual-tone luminance color remapping
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    black_color: vec4<f32>,
    white_color: vec4<f32>,
    amount: f32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    let luma = dot(src.rgb, vec3<f32>(0.299, 0.587, 0.114));
    let tinted_rgb = mix(params.black_color.rgb, params.white_color.rgb, clamp(luma, 0.0, 1.0));
    let out_rgb = mix(src.rgb, tinted_rgb, clamp(params.amount, 0.0, 1.0));

    return vec4<f32>(out_rgb, src.a);
}
