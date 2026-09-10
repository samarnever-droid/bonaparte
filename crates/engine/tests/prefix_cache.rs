//! Static-prefix compositing cache: output must be byte-identical to the
//! plain path on a mixed stack (solids, text rasters, a volatile effect,
//! an animated precomp, an animated title), under mid-sequence edits and
//! non-adjacent scrub jumps — and it must measurably pay off.
use bonaparte_engine::prefix::PrefixCache;
use bonaparte_engine::preview::{prepare_scene, render_scene_cpu, Resolution, SourceCache};
use bonaparte_engine::NoMedia;
use bonaparte_model::*;

fn scene(w: u32, h: u32, secs: i64) -> (Project, CompId) {
    let mut p = Project::new("Prefix");
    let c = p.create_comp("Main", w, h, FrameRate::FPS_30, Time(secs * 120_000));
    (p, c)
}

fn linear(time: i64, value: PropValue) -> Keyframe {
    Keyframe {
        time: Time(time),
        value,
        easing: Easing::Linear,
    }
}

/// Bottom-to-top: solid, rect, big static text, shape with invert effect
/// (volatile), animated precomp, animated title.
fn build_stack(p: &mut Project, c: CompId, texts: usize) {
    p.insert_layer(
        c,
        Layer::new_solid("bg", [0.1, 0.1, 0.2, 1.0], Time::ZERO, Time(600_000)),
    );
    p.insert_layer(
        c,
        Layer::new_rect("card", [0.9, 0.6, 0.2, 1.0], Time::ZERO, Time(600_000)),
    );
    for i in 0..texts {
        let mut l = Layer::new_text(
            format!("print-{i}"),
            &format!("static fine print {i} ").repeat(6),
            18.0 + i as f32,
            Time::ZERO,
            Time(600_000),
        );
        l.transform.position = [i as f32 * 3.0 - 10.0, i as f32 * 7.0 - 30.0];
        p.insert_layer(c, l);
    }
    let mut fx = Layer::new_rect("fx", [0.2, 0.8, 0.4, 1.0], Time::ZERO, Time(600_000));
    fx.effects
        .push(EffectInstance::new("invert", "builtin.invert"));
    p.insert_layer(c, fx);
    let child = p.create_comp("Child", 16, 12, FrameRate::FPS_30, Time(600_000));
    let mut spin = Layer::new_rect("spin", [1.0, 0.2, 0.2, 1.0], Time::ZERO, Time(600_000));
    {
        let mut t = Track::new();
        for i in 0..16 {
            t.set_key(linear(i * 40_000, PropValue::Scalar((i * 11) as f32)));
        }
        spin.tracks.insert(Property::Rotation, t);
    }
    p.insert_layer(child, spin);
    let mut pre = Layer::new_rect("precomp", [1.0, 1.0, 1.0, 1.0], Time::ZERO, Time(600_000));
    pre.kind = LayerKind::PreComp { comp: child };
    p.insert_layer(c, pre);
    let mut title = Layer::new_text("title", "HELLO", 26.0, Time::ZERO, Time(600_000));
    {
        let mut t = Track::new();
        for i in 0..16 {
            t.set_key(linear(
                i * 40_000,
                PropValue::Vec2([i as f32 * 3.0, -((i % 5) as f32) * 2.0]),
            ));
        }
        title.tracks.insert(Property::Position, t);
    }
    p.insert_layer(c, title);
}

fn frame(p: &Project, c: CompId, t: Time, src: &SourceCache) -> bonaparte_engine::preview::Scene {
    let registry = bonaparte_effects::registry::builtin_registry();
    prepare_scene(p, c, t, &NoMedia, registry, Resolution::FULL, src).expect("scene")
}

#[test]
fn byte_identical_through_mixed_frames_edits_and_scrub_jumps() {
    let (mut p, c) = scene(48, 36, 5);
    build_stack(&mut p, c, 3);
    let src = SourceCache::default();
    let registry = bonaparte_effects::registry::builtin_registry();
    let mut cache = PrefixCache::new();
    for f in 0..36 {
        if f == 20 {
            // Live edit mid-sequence: a bottom layer changes color. The cache
            // must notice through tokens and recompute — never paste stale bg.
            let ids: Vec<LayerId> = p.comps[&c].layer_order.clone();
            p.layer_mut(c, ids[0]).unwrap().kind = LayerKind::Solid {
                color: [0.4, 0.1, 0.1, 1.0],
            };
        }
        let t = Time(f * 4_000);
        let sc = frame(&p, c, t, &src);
        let plain = render_scene_cpu(&sc, registry).unwrap();
        let cached = cache.render(&sc, registry).unwrap();
        assert_eq!(cached.rgba, plain.rgba, "frame {f} must match");
    }
    // Non-adjacent jumps, including revisits.
    for f in [0i64, 35, 7, 35, 7, 21, 34, 0] {
        let t = Time(f * 4_000);
        let sc = frame(&p, c, t, &src);
        let plain = render_scene_cpu(&sc, registry).unwrap();
        let cached = cache.render(&sc, registry).unwrap();
        assert_eq!(cached.rgba, plain.rgba, "scrub to {f} must match");
    }
}

#[test]
fn two_caches_and_a_fresh_one_agree_on_every_frame() {
    let (mut p, c) = scene(48, 36, 5);
    build_stack(&mut p, c, 2);
    let src = SourceCache::default();
    let registry = bonaparte_effects::registry::builtin_registry();
    let mut a = PrefixCache::new();
    let mut b = PrefixCache::new();
    for f in 0..24 {
        let t = Time(f * 4_000);
        let sc = frame(&p, c, t, &src);
        let x = a.render(&sc, registry).unwrap();
        let y = b.render(&sc, registry).unwrap();
        assert_eq!(x.rgba, y.rgba, "two streams agree on {f}");
        // A fresh cache on the same scene equals both (first-frame path).
        let z = PrefixCache::new().render(&sc, registry).unwrap();
        assert_eq!(z.rgba, x.rgba, "fresh cache agrees on {f}");
    }
}

#[test]
fn a_plate_wall_resumes_past_every_static_layer() {
    // The heavy case motion graphics actually has: a tall stack of
    // full-frame plates (color management, glows, vignettes), one animated
    // headline on top. Every plate costs a full-frame blend; the cache
    // should turn 120 blends into one.
    let (mut p, c) = scene(320, 180, 2);
    for i in 0..120 {
        let mut l = Layer::new_solid(
            format!("plate-{i}"),
            [0.02, 0.03, 0.05, 0.985],
            Time::ZERO,
            Time(240_000),
        );
        l.transform.rotation = (i % 3) as f32 * 0.25;
        p.insert_layer(c, l);
    }
    let mut title = Layer::new_text("title", "LIVE", 30.0, Time::ZERO, Time(240_000));
    {
        let mut t = Track::new();
        for i in 0..10 {
            t.set_key(linear(
                i * 24_000,
                PropValue::Vec2([i as f32 * 2.5, -i as f32]),
            ));
        }
        title.tracks.insert(Property::Position, t);
    }
    p.insert_layer(c, title);

    let src = SourceCache::default();
    let registry = bonaparte_effects::registry::builtin_registry();
    let scenes: Vec<_> = (0..24)
        .map(|f| frame(&p, c, Time(f * 4_000), &src))
        .collect();
    let mut plain: Vec<_> = Vec::new();
    let mut plain_us: Vec<u128> = Vec::new();
    for sc in &scenes {
        let t = std::time::Instant::now();
        plain.push(render_scene_cpu(sc, registry).unwrap());
        plain_us.push(t.elapsed().as_micros());
    }
    let mut cache = PrefixCache::new();
    let mut cached: Vec<_> = Vec::new();
    let mut cached_us: Vec<u128> = Vec::new();
    for (i, sc) in scenes.iter().enumerate() {
        let t = std::time::Instant::now();
        cached.push(cache.render(sc, registry).unwrap());
        // Frame 0 pays setup; frame 1 establishes the snapshot. Steady
        // state — what scrubbing and playback actually feel — is frames 2+.
        if i >= 2 {
            cached_us.push(t.elapsed().as_micros());
        }
    }
    for (i, (a, b)) in plain.iter().zip(&cached).enumerate() {
        assert_eq!(a.rgba, b.rgba, "plate frame {i} identical");
    }
    let plain_avg = plain_us.iter().sum::<u128>() / plain_us.len() as u128;
    let cached_avg = cached_us.iter().sum::<u128>() / cached_us.len() as u128;
    println!(
        "plate wall (121 layers): plain {plain_avg} us/frame -> cached {cached_avg} us/frame steady = {}x",
        plain_avg / cached_avg.max(1)
    );
    assert!(
        cached_avg * 20 < plain_avg,
        "expected >=20x steady state on a 121-layer plate wall, got {plain_avg}us vs {cached_avg}us"
    );
}
