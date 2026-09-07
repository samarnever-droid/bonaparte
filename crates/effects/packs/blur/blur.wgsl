// builtin.blur — Gaussian blur effect (separable two-pass)
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)
//
// The host detects the `direction_x` uniform and runs this program twice:
// pass 1 with direction (1, 0) into an intermediate target, pass 2 with
// direction (0, 1) into the final target. The kernel matches the CPU
// reference exactly: per-axis normalized gaussian, premultiplied RGB
// accumulation, unpremultiplied at the output edge, and out-of-bounds
// samples skipped (not clamped) when repeat_edge is off.

struct Params {
    radius: f32,
    repeat_edge: u32,
    direction_x: f32,
    direction_y: f32,
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

    let dir = vec2<f32>(params.direction_x, params.direction_y);
    let texel = 1.0 / u_resolution;
    let r = max(1, i32(ceil(params.radius)));
    let sigma = max(0.5, params.radius * 0.5);
    let two_sigma_sq = 2.0 * sigma * sigma;

    // Normalize per axis like the CPU kernel (weights sum to 1 over [-r, r]).
    var total_weight = 0.0;
    for (var i = -r; i <= r; i = i + 1) {
        total_weight = total_weight + exp(-(f32(i) * f32(i)) / two_sigma_sq);
    }

    var acc = vec4<f32>(0.0);
    let base = uv * u_resolution - vec2<f32>(0.5);
    for (var i = -r; i <= r; i = i + 1) {
        let weight = exp(-(f32(i) * f32(i)) / two_sigma_sq) / total_weight;
        let s = base + dir * f32(i);
        var sample_uv = vec2<f32>(-1.0);
        if (params.repeat_edge != 0u) {
            sample_uv = (clamp(s, vec2<f32>(0.0), u_resolution - vec2<f32>(1.0))
                + vec2<f32>(0.5))
                * texel;
        } else if (s.x >= -0.5
            && s.x <= u_resolution.x - 0.5
            && s.y >= -0.5
            && s.y <= u_resolution.y - 0.5
        ) {
            sample_uv = (s + vec2<f32>(0.5)) * texel;
        }
        if (sample_uv.x >= 0.0) {
            // Explicit level: uniform control flow cannot guarantee implicit
            // derivatives inside this loop.
            let c = textureSampleLevel(u_texture, u_sampler, sample_uv, 0.0);
            let a = c.a;
            acc = acc + vec4<f32>(c.rgb * a, a) * weight;
        }
    }

    // Unpremultiply at the output edge, matching the CPU reference.
    if (acc.a > 0.000001) {
        return vec4<f32>(acc.rgb / acc.a, acc.a);
    }
    return vec4<f32>(0.0);
}
