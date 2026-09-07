// builtin.glow — alpha-aware Gaussian glow (three-pass)
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
// 2 = straight-alpha composite of original + tinted halo. The math matches
// the CPU reference kernel (evaluate_glow + evaluate_blur with repeat_edge
// off): per-axis normalized gaussian, premultiplied accumulation, unpremultiplied
// only at the composite edge, out-of-bounds samples skipped.

struct Params {
    radius: f32,
    intensity: f32,
    tint: vec4<f32>,
    _pass: u32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;
@group(0) @binding(5) var u_original: texture_2d<f32>;

// One axis of the separable gaussian. `already_premultiplied` must be true
// when the input is the previous blur pass output.
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
        // Out-of-bounds samples are skipped, not clamped (repeat_edge=false).
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
    // Premultiplied output: rgb stays premultiplied by alpha for the chain.
    return acc;
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    if (params.radius <= 0.0 || params.intensity <= 0.0) {
        // Degenerate settings behave like the CPU kernel: intensity <= 0
        // returns the input untouched; radius 0 makes an empty halo.
        if (params.intensity <= 0.0) {
            return textureSampleLevel(u_texture, u_sampler, uv, 0.0);
        }
        return vec4<f32>(0.0);
    }
    if (params._pass == 0u) {
        return blur_axis(uv, vec2<f32>(1.0, 0.0), false);
    }
    if (params._pass == 1u) {
        return blur_axis(uv, vec2<f32>(0.0, 1.0), true);
    }

    // Composite: original straight color over the tinted halo. The CPU
    // kernel materializes its halo as an 8-bit straight frame before
    // compositing; quantize the float chain to that same grid so the ratio
    // math sees identical discrete values (stacked scenes amplify any drift
    // through later divisions).
    let halo = textureSampleLevel(u_texture, u_sampler, uv, 0.0);
    let src = textureSampleLevel(u_original, u_sampler, uv, 0.0);
    let sa = src.a;
    let halo_a = round(halo.a * 255.0) / 255.0;
    let ha = clamp(halo_a * params.intensity * params.tint.a, 0.0, 1.0);
    let alpha = sa + ha * (1.0 - sa);
    if (alpha <= 0.000001) {
        return vec4<f32>(0.0);
    }
    let halo_straight = select(
        vec3<f32>(0.0),
        round((halo.rgb / max(halo.a, 0.000001)) * 255.0) / 255.0,
        halo.a > 0.000001,
    );
    let rgb = (src.rgb * sa + halo_straight * params.tint.rgb * ha) / alpha;
    return vec4<f32>(rgb, alpha);
}
