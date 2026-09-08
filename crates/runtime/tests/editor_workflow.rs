use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_model::*;
use bonaparte_runtime::*;
use serde_json::{json, Value};

fn session() -> EditorSession {
    let mut p = Project::new("Test");
    p.create_comp("Main", 4, 4, FrameRate::FPS_30, Time(120000));
    EditorSession::new(p).unwrap()
}
fn request(time: i64) -> RenderRequest {
    serde_json::from_value(json!({"compId":1,"time":time})).unwrap()
}

#[test]
fn editable_example_is_valid_and_rendered_by_normal_document_paths() {
    let mut s = EditorSession::default();
    assert_eq!(s.project.comps.len(), 2);
    s.project.validate().unwrap();
    let bytes = s.render_input(request(144000)).unwrap().png().unwrap();
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let catalog = s.command("catalog", json!({})).unwrap();
    assert!(catalog["effects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["id"] == "builtin.color_grade"));
}

#[test]
fn embedded_image_import_save_open_and_render_round_trip() {
    let mut s = session();
    let rgba = [255u8, 40, 10, 255].repeat(16);
    let state=s.command("import_image",json!({"name":"Image.png","width":4,"height":4,"rgbaBase64":STANDARD.encode(&rgba),"compId":1})).unwrap();
    assert_eq!(
        state["project"]["comps"]["1"]["layer_order"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        s.render_input(request(0)).unwrap().render().unwrap().rgba,
        rgba
    );
    let serialized = s
        .command("save_project", json!({}))
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        serde_json::from_str::<Value>(&serialized).unwrap()["version"],
        4
    );
    let p = parse_project(&serialized).unwrap();
    let reopened = EditorSession::new(p).unwrap();
    assert_eq!(
        reopened
            .render_input(request(0))
            .unwrap()
            .render()
            .unwrap()
            .rgba,
        rgba
    );
    s.command("undo", json!({})).unwrap();
    assert!(s.project.media.is_empty());
    assert!(s.project.comp(CompId(1)).unwrap().layers.is_empty());
    s.command("redo", json!({})).unwrap();
    assert_eq!(
        s.render_input(request(0)).unwrap().render().unwrap().rgba,
        rgba
    );
}

#[test]
fn malformed_images_unknown_effects_and_future_files_do_not_mutate_state() {
    let mut s = session();
    let before = serde_json::to_value(s.snapshot()).unwrap();
    assert!(s
        .command(
            "import_image",
            json!({"name":"bad","width":1,"height":1,"rgbaBase64":"!!!!!!!!","compId":1})
        )
        .is_err());
    assert!(s.command("open_project",json!({"json":json!({"format":"bonaparte","version":99,"project":s.project}).to_string()})).is_err());
    assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
    let mut layer = Layer::new_solid("bad", [1.0; 4], Time::ZERO, Time(120000));
    layer
        .effects
        .push(EffectInstance::new("unknown", "not.installed"));
    assert!(s
        .command("apply", json!({"op":Op::AddLayer{comp:CompId(1),layer}}))
        .is_err());
    assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
}

#[test]
fn transient_preview_and_global_bypass_never_enter_history_or_saved_project() {
    let mut s = session();
    let mut l = Layer::new_solid("Red", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(120000));
    l.effects
        .push(EffectInstance::new("invert", "builtin.invert"));
    s.command("apply", json!({"op":Op::AddLayer{comp:CompId(1),layer:l}}))
        .unwrap();
    let before = serde_json::to_value(s.snapshot()).unwrap();
    let normal = s.render_input(request(0)).unwrap().render().unwrap();
    let bypass: RenderRequest =
        serde_json::from_value(json!({"compId":1,"time":0,"bypassEffects":true})).unwrap();
    let source = s.render_input(bypass).unwrap().render().unwrap();
    assert_ne!(source.rgba, normal.rgba);
    let mut preview = s.project.layer(CompId(1), LayerId(1)).unwrap().clone();
    preview.effects.clear();
    preview.kind = LayerKind::Solid {
        color: [0.0, 1.0, 0.0, 1.0],
    };
    let request: RenderRequest =
        serde_json::from_value(json!({"compId":1,"time":0,"layerOverride":preview})).unwrap();
    assert_eq!(
        &s.render_input(request).unwrap().render().unwrap().rgba[..4],
        &[0, 255, 0, 255]
    );
    assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
}

#[test]
fn rgba_binary_protocol_and_video_flattening_are_correct() {
    let s = session();
    let bytes = s.render_input(request(0)).unwrap().raw().unwrap();
    assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 4);
    assert_eq!(bytes.len(), 8 + 4 * 4 * 4);
    let mut frame = bonaparte_engine::Frame {
        width: 2,
        height: 1,
        rgba: vec![255, 0, 0, 0, 255, 255, 255, 128],
    };
    flatten_on_black(&mut frame);
    assert_eq!(&frame.rgba[..4], &[0, 0, 0, 255]);
    assert!((frame.rgba[4] as i32 - 188).abs() <= 1);
    assert_eq!(frame.rgba[7], 255);
}

#[test]
fn ntsc_video_export_has_exact_frame_rate_and_count() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("FFmpeg unavailable; media export test skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ntsc.mp4");
    let fps = FrameRate {
        num: 24000,
        den: 1001,
    };
    let mut p = Project::new("NTSC");
    p.create_comp("Main", 4, 4, fps, Time(15015));
    let s = EditorSession::new(p).unwrap();
    s.render_input(request(0))
        .unwrap()
        .export_mp4(&path)
        .unwrap();
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=nb_frames,r_frame_rate,codec_name",
            "-of",
            "json",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["streams"][0]["nb_frames"], "3");
    assert_eq!(data["streams"][0]["r_frame_rate"], "24000/1001");
    assert_eq!(data["streams"][0]["codec_name"], "h264");
}

#[test]
fn failed_video_render_preserves_existing_output_file() {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("existing.mp4");
    std::fs::write(&path, b"previous successful export").unwrap();
    let mut p = Project::new("Missing footage");
    let c = p.create_comp("Main", 4, 4, FrameRate::FPS_30, Time(4000));
    let media = p.insert_media(MediaAsset {
        id: MediaId(0),
        name: "Missing".into(),
        path: None,
        kind: MediaKind::Image,
        embedded: None,
        audio: None,
        slot: None,
        alias: None,
        perception: None,
        video: None,
    });
    p.insert_layer(
        c,
        Layer::new(
            "Footage",
            LayerKind::Footage { media },
            Time::ZERO,
            Time(4000),
        ),
    );
    let s = EditorSession::new(p).unwrap();
    assert!(s
        .render_input(request(0))
        .unwrap()
        .export_mp4(&path)
        .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"previous successful export");
}

#[test]
fn generic_batch_with_correct_length_but_invalid_base64_is_atomic() {
    let mut s = session();
    let before = serde_json::to_value(s.snapshot()).unwrap();
    let asset = MediaAsset {
        id: MediaId(0),
        name: "Invalid".into(),
        path: None,
        kind: MediaKind::Image,
        embedded: Some(EmbeddedImage {
            width: 1,
            height: 1,
            rgba_base64: "!!!!!!!!".into(),
        }),
        audio: None,
        slot: None,
        alias: None,
        perception: None,
        video: None,
    };
    let op = Op::Batch {
        label: "Malformed image batch".into(),
        ops: vec![
            Op::RenameProject {
                name: "Do not keep".into(),
            },
            Op::AddMedia { asset },
        ],
    };
    assert!(s.command("apply", json!({"op":op})).is_err());
    assert_eq!(serde_json::to_value(s.snapshot()).unwrap(), before);
    assert!(s.render_input(request(0)).unwrap().render().is_ok());
}

#[test]
fn atomic_file_writer_cleans_temporary_files_on_success_and_failure() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("project.bonaparte");
    std::fs::write(&destination, b"previous file").unwrap();
    let serialized = serialize_project(&session().project).unwrap();
    write_file_atomic(&destination, serialized.as_bytes()).unwrap();
    assert_eq!(
        parse_project(&std::fs::read_to_string(&destination).unwrap())
            .unwrap()
            .name,
        "Test"
    );
    let occupied = dir.path().join("directory");
    std::fs::create_dir(&occupied).unwrap();
    let sentinel = occupied.join("keep.txt");
    std::fs::write(&sentinel, b"keep").unwrap();
    assert!(write_file_atomic(&occupied, b"cannot replace a directory").is_err());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"keep");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
