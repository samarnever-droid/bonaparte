// builtin.circle — First-Party Vector Circle Generator
//
// Standard generator shader contract:
//   Inputs: none (generator)
//   Resolution, time, and declared parameters

struct Params {
    color: vec4<f32>,
    radius: f32,
    stroke_color: vec4<f32>,
    stroke_width: f32,
}

@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let center = vec2<f32>(0.5, 0.5);
    let p = (uv - center) * 2.0; // Normalized coords [-1.0, 1.0]
    let dist = length(p);

    let min_dim = min(u_resolution.x, u_resolution.y);
    let edge_w = select(0.05, 2.0 / min_dim, min_dim > 1.0);

    let r = max(0.001, params.radius);
    if (dist > r + edge_w) {
        return vec4<f32>(0.0);
    }

    let alpha_cov = clamp((r + edge_w - dist) / (2.0 * edge_w), 0.0, 1.0);

    // Stroke evaluation
    if (params.stroke_width > 0.0) {
        let sw_norm = (params.stroke_width * 2.0) / min_dim;
        if (dist >= r - sw_norm - edge_w) {
            let stroke_cov = clamp((dist - (r - sw_norm - edge_w)) / (2.0 * edge_w), 0.0, 1.0);
            return mix(params.color, params.stroke_color, stroke_cov) * vec4<f32>(1.0, 1.0, 1.0, alpha_cov);
        }
    }

    return params.color * vec4<f32>(1.0, 1.0, 1.0, alpha_cov);
}
