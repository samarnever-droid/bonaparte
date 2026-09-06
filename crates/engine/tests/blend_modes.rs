use bonaparte_engine::{blend_channel, blend_pixel_colors, render_comp, NoMedia};
use bonaparte_model::{BlendMode, FrameRate, Layer, LayerKind, Project, StaticTransform, Time};

#[test]
fn test_all_blend_channels_analytical() {
    let s = 0.6f32;
    let d = 0.4f32;

    // Normal: returns source
    assert!((blend_channel(BlendMode::Normal, s, d) - 0.6).abs() < 1e-5);

    // Multiply: s * d = 0.24
    assert!((blend_channel(BlendMode::Multiply, s, d) - 0.24).abs() < 1e-5);

    // Screen: 1 - (1 - 0.6)*(1 - 0.4) = 1 - 0.24 = 0.76
    assert!((blend_channel(BlendMode::Screen, s, d) - 0.76).abs() < 1e-5);

    // Overlay: d <= 0.5 -> 2 * s * d = 2 * 0.6 * 0.4 = 0.48
    assert!((blend_channel(BlendMode::Overlay, s, d) - 0.48).abs() < 1e-5);

    // Add: min(1.0, 0.6 + 0.4) = 1.0
    assert!((blend_channel(BlendMode::Add, s, d) - 1.0).abs() < 1e-5);

    // Darken: min(0.6, 0.4) = 0.4
    assert!((blend_channel(BlendMode::Darken, s, d) - 0.4).abs() < 1e-5);

    // Lighten: max(0.6, 0.4) = 0.6
    assert!((blend_channel(BlendMode::Lighten, s, d) - 0.6).abs() < 1e-5);

    // Difference: |0.6 - 0.4| = 0.2
    assert!((blend_channel(BlendMode::Difference, s, d) - 0.2).abs() < 1e-5);
}

#[test]
fn test_alpha_compositing_math() {
    let src = [1.0, 0.0, 0.0, 0.5]; // 50% red
    let dst = [0.0, 0.0, 1.0, 1.0]; // 100% blue

    let out = blend_pixel_colors(BlendMode::Normal, src, dst);
    // Over blend: 0.5 * red + 0.5 * blue
    assert!((out[0] - 0.5).abs() < 1e-4);
    assert_eq!(out[1], 0.0);
    assert!((out[2] - 0.5).abs() < 1e-4);
    assert!((out[3] - 1.0).abs() < 1e-4);
}

#[test]
fn test_render_comp_with_all_blend_modes() {
    let modes = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Add,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Difference,
    ];

    for mode in modes {
        let mut p = Project::new(format!("Blend {:?}", mode));
        let c = p.create_comp("c", 10, 10, FrameRate::FPS_30, Time(100));
        p.comps.get_mut(&c).unwrap().background = [0.2, 0.4, 0.6, 1.0];

        let mut layer = Layer::new(
            "layer",
            LayerKind::Solid {
                color: [0.8, 0.5, 0.2, 1.0],
            },
            Time::ZERO,
            Time(100),
        );
        layer.blend_mode = mode;
        layer.transform = StaticTransform::default();
        p.insert_layer(c, layer);

        let f = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
        let px = f.pixel(5, 5);

        // All blend results must be bounded in [0.0, 1.0] and non-NaN
        for (ch, &val) in px.iter().enumerate() {
            assert!(!val.is_nan(), "Channel {ch} is NaN for mode {:?}", mode);
            assert!((0.0..=1.0).contains(&val), "Channel {ch} out of bounds for mode {:?}", mode);
        }
    }
}
