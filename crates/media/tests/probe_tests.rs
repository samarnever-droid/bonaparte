use bonaparte_media::probe::{compute_perception_card, parse_rational_framerate, probe_asset};
use bonaparte_model::{AssetRole, FrameRate, MediaId, MediaKind};
use std::process::Command;

#[test]
fn test_parse_rational_framerate() {
    assert_eq!(
        parse_rational_framerate("30/1"),
        Some(FrameRate { num: 30, den: 1 })
    );
    assert_eq!(
        parse_rational_framerate("24000/1001"),
        Some(FrameRate {
            num: 24000,
            den: 1001
        })
    );
    assert_eq!(
        parse_rational_framerate("25/1"),
        Some(FrameRate { num: 25, den: 1 })
    );
    assert_eq!(
        parse_rational_framerate("60/1"),
        Some(FrameRate { num: 60, den: 1 })
    );
    assert_eq!(parse_rational_framerate("24"), Some(FrameRate::FPS_24));
    assert_eq!(
        parse_rational_framerate("23.976"),
        Some(FrameRate::NTSC_FILM)
    );
}

#[test]
fn test_compute_perception_card_solid_red() {
    let width = 16;
    let height = 16;
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..(16 * 16) {
        rgba.extend_from_slice(&[255, 0, 0, 255]);
    }

    let card = compute_perception_card(width, height, &rgba, false, false);
    assert_eq!(card.width, 16);
    assert_eq!(card.height, 16);
    assert!(!card.palette.is_empty());
    assert_eq!(card.palette[0], [255, 0, 0]);
    // Red luminance = 0.299 * 255 / 255 = 0.299
    assert!((card.mean_luma - 0.299).abs() < 0.05);
    // Flat single color = low entropy
    assert!(card.entropy < 0.1);
    assert_eq!(card.alpha_fraction, None);
    assert_eq!(card.role, AssetRole::Graphic);
}

#[test]
fn test_compute_perception_card_logo_candidate() {
    let width = 32;
    let height = 32;
    let mut rgba = Vec::with_capacity(32 * 32 * 4);
    for y in 0..32 {
        for x in 0..32 {
            if (8..24).contains(&x) && (8..24).contains(&y) {
                rgba.extend_from_slice(&[0, 255, 128, 255]);
            } else {
                // Transparent border
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }

    let card = compute_perception_card(width, height, &rgba, false, false);
    assert!(card.alpha_fraction.is_some());
    let alpha_frac = card.alpha_fraction.unwrap();
    assert!(alpha_frac > 0.5);
    assert_eq!(card.role, AssetRole::LogoCandidate);
}

#[test]
fn test_compute_perception_card_video() {
    let width = 64;
    let height = 64;
    let mut rgba = Vec::with_capacity(64 * 64 * 4);
    for _ in 0..(64 * 64) {
        rgba.extend_from_slice(&[100, 150, 200, 255]);
    }

    let card = compute_perception_card(width, height, &rgba, true, false);
    assert_eq!(card.role, AssetRole::Video);
}

#[test]
fn test_probe_real_video_asset() {
    let temp_dir = std::env::temp_dir().join(format!("probe_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let video_path = temp_dir.join("test_probe.mp4");

    // Synthesize a tiny test video with ffmpeg
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("color=c=cyan:s=64x64:d=0.5")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg(&video_path)
        .status();

    if let Ok(st) = status {
        if st.success() {
            let asset =
                probe_asset(&video_path, MediaId(42), "Test Probe").expect("probing must succeed");
            assert_eq!(asset.id, MediaId(42));
            assert_eq!(asset.name, "Test Probe");
            assert!(matches!(asset.kind, MediaKind::Video { .. }));
            assert!(asset.perception.is_some());
            let card = asset.perception.unwrap();
            assert_eq!(card.width, 64);
            assert_eq!(card.height, 64);
            assert_eq!(card.role, AssetRole::Video);
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
