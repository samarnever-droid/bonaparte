//! WGSL reflection: manifest order is not a uniform-buffer layout.
//! vec3/vec4 alignment and padding come from Naga's parsed member offsets.
use bonaparte_effects::ParamValue;
use std::collections::HashMap;

#[derive(Clone)]
pub struct UniformLayout {
    size: usize,
    fields: Vec<(String, usize, naga::TypeInner)>,
}
impl UniformLayout {
    /// True when the reflected Params struct declares a member with this name.
    /// Used by the host to detect multi-pass contracts (e.g. the separable
    /// blur pack's `direction_x`/`direction_y` pass direction).
    pub fn has_field(&self, name: &str) -> bool {
        self.fields.iter().any(|(field, _, _)| field == name)
    }

    pub fn reflect(source: &str) -> Result<Self, String> {
        let module = naga::front::wgsl::parse_str(source).map_err(|e| e.emit_to_string(source))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|e| format!("WGSL validation: {e:?}"))?;
        let var = module.global_variables.iter().map(|(_, v)| v).find(|v| {
            v.binding
                .as_ref()
                .is_some_and(|b| b.group == 0 && b.binding == 4)
        });
        let Some(var) = var else {
            return Ok(Self {
                size: 16,
                fields: vec![],
            });
        };
        let naga::TypeInner::Struct { members, span } = &module.types[var.ty].inner else {
            return Err("Effect binding 4 must be a Params struct".into());
        };
        let mut fields = vec![];
        for member in members {
            fields.push((
                member.name.clone().ok_or("Unnamed uniform member")?,
                member.offset as usize,
                module.types[member.ty].inner.clone(),
            ));
        }
        Ok(Self {
            size: (*span as usize).max(16).div_ceil(16) * 16,
            fields,
        })
    }
    pub fn pack(&self, params: &HashMap<String, ParamValue>) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0u8; self.size];
        for (name, offset, ty) in &self.fields {
            let Some(value) = params.get(name) else {
                if name.starts_with('_') {
                    continue;
                }
                return Err(format!("Missing GPU uniform {name}"));
            };
            let data = match (value, ty) {
                (
                    ParamValue::Float(v),
                    naga::TypeInner::Scalar(naga::Scalar {
                        kind: naga::ScalarKind::Float,
                        width: 4,
                    }),
                ) => v.to_le_bytes().to_vec(),
                (
                    ParamValue::Bool(v),
                    naga::TypeInner::Scalar(naga::Scalar {
                        kind: naga::ScalarKind::Uint,
                        width: 4,
                    }),
                ) => u32::from(*v).to_le_bytes().to_vec(),
                (
                    ParamValue::Index(v),
                    naga::TypeInner::Scalar(naga::Scalar {
                        kind: naga::ScalarKind::Uint,
                        width: 4,
                    }),
                ) => (*v as u32).to_le_bytes().to_vec(),
                (
                    ParamValue::Color(v),
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Quad,
                        scalar:
                            naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            },
                    },
                ) => v.iter().flat_map(|f| f.to_le_bytes()).collect(),
                (
                    ParamValue::Point(v),
                    naga::TypeInner::Vector {
                        size: naga::VectorSize::Bi,
                        scalar:
                            naga::Scalar {
                                kind: naga::ScalarKind::Float,
                                width: 4,
                            },
                    },
                ) => v.iter().flat_map(|f| f.to_le_bytes()).collect(),
                _ => return Err(format!("GPU uniform type mismatch for {name}")),
            };
            let end = *offset + data.len();
            if end > bytes.len() {
                return Err("Uniform extends beyond reflected buffer".into());
            }
            bytes[*offset..end].copy_from_slice(&data);
        }
        Ok(bytes)
    }
}
