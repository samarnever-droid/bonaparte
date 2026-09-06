use bonaparte_media::export::{export_mp4_stream, ExportConfig};
use bonaparte_media::probe::probe_asset;
use bonaparte_model::{FrameRate, MediaId, MediaKind};

#[test]
fn test_streaming_export_mp4_video() {
    let temp_dir = std::env::temp_dir().join(format!("export_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let out_path = temp_dir.join("output_stream.mp4");

    let width = 64;
    let height = 64;
    let fps = FrameRate::FPS_30;
    let total_frames = 15; // 0.5s duration

    let config = ExportConfig::new(&out_path, width, height, fps, total_frames)
        .with_crf(20)
        .with_preset("ultrafast");

    let stats = export_mp4_stream(&config, |frame_idx, _time| {
        // Generate a single frame: alternating color gradient
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        let color_step = ((frame_idx * 16) % 256) as u8;
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&[color_step, 200, 255 - color_step, 255]);
        }
        Ok(rgba)
    })
    .expect("streaming export must succeed");

    assert_eq!(stats.frames_exported, 15);
    assert_eq!(stats.bytes_streamed, (64 * 64 * 4) * 15);
    assert!(stats.output_file_size > 0);
    assert!(out_path.is_file());

    // Verify exported MP4 with probe
    let probed = probe_asset(&out_path, MediaId(999), "Exported MP4")
        .expect("exported file must probe cleanly");

    assert!(matches!(probed.kind, MediaKind::Video { .. }));
    if let Some(card) = probed.perception {
        assert_eq!(card.width, 64);
        assert_eq!(card.height, 64);
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_export_invalid_frame_buffer_length_fails() {
    let temp_dir = std::env::temp_dir().join(format!("export_fail_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let out_path = temp_dir.join("output_fail.mp4");

    let config = ExportConfig::new(&out_path, 32, 32, FrameRate::FPS_30, 5);
    let res = export_mp4_stream(&config, |_idx, _time| {
        // Deliberately incorrect buffer length (10 bytes instead of 32*32*4 = 4096)
        Ok(vec![0u8; 10])
    });

    assert!(res.is_err(), "invalid buffer size must cause export failure");
    let _ = std::fs::remove_dir_all(&temp_dir);
}
