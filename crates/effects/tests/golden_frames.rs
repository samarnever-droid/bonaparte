use bonaparte_effects::{
    evaluate_blur, evaluate_chromatic_aberration, evaluate_color_adjust, evaluate_directional_blur,
    evaluate_drop_shadow, evaluate_effect, evaluate_glow, evaluate_invert, evaluate_tint,
    evaluate_transform, evaluate_vignette, CpuFrame, ParamValue,
};
use std::collections::HashMap;

/// Helper to generate a 16x16 test pattern:
/// Left half = Red [255, 0, 0, 255], Right half = Blue [0, 0, 255, 255].
fn make_test_frame_16x16() -> CpuFrame {
    let mut frame = CpuFrame::new(16, 16);
    for y in 0..16 {
        for x in 0..16 {
            if x < 8 {
                frame.set_pixel_u8(x, y, [255, 0, 0, 255]);
            } else {
                frame.set_pixel_u8(x, y, [0, 0, 255, 255]);
            }
        }
    }
    frame
}

/// Helper for a 4x4 checkerboard frame.
fn make_checkerboard_4x4() -> CpuFrame {
    let mut frame = CpuFrame::new(4, 4);
    for y in 0..4 {
        for x in 0..4 {
            if (x + y) % 2 == 0 {
                frame.set_pixel_u8(x, y, [255, 255, 255, 255]);
            } else {
                frame.set_pixel_u8(x, y, [0, 0, 0, 255]);
            }
        }
    }
    frame
}

// ---------------------------------------------------------------------------
// 1. Glow Test
// ---------------------------------------------------------------------------
#[test]
fn test_glow_deterministic() {
    let mut frame = CpuFrame::new(8, 8);
    // Single bright pixel in center
    frame.set_pixel_u8(4, 4, [255, 255, 255, 255]);

    let glowed = evaluate_glow(&frame, 1.0, 1.0, [1.0, 1.0, 1.0, 1.0]);

    // Center pixel must remain fully bright
    let center = glowed.get_pixel_u8(4, 4);
    assert_eq!(center[0], 255);
    assert_eq!(center[1], 255);
    assert_eq!(center[2], 255);

    // Neighboring pixel at (3, 4) should receive glow contribution
    let neighbor = glowed.get_pixel_u8(3, 4);
    assert!(neighbor[0] > 0 || neighbor[1] > 0 || neighbor[2] > 0);
}

// ---------------------------------------------------------------------------
// 2. Blur Test
// ---------------------------------------------------------------------------
#[test]
fn test_blur_identity_and_smoothing() {
    let frame = make_test_frame_16x16();

    // Radius 0 = identity
    let blur0 = evaluate_blur(&frame, 0.0, true);
    assert_eq!(blur0, frame);

    // Active blur softens sharp boundary between left (red) and right (blue)
    let blur4 = evaluate_blur(&frame, 4.0, true);
    let border_left = blur4.get_pixel_u8(7, 8);
    let border_right = blur4.get_pixel_u8(8, 8);

    // Both should now have a mix of red and blue channels
    assert!(border_left[0] > 0 && border_left[2] > 0);
    assert!(border_right[0] > 0 && border_right[2] > 0);
}

// ---------------------------------------------------------------------------
// 3. Drop Shadow Test
// ---------------------------------------------------------------------------
#[test]
fn test_drop_shadow_composite() {
    let mut frame = CpuFrame::new(8, 8);
    // Square 2x2 in top-left
    frame.set_pixel_u8(1, 1, [255, 255, 255, 255]);
    frame.set_pixel_u8(2, 1, [255, 255, 255, 255]);
    frame.set_pixel_u8(1, 2, [255, 255, 255, 255]);
    frame.set_pixel_u8(2, 2, [255, 255, 255, 255]);

    let shadowed = evaluate_drop_shadow(
        &frame,
        2.0,
        2.0,
        0.0, // Sharp offset
        [0.0, 0.0, 0.0, 1.0],
        1.0,
    );

    // Original pixels must still be white
    assert_eq!(shadowed.get_pixel_u8(1, 1), [255, 255, 255, 255]);

    // Offset pixel at (3, 3) must now be shadow black with alpha
    let shadow_px = shadowed.get_pixel_u8(3, 3);
    assert_eq!(shadow_px[3], 255);
    assert_eq!(shadow_px[0], 0);
    assert_eq!(shadow_px[1], 0);
    assert_eq!(shadow_px[2], 0);
}

// ---------------------------------------------------------------------------
// 4. Color Adjust Test
// ---------------------------------------------------------------------------
#[test]
fn test_color_adjust_identity_and_brightness() {
    let frame = make_test_frame_16x16();

    // Default params = identity
    let id_frame = evaluate_color_adjust(&frame, 0.0, 1.0, 1.0, 0.0);
    for y in 0..16 {
        for x in 0..16 {
            let orig = frame.get_pixel_u8(x, y);
            let res = id_frame.get_pixel_u8(x, y);
            assert_eq!(orig, res);
        }
    }

    // Brightness +0.2 increases values
    let bright = evaluate_color_adjust(&frame, 0.2, 1.0, 1.0, 0.0);
    let px = bright.get_pixel_u8(0, 0);
    assert_eq!(px[0], 255);
    assert!(px[1] > 0); // Black green increased to ~51
    assert!(px[2] > 0); // Black blue increased to ~51
}

// ---------------------------------------------------------------------------
// 5. Transform Test
// ---------------------------------------------------------------------------
#[test]
fn test_transform_identity_and_offset() {
    let frame = make_checkerboard_4x4();

    // Identity: 0 offset, 1 scale, 0 rotation
    let id_frame = evaluate_transform(&frame, 0.0, 0.0, 1.0, 1.0, 0.0);
    assert_eq!(id_frame, frame);

    // Large scale factor zooms in center
    let zoomed = evaluate_transform(&frame, 0.0, 0.0, 2.0, 2.0, 0.0);
    assert_eq!(zoomed.width, 4);
    assert_eq!(zoomed.height, 4);
}

// ---------------------------------------------------------------------------
// 6. Vignette Test
// ---------------------------------------------------------------------------
#[test]
fn test_vignette_center_preserved_corners_darkened() {
    let white = CpuFrame::filled(16, 16, [255, 255, 255, 255]);
    let vignetted = evaluate_vignette(
        &white,
        0.3, // Inner radius
        0.3, // Softness
        1.0, // Full intensity
        [0.0, 0.0, 0.0, 1.0],
    );

    // Center pixel (8, 8) must stay bright white
    let center = vignetted.get_pixel_u8(8, 8);
    assert_eq!(center, [255, 255, 255, 255]);

    // Outer corner (0, 0) must be significantly darkened
    let corner = vignetted.get_pixel_u8(0, 0);
    assert!(corner[0] < 100);
    assert!(corner[1] < 100);
    assert!(corner[2] < 100);
}

// ---------------------------------------------------------------------------
// 7. Chromatic Aberration Test
// ---------------------------------------------------------------------------
#[test]
fn test_chromatic_aberration_identity_and_channel_shift() {
    let frame = make_test_frame_16x16();

    // Amount 0 = identity
    let id_frame = evaluate_chromatic_aberration(&frame, 0.0, 0.0);
    assert_eq!(id_frame, frame);

    // Amount 4 with angle 0 (horizontal) shifts red right and blue left
    let shifted = evaluate_chromatic_aberration(&frame, 4.0, 0.0);
    assert_ne!(shifted, frame);
}

// ---------------------------------------------------------------------------
// 8. Invert Test
// ---------------------------------------------------------------------------
#[test]
fn test_invert_exact_roundtrip() {
    let frame = make_test_frame_16x16();

    // Invert amount 0 = identity
    let inv0 = evaluate_invert(&frame, 0.0, false);
    assert_eq!(inv0, frame);

    // Invert 1.0: red (255, 0, 0) becomes cyan (0, 255, 255)
    let inv1 = evaluate_invert(&frame, 1.0, false);
    let left_px = inv1.get_pixel_u8(0, 0);
    assert_eq!(left_px, [0, 255, 255, 255]);

    // Double invert = exact original image
    let inv2 = evaluate_invert(&inv1, 1.0, false);
    assert_eq!(inv2, frame);
}

// ---------------------------------------------------------------------------
// 9. Tint Test
// ---------------------------------------------------------------------------
#[test]
fn test_tint_duotone_remapping() {
    let mut frame = CpuFrame::new(2, 1);
    frame.set_pixel_u8(0, 0, [0, 0, 0, 255]); // Pure Black
    frame.set_pixel_u8(1, 0, [255, 255, 255, 255]); // Pure White

    // Tint: Black -> Dark Blue [10, 20, 30, 255], White -> Gold [255, 215, 0, 255]
    let tinted = evaluate_tint(
        &frame,
        [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 1.0],
        [1.0, 215.0 / 255.0, 0.0, 1.0],
        1.0,
    );

    let px_black = tinted.get_pixel_u8(0, 0);
    let px_white = tinted.get_pixel_u8(1, 0);

    assert_eq!(px_black[0], 10);
    assert_eq!(px_black[1], 20);
    assert_eq!(px_black[2], 30);

    assert_eq!(px_white[0], 255);
    assert_eq!(px_white[1], 215);
    assert_eq!(px_white[2], 0);
}

// ---------------------------------------------------------------------------
// 10. Directional Blur Test
// ---------------------------------------------------------------------------
#[test]
fn test_directional_blur_identity_and_streak() {
    let mut frame = CpuFrame::new(16, 16);
    frame.set_pixel_u8(8, 8, [255, 255, 255, 255]);

    // Length 0 = identity
    let dblur0 = evaluate_directional_blur(&frame, 0.0, 0.0);
    assert_eq!(dblur0, frame);

    // Horizontal blur (angle 0) spreads across X axis
    let dblur_h = evaluate_directional_blur(&frame, 6.0, 0.0);
    let px_left = dblur_h.get_pixel_u8(6, 8);
    let px_right = dblur_h.get_pixel_u8(10, 8);
    let px_up = dblur_h.get_pixel_u8(8, 6);

    assert!(px_left[0] > 0);
    assert!(px_right[0] > 0);
    assert_eq!(px_up[0], 0); // No spread in vertical Y
}

// ---------------------------------------------------------------------------
// Dynamic Dispatcher Test
// ---------------------------------------------------------------------------
#[test]
fn test_dynamic_evaluate_effect_all_10() {
    let frame = make_test_frame_16x16();
    let effect_ids = [
        "builtin.glow",
        "builtin.blur",
        "builtin.drop_shadow",
        "builtin.color_adjust",
        "builtin.transform",
        "builtin.vignette",
        "builtin.chromatic_aberration",
        "builtin.invert",
        "builtin.tint",
        "builtin.directional_blur",
    ];

    let empty_params = HashMap::new();
    for id in effect_ids {
        let res = evaluate_effect(id, &frame, &empty_params);
        assert!(
            res.is_ok(),
            "Effect {id} failed to evaluate via dynamic dispatcher"
        );
        let out = res.unwrap();
        assert_eq!(out.width, frame.width);
        assert_eq!(out.height, frame.height);
    }

    let mut custom_params = HashMap::new();
    custom_params.insert("amount".to_string(), ParamValue::Float(0.5));
    let res = evaluate_effect("builtin.invert", &frame, &custom_params).unwrap();
    assert_ne!(res, frame);
}
