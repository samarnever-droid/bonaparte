// builtin.drop_shadow — Cast shadow behind opaque content
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
    radius: f32,
    color: vec4<f32>,
    opacity: f32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    let texel = 1.0 / u_resolution;
    let offset_uv = vec2<f32>(params.offset_x, params.offset_y) * texel;
    let shadow_uv = uv - offset_uv;

    var shadow_alpha: f32 = 0.0;
    if (params.radius <= 0.0) {
        shadow_alpha = textureSample(u_texture, u_sampler, shadow_uv).a;
    } else {
        var acc: f32 = 0.0;
        var total_weight: f32 = 0.0;
        let r = max(1, i32(ceil(params.radius)));
        let sigma = max(0.5, params.radius * 0.5);
        let two_sigma_sq = 2.0 * sigma * sigma;

        for (var dy = -r; dy <= r; dy = dy + 1) {
            for (var dx = -r; dx <= r; dx = dx + 1) {
                let dist_sq = f32(dx * dx + dy * dy);
                let weight = exp(-dist_sq / two_sigma_sq);
                let sample_uv = shadow_uv + vec2<f32>(f32(dx), f32(dy)) * texel;
                acc = acc + textureSample(u_texture, u_sampler, sample_uv).a * weight;
                total_weight = total_weight + weight;
            }
        }
        shadow_alpha = acc / total_weight;
    }

    let final_shadow_alpha = shadow_alpha * params.color.a * params.opacity;
    let shadow_rgb = params.color.rgb * final_shadow_alpha;

    // Composite src over shadow
    let out_rgb = src.rgb + shadow_rgb * (1.0 - src.a);
    let out_a = src.a + final_shadow_alpha * (1.0 - src.a);

    return vec4<f32>(out_rgb, out_a);
}
