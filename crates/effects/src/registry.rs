//! Plugin registry and pack loader for bonaparte-effects.
//!
//! Owns:
//! - Compile-time embedded first-party built-in effect packs (manifest + WGSL).
//! - Dynamic filesystem scanner for third-party or local user plugin folders.
//! - Querying and looking up manifests and WGSL shader sources.

use crate::cpu_reference::{CpuEvalError, CpuFrame, ParamValue};
use bonaparte_model::{EffectInstance, EffectManifest, ManifestError, ParamKind, PropValue, Time};
use std::collections::HashMap;

/// Public CPU plugin callback. No engine hooks or filesystem privileges required.
pub type CpuEvaluator =
    fn(&str, &CpuFrame, &HashMap<String, ParamValue>) -> Result<CpuFrame, CpuEvalError>;

pub fn builtin_registry() -> &'static EffectRegistry {
    static REGISTRY: std::sync::OnceLock<EffectRegistry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(EffectRegistry::new)
}
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Manifest parse/validation error in {path}: {source}")]
    Manifest {
        path: PathBuf,
        #[source]
        source: ManifestError,
    },
    #[error("Shader file {shader} missing in pack directory {dir}")]
    ShaderMissing { shader: String, dir: PathBuf },
    #[error("Duplicate effect id {0}")]
    DuplicateId(String),
}

/// A registered effect with its validated manifest, shader source, and source provenance.
#[derive(Debug, Clone)]
pub struct RegisteredEffect {
    pub manifest: EffectManifest,
    pub shader_source: String,
    pub is_builtin: bool,
    pub directory: Option<PathBuf>,
    pub evaluator: Option<CpuEvaluator>,
}

/// Registry of effect/generator metadata and optional CPU evaluator callbacks.
#[derive(Debug, Clone, Default)]
pub struct EffectRegistry {
    effects: BTreeMap<String, RegisteredEffect>,
}

// Built-in pack manifests and WGSL shaders embedded at compile time:
pub const GLOW_MANIFEST: &str = include_str!("../packs/glow/manifest.toml");
pub const GLOW_SHADER: &str = include_str!("../packs/glow/glow.wgsl");

pub const BLUR_MANIFEST: &str = include_str!("../packs/blur/manifest.toml");
pub const BLUR_SHADER: &str = include_str!("../packs/blur/blur.wgsl");

pub const DROP_SHADOW_MANIFEST: &str = include_str!("../packs/drop_shadow/manifest.toml");
pub const DROP_SHADOW_SHADER: &str = include_str!("../packs/drop_shadow/drop_shadow.wgsl");

pub const COLOR_ADJUST_MANIFEST: &str = include_str!("../packs/color_adjust/manifest.toml");
pub const COLOR_ADJUST_SHADER: &str = include_str!("../packs/color_adjust/color_adjust.wgsl");

pub const TRANSFORM_MANIFEST: &str = include_str!("../packs/transform/manifest.toml");
pub const TRANSFORM_SHADER: &str = include_str!("../packs/transform/transform.wgsl");

pub const VIGNETTE_MANIFEST: &str = include_str!("../packs/vignette/manifest.toml");
pub const VIGNETTE_SHADER: &str = include_str!("../packs/vignette/vignette.wgsl");

pub const CHROMATIC_ABERRATION_MANIFEST: &str =
    include_str!("../packs/chromatic_aberration/manifest.toml");
pub const CHROMATIC_ABERRATION_SHADER: &str =
    include_str!("../packs/chromatic_aberration/chromatic_aberration.wgsl");

pub const INVERT_MANIFEST: &str = include_str!("../packs/invert/manifest.toml");
pub const INVERT_SHADER: &str = include_str!("../packs/invert/invert.wgsl");

pub const TINT_MANIFEST: &str = include_str!("../packs/tint/manifest.toml");
pub const TINT_SHADER: &str = include_str!("../packs/tint/tint.wgsl");

pub const DIRECTIONAL_BLUR_MANIFEST: &str = include_str!("../packs/directional_blur/manifest.toml");
pub const DIRECTIONAL_BLUR_SHADER: &str =
    include_str!("../packs/directional_blur/directional_blur.wgsl");

pub const CIRCLE_MANIFEST: &str = include_str!("../packs/circle/manifest.toml");
pub const CIRCLE_SHADER: &str = include_str!("../packs/circle/circle.wgsl");

/// All first-party built-in manifests in canonical UI presentation order.
pub fn builtin_manifests() -> Vec<EffectManifest> {
    builtin_packs()
        .into_iter()
        .map(|(manifest, _)| EffectManifest::parse(manifest).expect("valid embedded manifest"))
        .collect()
}

/// All first-party (manifest, shader) pairs.
pub fn builtin_packs() -> Vec<(&'static str, &'static str)> {
    vec![
        (GLOW_MANIFEST, GLOW_SHADER),
        (BLUR_MANIFEST, BLUR_SHADER),
        (DROP_SHADOW_MANIFEST, DROP_SHADOW_SHADER),
        (COLOR_ADJUST_MANIFEST, COLOR_ADJUST_SHADER),
        (TRANSFORM_MANIFEST, TRANSFORM_SHADER),
        (VIGNETTE_MANIFEST, VIGNETTE_SHADER),
        (CHROMATIC_ABERRATION_MANIFEST, CHROMATIC_ABERRATION_SHADER),
        (INVERT_MANIFEST, INVERT_SHADER),
        (TINT_MANIFEST, TINT_SHADER),
        (DIRECTIONAL_BLUR_MANIFEST, DIRECTIONAL_BLUR_SHADER),
        (CIRCLE_MANIFEST, CIRCLE_SHADER),
        (
            include_str!("../packs/color_grade/manifest.toml"),
            include_str!("../packs/color_grade/color_grade.wgsl"),
        ),
        (
            include_str!("../packs/gradient/manifest.toml"),
            include_str!("../packs/gradient/gradient.wgsl"),
        ),
    ]
}

impl EffectRegistry {
    /// Create a registry populated with the first-party filter and generator packs.
    pub fn new() -> Self {
        let mut registry = Self::empty();
        for (manifest_str, shader_str) in builtin_packs() {
            let manifest = EffectManifest::parse(manifest_str)
                .expect("embedded builtin effect manifests must always be valid");
            let id = manifest.id.clone();
            registry.effects.insert(
                id,
                RegisteredEffect {
                    manifest,
                    shader_source: shader_str.to_string(),
                    is_builtin: true,
                    directory: None,
                    evaluator: Some(crate::cpu_reference::evaluate_builtin),
                },
            );
        }
        registry
    }

    /// Create an empty registry with no loaded effects.
    pub fn empty() -> Self {
        Self {
            effects: BTreeMap::new(),
        }
    }

    /// Register an effect with an already-parsed manifest and shader source.
    pub fn register(
        &mut self,
        manifest: EffectManifest,
        shader_source: String,
        is_builtin: bool,
        directory: Option<PathBuf>,
    ) -> Result<(), RegistryError> {
        manifest
            .validate()
            .map_err(|source| RegistryError::Manifest {
                path: PathBuf::from("<registered>"),
                source,
            })?;
        let id = manifest.id.clone();
        if self.effects.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        self.effects.insert(
            id,
            RegisteredEffect {
                manifest,
                shader_source,
                is_builtin,
                directory,
                evaluator: None,
            },
        );
        Ok(())
    }

    /// Get a registered effect by its unique id.
    pub fn get(&self, id: &str) -> Option<&RegisteredEffect> {
        self.effects.get(id)
    }

    /// Get a registered effect manifest by id.
    pub fn get_manifest(&self, id: &str) -> Option<&EffectManifest> {
        self.effects.get(id).map(|e| &e.manifest)
    }

    /// Get a registered effect WGSL shader source by id.
    pub fn get_shader(&self, id: &str) -> Option<&str> {
        self.effects.get(id).map(|e| e.shader_source.as_str())
    }

    /// List all registered effect manifests.
    pub fn list(&self) -> Vec<&EffectManifest> {
        self.effects.values().map(|e| &e.manifest).collect()
    }

    /// Check whether an effect is registered.
    pub fn contains(&self, id: &str) -> bool {
        self.effects.contains_key(id)
    }

    /// Count of registered effects.
    pub fn len(&self) -> usize {
        self.effects.len()
    }

    /// True if no effects are registered.
    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    /// Iterator over registered effects.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &RegisteredEffect)> {
        self.effects.iter()
    }

    /// Load a single pack directory containing manifest.toml and its referenced shader file.
    pub fn load_pack_from_dir(
        &mut self,
        pack_dir: impl AsRef<Path>,
    ) -> Result<String, RegistryError> {
        let pack_dir = pack_dir.as_ref().to_path_buf();
        let manifest_path = pack_dir.join("manifest.toml");
        if !manifest_path.is_file() {
            return Err(RegistryError::Io {
                path: manifest_path,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "manifest.toml not found in pack directory",
                ),
            });
        }

        let toml_src = std::fs::read_to_string(&manifest_path).map_err(|e| RegistryError::Io {
            path: manifest_path.clone(),
            source: e,
        })?;

        let manifest = EffectManifest::parse(&toml_src).map_err(|e| RegistryError::Manifest {
            path: manifest_path.clone(),
            source: e,
        })?;

        let shader_path = pack_dir.join(&manifest.shader);
        if !shader_path.is_file() {
            return Err(RegistryError::ShaderMissing {
                shader: manifest.shader.clone(),
                dir: pack_dir,
            });
        }

        let canonical_dir =
            std::fs::canonicalize(&pack_dir).map_err(|source| RegistryError::Io {
                path: pack_dir.clone(),
                source,
            })?;
        let canonical_shader =
            std::fs::canonicalize(&shader_path).map_err(|source| RegistryError::Io {
                path: shader_path.clone(),
                source,
            })?;
        if !canonical_shader.starts_with(&canonical_dir) {
            return Err(RegistryError::ShaderMissing {
                shader: manifest.shader.clone(),
                dir: pack_dir,
            });
        }
        let shader_source =
            std::fs::read_to_string(&shader_path).map_err(|e| RegistryError::Io {
                path: shader_path,
                source: e,
            })?;

        let id = manifest.id.clone();
        if self.effects.contains_key(&id) {
            return Err(RegistryError::DuplicateId(id));
        }
        self.effects.insert(
            id.clone(),
            RegisteredEffect {
                manifest,
                shader_source,
                is_builtin: false,
                directory: Some(pack_dir),
                evaluator: None,
            },
        );

        Ok(id)
    }

    /// Dynamically scan a root directory for effect packs.
    ///
    /// Checks all immediate subdirectories for manifest.toml. Any valid pack found
    /// is parsed, validated, and registered. Returns the list of loaded effect IDs.
    pub fn scan_directory(
        &mut self,
        root_dir: impl AsRef<Path>,
    ) -> Result<Vec<String>, RegistryError> {
        let root_dir = root_dir.as_ref().to_path_buf();
        let mut loaded = Vec::new();

        if !root_dir.is_dir() {
            return Ok(loaded);
        }

        let read_dir = std::fs::read_dir(&root_dir).map_err(|e| RegistryError::Io {
            path: root_dir.clone(),
            source: e,
        })?;

        for entry in read_dir {
            let entry = entry.map_err(|e| RegistryError::Io {
                path: root_dir.clone(),
                source: e,
            })?;
            let path = entry.path();
            if path.is_dir() && path.join("manifest.toml").is_file() {
                match self.load_pack_from_dir(&path) {
                    Ok(id) => loaded.push(id),
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(loaded)
    }
}

impl EffectRegistry {
    /// A first- or third-party CPU pack uses this exact same registration contract.
    pub fn register_cpu(
        &mut self,
        manifest: EffectManifest,
        shader: String,
        evaluator: CpuEvaluator,
    ) -> Result<(), RegistryError> {
        let id = manifest.id.clone();
        self.register(manifest, shader, false, None)?;
        self.effects.get_mut(&id).expect("just inserted").evaluator = Some(evaluator);
        Ok(())
    }

    fn parameters(
        &self,
        id: &str,
        supplied: &HashMap<String, ParamValue>,
    ) -> Result<HashMap<String, ParamValue>, CpuEvalError> {
        let effect = self
            .get(id)
            .ok_or_else(|| CpuEvalError::UnknownEffect(id.into()))?;
        for (key, value) in supplied {
            let param = effect
                .manifest
                .params
                .iter()
                .find(|p| &p.id == key)
                .ok_or_else(|| CpuEvalError::InvalidParam(key.clone()))?;
            if !param.kind.accepts(value) {
                return Err(CpuEvalError::InvalidParam(key.clone()));
            }
        }
        Ok(effect
            .manifest
            .params
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    supplied
                        .get(&p.id)
                        .cloned()
                        .unwrap_or_else(|| p.kind.default_value()),
                )
            })
            .collect())
    }

    pub fn validate_instance(&self, instance: &EffectInstance) -> Result<(), CpuEvalError> {
        let values = instance.params.clone().into_iter().collect();
        self.parameters(&instance.effect_id, &values)?;
        let effect = self
            .get(&instance.effect_id)
            .ok_or_else(|| CpuEvalError::UnknownEffect(instance.effect_id.clone()))?;
        if instance.enabled && effect.evaluator.is_none() {
            return Err(CpuEvalError::UnsupportedBackend(instance.effect_id.clone()));
        }
        for (id, track) in &instance.tracks {
            let param = effect
                .manifest
                .params
                .iter()
                .find(|p| &p.id == id)
                .ok_or_else(|| CpuEvalError::InvalidParam(id.clone()))?;
            for key in &track.keys {
                let value = match key.value {
                    PropValue::Scalar(v) => ParamValue::Float(v),
                    PropValue::Vec2(v) => ParamValue::Point(v),
                };
                if !param.kind.accepts(&value) {
                    return Err(CpuEvalError::InvalidParam(id.clone()));
                }
            }
        }
        Ok(())
    }

    pub fn evaluate(
        &self,
        id: &str,
        input: &CpuFrame,
        supplied: &HashMap<String, ParamValue>,
    ) -> Result<CpuFrame, CpuEvalError> {
        let params = self.parameters(id, supplied)?;
        let effect = self
            .get(id)
            .ok_or_else(|| CpuEvalError::UnknownEffect(id.into()))?;
        let evaluator = effect
            .evaluator
            .ok_or_else(|| CpuEvalError::UnsupportedBackend(id.into()))?;
        evaluator(id, input, &params)
    }

    /// Resolve validated defaults, animation and pixel units once for either backend.
    pub fn instance_parameters(
        &self,
        instance: &EffectInstance,
        time: Time,
        pixel_scale: f32,
    ) -> Result<HashMap<String, ParamValue>, CpuEvalError> {
        if !pixel_scale.is_finite() || pixel_scale <= 0.0 || pixel_scale > 8.0 {
            return Err(CpuEvalError::InvalidParam("preview scale".into()));
        }
        self.validate_instance(instance)?;
        let mut params: HashMap<_, _> = instance.evaluated_params(time).into_iter().collect();
        let manifest = &self
            .get(&instance.effect_id)
            .expect("validated effect")
            .manifest;
        for param in &manifest.params {
            if instance.tracks.contains_key(&param.id) {
                if let (ParamKind::Slider { min, max, .. }, Some(ParamValue::Float(v))) =
                    (&param.kind, params.get_mut(&param.id))
                {
                    *v = v.clamp(*min, *max);
                }
            }
        }
        let mut params = self.parameters(&instance.effect_id, &params)?;
        for param in &manifest.params {
            if param.scale_with_resolution {
                match params.get_mut(&param.id) {
                    Some(ParamValue::Float(v)) => *v *= pixel_scale,
                    Some(ParamValue::Point(v)) => {
                        v[0] *= pixel_scale;
                        v[1] *= pixel_scale;
                    }
                    _ => {}
                }
            }
        }
        Ok(params)
    }
    pub fn evaluate_scaled(
        &self,
        id: &str,
        input: &CpuFrame,
        supplied: &HashMap<String, ParamValue>,
        density: f32,
    ) -> Result<CpuFrame, CpuEvalError> {
        if !density.is_finite() || density <= 0.0 || density > 8.0 {
            return Err(CpuEvalError::InvalidParam("raster density".into()));
        }
        let mut params = self.parameters(id, supplied)?;
        let pack = self
            .get(id)
            .ok_or_else(|| CpuEvalError::UnknownEffect(id.into()))?;
        for p in &pack.manifest.params {
            if p.scale_with_resolution {
                match params.get_mut(&p.id) {
                    Some(ParamValue::Float(v)) => *v *= density,
                    Some(ParamValue::Point(v)) => {
                        v[0] *= density;
                        v[1] *= density;
                    }
                    _ => {}
                }
            }
        }
        let evaluate = pack
            .evaluator
            .ok_or_else(|| CpuEvalError::UnsupportedBackend(id.into()))?;
        evaluate(id, input, &params)
    }

    pub fn evaluate_instance(
        &self,
        instance: &EffectInstance,
        input: &CpuFrame,
        time: Time,
    ) -> Result<CpuFrame, CpuEvalError> {
        self.evaluate_instance_scaled(instance, input, time, 1.0)
    }
    pub fn evaluate_instance_scaled(
        &self,
        instance: &EffectInstance,
        input: &CpuFrame,
        time: Time,
        pixel_scale: f32,
    ) -> Result<CpuFrame, CpuEvalError> {
        if !instance.enabled {
            return Ok(input.clone());
        }
        let params = self.instance_parameters(instance, time, pixel_scale)?;
        let evaluator = self
            .get(&instance.effect_id)
            .and_then(|e| e.evaluator)
            .ok_or_else(|| CpuEvalError::UnsupportedBackend(instance.effect_id.clone()))?;
        evaluator(&instance.effect_id, input, &params)
    }
}
