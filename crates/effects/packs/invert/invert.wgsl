// builtin.invert — Invert colors and optional alpha transparency
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    amount: f32,
    invert_alpha: u32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    let inverted_rgb = vec3<f32>(1.0) - src.rgb;
    let out_rgb = mix(src.rgb, inverted_rgb, clamp(params.amount, 0.0, 1.0));

    var out_a = src.a;
    if (params.invert_alpha != 0u) {
        let inverted_a = 1.0 - src.a;
        out_a = mix(src.a, inverted_a, clamp(params.amount, 0.0, 1.0));
    }

    return vec4<f32>(out_rgb, out_a);
}
