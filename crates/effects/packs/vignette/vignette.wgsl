// builtin.vignette — Edge darkening and focus vignette
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    radius: f32,
    softness: f32,
    intensity: f32,
    color: vec4<f32>,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    let dist = distance(uv, vec2<f32>(0.5, 0.5));
    let inner = params.radius;
    let outer = params.radius + max(0.001, params.softness);

    let factor = smoothstep(inner, outer, dist) * params.intensity;
    let factor_clamped = clamp(factor, 0.0, 1.0);

    let blended_rgb = mix(src.rgb, params.color.rgb, factor_clamped * params.color.a);
    return vec4<f32>(blended_rgb, src.a);
}
