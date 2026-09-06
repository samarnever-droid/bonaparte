// Color Grade — straight-alpha sRGB in/out. The native CPU implementation
// in grading.rs is the reference. This shader executes in the native bonaparte-gpu preview host.
struct Params {
    temperature: f32, tint: f32, exposure: f32, contrast: f32,
    highlights: f32, shadows: f32, whites: f32, blacks: f32,
    saturation: f32, vibrance: f32, gamma: f32, fade: f32,
}
@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;
fn linearize(v: vec3<f32>) -> vec3<f32> {
    return select(pow((v + 0.055) / 1.055, vec3<f32>(2.4)), v / 12.92, v <= vec3<f32>(0.04045));
}
fn encode(v: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(max(v, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.4)) - 0.055, v * 12.92, v <= vec3<f32>(0.0031308));
}
@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    if (src.a == 0.0) { return src; }
    let temp = params.temperature * 0.35;
    let tint = params.tint * 0.25;
    let balance = exp2(vec3<f32>(temp + tint * 0.5, -tint, -temp + tint * 0.5));
    var rgb = clamp(encode(linearize(src.rgb) * exp2(params.exposure) * balance), vec3<f32>(0.0), vec3<f32>(1.0));
    var y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let lift = params.shadows * (1.0 - smoothstep(0.0, 0.65, y)) * 0.35
        + params.highlights * smoothstep(0.35, 1.0, y) * 0.35
        + params.blacks * (1.0 - smoothstep(0.0, 0.3, y)) * 0.2
        + params.whites * smoothstep(0.7, 1.0, y) * 0.2;
    rgb = pow(clamp((rgb + lift - 0.5) * params.contrast + 0.5, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / params.gamma));
    y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let hi = max(rgb.r, max(rgb.g, rgb.b));
    let lo = min(rgb.r, min(rgb.g, rgb.b));
    let chroma = select(0.0, (hi - lo) / max(hi, 0.00001), hi > 0.00001);
    let sat = params.saturation * (1.0 + params.vibrance * (1.0 - chroma));
    rgb = clamp(vec3<f32>(y) + (rgb - y) * sat, vec3<f32>(0.0), vec3<f32>(1.0));
    let fade = params.fade * 0.25;
    return vec4<f32>(rgb * (1.0 - fade) + fade, src.a);
}
