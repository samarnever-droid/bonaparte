// builtin.drop_shadow — blurred, offset, colored shadow under the source
// (three-pass)
//
// Standard contract bindings 0..4 plus the multi-pass extension:
//   0: u_texture  — pass 0: original input; pass 1: blurred-x; pass 2: blurred halo
//   1: u_sampler
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)
//   5: u_original — the untouched input (composite pass)
//
// The host detects the `_pass` uniform and runs this program three times:
// 0 = horizontal premultiplied blur, 1 = vertical premultiplied blur,
// 2 = composite the source over the offset colored shadow. The math matches
// the CPU reference kernel (evaluate_drop_shadow): the shadow is the blurred
// ALPHA sampled at (x - offset), tinted and weighted, then straight-alpha
// composited under the source.

struct Params {
    offset_x: f32,
    offset_y: f32,
    radius: f32,
    color: vec4<f32>,
    opacity: f32,
    _pass: u32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var u_original: texture_2d<f32>;

fn blur_axis(uv: vec2<f32>, dir: vec2<f32>, already_premultiplied: bool) -> vec4<f32> {
    let r = max(1, i32(ceil(params.radius)));
    let sigma = max(0.5, params.radius * 0.5);
    let two_sigma_sq = 2.0 * sigma * sigma;

    var total_weight = 0.0;
    for (var i = -r; i <= r; i = i + 1) {
        total_weight = total_weight + exp(-(f32(i) * f32(i)) / two_sigma_sq);
    }

    var acc = vec4<f32>(0.0);
    let base = uv * u_resolution - vec2<f32>(0.5);
    let texel = 1.0 / u_resolution;
    for (var i = -r; i <= r; i = i + 1) {
        let weight = exp(-(f32(i) * f32(i)) / two_sigma_sq) / total_weight;
        let s = base + dir * f32(i);
        if (s.x >= -0.5
            && s.x <= u_resolution.x - 0.5
            && s.y >= -0.5
            && s.y <= u_resolution.y - 0.5
        ) {
            let sample_uv = (s + vec2<f32>(0.5)) * texel;
            let c = textureSampleLevel(u_texture, u_sampler, sample_uv, 0.0);
            let a = c.a;
            if (already_premultiplied) {
                acc = acc + vec4<f32>(c.rgb * weight, a * weight);
            } else {
                acc = acc + vec4<f32>(c.rgb * a * weight, a * weight);
            }
        }
    }
    return acc;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    if (params._pass == 0u) {
        if (params.radius <= 0.0) {
            return textureSampleLevel(u_texture, u_sampler, uv, 0.0);
        }
        return blur_axis(uv, vec2<f32>(1.0, 0.0), false);
    }
    if (params._pass == 1u) {
        return blur_axis(uv, vec2<f32>(0.0, 1.0), true);
    }

    // Composite: source over the offset colored shadow. The shadow samples
    // the blurred ALPHA at (uv - offset); out-of-bounds means no shadow.
    let src = textureSampleLevel(u_original, u_sampler, uv, 0.0);
    let sa = src.a;
    let texel = 1.0 / u_resolution;
    let shadow_uv = uv - vec2<f32>(params.offset_x, params.offset_y) * texel;
    var shadow = 0.0;
    if (shadow_uv.x >= 0.0
        && shadow_uv.x <= 1.0
        && shadow_uv.y >= 0.0
        && shadow_uv.y <= 1.0
    ) {
        let blurred = textureSampleLevel(u_texture, u_sampler, shadow_uv, 0.0);
        // The CPU reads its halo from an 8-bit frame; quantize to match.
        shadow = round(blurred.a * 255.0) / 255.0 * params.color.a * params.opacity;
    }
    let out_alpha = sa + shadow * (1.0 - sa);
    if (out_alpha <= 0.0) {
        return vec4<f32>(0.0);
    }
    let rgb = (src.rgb * sa + params.color.rgb * shadow * (1.0 - sa)) / out_alpha;
    return vec4<f32>(rgb, out_alpha);
}
