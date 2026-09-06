struct Params { start_color: vec4<f32>, end_color: vec4<f32>, angle: f32 }
@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> params: Params;
@fragment
fn fs_main(@location(0) uv: vec2<f32>) -> @location(0) vec4<f32> {
    let src = textureSample(u_texture, u_sampler, uv);
    let radians = params.angle * 0.01745329252;
    let direction = vec2<f32>(cos(radians), sin(radians));
    let extent = abs(direction.x) + abs(direction.y);
    let t = clamp(dot(uv - 0.5, direction) / extent + 0.5, 0.0, 1.0);
    let color = mix(params.start_color, params.end_color, t);
    return vec4<f32>(color.rgb, src.a * color.a);
}
