// builtin.chromatic_aberration — RGB channel shift & optical dispersion
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    amount: f32,
    angle: f32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    if (params.amount <= 0.0) {
        return textureSample(u_texture, u_sampler, uv);
    }

    let texel = 1.0 / u_resolution;
    let rad = radians(params.angle);
    let dir = vec2<f32>(cos(rad), sin(rad)) * params.amount * texel;

    let r_col = textureSample(u_texture, u_sampler, uv + dir);
    let g_col = textureSample(u_texture, u_sampler, uv);
    let b_col = textureSample(u_texture, u_sampler, uv - dir);

    let max_a = max(r_col.a, max(g_col.a, b_col.a));
    return vec4<f32>(r_col.r, g_col.g, b_col.b, max_a);
}
