// Linear-light, straight-alpha composition. RGBA8 textures store display sRGB.
// The uniform contains composition-space geometry; preview pixel size is explicit.
struct Draw {
    inverse: vec4<f32>,
    placement: vec4<f32>,
    geometry: vec4<f32>,
    fill: vec4<f32>,
    stroke: vec4<f32>,
    mode: vec4<f32>,
    flags: vec4<f32>,
}
@group(0) @binding(0) var backdrop: texture_2d<f32>;
@group(0) @binding(1) var source: texture_2d<f32>;
@group(0) @binding(2) var<uniform> draw: Draw;
fn decode(rgb: vec3<f32>) -> vec3<f32> {
    return select(pow((rgb + 0.055) / 1.055, vec3<f32>(2.4)), rgb / 12.92, rgb <= vec3<f32>(0.04045));
}
fn encode(rgb: vec3<f32>) -> vec3<f32> {
    let v=clamp(rgb,vec3<f32>(0.0),vec3<f32>(1.0));
    return select(1.055 * pow(v,vec3<f32>(1.0/2.4))-0.055,v*12.92,v<=vec3<f32>(0.0031308));
}
fn linear_pixel(p:vec4<f32>)->vec4<f32>{return vec4<f32>(decode(p.rgb),p.a);}
fn sample_source(uv:vec2<f32>)->vec4<f32>{
    let dims=vec2<i32>(textureDimensions(source));
    if draw.flags.x==0.0 {
        let p=clamp(vec2<i32>(uv*vec2<f32>(dims)),vec2<i32>(0),dims-vec2<i32>(1));
        let c=textureLoad(source,p,0);
        if draw.flags.y==1.0 {return vec4<f32>(draw.fill.rgb,c.a*draw.fill.a);}
        return linear_pixel(c);
    }
    let f=clamp(uv*vec2<f32>(dims)-0.5,vec2<f32>(0.0),vec2<f32>(dims-vec2<i32>(1)));
    let p0=vec2<i32>(floor(f));let p1=min(p0+vec2<i32>(1),dims-vec2<i32>(1));let t=f-vec2<f32>(p0);
    let a=linear_pixel(textureLoad(source,p0,0));
    let b=linear_pixel(textureLoad(source,vec2<i32>(p1.x,p0.y),0));
    let c=linear_pixel(textureLoad(source,vec2<i32>(p0.x,p1.y),0));
    let d=linear_pixel(textureLoad(source,p1,0));
    let wa=(1.0-t.x)*(1.0-t.y);let wb=t.x*(1.0-t.y);let wc=(1.0-t.x)*t.y;let wd=t.x*t.y;
    let alpha=a.a*wa+b.a*wb+c.a*wc+d.a*wd;
    let rgb=a.rgb*a.a*wa+b.rgb*b.a*wb+c.rgb*c.a*wc+d.rgb*d.a*wd;
    if draw.flags.y==1.0 {return vec4<f32>(draw.fill.rgb,alpha*draw.fill.a);}
    if alpha>0.000001{return vec4<f32>(rgb/alpha,alpha);}return vec4<f32>(rgb,alpha);
}
fn layer_source(position:vec2<f32>)->vec4<f32>{
    // Source kind 3 is an already transformed, filtered composition-sized image.
    if draw.mode.z==3.0 {return linear_pixel(textureLoad(source,vec2<i32>(position),0));}
    let world=position*draw.geometry.xy;
    let local=vec2<f32>(draw.inverse.x*world.x+draw.inverse.z*world.y+draw.placement.x,draw.inverse.y*world.x+draw.inverse.w*world.y+draw.placement.y);
    let half=draw.placement.zw*0.5;let uv=(local+half)/draw.placement.zw;
    if any(uv<vec2<f32>(0.0))||any(uv>=vec2<f32>(1.0)){return vec4<f32>(0.0);}
    if draw.mode.z==0.0{return draw.fill;}
    if draw.mode.z==2.0{return sample_source(uv);}
    let r=min(draw.geometry.w,min(half.x,half.y));let q=abs(local)-half+r;
    let distance=length(max(q,vec2<f32>(0.0)))+min(max(q.x,q.y),0.0)-r;
    let aa=max(max(length(draw.inverse.xy)*draw.geometry.x,length(draw.inverse.zw)*draw.geometry.y),0.01);
    let coverage=clamp(0.5-distance/aa,0.0,1.0);
    var stroke=0.0;if draw.mode.x>0.0{stroke=clamp(0.5+(distance+draw.mode.x)/aa,0.0,1.0);}
    let color=draw.fill*(1.0-stroke)+draw.stroke*stroke;
    return vec4<f32>(color.rgb,color.a*coverage);
}
fn blend(mode:u32,s:vec3<f32>,d:vec3<f32>)->vec3<f32>{
    switch mode {
        case 1u:{return s*d;}
        case 2u:{return 1.0-(1.0-s)*(1.0-d);}
        case 3u:{return select(1.0-2.0*(1.0-s)*(1.0-d),2.0*s*d,d<=vec3<f32>(0.5));}
        case 4u:{return min(s+d,vec3<f32>(1.0));}
        case 5u:{return min(s,d);}
        case 6u:{return max(s,d);}
        case 7u:{return abs(s-d);}
        default:{return s;}
    }
}
@fragment
fn fs_draw(@builtin(position) pos:vec4<f32>)->@location(0) vec4<f32>{
    let original=textureLoad(backdrop,vec2<i32>(pos.xy),0);
    var src=layer_source(pos.xy);
    // pass=1 writes the unblended source before its ordered effect stack.
    if draw.mode.w==1.0 {if src.a<=0.0{return vec4<f32>(0.0);}return vec4<f32>(encode(src.rgb),src.a);}
    let opacity=draw.geometry.z;
    if draw.mode.w==2.0 {
        if opacity==1.0{return textureLoad(source,vec2<i32>(pos.xy),0);}
        let old=linear_pixel(original);let alpha=old.a*(1.0-opacity)+src.a*opacity;
        var rgb=vec3<f32>(0.0);if alpha>0.000001{rgb=(old.rgb*old.a*(1.0-opacity)+src.rgb*src.a*opacity)/alpha;}
        return vec4<f32>(encode(rgb),alpha);
    }
    src.a=clamp(src.a*opacity,0.0,1.0);
    if src.a<=0.0{return original;}
    let dst=linear_pixel(original);let alpha=src.a+dst.a*(1.0-src.a);
    if alpha<=0.000001{return vec4<f32>(0.0);}
    let mixed=blend(u32(draw.mode.y),src.rgb,dst.rgb);
    let rgb=(src.a*(1.0-dst.a)*src.rgb+dst.a*(1.0-src.a)*dst.rgb+src.a*dst.a*mixed)/alpha;
    return vec4<f32>(encode(rgb),alpha);
}
