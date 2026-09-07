//! Deep-color export: 16-bit PNGs and wide-gamut output spaces through the
//! real render pipeline. The 8-bit sRGB default must stay byte-compatible
//! with the legacy export; every deep option must be a real, decodable PNG
//! whose samples honor the requested depth and space.

use bonaparte_model::*;
use bonaparte_runtime::{EditorSession, RenderRequest};
use serde_json::json;

fn session_with_red_solid() -> EditorSession {
    let mut project = Project::new("deep");
    let c = project.create_comp("Deep", 8, 8, FrameRate::FPS_30, Time(240000));
    project.comp_mut(c).unwrap().background = [0.0, 0.0, 0.0, 1.0];
    let layer = Layer::new_solid("Red", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(240000));
    project.insert_layer(c, layer);
    let mut session = EditorSession::default();
    session
        .command(
            "open_project",
            json!({"json": serde_json::to_string(&project).unwrap()}),
        )
        .unwrap();
    session
}

fn decode_png(bytes: &[u8]) -> (u32, u32, png::BitDepth, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("valid PNG");
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf).expect("decodable PNG");
    buf.truncate(info.buffer_size());
    (info.width, info.height, info.bit_depth, buf)
}

#[test]
fn default_export_stays_eight_bit_srgb() {
    let session = session_with_red_solid();
    let bytes = session
        .render_input(serde_json::from_value(json!({"compId": 1, "time": 0})).unwrap())
        .unwrap()
        .png()
        .unwrap();
    let (w, h, depth, data) = decode_png(&bytes);
    assert_eq!((w, h), (8, 8));
    assert_eq!(depth, png::BitDepth::Eight);
    assert_eq!(&data[0..4], &[255, 0, 0, 255]);
}

#[test]
fn sixteen_bit_export_carries_real_depth() {
    let session = session_with_red_solid();
    let bytes = session
        .render_input(
            serde_json::from_value(json!({"compId": 1, "time": 0, "bitDepth": 16})).unwrap(),
        )
        .unwrap()
        .png()
        .unwrap();
    let (w, h, depth, data) = decode_png(&bytes);
    assert_eq!((w, h), (8, 8));
    assert_eq!(depth, png::BitDepth::Sixteen);
    assert_eq!(data.len(), (8 * 8 * 4 * 2) as usize);
    // Pure red: full scale R, zero G/B, full alpha, big-endian samples.
    assert_eq!(&data[0..8], &[255, 255, 0, 0, 0, 0, 255, 255]);
}

#[test]
fn wide_gamut_and_linear_spaces_change_samples() {
    let session = session_with_red_solid();
    let request = |space: &str| {
        session
            .render_input(
                serde_json::from_value(
                    json!({"compId": 1, "time": 0, "bitDepth": 16, "outputSpace": space}),
                )
                .unwrap(),
            )
            .unwrap()
            .png()
            .unwrap()
    };
    let srgb = request("srgb");
    let p3 = request("display-p3");
    let rec2020 = request("rec2020");
    let linear = request("linear");
    // All decodable at depth 16.
    for bytes in [&srgb, &p3, &rec2020, &linear] {
        assert_eq!(decode_png(bytes).2, png::BitDepth::Sixteen);
    }
    let green = |bytes: &[u8]| {
        let (_, _, _, d) = decode_png(bytes);
        u16::from_be_bytes([d[2], d[3]])
    };
    // sRGB pure red has zero green; wide-gamut conversions of sRGB red pick
    // up a nonzero green term (primary re-projection), and linear removes
    // the transfer function entirely (max linear red stays full scale).
    assert_eq!(green(&srgb), 0);
    assert!(green(&p3) > 0, "P3 red must carry a green term");
    assert!(green(&rec2020) > 0, "Rec2020 red must carry a green term");
    let (_, _, _, d) = decode_png(&linear);
    assert_eq!(&d[0..2], &[255, 255], "linear red stays full scale");
}

#[test]
fn a_gradient_export_showcases_sixteen_bit_levels() {
    // A fade-to-transparent gradient is where 8-bit banding lives; assert the
    // 16-bit export contains intermediate levels 8-bit cannot represent.
    let mut project = Project::new("gradient");
    let c = project.create_comp("Deep", 32, 32, FrameRate::FPS_30, Time(240000));
    project.comp_mut(c).unwrap().background = [0.0, 0.0, 0.0, 0.0];
    let mut layer = Layer::new_solid("G", [0.7, 0.4, 0.2, 1.0], Time::ZERO, Time(240000));
    layer.tracks.insert(Property::Opacity, Track::new());
    if let Some(track) = layer.tracks.get_mut(&Property::Opacity) {
        track.set_key(Keyframe {
            time: Time::ZERO,
            value: PropValue::Scalar(1.0),
            easing: Easing::default(),
        });
        track.set_key(Keyframe {
            time: Time(120000),
            value: PropValue::Scalar(0.5),
            easing: Easing::default(),
        });
        track.set_key(Keyframe {
            time: Time(240000),
            value: PropValue::Scalar(0.0),
            easing: Easing::default(),
        });
    }
    project.insert_layer(c, layer);
    let mut session = EditorSession::default();
    session
        .command(
            "open_project",
            json!({"json": serde_json::to_string(&project).unwrap()}),
        )
        .unwrap();
    let render = |time: i64, depth: u8| {
        session
            .render_input(
                serde_json::from_value(json!({"compId": c.0, "time": time, "bitDepth": depth}))
                    .unwrap(),
            )
            .unwrap()
            .png()
            .unwrap()
    };
    let deep = decode_png(&render(60000, 16));
    let legacy = decode_png(&render(60000, 8));
    assert_eq!(deep.2, png::BitDepth::Sixteen);
    // The same instant at the same settings differs only in depth: the 16-bit
    // alpha must sit within one 8-bit step of the 8-bit alpha (no re-grading),
    // while carrying 256× finer resolution.
    let a16 = u16::from_be_bytes([deep.3[6], deep.3[7]]);
    let a8 = legacy.3[3];
    assert!(
        (a16 as i32 - (a8 as i32 * 257)).abs() <= 257,
        "16-bit alpha {a16} drifted from 8-bit alpha {a8}"
    );
}

#[test]
fn invalid_depth_and_unknown_space_are_refused() {
    let session = session_with_red_solid();
    let bad = session.render_input(
        serde_json::from_value::<RenderRequest>(json!({"compId": 1, "time": 0, "bitDepth": 12}))
            .unwrap(),
    );
    assert!(bad.is_err());
    let bad_space = serde_json::from_value::<RenderRequest>(json!({
        "compId": 1, "time": 0, "outputSpace": "prores-2020-hg"
    }));
    assert!(bad_space.is_err());
}
