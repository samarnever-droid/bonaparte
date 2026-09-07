// Color Grade — straight-alpha sRGB in/out. The native CPU implementation
// in grading.rs is the reference. This shader executes in the native bonaparte-gpu preview host.
struct Params {
    temperature: f32, tint: f32, exposure: f32, contrast: f32,
    highlights: f32, shadows: f32, whites: f32, blacks: f32,
    saturation: f32, vibrance: f32, gamma: f32, fade: f32,
    lift: f32, gain: f32, filmic: f32,
    lift_color: vec4<f32>, gain_color: vec4<f32>,
    split_shadow_hue: f32, split_highlight_hue: f32,
    split_balance: f32, split_strength: f32,
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
fn acesFitted(x: f32) -> f32 {
    return clamp(x * (2.51 * x + 0.03) / (x * (2.43 * x + 0.59) + 0.14), f32(0.0), f32(1.0));
}
// Saturated constant-hue color, identical table to grading.rs `hue_rgb`.
fn hueRgb(hDeg: f32) -> vec3<f32> {
    let h = (hDeg % 360.0 + 360.0) % 360.0 / 60.0;
    let sector = i32(floor(h));
    let f = h - floor(h);
    if (sector == 0) { return vec3<f32>(1.0, f, 0.0); }
    if (sector == 1) { return vec3<f32>(1.0 - f, 1.0, 0.0); }
    if (sector == 2) { return vec3<f32>(0.0, 1.0, f); }
    if (sector == 3) { return vec3<f32>(0.0, 1.0 - f, 1.0); }
    if (sector == 4) { return vec3<f32>(f, 0.0, 1.0); }
    return vec3<f32>(1.0, 0.0, 1.0 - f);
}
@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    if (src.a == 0.0) { return src; }
    let temp = params.temperature * 0.35;
    let tint = params.tint * 0.25;
    let balance = exp2(vec3<f32>(temp + tint * 0.5, -tint, -temp + tint * 0.5));
    var lin = linearize(src.rgb) * exp2(params.exposure) * balance;
    lin = lin * params.gain * exp2((params.gain_color.rgb - vec3<f32>(0.5)) * 2.0);
    lin = mix(lin, vec3<f32>(acesFitted(lin.r), acesFitted(lin.g), acesFitted(lin.b)), vec3<f32>(params.filmic));
    var rgb = clamp(encode(lin), vec3<f32>(0.0), vec3<f32>(1.0));
    var y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let lift = params.shadows * (1.0 - smoothstep(0.0, 0.65, y)) * 0.35
        + params.highlights * smoothstep(0.35, 1.0, y) * 0.35
        + params.blacks * (1.0 - smoothstep(0.0, 0.3, y)) * 0.2
        + params.whites * smoothstep(0.7, 1.0, y) * 0.2;
    rgb = pow(clamp((rgb + lift - 0.5) * params.contrast + 0.5, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / params.gamma));
    rgb = clamp(rgb + vec3<f32>(params.lift) + (params.lift_color.rgb - vec3<f32>(0.5)) * 0.5, vec3<f32>(0.0), vec3<f32>(1.0));
    y = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let hi = max(rgb.r, max(rgb.g, rgb.b));
    let lo = min(rgb.r, min(rgb.g, rgb.b));
    let chroma = select(0.0, (hi - lo) / max(hi, 0.00001), hi > 0.00001);
    let sat = params.saturation * (1.0 + params.vibrance * (1.0 - chroma));
    rgb = clamp(vec3<f32>(y) + (rgb - y) * sat, vec3<f32>(0.0), vec3<f32>(1.0));
    let crossover = clamp(y + params.split_balance * 0.5, vec2<f32>(0.0).x, vec2<f32>(1.0).x);
    let wHi = smoothstep(0.35, 0.65, crossover);
    let split = hueRgb(params.split_shadow_hue) * (1.0 - wHi) + hueRgb(params.split_highlight_hue) * wHi;
    rgb = clamp(rgb + split * (params.split_strength * 0.35), vec3<f32>(0.0), vec3<f32>(1.0));
    let fade = params.fade * 0.25;
    return vec4<f32>(rgb * (1.0 - fade) + fade, src.a);
}
