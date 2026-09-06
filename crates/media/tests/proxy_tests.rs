use bonaparte_media::proxy::{generate_proxy, ProxyError};
use std::process::Command;

#[test]
fn test_generate_half_resolution_proxy() {
    let temp_dir = std::env::temp_dir().join(format!("proxy_test_{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let input_path = temp_dir.join("input_hd.mp4");
    let proxy_path = temp_dir.join("input_hd_proxy.mp4");

    // Create 64x64 test video
    let status = Command::new("ffmpeg")
        .arg("-y")
        .arg("-v")
        .arg("error")
        .arg("-f")
        .arg("lavfi")
        .arg("-i")
        .arg("testsrc=size=64x64:rate=30:duration=0.5")
        .arg("-c:v")
        .arg("libx264")
        .arg("-pix_fmt")
        .arg("yuv420p")
        .arg(&input_path)
        .status();

    if let Ok(st) = status {
        if st.success() {
            let info = generate_proxy(&input_path, &proxy_path, 64, 64)
                .expect("proxy generation must succeed");

            assert_eq!(info.original_width, 64);
            assert_eq!(info.original_height, 64);
            assert_eq!(info.proxy_width, 32);
            assert_eq!(info.proxy_height, 32);
            assert!(proxy_path.is_file());
            assert!(std::fs::metadata(&proxy_path).unwrap().len() > 0);
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_proxy_missing_input_errors() {
    let bad_in = std::env::temp_dir().join("missing_in_9999.mp4");
    let bad_out = std::env::temp_dir().join("missing_out_9999.mp4");
    let res = generate_proxy(&bad_in, &bad_out, 64, 64);
    assert!(matches!(res, Err(ProxyError::Io { .. })));
}
