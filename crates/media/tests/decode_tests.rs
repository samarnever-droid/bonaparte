use bonaparte_media::decode::{decode_frame, DecodeError};
use bonaparte_model::Time;
use std::process::Command;

#[test]
fn test_decode_frame_from_video() {
    let temp_dir = std::env::temp_dir().join(format!("decode_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let video_path = temp_dir.join("test_decode.mp4");

    // Synthesize a 32x32 magenta video (0.5s duration)
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("color=c=magenta:s=32x32:d=0.5")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg(&video_path)
        .status();

    if let Ok(st) = status {
        if st.success() {
            let frame = decode_frame(&video_path, Time::ZERO, 32, 32)
                .expect("decoding frame at 0.0s must succeed");

            assert_eq!(frame.width, 32);
            assert_eq!(frame.height, 32);
            assert_eq!(frame.rgba.len(), 32 * 32 * 4);

            // Magenta = Red + Blue, Green near 0
            let r = frame.rgba[0];
            let g = frame.rgba[1];
            let b = frame.rgba[2];
            let a = frame.rgba[3];

            assert!(r > 200, "Red channel must be strong: {}", r);
            assert!(g < 50, "Green channel must be low: {}", g);
            assert!(b > 200, "Blue channel must be strong: {}", b);
            assert_eq!(a, 255);
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_decode_non_existent_file_errors() {
    let bad_path = std::env::temp_dir().join("this_file_does_not_exist_12345.mp4");
    let res = decode_frame(&bad_path, Time::ZERO, 16, 16);
    assert!(matches!(res, Err(DecodeError::Io { .. })));
}
