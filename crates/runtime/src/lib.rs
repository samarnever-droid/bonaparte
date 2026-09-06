//! The application service shared by Tauri and the development web bridge.
//! All persistent edits use the model's Op/History path. No browser-side model emulator.
use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_effects::{CpuFrame, EffectRegistry};
use bonaparte_engine::reference::{render_comp_with_registry, Frame, FrameView, MediaFrames};
use bonaparte_model::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const PROJECT_VERSION: u32 = 2;
pub const MAX_PROJECT_BYTES: usize = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    format: String,
    version: u32,
    project: Project,
}

pub fn serialize_project(project: &Project) -> Result<String, String> {
    project.validate()?;
    serde_json::to_string_pretty(&ProjectFile {
        format: "bonaparte".into(),
        version: PROJECT_VERSION,
        project: project.clone(),
    })
    .map_err(|e| e.to_string())
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
        if file.format != "bonaparte" || file.version != PROJECT_VERSION {
            return Err("Unsupported project format/version".into());
        }
        file.project
    } else {
        serde_json::from_value(root).map_err(|e| e.to_string())?
    };
    project.validate()?;
    validate_effects(&project, &EffectRegistry::new())?;
    decode_embedded_frames(&project)?;
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
    }
    history.commit(project, op).map_err(|e| e.to_string())
}

#[derive(Clone, Default)]
pub struct DecodedImages {
    pub frames: BTreeMap<MediaId, Arc<CpuFrame>>,
}
impl MediaFrames for DecodedImages {
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
                    .decode(&image.rgba_base64)
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
}

pub struct EditorSession {
    pub project: Project,
    pub history: History,
    pub registry: EffectRegistry,
    images: DecodedImages,
    revision: u64,
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
        Ok(Self {
            project,
            history: History::new(),
            registry,
            images,
            revision: 0,
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            project: self.project.clone(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            history: self.history.undo_descriptions(),
            revision: self.revision,
        }
    }
    fn changed(&mut self) -> Result<Value, String> {
        self.images.refresh(&self.project)?;
        self.revision += 1;
        serde_json::to_value(self.snapshot()).map_err(|e| e.to_string())
    }
    pub fn command(&mut self, name: &str, args: Value) -> Result<Value, String> {
        match name {
            "state" => serde_json::to_value(self.snapshot()).map_err(|e| e.to_string()),
            "catalog" => Ok(
                json!({ "effects": self.registry.list(), "renderer": "CPU reference", "projectVersion": PROJECT_VERSION, "ffmpeg": std::process::Command::new("ffmpeg").arg("-version").stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().map(|s| s.success()).unwrap_or(false) }),
            ),
            "apply" => {
                let op: Op =
                    serde_json::from_value(args.get("op").cloned().ok_or("Missing operation")?)
                        .map_err(|e| e.to_string())?;
                commit(&mut self.project, &mut self.history, &self.registry, op)?;
                self.changed()
            }
            "undo" => {
                self.history
                    .undo(&mut self.project)
                    .map_err(|e| e.to_string())?;
                self.changed()
            }
            "redo" => {
                self.history
                    .redo(&mut self.project)
                    .map_err(|e| e.to_string())?;
                self.changed()
            }
            "save_project" => Ok(json!(serialize_project(&self.project)?)),
            "open_project" => {
                let project = parse_project(args["json"].as_str().ok_or("Missing project JSON")?)?;
                let revision = self.revision;
                *self = Self::new(project)?;
                self.revision = revision;
                self.changed()
            }
            "new_project" => {
                let mut project = Project::new(args["name"].as_str().unwrap_or("Untitled project"));
                project.create_comp(
                    "Composition 01",
                    1920,
                    1080,
                    FrameRate::FPS_30,
                    Time::from_secs_f64(10.0),
                );
                let revision = self.revision;
                *self = Self::new(project)?;
                self.revision = revision;
                self.changed()
            }
            "load_example" => {
                let revision = self.revision;
                *self = Self::default();
                self.revision = revision;
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
                        rgba_base64: image.rgba_base64,
                    }),
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
            _ => Err(format!("Unknown editor command: {name}")),
        }
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
        Ok(RenderInput {
            project,
            comp_id: request.comp_id,
            time: request.time,
            images: self.images.clone(),
            registry: self.registry.clone(),
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
}
#[derive(Clone)]
pub struct RenderInput {
    pub project: Project,
    pub comp_id: CompId,
    pub time: Time,
    pub images: DecodedImages,
    pub registry: EffectRegistry,
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
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, frame.width, frame.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
            encoder
                .write_header()
                .map_err(|e| e.to_string())?
                .write_image_data(&frame.rgba)
                .map_err(|e| e.to_string())?;
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
        let config =
            bonaparte_media::export::ExportConfig::new(&temp, comp.width, comp.height, fps, count)
                .with_preset("veryfast");
        let mut stats = bonaparte_media::export::export_mp4_stream(&config, |_, time| {
            let mut frame = render_comp_with_registry(
                &self.project,
                self.comp_id,
                start + time,
                &self.images,
                &self.registry,
            )
            .map_err(|e| e.to_string())?;
            flatten_on_black(&mut frame);
            Ok(frame.rgba)
        })
        .map_err(|e| e.to_string())?;
        temp.persist(path).map_err(|e| e.to_string())?;
        stats.output_path = path.to_path_buf();
        Ok(stats)
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
