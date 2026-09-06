// builtin.chromatic_aberration — RGB channel shift & optical dispersion
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    amount: f32,
    angle: f32,
}

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;


fn sample_nearest(uv:vec2<f32>)->vec4<f32>{
    if any(uv<vec2<f32>(0.0))||any(uv>vec2<f32>(1.0)){return vec4<f32>(0.0);}
    let dims=vec2<i32>(textureDimensions(u_texture));
    let p=clamp(vec2<i32>(floor(uv*vec2<f32>(dims))),vec2<i32>(0),dims-vec2<i32>(1));
    return textureLoad(u_texture,p,0);
}

@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    if (params.amount <= 0.0) {
        return sample_nearest(uv);
    }

    let texel = 1.0 / u_resolution;
    let rad = radians(params.angle);
    let dir = vec2<f32>(cos(rad), sin(rad)) * params.amount * texel;

    let r_col = sample_nearest(uv + dir);
    let g_col = sample_nearest(uv);
    let b_col = sample_nearest(uv - dir);

    let max_a = max(r_col.a, max(g_col.a, b_col.a));
    return vec4<f32>(r_col.r, g_col.g, b_col.b, max_a);
}
