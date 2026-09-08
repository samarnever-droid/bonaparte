//! NO-MERCY engine torture. Degenerate transforms, randomized scene parity
//! against the independent legacy renderer, nesting/cycle limits, thousand-
//! layer comps, adversarial typography, extreme effect parameters, tile
//! cropping across the whole grid, and determinism invariants. Deterministic:
//! fixed seeds, no wall-clock, no network.

use bonaparte_effects::EffectRegistry;
use bonaparte_engine::reference::{
    render_comp, render_comp_legacy, Frame, MediaFrames, NoMedia, RenderError, MAX_PRECOMP_DEPTH,
};
use bonaparte_engine::tiles::{render_tile, Tile};
use bonaparte_model::*;
use std::sync::Arc;

struct Rng(u32);
impl Rng {
    fn new(seed: u32) -> Self {
        Self(seed | 1)
    }
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6D2B79F5);
        let mut t = self.0;
        t ^= t >> 15;
        t = t.wrapping_mul(0x2545F491);
        t ^= t >> 13;
        t.wrapping_mul(0x27D4EB2D)
    }
    fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next_u32() as usize) % n.max(1)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.range(items.len())]
    }
}

fn project_with_comp(w: u32, h: u32) -> (Project, CompId) {
    let mut p = Project::new("no mercy");
    let c = p.create_comp("Main", w, h, FrameRate::FPS_30, Time(10 * TICKS_PER_SEC));
    (p, c)
}

fn add(p: &mut Project, c: CompId, layer: Layer) -> LayerId {
    p.insert_layer(c, layer)
}

fn solid(name: &str, color: [f32; 4]) -> Layer {
    let mut l = Layer::new(
        name,
        LayerKind::Solid { color },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    l.transform = StaticTransform::default();
    l
}

fn bytes(frame: &Frame) -> &[u8] {
    &frame.rgba
}

#[test]
fn degenerate_transforms_never_panic_and_stay_deterministic() {
    let cases: Vec<(&str, Box<dyn Fn(&mut StaticTransform)>)> = vec![
        (
            "zero scale",
            Box::new(|t: &mut StaticTransform| t.scale = [0.0, 0.0]),
        ),
        (
            "one-sided zero scale",
            Box::new(|t: &mut StaticTransform| t.scale = [1.0, 0.0]),
        ),
        (
            "negative scale",
            Box::new(|t: &mut StaticTransform| t.scale = [-2.0, -0.5]),
        ),
        (
            "huge scale",
            Box::new(|t: &mut StaticTransform| t.scale = [1e5, 1e5]),
        ),
        (
            "tiny scale",
            Box::new(|t: &mut StaticTransform| t.scale = [1e-6, 1e-6]),
        ),
        (
            "giant rotation",
            Box::new(|t: &mut StaticTransform| t.rotation = 1e5),
        ),
        (
            "negative rotation",
            Box::new(|t: &mut StaticTransform| t.rotation = -1e5),
        ),
        (
            "fractional rotation",
            Box::new(|t: &mut StaticTransform| t.rotation = 360.25),
        ),
        (
            "far position",
            Box::new(|t: &mut StaticTransform| t.position = [1e5, -1e5]),
        ),
        (
            "far anchor",
            Box::new(|t: &mut StaticTransform| t.anchor_point = [1e4, -1e4]),
        ),
    ];
    for (name, mutate) in &cases {
        let (mut p, c) = project_with_comp(96, 64);
        let mut l = solid("victim", [0.9, 0.3, 0.1, 1.0]);
        mutate(&mut l.transform);
        add(&mut p, c, l);
        let first = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
        let second = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
        assert_eq!(
            bytes(&first),
            bytes(&second),
            "{name}: two identical renders must be byte-identical"
        );
        p.validate().unwrap();
    }

    // Rotation beyond the finite-magnitude ceiling is refused by document
    // validation with a typed error (the render path itself stays total and
    // deterministic even for absurd-but-finite values).
    let (mut p, c) = project_with_comp(32, 32);
    let mut l = solid("insane", [1.0; 4]);
    l.transform.rotation = 1e9;
    add(&mut p, c, l);
    let r1 = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    let r2 = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(
        bytes(&r1),
        bytes(&r2),
        "absurd rotation stays deterministic"
    );
    assert!(p.validate().is_err(), "rotation 1e9 must fail validation");
}

#[test]
fn zero_scale_layer_renders_background_only() {
    let (mut p, c) = project_with_comp(48, 32);
    p.comp_mut(c).unwrap().background = [0.25, 0.5, 0.75, 1.0];
    let mut l = solid("collapsed", [1.0, 0.0, 0.0, 1.0]);
    l.transform.scale = [0.0, 0.0];
    add(&mut p, c, l);
    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    for px in frame.rgba.chunks_exact(4) {
        assert_eq!(
            px,
            &Frame::filled(1, 1, [0.25, 0.5, 0.75, 1.0]).rgba[..],
            "collapsed layer must not paint"
        );
    }
}

/// The independent legacy renderer is the golden contract: the registered
/// (parallel) renderer must agree byte-for-byte on randomized scenes.
#[test]
fn randomized_scenes_match_the_legacy_renderer_byte_for_byte() {
    for seed in [7u32, 0xBEEF, 0x5EED, 99_999] {
        let mut rng = Rng::new(seed);
        let (mut p, c) = project_with_comp(160, 90);
        let blend_modes = [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::Add,
            BlendMode::Darken,
            BlendMode::Lighten,
            BlendMode::Difference,
        ];
        for i in 0..12 {
            let mut layer = match rng.range(4) {
                0 => {
                    let mut l = solid("s", [rng.f32(), rng.f32(), rng.f32(), rng.f32()]);
                    l
                }
                1 => {
                    let mut l = Layer::new_rect(
                        "r",
                        [rng.f32(), rng.f32(), rng.f32(), 0.3 + 0.7 * rng.f32()],
                        Time::ZERO,
                        Time(10 * TICKS_PER_SEC),
                    );
                    if let LayerKind::Shape { style, .. } = &mut l.kind {
                        style.size = Some([20.0 + rng.f32() * 120.0, 20.0 + rng.f32() * 60.0]);
                        style.corner_radius = rng.f32() * 20.0;
                        style.stroke_width = if rng.range(2) == 0 {
                            0.0
                        } else {
                            rng.f32() * 6.0
                        };
                        style.stroke_color = [rng.f32(), rng.f32(), rng.f32(), rng.f32()];
                    }
                    l
                }
                2 => {
                    let mut l = Layer::new(
                        "t",
                        LayerKind::Text {
                            text: ["NO MERCY", "line\nbreak", "géométrie ✓"].to_vec()[rng.range(3)]
                                .into(),
                            size: 8.0 + rng.f32() * 24.0,
                            style: TextStyle::default(),
                        },
                        Time::ZERO,
                        Time(10 * TICKS_PER_SEC),
                    );
                    l
                }
                _ => {
                    let mut l = solid("a", [rng.f32(), rng.f32(), rng.f32(), rng.f32()]);
                    l
                }
            };
            layer.transform.position = [rng.f32() * 160.0, rng.f32() * 90.0];
            layer.transform.scale = [0.25 + rng.f32() * 2.0, 0.25 + rng.f32() * 2.0];
            layer.transform.rotation = rng.f32() * 720.0 - 360.0;
            layer.transform.opacity = rng.f32();
            layer.blend_mode = *rng.pick(&blend_modes);
            if rng.range(3) == 0 {
                layer
                    .effects
                    .push(EffectInstance::new("fx", "builtin.vignette"));
            }
            if rng.range(4) == 0 {
                layer
                    .effects
                    .push(EffectInstance::new("fx2", "builtin.invert"));
            }
            add(&mut p, c, layer);
        }
        p.validate().unwrap();
        for t in [Time::ZERO, Time(150_000), Time(299_999)] {
            let fast = render_comp(&p, c, t, &NoMedia).unwrap_or_else(|e| {
                panic!("seed {seed:#x} t={t:?}: registered render failed: {e}")
            });
            let golden = render_comp_legacy(&p, c, t, &NoMedia)
                .unwrap_or_else(|e| panic!("seed {seed:#x} t={t:?}: legacy render failed: {e}"));
            assert_eq!(
                bytes(&fast),
                bytes(&golden),
                "seed {seed:#x} t={t:?}: parallel renderer diverged from the golden contract"
            );
        }
    }
}

#[test]
fn precomp_depth_limit_and_cycle_detection() {
    let (mut p, root) = project_with_comp(64, 64);
    let mut ids = vec![root];
    for i in 0..(MAX_PRECOMP_DEPTH + 3) {
        let child = p.create_comp(
            &format!("N{i}"),
            32,
            32,
            FrameRate::FPS_30,
            Time(TICKS_PER_SEC),
        );
        let parent = *ids.last().unwrap();
        add(
            &mut p,
            parent,
            Layer::new(
                "nest",
                LayerKind::PreComp { comp: child },
                Time::ZERO,
                Time(TICKS_PER_SEC),
            ),
        );
        ids.push(child);
    }
    // Root -> N0 -> ... 33 nested precomps deep must be refused cleanly.
    let err = render_comp(&p, root, Time::ZERO, &NoMedia).unwrap_err();
    assert!(matches!(err, RenderError::PreCompDepthLimit(_)));

    // Cycle: make N1 reference root.
    let n1 = ids[1];
    add(
        &mut p,
        n1,
        Layer::new(
            "back",
            LayerKind::PreComp { comp: root },
            Time::ZERO,
            Time(TICKS_PER_SEC),
        ),
    );
    // Rebuild a shallow chain so the cycle is the failure, not the depth.
    let (mut p2, root2) = project_with_comp(64, 64);
    let a = p2.create_comp("A", 32, 32, FrameRate::FPS_30, Time(TICKS_PER_SEC));
    let b = p2.create_comp("B", 32, 32, FrameRate::FPS_30, Time(TICKS_PER_SEC));
    add(
        &mut p2,
        root2,
        Layer::new(
            "toA",
            LayerKind::PreComp { comp: a },
            Time::ZERO,
            Time(TICKS_PER_SEC),
        ),
    );
    add(
        &mut p2,
        a,
        Layer::new(
            "toB",
            LayerKind::PreComp { comp: b },
            Time::ZERO,
            Time(TICKS_PER_SEC),
        ),
    );
    add(
        &mut p2,
        b,
        Layer::new(
            "back",
            LayerKind::PreComp { comp: root2 },
            Time::ZERO,
            Time(TICKS_PER_SEC),
        ),
    );
    assert!(matches!(
        render_comp(&p2, root2, Time::ZERO, &NoMedia),
        Err(RenderError::PreCompCycle(_))
    ));
}

#[test]
fn thousand_layer_comp_renders_and_far_offscreen_layers_cull() {
    let (mut p, c) = project_with_comp(320, 180);
    for i in 0..1000 {
        let mut l = solid(&format!("L{i}"), [0.5, 0.5, 0.5, 0.02]);
        l.transform.position = [160.0, 90.0];
        l.transform.scale = [0.5, 0.5];
        add(&mut p, c, l);
    }
    // One distinctive layer on top must still win where it covers.
    let marker = Layer::new_rect(
        "marker",
        [1.0, 0.0, 0.0, 1.0],
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    add(&mut p, c, marker);
    p.validate().unwrap();
    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(
        frame.rgba[((90 * 320 + 160) * 4) as usize],
        255,
        "marker red on top"
    );
    assert_eq!(frame.rgba[((90 * 320 + 160) * 4 + 1) as usize], 0);

    // 500 layers entirely outside the comp: background must be byte-exact.
    let (mut p2, c2) = project_with_comp(64, 64);
    p2.comp_mut(c2).unwrap().background = [0.1, 0.2, 0.3, 1.0];
    for i in 0..500 {
        let mut l = solid("off", [1.0, 1.0, 1.0, 1.0]);
        l.transform.position = [-1e4 + i as f32, 1e4];
        add(&mut p2, c2, l);
    }
    let background = Frame::filled(64, 64, [0.1, 0.2, 0.3, 1.0]);
    let frame = render_comp(&p2, c2, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(bytes(&frame), bytes(&background));
}

#[test]
fn adversarial_typography_never_panics() {
    let cases: Vec<(&str, &str, f32)> = vec![
        ("empty", "", 24.0),
        ("whitespace", "   ", 24.0),
        ("only newlines", "\n\n\n", 24.0),
        ("single char", "X", 1.0),
        (
            "emoji+cjk+rtl",
            "(goat emoji avoided) 中文 \u{202E}rtl\u{202C} émoji 🎬",
            24.0,
        ),
        ("huge size", "WIDE", 8000.0),
        ("micro size", "tiny", 0.5),
        ("newline soup", "a\nb\nc\nd\ne\nf\ng\nh", 32.0),
    ];
    for (name, text, size) in &cases {
        let (mut p, c) = project_with_comp(256, 256);
        let l = Layer::new(
            "t",
            LayerKind::Text {
                text: (*text).into(),
                size: *size,
                style: TextStyle::default(),
            },
            Time::ZERO,
            Time(10 * TICKS_PER_SEC),
        );
        add(&mut p, c, l);
        let first = render_comp(&p, c, Time::ZERO, &NoMedia);
        // Either it rendered deterministically or it hit a typed guard; never a panic.
        match first {
            Ok(f) => {
                let second = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
                assert_eq!(bytes(&f), bytes(&second), "{name} must be deterministic");
            }
            Err(RenderError::Invalid(msg)) => {
                assert!(
                    msg.contains("16 megapixels") || msg.contains("layout"),
                    "{name}: unexpected rejection: {msg}"
                );
            }
            Err(e) => panic!("{name}: unexpected error: {e}"),
        }
    }

    // A long-but-legal line renders; determinism holds.
    let (mut p, c) = project_with_comp(512, 128);
    let long = "NO MERCY ".repeat(400);
    let l = Layer::new(
        "t",
        LayerKind::Text {
            text: long,
            size: 24.0,
            style: TextStyle::default(),
        },
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    add(&mut p, c, l);
    let f1 = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    let f2 = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(bytes(&f1), bytes(&f2));
}

#[test]
fn effect_parameter_extremes_stay_deterministic() {
    let configs: Vec<(&str, &str, Vec<(&str, EffectValue)>)> = vec![
        (
            "blur zero",
            "builtin.blur",
            vec![("radius", EffectValue::Float(0.0))],
        ),
        (
            "blur max",
            "builtin.blur",
            vec![("radius", EffectValue::Float(100.0))],
        ),
        (
            "blur repeat off",
            "builtin.blur",
            vec![
                ("radius", EffectValue::Float(33.0)),
                ("repeat_edge", EffectValue::Bool(false)),
            ],
        ),
        (
            "glow intense",
            "builtin.glow",
            vec![
                ("radius", EffectValue::Float(50.0)),
                ("intensity", EffectValue::Float(4.0)),
            ],
        ),
        (
            "gradient max angle",
            "builtin.gradient",
            vec![
                ("angle", EffectValue::Float(180.0)),
                ("start_color", EffectValue::Color([0.0, 0.0, 0.0, 0.0])),
                ("end_color", EffectValue::Color([1.0, 1.0, 1.0, 1.0])),
            ],
        ),
        (
            "shadow far",
            "builtin.drop_shadow",
            vec![
                ("offset_x", EffectValue::Float(200.0)),
                ("offset_y", EffectValue::Float(-200.0)),
                ("radius", EffectValue::Float(4.0)),
                ("color", EffectValue::Color([0.0, 0.0, 0.0, 1.0])),
                ("opacity", EffectValue::Float(1.0)),
            ],
        ),
    ];
    let registry = EffectRegistry::new();
    for (name, id, params) in &configs {
        let (mut p, c) = project_with_comp(96, 64);
        let mut l = Layer::new_rect(
            "r",
            [0.9, 0.4, 0.2, 0.9],
            Time::ZERO,
            Time(10 * TICKS_PER_SEC),
        );
        if let LayerKind::Shape { style, .. } = &mut l.kind {
            style.size = Some([48.0, 32.0]);
        }
        let mut fx = EffectInstance::new("fx", (*id).to_string());
        if *id == "builtin.blur" {
            fx.params
                .entry("repeat_edge".to_string())
                .or_insert(EffectValue::Bool(true));
        }
        for (k, v) in params {
            fx.params.insert((*k).into(), v.clone());
        }
        l.effects.push(fx);
        add(&mut p, c, l);
        let a = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap_or_else(|e| panic!("{name}: {e}"));
        let b = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(bytes(&a), bytes(&b), "{name} must be deterministic");
        _ = &registry;
    }

    // Out-of-manifest-range parameter values must be refused, not clamped.
    {
        let (mut p, c) = project_with_comp(32, 32);
        let mut l = Layer::new_rect("r", [0.5; 4], Time::ZERO, Time(TICKS_PER_SEC));
        let mut fx = EffectInstance::new("fx", "builtin.gradient");
        fx.params.insert("angle".into(), EffectValue::Float(181.0));
        l.effects.push(fx);
        add(&mut p, c, l);
        assert!(render_comp(&p, c, Time::ZERO, &NoMedia).is_err());
    }
}

#[test]
fn tiles_across_the_whole_grid_match_the_full_frame() {
    let (mut p, c) = project_with_comp(300, 300);
    p.comp_mut(c).unwrap().background = [0.1, 0.2, 0.3, 1.0];
    let mut l = Layer::new_rect("r", [0.9, 0.3, 0.1, 0.8], Time::ZERO, Time(TICKS_PER_SEC));
    if let LayerKind::Shape { style, .. } = &mut l.kind {
        style.size = Some([220.0, 140.0]);
    }
    l.transform.rotation = 33.0;
    add(&mut p, c, l);
    let full = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    for ty in 0..2 {
        for tx in 0..2 {
            let tile = render_tile(&p, c, Tile { x: tx, y: ty }, Time::ZERO, &NoMedia).unwrap();
            for y in 0..tile.height {
                for x in 0..tile.width {
                    let canvas_x = tx * 256 + x;
                    let canvas_y = ty * 256 + y;
                    if canvas_x >= 300 || canvas_y >= 300 {
                        continue;
                    }
                    assert_eq!(
                        tile.pixel(x, y),
                        full.pixel(canvas_x, canvas_y),
                        "tile ({tx},{ty}) pixel ({x},{y}) diverged"
                    );
                }
            }
        }
    }
    // Out-of-range tile returns an empty tile, not a panic.
    let ghost = render_tile(&p, c, Tile { x: 99, y: 99 }, Time::ZERO, &NoMedia).unwrap();
    assert_eq!((ghost.width, ghost.height), (0, 0));
}

#[test]
fn adjustment_chains_match_the_legacy_renderer() {
    let (mut p, c) = project_with_comp(160, 90);
    add(&mut p, c, solid("base", [0.2, 0.4, 0.6, 1.0]));
    add(
        &mut p,
        c,
        Layer::new_rect(
            "shape",
            [0.9, 0.2, 0.2, 1.0],
            Time::ZERO,
            Time(10 * TICKS_PER_SEC),
        ),
    );
    for opacity in [0.25f32, 0.5, 0.75, 1.0] {
        let mut adj = Layer::new(
            "adj",
            LayerKind::Adjustment {},
            Time::ZERO,
            Time(10 * TICKS_PER_SEC),
        );
        adj.transform.opacity = opacity;
        let mut fx = EffectInstance::new("g", "builtin.color_grade");
        fx.params.insert("contrast".into(), EffectValue::Float(1.4));
        fx.params
            .insert("saturation".into(), EffectValue::Float(0.5));
        adj.effects.push(fx);
        add(&mut p, c, adj);
    }
    p.validate().unwrap();
    let fast = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    let golden = render_comp_legacy(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(bytes(&fast), bytes(&golden));
}

#[test]
fn media_injection_edge_cases() {
    struct Absent;
    impl MediaFrames for Absent {
        fn frame_rgba(&self, _: MediaId, _: Time) -> Option<bonaparte_engine::FrameView<'_>> {
            None
        }
    }
    struct OnePixel(Arc<[u8; 4]>);
    impl MediaFrames for OnePixel {
        fn frame_rgba(&self, _: MediaId, _: Time) -> Option<bonaparte_engine::FrameView<'_>> {
            None // borrowed path intentionally absent; shared_frame drives it
        }
        fn shared_frame(&self, _: MediaId, _: Time) -> Option<Arc<bonaparte_effects::CpuFrame>> {
            Some(Arc::new(bonaparte_effects::CpuFrame::from_rgba(
                1,
                1,
                self.0.to_vec(),
            )))
        }
    }

    let (mut p, c) = project_with_comp(32, 32);
    let media = p.insert_media(MediaAsset {
        id: MediaId(0),
        name: "gone".into(),
        path: None,
        kind: MediaKind::Image,
        embedded: None,
        audio: None,
        slot: None,
        alias: None,
        perception: None,
        video: None,
    });
    add(
        &mut p,
        c,
        Layer::new(
            "f",
            LayerKind::Footage { media },
            Time::ZERO,
            Time(TICKS_PER_SEC),
        ),
    );
    assert!(matches!(
        render_comp(&p, c, Time::ZERO, &Absent).unwrap_err(),
        RenderError::MediaUnavailable(_)
    ));

    // A shared-frame-only provider renders through the shared path.
    let red: Arc<[u8; 4]> = Arc::new([255, 0, 0, 255]);
    struct Shared(Arc<[u8; 4]>);
    impl MediaFrames for Shared {
        fn frame_rgba(&self, _: MediaId, _: Time) -> Option<bonaparte_engine::FrameView<'_>> {
            None
        }
        fn shared_frame(&self, _: MediaId, _: Time) -> Option<Arc<bonaparte_effects::CpuFrame>> {
            Some(Arc::new(bonaparte_effects::CpuFrame::from_rgba(
                1,
                1,
                self.0.to_vec(),
            )))
        }
    }
    let (mut p2, c2) = project_with_comp(32, 32);
    p2.comp_mut(c2).unwrap().background = [0.0; 4];
    let m2 = p2.insert_media(MediaAsset {
        id: MediaId(0),
        name: "px".into(),
        path: None,
        kind: MediaKind::Image,
        embedded: None,
        audio: None,
        slot: None,
        alias: None,
        perception: None,
        video: None,
    });
    add(
        &mut p2,
        c2,
        Layer::new(
            "f",
            LayerKind::Footage { media: m2 },
            Time::ZERO,
            Time(TICKS_PER_SEC),
        ),
    );
    let frame = render_comp(&p2, c2, Time::ZERO, &Shared(red.clone())).unwrap();
    // The 1x1 source covers exactly the pixel whose +0.5 center sample has
    // u,v < 1: canvas (15,15) for a comp-centered 1px layer.
    assert_eq!(frame.rgba[((15 * 32 + 15) * 4) as usize], 255);
    assert_eq!(frame.rgba[((16 * 32 + 16) * 4) as usize + 3], 0);
    drop(OnePixel(red));
}

#[test]
fn maximum_valid_comp_renders_within_reason() {
    // 16.7MP validation ceiling allows exactly 4096x4096; render the corners
    // and require the fast path to agree with legacy on a sparse scene.
    let (mut p, c) = project_with_comp(4096, 4096);
    p.comp_mut(c).unwrap().background = [0.0; 4];
    let mut l = Layer::new_rect(
        "center",
        [1.0, 1.0, 1.0, 1.0],
        Time::ZERO,
        Time(10 * TICKS_PER_SEC),
    );
    if let LayerKind::Shape { style, .. } = &mut l.kind {
        style.size = Some([64.0, 64.0]);
    }
    // Position is a pixel offset from the comp CENTER: [0,0] dead-centers it.
    add(&mut p, c, l);
    let fast = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    let golden = render_comp_legacy(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(bytes(&fast), bytes(&golden));
    assert_eq!(fast.pixel(2048, 2048)[0], 1.0);
    assert_eq!(fast.pixel(0, 0)[3], 0.0);
}
