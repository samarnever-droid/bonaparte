// builtin.glow — first-party pack, ordinary plugin (microkernel: RULES §4.1).
//
// Shader contract (the ONLY guarantees an effect gets):
//   u_texture     : input sampler, bound from the graph edge named "input"
//   u_resolution  : canvas size in px (vec2<f32>)
//   u_time        : comp time in seconds (f32)
// Everything else is declared by the manifest's params and arrives as a
// uniform buffer in manifest-declared order.
//
// Multi-pass effects (blur, bloom) provide buildPasses(params) in the host
// registry — see ARCHITECTURE.md "Lessons adopted from OpenCut": sampling
// step ≤ 4, stack H+V iterations for large sigma.

struct Params {
    radius: f32,
    intensity: f32,
    tint: vec4<f32>,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);

    // v1 stub: halo approximated by sampling the 4-neighbourhood at the
    // radius offset. The real Gaussian H+V pass chain lands with the GPU
    // backend; the manifest, params, and this contract are already final —
    // that's the microkernel point.
    let texel = 1.0 / u_resolution;
    var halo = vec4<f32>(0.0);
    for (var d = 0; d < 4; d = d + 1) {
        let dir = vec2<f32>(f32((d & 1) * 2 - 1), f32((d & 2) * 2 - 1) * 0.5);
        halo = halo + textureSample(u_texture, u_sampler, uv + dir * params.radius * texel);
    }
    halo = halo / 4.0 * params.tint * params.intensity;

    return vec4<f32>(max(src.rgb, halo.rgb), src.a);
}
