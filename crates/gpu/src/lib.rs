//! Executable native wgpu preview renderer. Prepared scenes share geometry and
//! source pixels with CPU preview; transforms, blending and opted-in WGSL effects
//! execute on the selected graphics adapter. Software adapters are reported honestly.
mod uniforms;
use bonaparte_effects::{CpuFrame, EffectRegistry};
use bonaparte_engine::preview::{Sampling, Scene, SceneLayer, Source};
use bonaparte_engine::{linear_to_srgb_byte, Frame};
use bonaparte_model::{BlendMode, EffectInstance};
use serde::Serialize;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use uniforms::UniformLayout;
use wgpu::util::DeviceExt;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const POOL_BUDGET: usize = 128 * 1024 * 1024;
const UPLOAD_BUDGET: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterInfo {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub software: bool,
    pub max_texture_size: u32,
}
fn adapter() -> Result<(wgpu::Instance, wgpu::Adapter, AdapterInfo), String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..Default::default()
    });
    let mut adapters = instance.enumerate_adapters(wgpu::Backends::PRIMARY);
    adapters.sort_by_key(|a| match a.get_info().device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Other => 3,
        wgpu::DeviceType::Cpu => 4,
    });
    let adapter = adapters
        .into_iter()
        .next()
        .ok_or("No compatible Vulkan, Metal or DX12 adapter was found")?;
    let info = adapter.get_info();
    let description = AdapterInfo {
        name: info.name,
        backend: format!("{:?}", info.backend),
        device_type: format!("{:?}", info.device_type),
        software: info.device_type == wgpu::DeviceType::Cpu,
        max_texture_size: adapter.limits().max_texture_dimension_2d,
    };
    Ok((instance, adapter, description))
}
pub fn probe() -> Result<AdapterInfo, String> {
    adapter().map(|(_, _, info)| info)
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuStats {
    pub pooled_bytes: usize,
    pub pool_budget_bytes: usize,
    pub upload_bytes: usize,
    pub upload_budget_bytes: usize,
    pub uploads: u64,
    pub upload_hits: u64,
    pub submitted_frames: u64,
    pub effect_programs: usize,
}
struct Image {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}
impl Image {
    fn bytes(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }
}
struct Upload {
    pixels: Arc<CpuFrame>,
    image: Arc<Image>,
}
struct Program {
    source: String,
    uniforms: UniformLayout,
    pipeline: wgpu::RenderPipeline,
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Draw {
    inverse: [f32; 4],
    placement: [f32; 4],
    geometry: [f32; 4],
    fill: [f32; 4],
    stroke: [f32; 4],
    mode: [f32; 4],
    flags: [f32; 4],
}
pub struct GpuRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub info: AdapterInfo,
    vertex: wgpu::ShaderModule,
    draw_pipeline: wgpu::RenderPipeline,
    draw_layout: wgpu::BindGroupLayout,
    effect_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    programs: HashMap<String, Program>,
    pool: Vec<Arc<Image>>,
    uploads: VecDeque<Upload>,
    upload_count: u64,
    upload_hits: u64,
    submitted: u64,
    device_error: Arc<Mutex<Option<String>>>,
}
fn texture_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}
fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
fn pipeline(
    device: &wgpu::Device,
    vertex: &wgpu::ShaderModule,
    fragment: &wgpu::ShaderModule,
    layout: &wgpu::BindGroupLayout,
    entry: &str,
    label: &str,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: vertex,
            entry_point: Some("vs_fullscreen"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: fragment,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}
impl GpuRenderer {
    pub fn new() -> Result<Self, String> {
        let (_instance, adapter, info) = adapter()?;
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("Bonaparte preview device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default().using_resolution(adapter.limits()),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
            },
            None,
        ))
        .map_err(|e| format!("GPU device request failed: {e}"))?;
        let errors = Arc::new(Mutex::new(None));
        let sink = errors.clone();
        device.on_uncaptured_error(Box::new(move |e| {
            *sink.lock().unwrap_or_else(|e| e.into_inner()) = Some(e.to_string());
        }));
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Preview fullscreen vertex"),
            source: wgpu::ShaderSource::Wgsl(include_str!("vertex.wgsl").into()),
        });
        let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Linear-light layer compositor"),
            source: wgpu::ShaderSource::Wgsl(include_str!("draw.wgsl").into()),
        });
        let draw_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Layer inputs"),
            entries: &[texture_entry(0), texture_entry(1), uniform_entry(2)],
        });
        let effect_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Public fragment effect contract"),
            entries: &[
                texture_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                uniform_entry(2),
                uniform_entry(3),
                uniform_entry(4),
            ],
        });
        let draw_pipeline = pipeline(
            &device,
            &vertex,
            &fragment,
            &draw_layout,
            "fs_draw",
            "Layer compositor",
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Effect linear clamp sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });
        if let Some(error) = pollster::block_on(device.pop_error_scope()) {
            return Err(format!("GPU pipeline initialization: {error}"));
        }
        Ok(Self {
            device,
            queue,
            info,
            vertex,
            draw_pipeline,
            draw_layout,
            effect_layout,
            sampler,
            programs: HashMap::new(),
            pool: vec![],
            uploads: VecDeque::new(),
            upload_count: 0,
            upload_hits: 0,
            submitted: 0,
            device_error: errors,
        })
    }
    pub fn stats(&self) -> GpuStats {
        GpuStats {
            pooled_bytes: self.pool.iter().map(|i| i.bytes()).sum(),
            pool_budget_bytes: POOL_BUDGET,
            upload_bytes: self.uploads.iter().map(|i| i.image.bytes()).sum(),
            upload_budget_bytes: UPLOAD_BUDGET,
            uploads: self.upload_count,
            upload_hits: self.upload_hits,
            submitted_frames: self.submitted,
            effect_programs: self.programs.len(),
        }
    }
    pub fn clear(&mut self) {
        self.pool.clear();
        self.uploads.clear();
    }
    fn image(&self, width: u32, height: u32, label: &str) -> Result<Arc<Image>, String> {
        if width == 0
            || height == 0
            || width > self.info.max_texture_size
            || height > self.info.max_texture_size
        {
            return Err(format!(
                "GPU texture {width}×{height} exceeds adapter limits"
            ));
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Ok(Arc::new(Image {
            texture,
            view,
            width,
            height,
        }))
    }
    fn target(&mut self, width: u32, height: u32) -> Result<Arc<Image>, String> {
        if let Some(image) = self
            .pool
            .iter()
            .find(|i| i.width == width && i.height == height && Arc::strong_count(i) == 1)
        {
            return Ok(image.clone());
        }
        let bytes = width as usize * height as usize * 4;
        while self.pool.iter().map(|i| i.bytes()).sum::<usize>() + bytes > POOL_BUDGET {
            let Some(index) = self.pool.iter().position(|i| Arc::strong_count(i) == 1) else {
                return Err(
                    "GPU working textures exceed the 128 MiB pool budget; using CPU fallback"
                        .into(),
                );
            };
            self.pool.remove(index);
        }
        let image = self.image(width, height, "Pooled RGBA preview target")?;
        self.pool.push(image.clone());
        Ok(image)
    }
    fn upload(&mut self, pixels: &Arc<CpuFrame>) -> Result<Arc<Image>, String> {
        if let Some(index) = self
            .uploads
            .iter()
            .position(|entry| Arc::ptr_eq(&entry.pixels, pixels))
        {
            let entry = self.uploads.remove(index).expect("located upload");
            let image = entry.image.clone();
            self.uploads.push_back(entry);
            self.upload_hits += 1;
            return Ok(image);
        }
        if pixels.rgba.len() != pixels.width as usize * pixels.height as usize * 4 {
            return Err("Malformed raster upload".into());
        }
        if pixels.rgba.len() > UPLOAD_BUDGET {
            return Err("Source image exceeds the 64 MiB GPU upload budget".into());
        }
        while self.uploads.iter().map(|i| i.image.bytes()).sum::<usize>() + pixels.rgba.len()
            > UPLOAD_BUDGET
            || self.uploads.len() >= 1024
        {
            self.uploads.pop_front();
        }
        let image = self.image(pixels.width, pixels.height, "Cached source raster")?;
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &image.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pixels.width * 4),
                rows_per_image: Some(pixels.height),
            },
            wgpu::Extent3d {
                width: pixels.width,
                height: pixels.height,
                depth_or_array_layers: 1,
            },
        );
        self.upload_count += 1;
        self.uploads.push_back(Upload {
            pixels: pixels.clone(),
            image: image.clone(),
        });
        Ok(image)
    }
    fn clear_target(&self, encoder: &mut wgpu::CommandEncoder, target: &Image, color: [f32; 4]) {
        let clear = wgpu::Color {
            r: linear_to_srgb_byte(color[0]) as f64 / 255.0,
            g: linear_to_srgb_byte(color[1]) as f64 / 255.0,
            b: linear_to_srgb_byte(color[2]) as f64 / 255.0,
            a: (color[3].clamp(0.0, 1.0) * 255.0).round() as f64 / 255.0,
        };
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Clear composition"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    fn draw(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &Image,
        backdrop: &Image,
        source: &Image,
        params: Draw,
        bounds: [u32; 4],
        copy_backdrop: bool,
    ) {
        if copy_backdrop {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &backdrop.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &target.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: target.width,
                    height: target.height,
                    depth_or_array_layers: 1,
                },
            );
        }
        if bounds[2] == 0 || bounds[3] == 0 {
            return;
        }
        let uniform = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Layer uniforms"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Layer textures"),
            layout: &self.draw_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&backdrop.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&source.view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Transform and composite layer"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target.view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.draw_pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.set_scissor_rect(bounds[0], bounds[1], bounds[2], bounds[3]);
        pass.draw(0..3, 0..1);
    }
    fn effect(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        input: Arc<Image>,
        instance: &EffectInstance,
        scene: &Scene,
        registry: &EffectRegistry,
    ) -> Result<Arc<Image>, String> {
        let pack = registry
            .get(&instance.effect_id)
            .ok_or_else(|| format!("Unknown effect {}", instance.effect_id))?;
        if !pack.manifest.gpu_preview {
            return Err(format!(
                "{} uses the CPU preview kernel",
                pack.manifest.name
            ));
        }
        let values = registry
            .instance_parameters(instance, scene.time, scene.raster_scale)
            .map_err(|e| e.to_string())?;
        if !self
            .programs
            .get(&instance.effect_id)
            .is_some_and(|p| p.source == pack.shader_source)
        {
            let uniforms = UniformLayout::reflect(&pack.shader_source)?;
            let shader = self
                .device
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some(&instance.effect_id),
                    source: wgpu::ShaderSource::Wgsl(pack.shader_source.clone().into()),
                });
            let pipeline = pipeline(
                &self.device,
                &self.vertex,
                &shader,
                &self.effect_layout,
                "fs_main",
                &instance.effect_id,
            );
            self.programs.insert(
                instance.effect_id.clone(),
                Program {
                    source: pack.shader_source.clone(),
                    uniforms,
                    pipeline,
                },
            );
        }
        let target = self.target(input.width, input.height)?;
        let program = self
            .programs
            .get(&instance.effect_id)
            .expect("compiled program");
        let params = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Reflected effect uniforms"),
                contents: &program.uniforms.pack(&values)?,
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let resolution = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Effect raster resolution"),
                contents: bytemuck::cast_slice(&[input.width as f32, input.height as f32]),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let time = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Effect composition time"),
                contents: &(scene.time.as_secs_f64() as f32).to_le_bytes(),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Effect inputs"),
            layout: &self.effect_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&input.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: resolution.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: time.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: params.as_entire_binding(),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&instance.effect_id),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&program.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.draw(0..3, 0..1);
        }
        Ok(target)
    }
    fn scene(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &Scene,
        registry: &EffectRegistry,
    ) -> Result<Arc<Image>, String> {
        let mut backdrop = self.target(scene.width, scene.height)?;
        self.clear_target(encoder, &backdrop, scene.background);
        for layer in &scene.layers {
            let adjustment = matches!(layer.source, Source::Adjustment);
            if adjustment && layer.effects.is_empty() {
                continue;
            }
            let raster = match &layer.source {
                Source::Raster { pixels, .. } => Some(self.upload(pixels)?),
                Source::Composition(child) => Some(self.scene(encoder, child, registry)?),
                _ => None,
            };
            let mut params = draw_params(scene, layer);
            if layer.effects.is_empty() {
                if layer.bounds[2] == 0 || layer.bounds[3] == 0 {
                    continue;
                }
                let target = self.target(scene.width, scene.height)?;
                self.draw(
                    encoder,
                    &target,
                    &backdrop,
                    raster.as_deref().unwrap_or(&backdrop),
                    params,
                    layer.bounds,
                    true,
                );
                backdrop = target;
                continue;
            }
            let mut filtered = if adjustment {
                backdrop.clone()
            } else {
                let target = self.target(scene.width, scene.height)?;
                self.clear_target(encoder, &target, [0.0; 4]);
                params.mode[3] = 1.0;
                self.draw(
                    encoder,
                    &target,
                    &backdrop,
                    raster.as_deref().unwrap_or(&backdrop),
                    params,
                    layer.bounds,
                    false,
                );
                target
            };
            for effect in &layer.effects {
                filtered = self.effect(encoder, filtered, effect, scene, registry)?;
            }
            if adjustment && layer.opacity == 1.0 {
                backdrop = filtered;
                continue;
            }
            let target = self.target(scene.width, scene.height)?;
            params.mode[2] = 3.0;
            params.mode[3] = if adjustment { 2.0 } else { 0.0 };
            self.draw(
                encoder,
                &target,
                &backdrop,
                &filtered,
                params,
                [0, 0, scene.width, scene.height],
                false,
            );
            backdrop = target;
        }
        Ok(backdrop)
    }
    pub fn unsupported(scene: &Scene, registry: &EffectRegistry) -> Option<String> {
        for layer in &scene.layers {
            for effect in &layer.effects {
                let Some(pack) = registry.get(&effect.effect_id) else {
                    return Some(format!("Unknown GPU effect {}", effect.effect_id));
                };
                if !pack.manifest.gpu_preview {
                    return Some(format!(
                        "{} is not GPU-enabled; the entire frame uses CPU for consistent output",
                        pack.manifest.name
                    ));
                }
            }
            if let Source::Composition(child) = &layer.source {
                if let Some(reason) = Self::unsupported(child, registry) {
                    return Some(reason);
                }
            }
        }
        None
    }
    pub fn render(&mut self, scene: &Scene, registry: &EffectRegistry) -> Result<Frame, String> {
        if let Some(reason) = Self::unsupported(scene, registry) {
            return Err(reason);
        }
        if let Some(error) = self
            .device_error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return Err(format!("GPU device error: {error}"));
        }
        self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let result = self.render_inner(scene, registry);
        let memory = pollster::block_on(self.device.pop_error_scope());
        let validation = pollster::block_on(self.device.pop_error_scope());
        if let Some(error) = memory.or(validation) {
            return Err(format!("GPU render rejected: {error}"));
        }
        result
    }
    fn render_inner(&mut self, scene: &Scene, registry: &EffectRegistry) -> Result<Frame, String> {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Bonaparte prepared scene"),
            });
        let image = self.scene(&mut encoder, scene, registry)?;
        let pitch = (image.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let size = pitch as u64 * image.height as u64;
        if size > self.device.limits().max_buffer_size {
            return Err("GPU readback exceeds device buffer limits".into());
        }
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Padded preview readback"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &image.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(pitch),
                    rows_per_image: Some(image.height),
                },
            },
            wgpu::Extent3d {
                width: image.width,
                height: image.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));
        self.submitted += 1;
        let slice = buffer.slice(..);
        let (send, receive) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = send.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);
        receive
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| format!("GPU readback timed out: {e}"))?
            .map_err(|e| format!("GPU readback: {e}"))?;
        let mapped = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity(image.width as usize * image.height as usize * 4);
        for row in mapped
            .chunks_exact(pitch as usize)
            .take(image.height as usize)
        {
            rgba.extend_from_slice(&row[..image.width as usize * 4]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(Frame {
            width: image.width,
            height: image.height,
            rgba,
        })
    }
}
fn draw_params(scene: &Scene, layer: &SceneLayer) -> Draw {
    let pixel = scene.pixel_size();
    let m = layer.inverse;
    let mut p = Draw {
        inverse: [m.a, m.b, m.c, m.d],
        placement: [m.tx, m.ty, layer.size[0], layer.size[1]],
        geometry: [pixel[0], pixel[1], layer.opacity, 0.0],
        fill: [0.0; 4],
        stroke: [0.0; 4],
        mode: [
            0.0,
            match layer.blend {
                BlendMode::Normal => 0.0,
                BlendMode::Multiply => 1.0,
                BlendMode::Screen => 2.0,
                BlendMode::Overlay => 3.0,
                BlendMode::Add => 4.0,
                BlendMode::Darken => 5.0,
                BlendMode::Lighten => 6.0,
                BlendMode::Difference => 7.0,
            },
            0.0,
            0.0,
        ],
        flags: [0.0; 4],
    };
    match &layer.source {
        Source::Solid(color) => p.fill = *color,
        Source::Rectangle { color, style } => {
            p.fill = *color;
            p.stroke = style.stroke_color;
            p.geometry[3] = style.corner_radius;
            p.mode[0] = style.stroke_width;
            p.mode[2] = 1.0;
        }
        Source::Raster { sampling, tint, .. } => {
            p.mode[2] = 2.0;
            p.flags[0] = if *sampling == Sampling::Linear {
                1.0
            } else {
                0.0
            };
            if let Some(color) = tint {
                p.fill = *color;
                p.flags[1] = 1.0;
            }
        }
        Source::Composition(_) => {
            p.mode[2] = 2.0;
            p.flags[0] = 1.0;
        }
        Source::Adjustment => {
            p.mode[2] = 3.0;
        }
    }
    p
}
