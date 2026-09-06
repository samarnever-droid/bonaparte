//! Legacy shader and descriptor fixtures. Executable rendering now lives in
//! the bonaparte-gpu crate; these compatibility types do not initialize a device.
//!
//! Provides the WGSL shader contract, pipeline configuration, and GPU
//! compositor abstractions for hardware-accelerated tile rendering.

use serde::{Deserialize, Serialize};

/// Standard WGSL shader contract constants adhering to the public engine plugin contract:
/// - @group(0) @binding(0) var u_texture: texture_2d<f32>;
/// - @group(0) @binding(1) var u_sampler: sampler;
/// - @group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
/// - @group(0) @binding(3) var<uniform> u_time: f32;
/// - @group(0) @binding(4) var<uniform> params: Params;
pub const SHADER_BINDING_TEXTURE: u32 = 0;
pub const SHADER_BINDING_SAMPLER: u32 = 1;
pub const SHADER_BINDING_RESOLUTION: u32 = 2;
pub const SHADER_BINDING_TIME: u32 = 3;
pub const SHADER_BINDING_PARAMS: u32 = 4;

/// Standard vertex + fragment WGSL shader for tile compositing and 2D affine transforms.
pub const COMPOSITOR_WGSL: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

struct TileUniforms {
    transform: mat3x3<f32>,
    opacity: f32,
    blend_mode: u32,
    _pad: vec2<f32>,
};

@group(0) @binding(0) var u_texture: texture_2d<f32>;
@group(0) @binding(1) var u_sampler: sampler;
@group(0) @binding(2) var<uniform> u_resolution: vec2<f32>;
@group(0) @binding(3) var<uniform> u_time: f32;
@group(0) @binding(4) var<uniform> u_tile: TileUniforms;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let comp_pos = u_tile.transform * vec3<f32>(in.position, 1.0);
    let clip_x = (comp_pos.x / u_resolution.x) * 2.0 - 1.0;
    let clip_y = 1.0 - (comp_pos.y / u_resolution.y) * 2.0;
    out.clip_position = vec4<f32>(clip_x, clip_y, 0.0, 1.0);
    out.uv = in.uv;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(u_texture, u_sampler, in.uv);
    return vec4<f32>(color.rgb, color.a * u_tile.opacity);
}
"#;

/// WGSL shader implementation of all 8 Bonaparte alpha blend modes:
/// Normal, Multiply, Screen, Overlay, Add, Darken, Lighten, Difference.
pub const BLEND_WGSL: &str = r#"
fn blend_channel(mode: u32, s: f32, d: f32) -> f32 {
    switch mode {
        case 0u: { return s; } // Normal
        case 1u: { return s * d; } // Multiply
        case 2u: { return s + d - s * d; } // Screen
        case 3u: { // Overlay
            if (d <= 0.5) {
                return 2.0 * s * d;
            } else {
                return 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
            }
        }
        case 4u: { return min(1.0, s + d); } // Add (Linear Dodge)
        case 5u: { return min(s, d); } // Darken
        case 6u: { return max(s, d); } // Lighten
        case 7u: { return abs(s - d); } // Difference
        default: { return s; }
    }
}

fn blend_pixels(mode: u32, src: vec4<f32>, dst: vec4<f32>) -> vec4<f32> {
    let out_a = src.a + dst.a * (1.0 - src.a);
    if (out_a <= 0.0001) {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }
    let r = blend_channel(mode, src.r, dst.r);
    let g = blend_channel(mode, src.g, dst.g);
    let b = blend_channel(mode, src.b, dst.b);
    let blended = vec3<f32>(r, g, b);
    let color = (src.a * (1.0 - dst.a) * src.rgb + dst.a * (1.0 - src.a) * dst.rgb + src.a * dst.a * blended) / out_a;
    return vec4<f32>(color, out_a);
}
"#;

/// Configuration for the GPU render pipeline and memory allocations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuPipelineConfig {
    pub tile_size: u32,
    pub texture_format: String,
    pub max_texture_dimension: u32,
}

impl Default for GpuPipelineConfig {
    fn default() -> Self {
        Self {
            tile_size: 256,
            texture_format: "rgba8unorm".to_string(),
            max_texture_dimension: 8192,
        }
    }
}

/// Metadata and CPU-side descriptors for a GPU tile buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuTileDescriptor {
    pub tile_x: u32,
    pub tile_y: u32,
    pub width: u32,
    pub height: u32,
    pub bytes_per_row: u32,
}

impl GpuTileDescriptor {
    pub fn new(tile_x: u32, tile_y: u32, width: u32, height: u32) -> Self {
        let bytes_per_row = (width * 4).div_ceil(256) * 256;
        Self {
            tile_x,
            tile_y,
            width,
            height,
            bytes_per_row,
        }
    }

    pub fn buffer_size(&self) -> usize {
        (self.bytes_per_row * self.height) as usize
    }
}
