use bonaparte_effects::{CpuEvalError, CpuFrame, EffectRegistry, ParamValue};
use bonaparte_engine::preview::{prepare_scene, render_scene_cpu, Resolution, SourceCache};
use bonaparte_engine::{Frame, FrameView, MediaFrames, NoMedia};
use bonaparte_gpu::GpuRenderer;
use bonaparte_model::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
fn gpu(test: impl FnOnce(&mut GpuRenderer)) {
    static GPU: OnceLock<Mutex<Option<GpuRenderer>>> = OnceLock::new();
    let mut renderer=GPU.get_or_init(||{
        if let Err(error)=bonaparte_gpu::probe(){
            assert!(std::env::var("BONAPARTE_REQUIRE_GPU_TESTS").as_deref()!=Ok("1"),"A GPU API adapter is required: {error}");
            eprintln!("GPU tests unavailable: {error}. Set BONAPARTE_REQUIRE_GPU_TESTS=1 to prohibit skipping.");return Mutex::new(None);
        }
        let renderer=GpuRenderer::new().expect("An available adapter must initialize the actual GPU pipelines");
        eprintln!("Executing GPU tests on {} ({}, software={})",renderer.info.name,renderer.info.backend,renderer.info.software);
        Mutex::new(Some(renderer))
    }).lock().unwrap_or_else(|e|e.into_inner());
    if let Some(renderer) = renderer.as_mut() {
        test(renderer);
    }
}
fn compare(a: &Frame, b: &Frame, tolerance: u8, label: &str) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    assert_eq!(a.rgba.len(), b.rgba.len());
    let max = a
        .rgba
        .iter()
        .zip(&b.rgba)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    let count = a
        .rgba
        .iter()
        .zip(&b.rgba)
        .filter(|(a, b)| a.abs_diff(**b) > tolerance)
        .count();
    assert!(
        max <= tolerance,
        "{label}: maximum channel difference {max}, {count} channels beyond {tolerance}"
    );
}
fn fixture(w: u32, h: u32) -> (Project, CompId) {
    let mut p = Project::new("GPU parity");
    let c = p.create_comp("Main", w, h, FrameRate::FPS_30, Time(240000));
    p.comp_mut(c).unwrap().background = [0.1, 0.03, 0.2, 0.35];
    (p, c)
}
#[test]
fn real_draw_passes_cover_all_blends_transforms_and_rgba_readback_padding() {
    gpu(|gpu| {
        let (mut p, c) = fixture(37, 23);
        let mut l = Layer::new_rect("Rectangle", [0.8, 0.4, 0.15, 0.6], Time::ZERO, Time(240000));
        if let LayerKind::Shape { style, .. } = &mut l.kind {
            style.size = Some([27.0, 16.0]);
            style.corner_radius = 3.0;
            style.stroke_width = 1.75;
            style.stroke_color = [0.1, 0.5, 0.8, 0.7];
        }
        l.transform.rotation = 17.0;
        l.transform.position = [0.3, -0.5];
        let id = p.insert_layer(c, l);
        let registry = EffectRegistry::new();
        let cache = SourceCache::default();
        let before = gpu.stats().submitted_frames;
        for mode in [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Add,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::Difference,
        ] {
            p.layer_mut(c, id).unwrap().blend_mode = mode;
            let scene = prepare_scene(
                &p,
                c,
                Time::ZERO,
                &NoMedia,
                &registry,
                Resolution::FULL,
                &cache,
            )
            .unwrap();
            compare(
                &render_scene_cpu(&scene, &registry).unwrap(),
                &gpu.render(&scene, &registry).unwrap(),
                2,
                &format!("{mode:?}"),
            );
        }
        assert_eq!(gpu.stats().submitted_frames - before, 8);
    });
}
struct Pixels(Arc<CpuFrame>);
impl MediaFrames for Pixels {
    fn frame_rgba(&self, _: MediaId, _: Time) -> Option<FrameView<'_>> {
        Some(FrameView {
            width: self.0.width,
            height: self.0.height,
            rgba: &self.0.rgba,
        })
    }
    fn shared_frame(&self, _: MediaId, _: Time) -> Option<Arc<CpuFrame>> {
        Some(self.0.clone())
    }
}
#[test]
fn opted_in_effect_shaders_match_cpu_on_nontrivial_rgba_frames() {
    gpu(|gpu| {
        let (mut p, c) = fixture(19, 13);
        p.comp_mut(c).unwrap().background = [0.0; 4];
        let media = p.insert_media(MediaAsset {
            id: MediaId(0),
            name: "Pixels".into(),
            kind: MediaKind::Image,
            path: None,
            embedded: None,
            audio: None,
            slot: None,
            alias: None,
            perception: None,
            video: None,
        });
        let id = p.insert_layer(
            c,
            Layer::new(
                "Image",
                LayerKind::Footage { media },
                Time::ZERO,
                Time(240000),
            ),
        );
        let mut pixels = CpuFrame::new(19, 13);
        for y in 0..13 {
            for x in 0..19 {
                pixels.set_pixel_u8(
                    x,
                    y,
                    [
                        (x * 13) as u8,
                        (y * 19) as u8,
                        ((x + y) * 7) as u8,
                        if (x + y) % 7 == 0 {
                            0
                        } else if x % 3 == 0 {
                            93
                        } else {
                            255
                        },
                    ],
                );
            }
        }
        let pixels = Pixels(Arc::new(pixels));
        let registry = EffectRegistry::new();
        let cache = SourceCache::default();
        for manifest in registry.list().into_iter().filter(|m| m.gpu_preview) {
            let mut effect = EffectInstance::new("fx", &manifest.id);
            if manifest.id == "builtin.color_grade" {
                for (id, value) in [
                    ("exposure", EffectValue::Float(1.25)),
                    ("temperature", EffectValue::Float(0.4)),
                    ("tint", EffectValue::Float(-0.2)),
                    ("contrast", EffectValue::Float(1.14)),
                    ("highlights", EffectValue::Float(-0.3)),
                    ("shadows", EffectValue::Float(0.2)),
                    ("saturation", EffectValue::Float(0.7)),
                    ("vibrance", EffectValue::Float(0.3)),
                    ("gamma", EffectValue::Float(1.1)),
                    ("fade", EffectValue::Float(0.1)),
                    ("lift", EffectValue::Float(-0.08)),
                    ("gain", EffectValue::Float(1.2)),
                    ("lift_color", EffectValue::Color([0.5, 0.5, 0.5, 1.0])),
                    ("gain_color", EffectValue::Color([0.75, 0.45, 0.5, 1.0])),
                    ("filmic", EffectValue::Float(0.35)),
                    ("split_shadow_hue", EffectValue::Float(205.0)),
                    ("split_highlight_hue", EffectValue::Float(45.0)),
                    ("split_balance", EffectValue::Float(0.2)),
                    ("split_strength", EffectValue::Float(0.6)),
                ] {
                    effect.params.insert(id.into(), value);
                }
            }
            if manifest.id == "builtin.color_adjust" {
                effect
                    .params
                    .insert("hue_shift".into(), EffectValue::Float(-90.0));
                effect
                    .params
                    .insert("saturation".into(), EffectValue::Float(0.7));
            }
            if manifest.id == "builtin.transform" {
                effect
                    .params
                    .insert("rotation".into(), EffectValue::Float(23.0));
                effect
                    .params
                    .insert("scale_x".into(), EffectValue::Float(0.81));
                effect
                    .params
                    .insert("offset_y".into(), EffectValue::Float(1.3));
            }
            if manifest.id == "builtin.invert" {
                effect
                    .params
                    .insert("invert_alpha".into(), EffectValue::Bool(true));
                effect
                    .params
                    .insert("amount".into(), EffectValue::Float(0.73));
            }
            p.layer_mut(c, id).unwrap().effects = vec![effect];
            let scene = prepare_scene(
                &p,
                c,
                Time(4000),
                &pixels,
                &registry,
                Resolution::FULL,
                &cache,
            )
            .unwrap();
            // Glow and drop shadow composite a float halo on the GPU while the
            // CPU kernel rounds its halo to 8 bits before compositing; their
            // fixtures carry the standard one-extra-level allowance.
            let tolerance = if manifest.id.contains("glow") || manifest.id.contains("drop_shadow") {
                3
            } else {
                2
            };
            compare(
                &render_scene_cpu(&scene, &registry).unwrap(),
                &gpu.render(&scene, &registry).unwrap(),
                tolerance,
                &manifest.id,
            );
        }
    });
}
#[test]
fn nested_compositions_text_and_adjustments_match_at_all_preview_resolutions() {
    gpu(|gpu| {
        let (mut p, c) = fixture(65, 39);
        let child = p.create_comp("Child", 33, 17, FrameRate::FPS_30, Time(240000));
        p.comp_mut(child).unwrap().background = [0.0; 4];
        let mut circle =
            Layer::new_circle("Ellipse", [0.25, 0.7, 0.1, 0.8], Time::ZERO, Time(240000));
        circle.transform.scale = [70.0, 80.0];
        p.insert_layer(child, circle);
        let mut text = Layer::new_text("Type", "Hi", 10.0, Time::ZERO, Time(240000));
        text.transform.position = [-4.0, -2.0];
        if let LayerKind::Text { style, .. } = &mut text.kind {
            style.color = [0.3, 0.1, 0.8, 0.7];
        }
        p.insert_layer(child, text);
        let mut nested = Layer::new(
            "Nested",
            LayerKind::PreComp { comp: child },
            Time::ZERO,
            Time(240000),
        );
        nested.transform.rotation = 32.0;
        nested.transform.scale = [130.0, 85.0];
        nested.transform.position = [4.0, -2.0];
        nested.transform.opacity = 0.68;
        p.insert_layer(c, nested);
        let mut adjustment =
            Layer::new("Grade", LayerKind::Adjustment {}, Time::ZERO, Time(240000));
        adjustment.transform.opacity = 0.4;
        let mut fx = EffectInstance::new("grade", "builtin.gradient");
        fx.params
            .insert("end_color".into(), EffectValue::Color([0.1, 0.3, 0.7, 0.2]));
        adjustment.effects.push(fx);
        p.insert_layer(c, adjustment);
        let registry = EffectRegistry::new();
        let cache = SourceCache::default();
        for d in [1, 2, 4] {
            let scene = prepare_scene(
                &p,
                c,
                Time(6000),
                &NoMedia,
                &registry,
                Resolution::new(d).unwrap(),
                &cache,
            )
            .unwrap();
            compare(
                &render_scene_cpu(&scene, &registry).unwrap(),
                &gpu.render(&scene, &registry).unwrap(),
                3,
                &format!("nested divisor {d}"),
            );
        }
    });
}
#[test]
fn raster_uploads_and_targets_are_reused_with_bounded_retention() {
    gpu(|gpu| {
        gpu.clear();
        let (mut p, c) = fixture(40, 30);
        p.insert_layer(
            c,
            Layer::new_text("Type", "GPU", 12.0, Time::ZERO, Time(240000)),
        );
        let registry = EffectRegistry::new();
        let cache = SourceCache::default();
        let scene = prepare_scene(
            &p,
            c,
            Time::ZERO,
            &NoMedia,
            &registry,
            Resolution::FULL,
            &cache,
        )
        .unwrap();
        let before = gpu.stats().uploads;
        gpu.render(&scene, &registry).unwrap();
        let warm = gpu.stats();
        gpu.render(&scene, &registry).unwrap();
        let after = gpu.stats();
        assert_eq!(warm.uploads, before + 1);
        assert_eq!(after.uploads, warm.uploads);
        assert!(after.upload_hits > warm.upload_hits);
        assert!(after.pooled_bytes <= after.pool_budget_bytes);
        assert!(after.upload_bytes <= after.upload_budget_bytes);
        gpu.clear();
        assert_eq!(gpu.stats().pooled_bytes, 0);
        assert_eq!(gpu.stats().upload_bytes, 0);
    });
}
#[test]
fn unsupported_kernels_are_explicit_and_never_masquerade_as_gpu_frames() {
    gpu(|gpu| {
        let (mut p, c) = fixture(8, 8);
        let mut l = Layer::new_solid("Legacy", [1.0; 4], Time::ZERO, Time(240000));
        l.effects
            .push(EffectInstance::new("legacy", "test.cpu_only"));
        p.insert_layer(c, l);
        let mut registry = EffectRegistry::new();
        let manifest = EffectManifest::parse(
            "api_version='1.0'\nid='test.cpu_only'\nname='CPU only'\ncost='light'\nshader='none.wgsl'\ngpu_preview=false\ninputs=['input']\n",
        )
        .unwrap();
        registry
            .register_cpu(manifest, String::new(), |_, input, _| Ok(input.clone()))
            .unwrap();
        let scene = prepare_scene(
            &p,
            c,
            Time::ZERO,
            &NoMedia,
            &registry,
            Resolution::FULL,
            &SourceCache::default(),
        )
        .unwrap();
        let before = gpu.stats().submitted_frames;
        let error = gpu.render(&scene, &registry).unwrap_err();
        assert!(error.contains("CPU only"), "unexpected reason: {error}");
        assert_eq!(gpu.stats().submitted_frames, before);
    });

    // Every first-party filter is now GPU-enabled: a scene stacking glow and
    // drop shadow with blur still executes natively.
    gpu(|gpu| {
        let (mut p, c) = fixture(31, 17);
        p.comp_mut(c).unwrap().background = [0.0; 4];
        let mut l = Layer::new_rect("Text-ish", [0.95, 0.8, 0.2, 1.0], Time::ZERO, Time(240000));
        if let LayerKind::Shape { style, .. } = &mut l.kind {
            style.size = Some([13.0, 7.0]);
        }
        let mut glow = EffectInstance::new("glow", "builtin.glow");
        glow.params.insert("radius".into(), EffectValue::Float(4.0));
        glow.params
            .insert("intensity".into(), EffectValue::Float(1.6));
        let mut shadow = EffectInstance::new("shadow", "builtin.drop_shadow");
        shadow
            .params
            .insert("offset_x".into(), EffectValue::Float(3.0));
        shadow
            .params
            .insert("offset_y".into(), EffectValue::Float(2.0));
        shadow
            .params
            .insert("radius".into(), EffectValue::Float(5.0));
        shadow
            .params
            .insert("color".into(), EffectValue::Color([0.0, 0.0, 0.1, 0.9]));
        shadow
            .params
            .insert("opacity".into(), EffectValue::Float(0.8));
        l.effects = vec![glow, shadow];
        p.insert_layer(c, l);
        let registry = EffectRegistry::new();
        let scene = prepare_scene(
            &p,
            c,
            Time::ZERO,
            &NoMedia,
            &registry,
            Resolution::FULL,
            &SourceCache::default(),
        )
        .unwrap();
        assert!(
            GpuRenderer::unsupported(&scene, &registry).is_none(),
            "glow + drop shadow scenes must be GPU-eligible now"
        );
        let before = gpu.stats().submitted_frames;
        compare(
            &render_scene_cpu(&scene, &registry).unwrap(),
            &gpu.render(&scene, &registry).unwrap(),
            3,
            "glow + drop shadow stack",
        );
        assert_eq!(gpu.stats().submitted_frames - before, 1);
    });
}

#[test]
fn separable_gpu_blur_matches_cpu_reference() {
    gpu(|gpu| {
        let registry = EffectRegistry::new();
        for repeat_edge in [true, false] {
            let (mut p, c) = fixture(37, 23);
            p.comp_mut(c).unwrap().background = [0.0; 4];
            let mut l = Layer::new_rect("Shape", [0.9, 0.5, 0.2, 0.8], Time::ZERO, Time(240000));
            if let LayerKind::Shape { style, .. } = &mut l.kind {
                style.size = Some([15.0, 9.0]);
            }
            l.transform.rotation = 12.0;
            l.effects.push(EffectInstance::new("blur", "builtin.blur"));
            l.effects[0]
                .params
                .insert("radius".into(), EffectValue::Float(6.5));
            l.effects[0]
                .params
                .insert("repeat_edge".into(), EffectValue::Bool(repeat_edge));
            p.insert_layer(c, l);
            let scene = prepare_scene(
                &p,
                c,
                Time::ZERO,
                &NoMedia,
                &registry,
                Resolution::FULL,
                &SourceCache::default(),
            )
            .unwrap();
            let cpu = render_scene_cpu(&scene, &registry).unwrap();
            let before = gpu.stats().submitted_frames;
            let rendered = gpu.render(&scene, &registry).unwrap();
            assert_eq!(gpu.stats().submitted_frames - before, 1);
            compare(
                &cpu,
                &rendered,
                2,
                &format!("gaussian blur repeat_edge={repeat_edge}"),
            );
        }
        // An animated radius also exercises the two-pass host path through the
        // shared parameter evaluation (scaled with the raster grid).
        let (mut p, c) = fixture(19, 13);
        p.comp_mut(c).unwrap().background = [0.0; 4];
        let mut l = Layer::new_rect("Shape", [0.2, 0.8, 0.4, 0.9], Time::ZERO, Time(240000));
        if let LayerKind::Shape { style, .. } = &mut l.kind {
            style.size = Some([9.0, 7.0]);
        }
        let mut blur = EffectInstance::new("blur", "builtin.blur");
        let mut radius_track = Track::new();
        radius_track.set_key(Keyframe {
            time: Time::ZERO,
            value: PropValue::Scalar(2.0),
            easing: Easing::Linear,
        });
        radius_track.set_key(Keyframe {
            time: Time(120000),
            value: PropValue::Scalar(9.0),
            easing: Easing::Linear,
        });
        blur.tracks.insert("radius".into(), radius_track);
        l.effects.push(blur);
        p.insert_layer(c, l);
        let scene = prepare_scene(
            &p,
            c,
            Time(60000),
            &NoMedia,
            &registry,
            Resolution::FULL,
            &SourceCache::default(),
        )
        .unwrap();
        compare(
            &render_scene_cpu(&scene, &registry).unwrap(),
            &gpu.render(&scene, &registry).unwrap(),
            2,
            "gaussian blur animated radius",
        );
    });
}
#[test]
fn third_party_shader_uses_reflected_padding_not_private_id_hooks() {
    gpu(|gpu| {
        let manifest = EffectManifest::parse(
            r#"api_version='1.0'
id='test.gpu_color'
name='Test GPU color'
cost='light'
shader='color.wgsl'
gpu_preview=true
inputs=['input']
[[params]]
id='gain'
name='Gain'
doc='Scalar before an aligned color.'
kind={Slider={min=0.0,max=1.0,default=0.6}}
[[params]]
id='color'
name='Color'
doc='Aligned vec4.'
kind={Color={default=[0.2,0.8,0.5,1.0]}}
[[params]]
id='point'
name='Point'
doc='Aligned vec2.'
kind={Point={default=[0.2,0.3]}}
[[params]]
id='enabled'
name='Enabled'
doc='Boolean encoded as u32.'
kind={Checkbox={default=true}}
"#,
        )
        .unwrap();
        let shader = r#"struct Params { gain:f32, color:vec4<f32>, point:vec2<f32>, enabled:u32 }
@group(0) @binding(4) var<uniform> params:Params;
@fragment fn fs_main(@location(0) uv:vec2<f32>)->@location(0) vec4<f32>{return vec4<f32>(params.color.r*params.gain,params.point,select(0.0,1.0,params.enabled!=0u));}"#;
        fn evaluate(
            _: &str,
            f: &CpuFrame,
            p: &HashMap<String, ParamValue>,
        ) -> Result<CpuFrame, CpuEvalError> {
            let ParamValue::Float(g) = p["gain"] else {
                panic!()
            };
            let ParamValue::Color(c) = p["color"] else {
                panic!()
            };
            let ParamValue::Point(v) = p["point"] else {
                panic!()
            };
            Ok(CpuFrame::filled(
                f.width,
                f.height,
                [
                    (c[0] * g * 255.0).round() as u8,
                    (v[0] * 255.0).round() as u8,
                    (v[1] * 255.0).round() as u8,
                    255,
                ],
            ))
        }
        let mut registry = EffectRegistry::empty();
        registry
            .register_cpu(manifest, shader.into(), evaluate)
            .unwrap();
        let (mut p, c) = fixture(9, 7);
        let mut l = Layer::new_solid("Source", [1.0; 4], Time::ZERO, Time(240000));
        l.effects
            .push(EffectInstance::new("custom", "test.gpu_color"));
        p.insert_layer(c, l);
        let scene = prepare_scene(
            &p,
            c,
            Time::ZERO,
            &NoMedia,
            &registry,
            Resolution::FULL,
            &SourceCache::default(),
        )
        .unwrap();
        compare(
            &render_scene_cpu(&scene, &registry).unwrap(),
            &gpu.render(&scene, &registry).unwrap(),
            1,
            "public shader registration",
        );
    });
}

#[test]
fn camera_projection_depth_order_and_dof_match_between_gpu_and_cpu() {
    gpu(|gpu| {
        let (mut p, c) = fixture(65, 39);
        // Two rotated cards at different depths behind a live camera with
        // depth of field — the full 3D scene description.
        p.comp_mut(c).unwrap().camera = Camera {
            position: [3.0, -2.0],
            z: 0.0,
            fov: 300.0,
            focus: 0.0,
            dof: 0.8,
        };
        let mut far = Layer::new_rect("far", [0.15, 0.8, 0.3, 0.9], Time::ZERO, Time(240000));
        if let LayerKind::Shape { style, .. } = &mut far.kind {
            style.size = Some([40.0, 26.0]);
        }
        far.transform.rotation = 12.0;
        far.transform.position = [10.0, 2.0];
        far.transform.z = 420.0; // off-focus: DoF blurs it
        let mut near = Layer::new_rect("near", [0.85, 0.3, 0.12, 0.8], Time::ZERO, Time(240000));
        if let LayerKind::Shape { style, .. } = &mut near.kind {
            style.size = Some([30.0, 20.0]);
        }
        near.transform.rotation = -21.0;
        near.transform.position = [-6.0, -1.0];
        near.transform.z = -180.0; // close to the camera: large
                                   // Composed ABOVE the far card but NEARER in depth: draw_order must
                                   // flip the paint order on both backends.
        let near_id = p.insert_layer(c, near);
        let _far_id = p.insert_layer(c, far);
        assert_eq!(p.comp(c).unwrap().layer_order.len(), 2);

        let registry = EffectRegistry::new();
        let cache = SourceCache::default();
        let scene = prepare_scene(
            &p,
            c,
            Time::ZERO,
            &NoMedia,
            &registry,
            Resolution::FULL,
            &cache,
        )
        .unwrap();
        // The scene carries the depth-sorted order (far first).
        assert_eq!(scene.layers.len(), 2);
        compare(
            &render_scene_cpu(&scene, &registry).unwrap(),
            &gpu.render(&scene, &registry).unwrap(),
            2,
            "3d camera + depth + dof",
        );
        let _ = near_id;
    });
}
