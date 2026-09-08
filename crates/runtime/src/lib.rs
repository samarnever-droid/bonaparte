//! The application service shared by Tauri and the development web bridge.
//! All persistent edits use the model's Op/History path. No browser-side model emulator.
use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_effects::{CpuFrame, EffectRegistry};
use bonaparte_engine::import_obj::obj_layers;
use bonaparte_engine::import_svg::svg_layers;
use bonaparte_engine::reference::{render_comp_with_registry, Frame, FrameView, MediaFrames};
use bonaparte_model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

pub mod audio;
pub mod color;
pub mod interaction;
mod patch;
pub mod preview;
pub use audio::{decode_project_audio, AudioChunkRequest, DecodedAudios};
pub use preview::{PreviewJob, PreviewRenderer, PreviewRequest};

pub const PROJECT_VERSION: u32 = 4;
pub const MAX_PROJECT_BYTES: usize = 64 * 1024 * 1024;

/// Live export telemetry, shared with the UI through the `export_progress`
/// command and flipped by `export_cancel`. Process-global: one export at a
/// time, exactly like the single temp-file pipeline it describes.
pub struct ExportTelemetry {
    pub active: AtomicBool,
    pub canceled: AtomicBool,
    pub frames_done: AtomicU64,
    pub total_frames: AtomicU64,
    /// 0 = idle, 1 = rendering + encoding, 2 = finalizing.
    pub stage: AtomicU8,
    pub started_ms: AtomicU64,
    pub last_frame_ms: AtomicU64,
}
pub static EXPORT_TELEMETRY: ExportTelemetry = ExportTelemetry {
    active: AtomicBool::new(false),
    canceled: AtomicBool::new(false),
    frames_done: AtomicU64::new(0),
    total_frames: AtomicU64::new(0),
    stage: AtomicU8::new(0),
    started_ms: AtomicU64::new(0),
    last_frame_ms: AtomicU64::new(0),
};

pub fn unix_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Snapshot for the `export_progress` bridge command.
pub fn export_progress_json() -> Value {
    json!({
        "active": EXPORT_TELEMETRY.active.load(Ordering::Relaxed),
        "canceled": EXPORT_TELEMETRY.canceled.load(Ordering::Relaxed),
        "framesDone": EXPORT_TELEMETRY.frames_done.load(Ordering::Relaxed),
        "totalFrames": EXPORT_TELEMETRY.total_frames.load(Ordering::Relaxed),
        "stage": EXPORT_TELEMETRY.stage.load(Ordering::Relaxed),
        "startedMs": EXPORT_TELEMETRY.started_ms.load(Ordering::Relaxed),
        "lastFrameMs": EXPORT_TELEMETRY.last_frame_ms.load(Ordering::Relaxed),
    })
}

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    format: String,
    version: u32,
    project: Project,
}

pub fn serialize_project(project: &Project) -> Result<String, String> {
    project.validate()?;
    let text = serde_json::to_string_pretty(&ProjectFile {
        format: "bonaparte".into(),
        version: PROJECT_VERSION,
        project: project.clone(),
    })
    .map_err(|e| e.to_string())?;
    if text.len() > MAX_PROJECT_BYTES {
        return Err("Serialized project exceeds the 64 MiB portable file limit".into());
    }
    Ok(text)
}

/// Write beside the destination and rename only after all bytes have reached
/// the file. Used for project files and still images by desktop and automation.
pub fn write_file_atomic(destination: &std::path::Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let directory = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let mut temp = tempfile::NamedTempFile::new_in(directory).map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(destination).map_err(|e| e.to_string())?;
    Ok(())
}

/// v0.1 raw documents remain readable via serde defaults. Unknown future versions
/// fail before replacing the session, instead of silently discarding newer fields.
pub fn parse_project(text: &str) -> Result<Project, String> {
    if text.len() > MAX_PROJECT_BYTES {
        return Err("Project exceeds the 64 MB file limit".into());
    }
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("Invalid project JSON: {e}"))?;
    let project = if root.get("version").is_some() || root.get("format").is_some() {
        let file: ProjectFile = serde_json::from_value(root).map_err(|e| e.to_string())?;
        if file.format != "bonaparte" || !matches!(file.version, 2 | 3 | PROJECT_VERSION) {
            return Err("Unsupported project format/version".into());
        }
        file.project
    } else {
        serde_json::from_value(root).map_err(|e| e.to_string())?
    };
    project.validate()?;
    validate_effects(&project, &EffectRegistry::new())?;
    decode_embedded_frames(&project)?;
    decode_project_audio(&project)?;
    Ok(project)
}

pub fn validate_effects(project: &Project, registry: &EffectRegistry) -> Result<(), String> {
    for layer in project.comps.values().flat_map(|c| c.layers.values()) {
        for effect in &layer.effects {
            registry
                .validate_instance(effect)
                .map_err(|e| e.to_string())?;
        }
        if let LayerKind::Shape {
            generator: Some(id),
            ..
        } = &layer.kind
        {
            let manifest = registry
                .get_manifest(id)
                .ok_or_else(|| format!("Unknown generator {id}"))?;
            if !manifest.inputs.is_empty() {
                return Err(format!("{id} is not a generator"));
            }
        }
    }
    Ok(())
}

/// Shared validated mutation path for the desktop, web bridge and MCP.
pub fn commit(
    project: &mut Project,
    history: &mut History,
    registry: &EffectRegistry,
    op: Op,
) -> Result<(), String> {
    commit_grouped(project, history, registry, op, None)
}

/// Machine-actionable guidance appended to every rejected op so agents (and
/// humans) can self-correct without reading the source.
const APPLY_HINT: &str = "hint: properties are Position|Scale|Rotation|Opacity|AnchorPoint|Z; values are {\"Scalar\": number} or {\"Vec2\": [x, y]}; comp/layer ids must exist in /api/state; time is ticks (120000 = 1s); POST /api/describe returns the full op catalog with examples";
pub fn commit_grouped(
    project: &mut Project,
    history: &mut History,
    registry: &EffectRegistry,
    op: Op,
    group: Option<String>,
) -> Result<(), String> {
    project.validate()?;
    let mut candidate = project.clone();
    op.clone()
        .apply(&mut candidate)
        .map_err(|e| e.to_string())?;
    candidate.validate()?;
    validate_effects(&candidate, registry)?;
    fn touches_media(op: &Op) -> bool {
        match op {
            Op::AddMedia { .. } | Op::RestoreMedia { .. } => true,
            Op::Batch { ops, .. } => ops.iter().any(touches_media),
            _ => false,
        }
    }
    if touches_media(&op) {
        decode_embedded_frames(&candidate)?;
        decode_project_audio(&candidate)?;
    }
    history
        .commit_grouped(project, op, group)
        .map_err(|e| e.to_string())
}

#[derive(Clone, Default)]
pub struct DecodedImages {
    pub frames: BTreeMap<MediaId, Arc<CpuFrame>>,
}
impl MediaFrames for DecodedImages {
    fn shared_frame(&self, media: MediaId, _time: Time) -> Option<Arc<CpuFrame>> {
        self.frames.get(&media).cloned()
    }
    fn frame_rgba(&self, media: MediaId, _time: Time) -> Option<FrameView<'_>> {
        let frame = self.frames.get(&media)?;
        Some(FrameView {
            width: frame.width,
            height: frame.height,
            rgba: &frame.rgba,
        })
    }
}

pub fn decode_embedded_frames(project: &Project) -> Result<DecodedImages, String> {
    let mut images = DecodedImages::default();
    images.refresh(project)?;
    Ok(images)
}
impl DecodedImages {
    fn refresh(&mut self, project: &Project) -> Result<(), String> {
        self.frames.retain(|id, _| project.media.contains_key(id));
        for (id, asset) in &project.media {
            if self.frames.contains_key(id) {
                continue;
            }
            if let Some(image) = &asset.embedded {
                let bytes = STANDARD
                    .decode(image.rgba_base64.as_ref())
                    .map_err(|e| format!("Invalid image encoding: {e}"))?;
                if bytes.len() != image.width as usize * image.height as usize * 4 {
                    return Err("Image dimensions do not match pixel data".into());
                }
                self.frames.insert(
                    *id,
                    Arc::new(CpuFrame::from_rgba(image.width, image.height, bytes)),
                );
            }
        }
        Ok(())
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub project: Project,
    pub can_undo: bool,
    pub can_redo: bool,
    pub history: Vec<String>,
    pub revision: u64,
    pub audio_revision: u64,
}

pub struct EditorSession {
    pub project: Project,
    pub history: History,
    pub registry: EffectRegistry,
    images: DecodedImages,
    pub audio: DecodedAudios,
    audio_plans: audio::MixCache,
    revision: u64,
    audio_revision: u64,
    delta_base: Option<(u64, Arc<Project>)>,
    preview_snapshot: Arc<Project>,
    pub preview: Arc<PreviewRenderer>,
}
impl Default for EditorSession {
    fn default() -> Self {
        Self::new(demo_project()).expect("valid bundled example")
    }
}
impl EditorSession {
    pub fn new(project: Project) -> Result<Self, String> {
        project.validate()?;
        let registry = EffectRegistry::new();
        validate_effects(&project, &registry)?;
        let images = decode_embedded_frames(&project)?;
        let audio = decode_project_audio(&project)?;
        Ok(Self {
            preview_snapshot: Arc::new(project.clone()),
            preview: Arc::new(PreviewRenderer::default()),
            project,
            audio,
            audio_plans: Default::default(),
            history: History::new(),
            registry,
            images,
            revision: 0,
            audio_revision: 0,
            delta_base: None,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            project: self.project.clone(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            history: self.history.undo_descriptions(),
            revision: self.revision,
            audio_revision: self.audio_revision,
        }
    }
    fn changed(&mut self) -> Result<Value, String> {
        self.images.refresh(&self.project)?;
        self.audio.refresh(&self.project)?;
        if patch::audio_changed(&self.preview_snapshot, &self.project) {
            self.audio_revision += 1;
            self.audio_plans
                .lock()
                .map_err(|_| "Audio plan cache unavailable")?
                .clear();
        }
        self.revision += 1;
        self.preview_snapshot = Arc::new(self.project.clone());
        self.preview.invalidate();
        if let Some((base, before)) = self.delta_base.take() {
            return Ok(patch::delta(
                &before,
                &self.project,
                base,
                self.revision,
                self.audio_revision,
                &self.history,
            ));
        }
        self.ui_snapshot()
    }
    fn ui_snapshot(&self) -> Result<Value, String> {
        let mut snapshot = self.snapshot();
        // Original audio is transferred on file save/recovery, not every clip edit.
        for asset in snapshot.project.media.values_mut() {
            if let Some(audio) = &mut asset.audio {
                audio.data_base64 = Arc::from("");
            }
        }
        serde_json::to_value(snapshot).map_err(|e| e.to_string())
    }
    pub fn command(&mut self, name: &str, args: Value) -> Result<Value, String> {
        self.delta_base = if args["delta"].as_bool() == Some(true)
            && args["baseRevision"].as_u64() == Some(self.revision)
            && matches!(name, "apply" | "undo" | "redo" | "split_audio_clip")
        {
            Some((self.revision, self.preview_snapshot.clone()))
        } else {
            None
        };
        let result = self.command_inner(name, args);
        self.delta_base = None;
        result
    }
    fn command_inner(&mut self, name: &str, args: Value) -> Result<Value, String> {
        match name {
            "state" => self.ui_snapshot(),
            "audio_waveform" => self.audio_waveform(args),
            "audio_eq_curve" => {
                let eq: AudioEq =
                    serde_json::from_value(args["eq"].clone()).map_err(|e| e.to_string())?;
                Ok(json!(bonaparte_audio::eq_response(&eq, 48000)?))
            }
            "split_audio_clip" => self.split_audio(args),
            "catalog" => Ok(
                json!({ "effects": self.registry.list(), "renderer": "Rust preview runtime", "preview": self.preview.status(), "projectVersion": PROJECT_VERSION, "deltaProtocol":1, "interactionProtocol":1, "audio": {"protocol":2,"sampleRate":48000,"channels":2,"maxImportBytes":bonaparte_media::audio::MAX_AUDIO_FILE_BYTES,"originalMediaInSnapshot":false}, "ffmpeg": std::process::Command::new("ffmpeg").arg("-version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false) }),
            ),
            "preview_status" => {
                serde_json::to_value(self.preview.status()).map_err(|e| e.to_string())
            }
            "clear_preview_cache" | "retry_gpu" => {
                self.preview.clear(name == "retry_gpu");
                serde_json::to_value(self.preview.status()).map_err(|e| e.to_string())
            }
            "apply" => {
                let op: Op =
                    serde_json::from_value(args.get("op").cloned().ok_or("Missing operation")?)
                        .map_err(|e| format!("unknown op shape: {e} | {APPLY_HINT}"))?;
                let group = args["editGroup"].as_str().map(str::to_owned);
                commit_grouped(
                    &mut self.project,
                    &mut self.history,
                    &self.registry,
                    op,
                    group,
                )
                .map_err(|e| format!("{e} | {APPLY_HINT}"))?;
                self.changed()
            }
            "undo" => {
                // Name the reverted entry only when an undo actually applied —
                // a drained history must not report a stale label.
                let applied = self
                    .history
                    .undo(&mut self.project)
                    .map_err(|e| e.to_string())?;
                let mut response = self.changed()?;
                if applied {
                    if let Some(label) = self.history.redo_top_label() {
                        if let Some(object) = response.as_object_mut() {
                            object.insert("lastUndone".into(), json!(label));
                        }
                    }
                }
                Ok(response)
            }
            "redo" => {
                let applied = self
                    .history
                    .redo(&mut self.project)
                    .map_err(|e| e.to_string())?;
                let mut response = self.changed()?;
                if applied {
                    if let Some(label) = self.history.undo_top_label() {
                        if let Some(object) = response.as_object_mut() {
                            object.insert("lastRedone".into(), json!(label));
                        }
                    }
                }
                Ok(response)
            }
            "save_project" => {
                if args["compact"].as_bool() == Some(true) {
                    let text = serde_json::to_string(&ProjectFile {
                        format: "bonaparte".into(),
                        version: PROJECT_VERSION,
                        project: self.project.clone(),
                    })
                    .map_err(|e| e.to_string())?;
                    if text.len() > MAX_PROJECT_BYTES {
                        return Err("Project exceeds 64 MiB".into());
                    }
                    Ok(json!(text))
                } else {
                    Ok(json!(serialize_project(&self.project)?))
                }
            }
            "open_project" => {
                let project = parse_project(args["json"].as_str().ok_or("Missing project JSON")?)?;
                let revision = self.revision;
                let audio_revision = self.audio_revision + 1;
                let preview = self.preview.clone();
                *self = Self::new(project)?;
                self.preview = preview;
                self.revision = revision;
                self.audio_revision = audio_revision;
                self.changed()
            }
            "new_project" => {
                let mut project = Project::new(args["name"].as_str().unwrap_or("Untitled project"));
                project.create_comp(
                    "Composition 01",
                    1920,
                    1080,
                    FrameRate::FPS_30,
                    Time::from_secs_f64(30.0),
                );
                let revision = self.revision;
                let audio_revision = self.audio_revision + 1;
                let preview = self.preview.clone();
                *self = Self::new(project)?;
                self.preview = preview;
                self.revision = revision;
                self.audio_revision = audio_revision;
                self.changed()
            }
            "load_example" => {
                let revision = self.revision;
                let audio_revision = self.audio_revision + 1;
                let preview = self.preview.clone();
                *self = Self::default();
                self.preview = preview;
                self.revision = revision;
                self.audio_revision = audio_revision;
                self.changed()
            }
            "import_image" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Import {
                    name: String,
                    width: u32,
                    height: u32,
                    rgba_base64: String,
                    comp_id: CompId,
                }
                let image: Import = serde_json::from_value(args).map_err(|e| e.to_string())?;
                let comp = self
                    .project
                    .comp(image.comp_id)
                    .ok_or("Composition not found")?;
                let duration = comp.duration;
                let scale = (comp.width as f32 / image.width.max(1) as f32)
                    .min(comp.height as f32 / image.height.max(1) as f32)
                    .min(1.0)
                    * 100.0;
                let asset = MediaAsset {
                    id: MediaId(0),
                    name: image.name.clone(),
                    path: None,
                    kind: MediaKind::Image,
                    embedded: Some(EmbeddedImage {
                        width: image.width,
                        height: image.height,
                        rgba_base64: image.rgba_base64.into(),
                    }),
                    audio: None,
                    slot: None,
                    alias: None,
                    perception: None,
                };
                let mut layer = Layer::new(
                    &image.name,
                    LayerKind::Footage {
                        media: self.project.next_media,
                    },
                    Time::ZERO,
                    duration,
                );
                layer.transform.scale = [scale, scale];
                let op = Op::Batch {
                    label: format!("Imported {}", image.name),
                    ops: vec![
                        Op::AddMedia { asset },
                        Op::AddLayer {
                            comp: image.comp_id,
                            layer,
                        },
                    ],
                };
                // Decode/validate on a candidate first, keeping import atomic on malformed data.
                let mut candidate = self.project.clone();
                op.clone()
                    .apply(&mut candidate)
                    .map_err(|e| e.to_string())?;
                candidate.validate()?;
                decode_embedded_frames(&candidate)?;
                commit(&mut self.project, &mut self.history, &self.registry, op)?;
                self.changed()
            }
            "import_svg" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ImportSvg {
                    name: String,
                    svg: String,
                    comp_id: CompId,
                }
                let import: ImportSvg = serde_json::from_value(args).map_err(|e| e.to_string())?;
                if import.svg.len() > 8 * 1024 * 1024 {
                    return Err("SVG files must be smaller than 8 MB.".into());
                }
                let comp = self
                    .project
                    .comp(import.comp_id)
                    .ok_or("Composition not found")?;
                let duration = comp.duration;
                let comp_w = comp.width as f32;
                let comp_h = comp.height as f32;
                let (doc_w, doc_h, mut layers) =
                    svg_layers(&import.svg, duration.0).map_err(|e| e.to_string())?;
                // Fit the drawing inside the comp, never upscaling.
                let scale = (comp_w / doc_w.max(1.0))
                    .min(comp_h / doc_h.max(1.0))
                    .min(1.0);
                for layer in &mut layers {
                    layer.transform.scale = [scale * 100.0, scale * 100.0];
                    layer.transform.position = [
                        layer.transform.position[0] * scale,
                        layer.transform.position[1] * scale,
                    ];
                }
                let op = Op::Batch {
                    label: format!("Imported {}", import.name),
                    ops: layers
                        .into_iter()
                        .map(|layer| Op::AddLayer {
                            comp: import.comp_id,
                            layer,
                        })
                        .collect(),
                };
                let mut candidate = self.project.clone();
                op.clone()
                    .apply(&mut candidate)
                    .map_err(|e| e.to_string())?;
                candidate.validate()?;
                commit(&mut self.project, &mut self.history, &self.registry, op)?;
                self.changed()
            }
            "import_obj" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct ImportObj {
                    name: String,
                    obj: String,
                    comp_id: CompId,
                }
                let import: ImportObj = serde_json::from_value(args).map_err(|e| e.to_string())?;
                if import.obj.len() > 32 * 1024 * 1024 {
                    return Err("OBJ files must be smaller than 32 MB.".into());
                }
                let comp = self
                    .project
                    .comp(import.comp_id)
                    .ok_or("Composition not found")?;
                let duration = comp.duration;
                let comp_w = comp.width as f32;
                let comp_h = comp.height as f32;
                let mut layers = obj_layers(&import.obj, duration.0).map_err(|e| e.to_string())?;
                // Global model extent → fit the whole model inside the comp.
                // Units are arbitrary in OBJ, so upscaling is allowed.
                let mut min = [f32::MAX; 2];
                let mut max = [f32::MIN; 2];
                for layer in &layers {
                    let LayerKind::Shape { style, .. } = &layer.kind else {
                        continue;
                    };
                    let [w, h] = style.size.unwrap_or([0.0, 0.0]);
                    let [cx, cy] = layer.transform.position;
                    min[0] = min[0].min(cx - w * 0.5);
                    max[0] = max[0].max(cx + w * 0.5);
                    min[1] = min[1].min(cy - h * 0.5);
                    max[1] = max[1].max(cy + h * 0.5);
                }
                let model_w = (max[0] - min[0]).max(1e-3);
                let model_h = (max[1] - min[1]).max(1e-3);
                let scale = (comp_w * 0.8 / model_w)
                    .min(comp_h * 0.8 / model_h)
                    .min(1000.0);
                for layer in &mut layers {
                    layer.transform.scale = [scale * 100.0, scale * 100.0];
                    layer.transform.position = [
                        layer.transform.position[0] * scale,
                        layer.transform.position[1] * scale,
                    ];
                    layer.transform.z *= scale;
                }
                let op = Op::Batch {
                    label: format!("Imported {}", import.name),
                    ops: layers
                        .into_iter()
                        .map(|layer| Op::AddLayer {
                            comp: import.comp_id,
                            layer,
                        })
                        .collect(),
                };
                let mut candidate = self.project.clone();
                op.clone()
                    .apply(&mut candidate)
                    .map_err(|e| e.to_string())?;
                candidate.validate()?;
                commit(&mut self.project, &mut self.history, &self.registry, op)?;
                self.changed()
            }
            "export_progress" => Ok(export_progress_json()),
            "export_cancel" => {
                EXPORT_TELEMETRY.canceled.store(true, Ordering::Relaxed);
                Ok(json!({
                    "canceled": true,
                    "active": EXPORT_TELEMETRY.active.load(Ordering::Relaxed),
                }))
            }
            "vectorize_image" => {
                #[derive(Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Vectorize {
                    comp_id: CompId,
                    layer_id: LayerId,
                    #[serde(default)]
                    max_colors: Option<usize>,
                }
                let req: Vectorize = serde_json::from_value(args).map_err(|e| e.to_string())?;
                let comp = self
                    .project
                    .comp(req.comp_id)
                    .ok_or("Composition not found")?;
                let layer = self
                    .project
                    .layer(req.comp_id, req.layer_id)
                    .ok_or("Layer not found")?;
                let LayerKind::Footage { media } = &layer.kind else {
                    return Err("Only image layers can be vectorized".into());
                };
                let asset = self
                    .project
                    .media
                    .get(media)
                    .ok_or("The layer's media is missing")?;
                let Some(embedded) = &asset.embedded else {
                    return Err(
                        "Only embedded images can be vectorized (re-import the file to embed it)"
                            .into(),
                    );
                };
                let name = asset.name.clone();
                let rgba = STANDARD
                    .decode(embedded.rgba_base64.as_bytes())
                    .map_err(|e| format!("Embedded image is corrupt: {e}"))?;
                let (width, height) = (embedded.width, embedded.height);
                let duration = comp.duration;
                let source = layer.transform;
                let mut layers = bonaparte_engine::trace::trace_image(
                    &rgba,
                    width,
                    height,
                    req.max_colors.unwrap_or(8),
                    duration.0,
                )?;
                // Inherit the source layer's placement so the trace lands
                // exactly on top of the pixels it came from.
                for layer in &mut layers {
                    layer.transform = source;
                }
                let op = Op::Batch {
                    label: format!("Vectorized {name}"),
                    ops: vec![Op::SetLayerVisible {
                        comp: req.comp_id,
                        layer: req.layer_id,
                        visible: false,
                    }]
                    .into_iter()
                    .chain(layers.into_iter().map(|layer| Op::AddLayer {
                        comp: req.comp_id,
                        layer,
                    }))
                    .collect(),
                };
                let mut candidate = self.project.clone();
                op.clone()
                    .apply(&mut candidate)
                    .map_err(|e| e.to_string())?;
                candidate.validate()?;
                commit(&mut self.project, &mut self.history, &self.registry, op)?;
                self.changed()
            }
            _ => Err(format!("Unknown editor command: {name}")),
        }
    }
    /// Preview jobs share a document snapshot. Normal playback does not clone
    /// embedded image strings on every frame. Overrides are separately validated
    /// and deliberately excluded from the persistent frame cache.
    pub fn preview_input(&self, request: PreviewRequest) -> Result<PreviewJob, String> {
        if let Some(transform) = &request.transform_override {
            transform.validate(&self.project)?;
            if transform.comp_id != request.render.comp_id {
                return Err("Transform belongs to a different composition".into());
            }
        }
        let project = if request.render.layer_override.is_some() {
            Arc::new(
                self.render_input(RenderRequest {
                    comp_id: request.render.comp_id,
                    time: request.render.time,
                    bypass_effects: false,
                    layer_override: request.render.layer_override.clone(),
                    bit_depth: 8,
                    output_space: Default::default(),
                })?
                .project,
            )
        } else {
            self.preview_snapshot.clone()
        };
        self.preview.job(
            project,
            self.images.clone(),
            Arc::new(self.registry.clone()),
            self.revision,
            request,
        )
    }

    pub fn render_input(&self, request: RenderRequest) -> Result<RenderInput, String> {
        let mut project = self.project.clone();
        if let Some(layer) = request.layer_override {
            let c = project
                .comps
                .get_mut(&request.comp_id)
                .ok_or("Composition not found")?;
            if !c.layers.contains_key(&layer.id) {
                return Err("Preview layer not found".into());
            }
            c.layers.insert(layer.id, layer);
        }
        if request.bypass_effects {
            for layer in project
                .comps
                .values_mut()
                .flat_map(|c| c.layers.values_mut())
            {
                layer.effects.clear();
            }
        }
        project.validate()?;
        validate_effects(&project, &self.registry)?;
        if request.bit_depth != 0 && request.bit_depth != 8 && request.bit_depth != 16 {
            return Err("PNG export supports 8 or 16 bits per channel".into());
        }
        Ok(RenderInput {
            project,
            comp_id: request.comp_id,
            time: request.time,
            images: self.images.clone(),
            audio: self.audio.clone(),
            registry: self.registry.clone(),
            bit_depth: if request.bit_depth == 0 {
                8
            } else {
                request.bit_depth
            },
            output_space: request.output_space,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderRequest {
    pub comp_id: CompId,
    pub time: Time,
    #[serde(default)]
    pub bypass_effects: bool,
    #[serde(default)]
    pub layer_override: Option<Layer>,
    /// Deep-color export options: 8 (default) or 16 bits per channel, and
    /// the output color space (sRGB default; display-p3, rec2020, linear).
    /// Preview and canvas rendering are unaffected by these fields.
    #[serde(default)]
    pub bit_depth: u8,
    #[serde(default)]
    pub output_space: crate::color::OutputSpace,
}
#[derive(Clone)]
pub struct RenderInput {
    pub project: Project,
    pub comp_id: CompId,
    pub time: Time,
    pub images: DecodedImages,
    pub audio: DecodedAudios,
    pub registry: EffectRegistry,
    pub bit_depth: u8,
    pub output_space: crate::color::OutputSpace,
}
impl RenderInput {
    pub fn render(&self) -> Result<Frame, String> {
        render_comp_with_registry(
            &self.project,
            self.comp_id,
            self.time,
            &self.images,
            &self.registry,
        )
        .map_err(|e| e.to_string())
    }
    pub fn raw(&self) -> Result<Vec<u8>, String> {
        let frame = self.render()?;
        let mut bytes = Vec::with_capacity(frame.rgba.len() + 8);
        bytes.extend_from_slice(&frame.width.to_le_bytes());
        bytes.extend_from_slice(&frame.height.to_le_bytes());
        bytes.extend(frame.rgba);
        Ok(bytes)
    }
    pub fn png(&self) -> Result<Vec<u8>, String> {
        let frame = self.render()?;
        let deep = self.bit_depth == 16 || self.output_space != crate::color::OutputSpace::Srgb;
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, frame.width, frame.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(if self.bit_depth == 16 {
                png::BitDepth::Sixteen
            } else {
                png::BitDepth::Eight
            });
            if !deep {
                encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            }
            let mut encoder = encoder.write_header().map_err(|e| e.to_string())?;
            if deep {
                let samples = crate::color::convert_rgba(
                    frame.width,
                    frame.height,
                    &frame.rgba,
                    self.output_space,
                    self.bit_depth,
                )?;
                encoder
                    .write_image_data(&samples)
                    .map_err(|e| e.to_string())?;
            } else {
                encoder
                    .write_image_data(&frame.rgba)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(bytes)
    }
    pub fn export_mp4(&self, path: &std::path::Path) -> Result<(), String> {
        let comp = self
            .project
            .comp(self.comp_id)
            .ok_or("Composition not found")?;
        self.export_mp4_range(path, Time::ZERO, comp.duration, comp.fps)
            .map(|_| ())
    }
    pub fn export_mp4_range(
        &self,
        path: &std::path::Path,
        start: Time,
        end: Time,
        fps: FrameRate,
    ) -> Result<bonaparte_media::export::ExportStats, String> {
        let comp = self
            .project
            .comp(self.comp_id)
            .ok_or("Composition not found")?;
        if comp.width % 2 != 0 || comp.height % 2 != 0 {
            return Err("H.264 export requires even composition dimensions".into());
        }
        if start.0 < 0 || end <= start || end > comp.duration {
            return Err("Export range must be nonempty and inside the composition".into());
        }
        bonaparte_model::validation::validate_comp_size(comp.width, comp.height, fps, end - start)?;
        let tpf = fps.ticks_per_frame();
        let count = ((end.0 - start.0 + tpf - 1) / tpf) as usize;
        let directory = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let temp = tempfile::Builder::new()
            .prefix(".bonaparte-")
            .suffix(".mp4")
            .tempfile_in(directory)
            .map_err(|e| e.to_string())?
            .into_temp_path();
        let mut config =
            bonaparte_media::export::ExportConfig::new(&temp, comp.width, comp.height, fps, count)
                .with_preset("veryfast");
        let mut audio_temp = None;
        if self.project.has_audio(self.comp_id) {
            let plan = bonaparte_audio::MixPlan::new(&self.project, self.comp_id, &self.audio)?;
            let wav = tempfile::Builder::new()
                .prefix(".bonaparte-mix-")
                .suffix(".wav")
                .tempfile_in(directory)
                .map_err(|e| e.to_string())?
                .into_temp_path();
            let frames = ((count as u128 * fps.den as u128 * AUDIO_RATE as u128)
                .div_ceil(fps.num as u128)) as u64;
            bonaparte_media::audio::write_mix_wav(
                &plan,
                &wav,
                audio_frames(start).max(0) as u64,
                frames,
            )?;
            config.audio_wav = Some(wav.to_path_buf());
            audio_temp = Some(wav);
        }
        // One export at a time; the telemetry slot is process-global.
        if EXPORT_TELEMETRY
            .active
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("An export is already running — cancel it first".into());
        }
        EXPORT_TELEMETRY.canceled.store(false, Ordering::Relaxed);
        EXPORT_TELEMETRY.frames_done.store(0, Ordering::Relaxed);
        EXPORT_TELEMETRY
            .total_frames
            .store(count as u64, Ordering::Relaxed);
        EXPORT_TELEMETRY.stage.store(1, Ordering::Relaxed);
        EXPORT_TELEMETRY
            .started_ms
            .store(unix_now_ms(), Ordering::Relaxed);
        EXPORT_TELEMETRY.last_frame_ms.store(0, Ordering::Relaxed);
        let result = self.run_export_pipeline(&config, count, start, tpf);
        let outcome = match result {
            Ok(stats) => match temp.persist(path) {
                Ok(()) => {
                    drop(audio_temp);
                    let mut stats = stats;
                    stats.output_path = path.to_path_buf();
                    Ok(stats)
                }
                Err(e) => Err(e.to_string()),
            },
            Err(e) => Err(e),
        };
        EXPORT_TELEMETRY.stage.store(0, Ordering::Relaxed);
        EXPORT_TELEMETRY.active.store(false, Ordering::Relaxed);
        outcome
    }

    /// Render every frame through a parallel worker pool while the encoder
    /// consumes strictly in order. Rendering dominates export time, so this
    /// multiplies throughput by the core count; ffmpeg still sees one
    /// sequential RGBA stream. Progress/cancel ride the global telemetry.
    fn run_export_pipeline(
        &self,
        config: &bonaparte_media::export::ExportConfig,
        count: usize,
        start: Time,
        tpf: i64,
    ) -> Result<bonaparte_media::export::ExportStats, String> {
        let frame_bytes = (config.width as usize) * (config.height as usize) * 4;
        let hardware = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        // Cap workers by a 64 MB render buffer so 4K exports stay sane.
        let memory_slots = ((64 * 1024 * 1024) / frame_bytes.max(1)).clamp(1, 16);
        let workers = hardware.min(8).max(1).min(memory_slots);
        let canceled = || {
            EXPORT_TELEMETRY
                .canceled
                .load(std::sync::atomic::Ordering::Relaxed)
        };

        if workers <= 1 || count <= 1 {
            return bonaparte_media::export::export_mp4_stream(config, |_index, time| {
                if canceled() {
                    return Err("Export canceled".into());
                }
                let mut frame = render_comp_with_registry(
                    &self.project,
                    self.comp_id,
                    start + time,
                    &self.images,
                    &self.registry,
                )
                .map_err(|e| e.to_string())?;
                flatten_on_black(&mut frame);
                EXPORT_TELEMETRY
                    .frames_done
                    .store(_index as u64 + 1, Ordering::Relaxed);
                EXPORT_TELEMETRY
                    .last_frame_ms
                    .store(unix_now_ms(), Ordering::Relaxed);
                Ok(frame.rgba)
            })
            .map_err(|e| e.to_string());
        }

        struct Shared {
            /// Finished frames keyed by index (workers complete out of order)
            /// plus the number of frames currently being rendered.
            queue: Mutex<(VecDeque<(usize, Vec<u8>)>, usize)>,
            cv: Condvar,
        }
        let shared = Arc::new(Shared {
            queue: Mutex::new((VecDeque::new(), 0)),
            cv: Condvar::new(),
        });
        let next_job = AtomicUsize::new(0);
        let first_error: Mutex<Option<String>> = Mutex::new(None);
        let alive = AtomicUsize::new(workers);

        std::thread::scope(|scope| {
            for _ in 0..workers {
                let shared = Arc::clone(&shared);
                let next = &next_job;
                let error_slot = &first_error;
                let alive = &alive;
                scope.spawn(move || {
                    loop {
                        if canceled() {
                            break;
                        }
                        let job = next.fetch_add(1, Ordering::SeqCst);
                        if job >= count {
                            break;
                        }
                        shared.queue.lock().unwrap().1 += 1;
                        let rendered = render_comp_with_registry(
                            &self.project,
                            self.comp_id,
                            start + Time((job as i64) * tpf),
                            &self.images,
                            &self.registry,
                        )
                        .map(|mut frame| {
                            flatten_on_black(&mut frame);
                            frame.rgba
                        })
                        .map_err(|e| e.to_string());
                        {
                            let mut guard = shared.queue.lock().unwrap();
                            guard.1 -= 1;
                            match rendered {
                                Ok(rgba) => guard.0.push_back((job, rgba)),
                                Err(e) => {
                                    if !canceled() {
                                        let mut slot = error_slot.lock().unwrap();
                                        if slot.is_none() {
                                            *slot = Some(e);
                                        }
                                    }
                                }
                            }
                        }
                        shared.cv.notify_all();
                    }
                    alive.fetch_sub(1, Ordering::SeqCst);
                    shared.cv.notify_all();
                });
            }

            let consume = |index: usize, _time: Time| -> Result<Vec<u8>, String> {
                loop {
                    if canceled() {
                        return Err("Export canceled".into());
                    }
                    let mut guard = shared.queue.lock().unwrap();
                    if let Some(pos) = guard.0.iter().position(|(i, _)| *i == index) {
                        let (_, rgba) = guard.0.remove(pos).unwrap();
                        drop(guard);
                        EXPORT_TELEMETRY
                            .frames_done
                            .store(index as u64 + 1, Ordering::Relaxed);
                        EXPORT_TELEMETRY
                            .last_frame_ms
                            .store(unix_now_ms(), Ordering::Relaxed);
                        return Ok(rgba);
                    }
                    let idle = guard.0.is_empty() && guard.1 == 0;
                    let dead = alive.load(Ordering::SeqCst) == 0;
                    if idle && dead {
                        let msg = first_error
                            .lock()
                            .unwrap()
                            .clone()
                            .unwrap_or_else(|| "Frame rendering stopped early".into());
                        return Err(msg);
                    }
                    let (waited, _timeout) = shared
                        .cv
                        .wait_timeout(guard, std::time::Duration::from_millis(40))
                        .unwrap();
                    drop(waited);
                }
            };
            bonaparte_media::export::export_mp4_stream(config, consume).map_err(|e| e.to_string())
        })
    }
}

/// A real, editable example document, not renderer-side special cases.
pub fn demo_project() -> Project {
    serde_json::from_str(include_str!("../../../examples/orbit.bonaparte.json"))
        .expect("bundled editable example")
}

/// Video encoders discard alpha; flatten in linear light instead of leaking
/// hidden RGB from transparent pixels into the exported image.
pub fn flatten_on_black(frame: &mut Frame) {
    for pixel in frame.rgba.chunks_exact_mut(4) {
        if pixel[3] == 255 {
            continue;
        }
        let alpha = pixel[3] as f32 / 255.0;
        for channel in &mut pixel[..3] {
            let linear =
                bonaparte_effects::grading::srgb_to_linear(*channel as f32 / 255.0) * alpha;
            *channel = (bonaparte_effects::grading::linear_to_srgb(linear) * 255.0).round() as u8;
        }
        pixel[3] = 255;
    }
}
