//! Native preview orchestration: immutable snapshots, bounded LRU frames, source
//! reuse, truthful backend selection, and a versioned binary packet for both IPCs.
use crate::{DecodedImages, RenderRequest};
use bonaparte_effects::EffectRegistry;
use bonaparte_engine::preview::{
    prepare_scene_with_transform, render_scene_cpu, Resolution, Scene, Source, SourceCache,
    SourceStats,
};
use bonaparte_engine::Frame;
use bonaparte_model::{CompId, Project, Time};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
const FRAME_BUDGET: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Auto,
    Cpu,
    Gpu,
}
fn full() -> u32 {
    1
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRequest {
    #[serde(flatten)]
    pub render: RenderRequest,
    #[serde(default = "full")]
    pub divisor: u32,
    #[serde(default)]
    pub backend: Backend,
    #[serde(default)]
    pub transform_override: Option<bonaparte_model::TransformOverride>,
}
#[derive(Clone, PartialEq, Eq)]
struct FrameKey {
    epoch: u64,
    revision: u64,
    comp: CompId,
    time: Time,
    divisor: u32,
    bypass: bool,
    backend: Backend,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMetadata {
    pub width: u32,
    pub height: u32,
    pub logical_width: u32,
    pub logical_height: u32,
    pub divisor: u32,
    pub time: Time,
    pub revision: u64,
    /// Actual execution, not the requested preference: cpu / gpu / gpu-software.
    pub backend: String,
    pub adapter: Option<String>,
    pub fallback_reason: Option<String>,
    pub cache_hit: bool,
    pub render_ms: f64,
    pub original_render_ms: f64,
}
struct CachedFrame {
    key: FrameKey,
    pixels: Arc<Frame>,
    metadata: PreviewMetadata,
}
struct FrameCache {
    entries: VecDeque<CachedFrame>,
    bytes: usize,
    hits: u64,
    misses: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameStats {
    pub bytes: usize,
    pub budget_bytes: usize,
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub compiled: bool,
    pub available: bool,
    pub software: bool,
    pub name: Option<String>,
    pub api: Option<String>,
    pub device_type: Option<String>,
    pub reason: Option<String>,
    pub initialized: bool,
    pub pooled_bytes: usize,
    pub upload_bytes: usize,
    pub submitted_frames: u64,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStatus {
    pub protocol: u32,
    pub frames: FrameStats,
    pub sources: SourceStats,
    pub gpu: DeviceStatus,
    pub renders: u64,
    pub cpu_frames: u64,
    pub gpu_frames: u64,
    pub fallbacks: u64,
}
#[cfg(feature = "gpu")]
#[derive(Default)]
struct GpuState {
    probe: Option<Result<bonaparte_gpu::AdapterInfo, String>>,
    renderer: Option<bonaparte_gpu::GpuRenderer>,
    failure: Option<String>,
}

pub struct PreviewRenderer {
    frames: Mutex<FrameCache>,
    pub sources: SourceCache,
    epoch: AtomicU64,
    renders: AtomicU64,
    cpu_frames: AtomicU64,
    gpu_frames: AtomicU64,
    fallbacks: AtomicU64,
    gpu_disabled: bool,
    #[cfg(feature = "gpu")]
    gpu: Mutex<GpuState>,
}
impl Default for PreviewRenderer {
    fn default() -> Self {
        Self::new(false)
    }
}
impl PreviewRenderer {
    pub fn new(disable_gpu: bool) -> Self {
        Self {
            frames: Mutex::new(FrameCache {
                entries: VecDeque::new(),
                bytes: 0,
                hits: 0,
                misses: 0,
            }),
            sources: SourceCache::default(),
            epoch: AtomicU64::new(0),
            renders: AtomicU64::new(0),
            cpu_frames: AtomicU64::new(0),
            gpu_frames: AtomicU64::new(0),
            fallbacks: AtomicU64::new(0),
            gpu_disabled: disable_gpu
                || std::env::var("BONAPARTE_DISABLE_GPU").as_deref() == Ok("1"),
            #[cfg(feature = "gpu")]
            gpu: Mutex::new(GpuState::default()),
        }
    }
    pub fn invalidate(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let mut cache = self.frames.lock().unwrap_or_else(|e| e.into_inner());
        cache.entries.clear();
        cache.bytes = 0;
    }
    pub fn clear(&self, retry_gpu: bool) {
        self.invalidate();
        self.sources.clear();
        #[cfg(feature = "gpu")]
        {
            let mut gpu = self.gpu.lock().unwrap_or_else(|e| e.into_inner());
            if retry_gpu {
                *gpu = GpuState::default();
            } else if let Some(renderer) = &mut gpu.renderer {
                renderer.clear();
            }
        }
        #[cfg(not(feature = "gpu"))]
        let _ = retry_gpu;
    }
    fn gpu_status(&self) -> DeviceStatus {
        let mut status = DeviceStatus {
            compiled: cfg!(feature = "gpu"),
            available: false,
            software: false,
            name: None,
            api: None,
            device_type: None,
            reason: None,
            initialized: false,
            pooled_bytes: 0,
            upload_bytes: 0,
            submitted_frames: 0,
        };
        if self.gpu_disabled {
            status.reason = Some("GPU disabled by runtime configuration".into());
            return status;
        }
        #[cfg(feature = "gpu")]
        {
            let mut gpu = self.gpu.lock().unwrap_or_else(|e| e.into_inner());
            if gpu.probe.is_none() {
                gpu.probe = Some(bonaparte_gpu::probe());
            }
            match gpu.probe.as_ref().expect("probed") {
                Ok(info) => {
                    status.available = true;
                    status.software = info.software;
                    status.name = Some(info.name.clone());
                    status.api = Some(info.backend.clone());
                    status.device_type = Some(info.device_type.clone());
                    if info.software {
                        status.reason=Some("Software graphics adapter detected. Auto uses CPU; GPU mode is available for validation, not a hardware-speed promise.".into());
                    }
                }
                Err(error) => status.reason = Some(error.clone()),
            }
            if let Some(error) = &gpu.failure {
                status.reason = Some(error.clone());
            }
            if let Some(renderer) = &gpu.renderer {
                let stats = renderer.stats();
                status.initialized = true;
                status.pooled_bytes = stats.pooled_bytes;
                status.upload_bytes = stats.upload_bytes;
                status.submitted_frames = stats.submitted_frames;
            }
        }
        #[cfg(not(feature = "gpu"))]
        {
            status.reason = Some("This binary was built without GPU support".into());
        }
        status
    }
    pub fn status(&self) -> PreviewStatus {
        let frames = {
            let cache = self.frames.lock().unwrap_or_else(|e| e.into_inner());
            FrameStats {
                bytes: cache.bytes,
                budget_bytes: FRAME_BUDGET,
                entries: cache.entries.len(),
                hits: cache.hits,
                misses: cache.misses,
            }
        };
        PreviewStatus {
            protocol: 3,
            frames,
            sources: self.sources.stats(),
            gpu: self.gpu_status(),
            renders: self.renders.load(Ordering::Relaxed),
            cpu_frames: self.cpu_frames.load(Ordering::Relaxed),
            gpu_frames: self.gpu_frames.load(Ordering::Relaxed),
            fallbacks: self.fallbacks.load(Ordering::Relaxed),
        }
    }
    pub(crate) fn job(
        self: &Arc<Self>,
        project: Arc<Project>,
        images: DecodedImages,
        registry: Arc<EffectRegistry>,
        revision: u64,
        request: PreviewRequest,
    ) -> Result<PreviewJob, String> {
        let resolution = Resolution::new(request.divisor)?;
        let comp = project
            .comp(request.render.comp_id)
            .ok_or("Composition not found")?;
        if request.render.time.0 < 0 || request.render.time > comp.duration {
            return Err("Preview time must be inside the composition".into());
        }
        let key = FrameKey {
            epoch: self.epoch.load(Ordering::SeqCst),
            revision,
            comp: request.render.comp_id,
            time: request.render.time,
            divisor: resolution.divisor(),
            bypass: request.render.bypass_effects,
            backend: request.backend,
        };
        let cacheable =
            request.render.layer_override.is_none() && request.transform_override.is_none();
        Ok(PreviewJob {
            renderer: self.clone(),
            project,
            images,
            registry,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            resolution,
            key,
            cacheable,
            transient: request.transform_override,
        })
    }
    fn cached(&self, key: &FrameKey) -> Option<PreviewFrame> {
        let mut cache = self.frames.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(index) = cache.entries.iter().position(|entry| &entry.key == key) {
            let entry = cache.entries.remove(index).expect("cached entry");
            let pixels = entry.pixels.clone();
            let mut metadata = entry.metadata.clone();
            metadata.cache_hit = true;
            metadata.render_ms = 0.0;
            cache.entries.push_back(entry);
            cache.hits += 1;
            Some(PreviewFrame { pixels, metadata })
        } else {
            cache.misses += 1;
            None
        }
    }
    fn retain(&self, key: FrameKey, pixels: Arc<Frame>, metadata: PreviewMetadata) {
        if pixels.rgba.len() > FRAME_BUDGET {
            return;
        }
        let mut cache = self.frames.lock().unwrap_or_else(|e| e.into_inner());
        // An old render can finish after edit/clear/open. It must not resurrect a stale cache entry.
        if key.epoch != self.epoch.load(Ordering::SeqCst) {
            return;
        }
        if cache.entries.iter().any(|e| e.key == key) {
            return;
        }
        while cache.bytes + pixels.rgba.len() > FRAME_BUDGET || cache.entries.len() >= 2048 {
            if let Some(entry) = cache.entries.pop_front() {
                cache.bytes -= entry.pixels.rgba.len();
            } else {
                break;
            }
        }
        cache.bytes += pixels.rgba.len();
        cache.entries.push_back(CachedFrame {
            key,
            pixels,
            metadata,
        });
    }
    fn execute(
        &self,
        scene: &Scene,
        registry: &EffectRegistry,
        requested: Backend,
    ) -> Result<(Frame, String, Option<String>, Option<String>), String> {
        if requested == Backend::Cpu {
            self.cpu_frames.fetch_add(1, Ordering::Relaxed);
            return render_scene_cpu(scene, registry)
                .map(|f| (f, "cpu".into(), None, None))
                .map_err(|e| e.to_string());
        }
        let status = self.gpu_status();
        let reason = status.reason.clone();
        #[cfg(feature = "gpu")]
        let mut reason = reason;
        let should_try = status.available
            && !self.gpu_disabled
            && (requested == Backend::Gpu || !status.software);
        #[cfg(feature = "gpu")]
        if should_try {
            if let Some(unsupported) = bonaparte_gpu::GpuRenderer::unsupported(scene, registry) {
                reason = Some(unsupported);
            } else {
                let mut gpu = self.gpu.lock().unwrap_or_else(|e| e.into_inner());
                if gpu.failure.is_none() && gpu.renderer.is_none() {
                    match bonaparte_gpu::GpuRenderer::new() {
                        Ok(renderer) => gpu.renderer = Some(renderer),
                        Err(error) => gpu.failure = Some(error),
                    }
                }
                if gpu.failure.is_none() {
                    if let Some(renderer) = &mut gpu.renderer {
                        match renderer.render(scene, registry) {
                            Ok(frame) => {
                                self.gpu_frames.fetch_add(1, Ordering::Relaxed);
                                return Ok((
                                    frame,
                                    if renderer.info.software {
                                        "gpu-software"
                                    } else {
                                        "gpu"
                                    }
                                    .into(),
                                    Some(renderer.info.name.clone()),
                                    None,
                                ));
                            }
                            Err(error) => {
                                gpu.failure=Some(format!("GPU rendering failed: {error}. CPU fallback is active; Retry GPU can reinitialize the device."));
                            }
                        }
                    }
                }
                if let Some(error) = &gpu.failure {
                    reason = Some(error.clone());
                }
            }
        }
        #[cfg(not(feature = "gpu"))]
        let _ = should_try;
        if reason.is_some() {
            self.fallbacks.fetch_add(1, Ordering::Relaxed);
        }
        self.cpu_frames.fetch_add(1, Ordering::Relaxed);
        render_scene_cpu(scene, registry)
            .map(|f| (f, "cpu".into(), status.name, reason))
            .map_err(|e| e.to_string())
    }
}

pub struct PreviewJob {
    renderer: Arc<PreviewRenderer>,
    project: Arc<Project>,
    images: DecodedImages,
    registry: Arc<EffectRegistry>,
    resolution: Resolution,
    key: FrameKey,
    cacheable: bool,
    transient: Option<bonaparte_model::TransformOverride>,
    cancel: Arc<std::sync::atomic::AtomicBool>,
}
pub struct PreviewFrame {
    pub pixels: Arc<Frame>,
    pub metadata: PreviewMetadata,
}
impl PreviewJob {
    /// The job's cancel signal. Calling this makes an in-flight or about-to-run
    /// render abort cooperatively between layers with a "cancelled" error.
    /// Cached frames are unaffected; the job holds its own token.
    pub fn cancel(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn render(&self) -> Result<PreviewFrame, String> {
        if self.cacheable {
            if let Some(frame) = self.renderer.cached(&self.key) {
                return Ok(frame);
            }
        }
        if self.is_cancelled() {
            return Err("cancelled".into());
        }
        let started = Instant::now();
        let token = self.cancel.clone();
        let prepared = bonaparte_engine::cancel::scope(&token, || {
            prepare_scene_with_transform(
                &self.project,
                self.key.comp,
                self.key.time,
                &self.images,
                &self.registry,
                self.resolution,
                &self.renderer.sources,
                self.transient.as_ref(),
            )
        })
        .map_err(|e| e.to_string())?;
        let mut scene = prepared;
        if self.key.bypass {
            fn clear(scene: &mut Scene) {
                for layer in &mut scene.layers {
                    layer.effects.clear();
                    if let Source::Composition(child) = &mut layer.source {
                        clear(child);
                    }
                }
            }
            clear(&mut scene);
        }
        let executed = bonaparte_engine::cancel::scope(&token, || {
            self.renderer
                .execute(&scene, &self.registry, self.key.backend)
        });
        let (frame, backend, adapter, fallback_reason) = executed?;
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        self.renderer.renders.fetch_add(1, Ordering::Relaxed);
        let metadata = PreviewMetadata {
            width: frame.width,
            height: frame.height,
            logical_width: scene.logical_size[0] as u32,
            logical_height: scene.logical_size[1] as u32,
            divisor: self.resolution.divisor(),
            time: self.key.time,
            revision: self.key.revision,
            backend,
            adapter,
            fallback_reason,
            cache_hit: false,
            render_ms: ms,
            original_render_ms: ms,
        };
        let pixels = Arc::new(frame);
        if self.cacheable {
            self.renderer
                .retain(self.key.clone(), pixels.clone(), metadata.clone());
        }
        Ok(PreviewFrame { pixels, metadata })
    }
    pub fn packet(&self) -> Result<Vec<u8>, String> {
        self.render()?.packet()
    }
}
impl PreviewFrame {
    /// BPF3 + little-endian metadata length + JSON metadata + tightly packed RGBA.
    /// The old width/height/raw endpoint remains available for existing callers.
    pub fn packet(&self) -> Result<Vec<u8>, String> {
        let metadata = serde_json::to_vec(&self.metadata).map_err(|e| e.to_string())?;
        let mut packet = Vec::with_capacity(8 + metadata.len() + self.pixels.rgba.len());
        packet.extend_from_slice(b"BPF3");
        packet.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        packet.extend(metadata);
        packet.extend_from_slice(&self.pixels.rgba);
        Ok(packet)
    }
}
