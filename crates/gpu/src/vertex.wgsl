struct VertexOutput { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> }
@vertex
fn vs_fullscreen(@builtin(vertex_index) id:u32)->VertexOutput {
    var coordinates=array<vec2<f32>,3>(vec2<f32>(-1.0,-1.0),vec2<f32>(3.0,-1.0),vec2<f32>(-1.0,3.0));
    let p=coordinates[id];var out:VertexOutput;out.position=vec4<f32>(p,0.0,1.0);out.uv=vec2<f32>((p.x+1.0)*0.5,(1.0-p.y)*0.5);return out;
}
