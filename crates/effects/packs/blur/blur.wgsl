// builtin.blur — Gaussian blur effect
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    radius: f32,
    repeat_edge: u32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    if (params.radius <= 0.0) {
        return textureSample(u_texture, u_sampler, uv);
    }

    let texel = 1.0 / u_resolution;
    var acc = vec4<f32>(0.0);
    var total_weight: f32 = 0.0;
    let r = max(1, i32(ceil(params.radius)));
    let sigma = max(0.5, params.radius * 0.5);
    let two_sigma_sq = 2.0 * sigma * sigma;

    for (var dy = -r; dy <= r; dy = dy + 1) {
        for (var dx = -r; dx <= r; dx = dx + 1) {
            let dist_sq = f32(dx * dx + dy * dy);
            let weight = exp(-dist_sq / two_sigma_sq);
            let offset = vec2<f32>(f32(dx), f32(dy)) * texel;
            var sample_uv = uv + offset;
            if (params.repeat_edge != 0u) {
                sample_uv = clamp(sample_uv, vec2<f32>(0.0), vec2<f32>(1.0));
            }
            acc = acc + textureSample(u_texture, u_sampler, sample_uv) * weight;
            total_weight = total_weight + weight;
        }
    }

    return acc / total_weight;
}
