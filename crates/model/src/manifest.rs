//! Plugin manifest schema — the day-one API contract (ARCHITECTURE.md).
//!
//! Every manifest declares `api_version`; a mismatched MAJOR is refused loudly.
//! Every parameter carries a human-readable `doc` string: the single source
//! rendered as the UI tooltip, the MCP tool description, and the generated
//! reference (RULES §7.2).

use serde::{Deserialize, Serialize};

/// The major version Bonaparte supports. A plugin built for a different
/// major is refused with a human-readable message (RULES §4.2).
pub const SUPPORTED_API_MAJOR: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectManifest {
    /// Semver-ish "MAJOR.MINOR", e.g. "1.0".
    pub api_version: String,
    /// Globally unique, namespaced: "builtin.glow", "com.acme.rain".
    pub id: String,
    pub name: String,
    /// Declared GPU cost — the engine's adaptive-quality budget uses this
    /// (RULES §3.2: the engine decides what runs, not the effect).
    pub cost: GpuCost,
    /// WGSL shader file, relative to the manifest.
    pub shader: String,
    /// Named texture inputs (e.g. ["input"]). Uniforms `u_texture`,
    /// `u_resolution`, `u_time` are auto-injected and not listed here.
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub params: Vec<ParamDef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuCost {
    Light,
    Medium,
    Heavy,
}

/// One typed parameter. The UI handle (slider/point on canvas), the MCP tool
/// schema, undo, and preset serialization are all generated from this
/// definition — one field, all consumers (ARCHITECTURE.md plugin system).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDef {
    pub id: String,
    pub name: String,
    /// What it does and when to use it. Rendered as tooltip + MCP docs.
    pub doc: String,
    pub kind: ParamKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ParamKind {
    Slider {
        min: f32,
        max: f32,
        default: f32,
    },
    Color {
        default: [f32; 4],
    },
    /// A 2D point — gets an on-canvas draggable handle automatically.
    Point {
        default: [f32; 2],
    },
    Checkbox {
        default: bool,
    },
    Dropdown {
        options: Vec<String>,
        default: usize,
    },
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ManifestError {
    #[error("unsupported api_version `{0}` (supported major: {SUPPORTED_API_MAJOR})")]
    UnsupportedApi(String),
    #[error("invalid manifest: {0}")]
    Invalid(String),
}

impl EffectManifest {
    /// Parse and validate a manifest from TOML text.
    pub fn parse(toml_src: &str) -> Result<Self, ManifestError> {
        let manifest: EffectManifest =
            toml::from_str(toml_src).map_err(|e| ManifestError::Invalid(e.to_string()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        let major = self
            .api_version
            .split('.')
            .next()
            .and_then(|m| m.parse::<u32>().ok())
            .ok_or_else(|| ManifestError::UnsupportedApi(self.api_version.clone()))?;
        if major != SUPPORTED_API_MAJOR {
            return Err(ManifestError::UnsupportedApi(self.api_version.clone()));
        }
        if self.id.is_empty() || self.name.is_empty() || self.shader.is_empty() {
            return Err(ManifestError::Invalid("id, name, and shader are required".into()));
        }
        let mut seen = std::collections::BTreeSet::new();
        for p in &self.params {
            if p.id.is_empty() {
                return Err(ManifestError::Invalid(format!(
                    "param in `{}` has empty id",
                    self.id
                )));
            }
            if !seen.insert(p.id.as_str()) {
                return Err(ManifestError::Invalid(format!(
                    "duplicate param id `{}` in `{}`",
                    p.id, self.id
                )));
            }
            if let ParamKind::Slider { min, max, .. } = &p.kind {
                if min > max {
                    return Err(ManifestError::Invalid(format!(
                        "param `{}`: min > max",
                        p.id
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLOW: &str = r#"
api_version = "1.0"
id = "builtin.glow"
name = "Glow"
cost = "medium"
shader = "glow.wgsl"
inputs = ["input"]

[[params]]
id = "radius"
name = "Radius"
doc = "Blur radius of the glow halo, in pixels."
kind = { Slider = { min = 0.0, max = 100.0, default = 12.0 } }
"#;

    #[test]
    fn parses_valid_manifest() {
        let m = EffectManifest::parse(GLOW).unwrap();
        assert_eq!(m.id, "builtin.glow");
        assert_eq!(m.cost, GpuCost::Medium);
        assert_eq!(m.params[0].id, "radius");
    }

    #[test]
    fn refuses_wrong_api_major() {
        let bad = GLOW.replace("api_version = \"1.0\"", "api_version = \"2.0\"");
        assert!(matches!(
            EffectManifest::parse(&bad),
            Err(ManifestError::UnsupportedApi(_))
        ));
    }

    #[test]
    fn refuses_duplicate_param_ids() {
        let dup = format!("{GLOW}\n[[params]]\nid = \"radius\"\nname = \"R2\"\ndoc = \"d\"\nkind = {{ Slider = {{ min = 0.0, max = 1.0, default = 0.5 }} }}");
        assert!(matches!(
            EffectManifest::parse(&dup),
            Err(ManifestError::Invalid(_))
        ));
    }

    #[test]
    fn refuses_inverted_slider_range() {
        let bad = GLOW.replace("min = 0.0, max = 100.0", "min = 100.0, max = 0.0");
        assert!(matches!(
            EffectManifest::parse(&bad),
            Err(ManifestError::Invalid(_))
        ));
    }
}
