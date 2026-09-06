// builtin.color_adjust — Brightness, Contrast, Saturation, and Hue Shift
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    brightness: f32,
    contrast: f32,
    saturation: f32,
    hue_shift: f32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;

fn rgb_to_hsv(rgb: vec3<f32>) -> vec3<f32> {
    let cmax = max(rgb.r, max(rgb.g, rgb.b));
    let cmin = min(rgb.r, min(rgb.g, rgb.b));
    let delta = cmax - cmin;

    var h: f32 = 0.0;
    if (delta > 1e-5) {
        if (cmax == rgb.r) {
            h = ((rgb.g - rgb.b) / delta) % 6.0;
        } else if (cmax == rgb.g) {
            h = ((rgb.b - rgb.r) / delta) + 2.0;
        } else {
            h = ((rgb.r - rgb.g) / delta) + 4.0;
        }
        h = h / 6.0;
        if (h < 0.0) {
            h = h + 1.0;
        }
    }

    let s = select(0.0, delta / cmax, cmax > 1e-5);
    let v = cmax;
    return vec3<f32>(h, s, v);
}

fn hsv_to_rgb(hsv: vec3<f32>) -> vec3<f32> {
    let h = (hsv.x % 1.0 + 1.0) % 1.0;
    let s = clamp(hsv.y, 0.0, 1.0);
    let v = clamp(hsv.z, 0.0, 1.0);

    let c = v * s;
    let x = c * (1.0 - abs((h * 6.0) % 2.0 - 1.0));
    let m = v - c;

    var rgb = vec3<f32>(0.0);
    let hi = u32(floor(h * 6.0)) % 6u;
    if (hi == 0u) {
        rgb = vec3<f32>(c, x, 0.0);
    } else if (hi == 1u) {
        rgb = vec3<f32>(x, c, 0.0);
    } else if (hi == 2u) {
        rgb = vec3<f32>(0.0, c, x);
    } else if (hi == 3u) {
        rgb = vec3<f32>(0.0, x, c);
    } else if (hi == 4u) {
        rgb = vec3<f32>(x, 0.0, c);
    } else {
        rgb = vec3<f32>(c, 0.0, x);
    }

    return rgb + vec3<f32>(m);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    var rgb = src.rgb;

    // Brightness
    rgb = rgb + vec3<f32>(params.brightness);

    // Contrast: centered at 0.5
    rgb = (rgb - vec3<f32>(0.5)) * params.contrast + vec3<f32>(0.5);

    // Hue shift and Saturation in HSV space
    var hsv = rgb_to_hsv(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)));
    let hue_rot = params.hue_shift / 360.0;
    hsv.x = (hsv.x + hue_rot) % 1.0;
    hsv.y = clamp(hsv.y * params.saturation, 0.0, 1.0);
    rgb = hsv_to_rgb(hsv);

    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), src.a);
}
