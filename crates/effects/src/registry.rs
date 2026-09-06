//! Plugin registry and pack loader for bonaparte-effects.
//!
//! Owns:
//! - Compile-time embedded first-party built-in effect packs (manifest + WGSL).
//! - Dynamic filesystem scanner for third-party or local user plugin folders.
//! - Querying and looking up manifests and WGSL shader sources.

use bonaparte_model::{EffectManifest, ManifestError};
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
}

/// Registry holding all available GPU motion graphics effect plugins.
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

pub const DIRECTIONAL_BLUR_MANIFEST: &str =
    include_str!("../packs/directional_blur/manifest.toml");
pub const DIRECTIONAL_BLUR_SHADER: &str =
    include_str!("../packs/directional_blur/directional_blur.wgsl");

pub const CIRCLE_MANIFEST: &str = include_str!("../packs/circle/manifest.toml");
pub const CIRCLE_SHADER: &str = include_str!("../packs/circle/circle.wgsl");

/// All first-party built-in manifests in canonical UI presentation order.
pub fn builtin_manifests() -> Vec<EffectManifest> {
    vec![
        EffectManifest::parse(GLOW_MANIFEST).expect("builtin glow manifest must parse"),
        EffectManifest::parse(BLUR_MANIFEST).expect("builtin blur manifest must parse"),
        EffectManifest::parse(DROP_SHADOW_MANIFEST)
            .expect("builtin drop_shadow manifest must parse"),
        EffectManifest::parse(COLOR_ADJUST_MANIFEST)
            .expect("builtin color_adjust manifest must parse"),
        EffectManifest::parse(TRANSFORM_MANIFEST).expect("builtin transform manifest must parse"),
        EffectManifest::parse(VIGNETTE_MANIFEST).expect("builtin vignette manifest must parse"),
        EffectManifest::parse(CHROMATIC_ABERRATION_MANIFEST)
            .expect("builtin chromatic_aberration manifest must parse"),
        EffectManifest::parse(INVERT_MANIFEST).expect("builtin invert manifest must parse"),
        EffectManifest::parse(TINT_MANIFEST).expect("builtin tint manifest must parse"),
        EffectManifest::parse(DIRECTIONAL_BLUR_MANIFEST)
            .expect("builtin directional_blur manifest must parse"),
        EffectManifest::parse(CIRCLE_MANIFEST).expect("builtin circle manifest must parse"),
    ]
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
    ]
}

impl EffectRegistry {
    /// Create an EffectRegistry populated with all 10 core built-in effect packs.
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
    pub fn load_pack_from_dir(&mut self, pack_dir: impl AsRef<Path>) -> Result<String, RegistryError> {
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

        let shader_source =
            std::fs::read_to_string(&shader_path).map_err(|e| RegistryError::Io {
                path: shader_path,
                source: e,
            })?;

        let id = manifest.id.clone();
        self.effects.insert(
            id.clone(),
            RegisteredEffect {
                manifest,
                shader_source,
                is_builtin: false,
                directory: Some(pack_dir),
            },
        );

        Ok(id)
    }

    /// Dynamically scan a root directory for effect packs.
    ///
    /// Checks all immediate subdirectories for manifest.toml. Any valid pack found
    /// is parsed, validated, and registered. Returns the list of loaded effect IDs.
    pub fn scan_directory(&mut self, root_dir: impl AsRef<Path>) -> Result<Vec<String>, RegistryError> {
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
