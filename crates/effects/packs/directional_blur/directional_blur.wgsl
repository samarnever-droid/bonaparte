// builtin.directional_blur — Linear motion blur along an arbitrary angle
//
// Standard shader contract (5 bindings):
//   0: u_texture (texture_2d<f32>)
//   1: u_sampler (sampler)
//   2: u_resolution (vec2<f32>)
//   3: u_time (f32)
//   4: params (Params)

struct Params {
    length: f32,
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
    if (params.length <= 0.0) {
        return sample_nearest(uv);
    }

    let texel = 1.0 / u_resolution;
    let rad = radians(params.angle);
    let dir = vec2<f32>(cos(rad), sin(rad)) * texel;

    let samples = max(1, i32(ceil(params.length * 0.5)));
    var acc = vec4<f32>(0.0);
    var total_weight: f32 = 0.0;

    for (var i = -samples; i <= samples; i = i + 1) {
        let t = f32(i) / f32(samples);
        let offset = dir * (t * params.length * 0.5);
        let weight = 1.0 - abs(t) * 0.5;
        acc = acc + sample_nearest(uv + offset) * weight;
        total_weight = total_weight + weight;
    }

    return acc / total_weight;
}
