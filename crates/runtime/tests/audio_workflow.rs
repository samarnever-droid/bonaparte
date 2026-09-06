use base64::{engine::general_purpose::STANDARD, Engine};
use bonaparte_model::*;
use bonaparte_runtime::audio::prepare_import;
use bonaparte_runtime::{
    parse_project, serialize_project, AudioChunkRequest, EditorSession, RenderRequest,
};
use serde_json::{json, Value};
fn wav(rate: u32, channels: u16, frames: usize) -> Vec<u8> {
    let size = frames * channels as usize * 2;
    let mut b = vec![];
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(size as u32 + 36).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * channels as u32 * 2).to_le_bytes());
    b.extend_from_slice(&(channels * 2).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&(size as u32).to_le_bytes());
    for i in 0..frames {
        for c in 0..channels {
            let v = (0.35
                * (2.0 * std::f64::consts::PI * (440 + c as u32 * 220) as f64 * i as f64
                    / rate as f64)
                    .sin()
                * 32767.0) as i16;
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b
}
fn session() -> EditorSession {
    let mut p = Project::new("Audio workflow");
    p.create_comp("Main", 16, 16, FrameRate::FPS_24, Time(120000));
    EditorSession::new(p).unwrap()
}
fn import(s: &mut EditorSession, bytes: &[u8], start: i64) -> Value {
    let p=prepare_import(json!({"compId":1,"name":"Test source.wav","dataBase64":STANDARD.encode(bytes),"startFrame":start})).unwrap();
    s.import_audio(p).unwrap()
}
#[test]
fn native_import_resamples_retains_original_and_is_one_undo_transaction() {
    let mut s = session();
    let bytes = wav(44100, 1, 4410);
    let reply = import(&mut s, &bytes, 7);
    let meta = s.project.media[&MediaId(1)].audio.as_ref().unwrap();
    assert_eq!(meta.sample_rate, 48000);
    assert_eq!(meta.frames, 4800);
    assert_eq!(meta.original_sample_rate, 44100);
    assert_eq!(meta.channels, 1);
    assert_eq!(STANDARD.decode(meta.data_base64.as_bytes()).unwrap(), bytes);
    assert_eq!(reply["project"]["media"]["1"]["audio"]["data_base64"], "");
    assert_eq!(s.snapshot().history.len(), 1);
    assert_eq!(
        s.project.comp(CompId(1)).unwrap().audio.tracks[0].clips[0].start_frame,
        7
    );
    s.command("undo", json!({})).unwrap();
    assert!(s.project.media.is_empty());
    assert!(s.project.comp(CompId(1)).unwrap().audio.tracks.is_empty());
    s.command("redo", json!({})).unwrap();
    assert_eq!(s.project.media.len(), 1);
}
#[test]
fn save_open_restores_audio_and_malformed_source_replacement_is_atomic() {
    let mut s = session();
    import(&mut s, &wav(48000, 2, 4800), 0);
    let serialized = serialize_project(&s.project).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&serialized).unwrap()["version"],
        4
    );
    let restored = parse_project(&serialized).unwrap();
    assert_eq!(
        restored.comp(CompId(1)).unwrap().audio,
        s.project.comp(CompId(1)).unwrap().audio
    );
    let before = serialize_project(&s.project).unwrap();
    let mut bad: Value = serde_json::from_str(&serialized).unwrap();
    bad["project"]["media"]["1"]["audio"]["frames"] = json!(4801);
    assert!(s
        .command("open_project", json!({"json":bad.to_string()}))
        .is_err());
    assert_eq!(serialize_project(&s.project).unwrap(), before);
    assert!(prepare_import(
        json!({"compId":1,"name":"bad.wav","dataBase64":STANDARD.encode(b"not audio")})
    )
    .is_err());
}
#[test]
fn waveform_levels_chunk_headers_and_wav_are_real_samples() {
    let mut s = session();
    import(&mut s, &wav(48000, 2, 4800), 2400);
    let peaks = s
        .command("audio_waveform", json!({"mediaId":1,"points":128}))
        .unwrap();
    assert_eq!(peaks["channels"], 2);
    assert_eq!(peaks["peaks"].as_array().unwrap().len(), 128);
    assert!(peaks["peaks"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v[1].as_f64().unwrap() > 0.2));
    let input = s.audio_input(CompId(1)).unwrap();
    let req:AudioChunkRequest=serde_json::from_value(json!({"compId":1,"startFrame":0,"frameCount":9600,"sampleRate":48000,"revision":s.snapshot().revision})).unwrap();
    let packet = input.packet(req).unwrap();
    assert_eq!(&packet[..4], b"BAP1");
    let n = u32::from_le_bytes(packet[4..8].try_into().unwrap()) as usize;
    let meta: Value = serde_json::from_slice(&packet[8..8 + n]).unwrap();
    assert_eq!(meta["frameCount"], 9600);
    let samples: Vec<f32> = packet[8 + n..]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    assert!(samples[..4800].iter().all(|v| *v == 0.0));
    assert!(samples[4800..].iter().any(|v| v.abs() > 0.1));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("mix.wav");
    input.wav(&path).unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    assert_eq!(u16::from_le_bytes(bytes[20..22].try_into().unwrap()), 3);
    assert_eq!(&bytes[56..56 + samples.len() * 4], &packet[8 + n..]);
}
#[test]
fn sample_split_and_media_removal_rejection_keep_state_consistent() {
    let mut s = session();
    import(&mut s, &wav(48000, 2, 4800), 0);
    let before = s
        .audio_input(CompId(1))
        .unwrap()
        .plan()
        .unwrap()
        .render(0, 6000, 48000)
        .unwrap()
        .samples;
    s.command(
        "split_audio_clip",
        json!({"compId":1,"trackId":"track-1","clipId":"clip-1","atFrame":1777}),
    )
    .unwrap();
    assert_eq!(
        s.project.comp(CompId(1)).unwrap().audio.tracks[0]
            .clips
            .len(),
        2
    );
    assert_eq!(
        before,
        s.audio_input(CompId(1))
            .unwrap()
            .plan()
            .unwrap()
            .render(0, 6000, 48000)
            .unwrap()
            .samples
    );
    assert!(s
        .command("apply", json!({"op":{"type":"removeMedia","media":1}}))
        .is_err());
    assert_eq!(s.project.media.len(), 1);
}
#[test]
fn mp4_contains_synchronized_audio_and_preserves_ntsc_video_frames() {
    let mut s = session();
    import(&mut s, &wav(48000, 2, 12000), 4800);
    let input = s
        .render_input(
            serde_json::from_value::<RenderRequest>(json!({"compId":1,"time":0})).unwrap(),
        )
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("audio.mp4");
    let fps = FrameRate {
        num: 24000,
        den: 1001,
    };
    let stats = input
        .export_mp4_range(&path, Time::ZERO, Time(15015), fps)
        .unwrap();
    assert_eq!(stats.frames_exported, 3);
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name,sample_rate,channels,nb_frames,r_frame_rate,duration",
            "-of",
            "json",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(out.status.success());
    let p: Value = serde_json::from_slice(&out.stdout).unwrap();
    let streams = p["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 2);
    let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    assert_eq!(video["nb_frames"], "3");
    assert_eq!(video["r_frame_rate"], "24000/1001");
    let audio = streams.iter().find(|s| s["codec_type"] == "audio").unwrap();
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(audio["sample_rate"], "48000");
    assert_eq!(audio["channels"], 2);
}
#[test]
fn legacy_v2_still_opens_and_unknown_versions_do_not() {
    let s = session();
    let mut v: Value = serde_json::from_str(&serialize_project(&s.project).unwrap()).unwrap();
    v["version"] = json!(2);
    assert!(parse_project(&v.to_string()).is_ok());
    v["version"] = json!(99);
    assert!(parse_project(&v.to_string()).is_err());
}

#[test]
fn common_encoded_formats_decode_through_the_same_native_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.wav");
    std::fs::write(&source, wav(48000, 2, 12000)).unwrap();
    for (ext, codec) in [
        ("mp3", "libmp3lame"),
        ("flac", "flac"),
        ("ogg", "libvorbis"),
        ("aiff", "pcm_s16be"),
        ("m4a", "aac"),
        ("aac", "aac"),
    ] {
        let target = dir.path().join(format!("source.{ext}"));
        let status = std::process::Command::new("ffmpeg")
            .args(["-y", "-v", "error", "-i"])
            .arg(&source)
            .args(["-c:a", codec])
            .arg(&target)
            .status()
            .unwrap();
        assert!(status.success());
        let bytes = std::fs::read(&target).unwrap();
        let prepared = prepare_import(
            json!({"compId":1,"name":format!("source.{ext}"),"dataBase64":STANDARD.encode(bytes)}),
        )
        .unwrap();
        let mut s = session();
        s.import_audio(prepared).unwrap();
        let asset = s.project.media[&MediaId(1)].audio.as_ref().unwrap();
        assert_eq!(asset.sample_rate, 48000);
        assert_eq!(asset.channels, 2);
        assert!(
            asset.frames > 11000 && asset.frames < 16000,
            "{ext}: {}",
            asset.frames
        );
    }
}

#[test]
fn processed_packets_and_native_wav_share_the_compensated_timeline() {
    let mut s = session();
    import(&mut s, &wav(48000, 2, 24000), 4800);
    let mut a = s.project.comp(CompId(1)).unwrap().audio.clone();
    a.processing.eq.enabled = true;
    a.processing.eq.mid_db = 4.0;
    a.processing.compressor.enabled = true;
    a.processing.compressor.threshold_db = -24.0;
    a.limiter.enabled = true;
    s.command(
        "apply",
        json!({"op":Op::SetCompAudio{comp:CompId(1),audio:a}}),
    )
    .unwrap();
    let input = s.audio_input(CompId(1)).unwrap();
    let packet=input.packet(serde_json::from_value(json!({"compId":1,"startFrame":0,"frameCount":12000,"sampleRate":48000,"revision":s.snapshot().audio_revision})).unwrap()).unwrap();
    let n = u32::from_le_bytes(packet[4..8].try_into().unwrap()) as usize;
    let metadata: Value = serde_json::from_slice(&packet[8..8 + n]).unwrap();
    assert_eq!(metadata["compensatedLatency"], 240);
    assert!(!metadata["processing"].as_array().unwrap().is_empty());
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("processed.wav");
    input.wav(&path).unwrap();
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[56..56 + 12000 * 8], &packet[8 + n..]);
}
#[test]
fn distant_processed_seeks_prepare_in_bounded_steps_and_outside_time_is_silent() {
    let mut s = session();
    import(&mut s, &wav(48000, 2, 1200), 0);
    let mut c = s.project.comp(CompId(1)).unwrap().clone();
    c.duration = Time(2400000);
    s.command("apply",json!({"op":Op::SetCompProps{comp:c.id,name:c.name,width:c.width,height:c.height,fps:c.fps,duration:c.duration,background:c.background}})).unwrap();
    let mut a = c.audio;
    a.processing.eq.enabled = true;
    s.command("apply", json!({"op":Op::SetCompAudio{comp:c.id,audio:a}}))
        .unwrap();
    let input = s.audio_input(CompId(1)).unwrap();
    let req = |start| {
        serde_json::from_value(
            json!({"compId":1,"startFrame":start,"frameCount":128,"sampleRate":48000}),
        )
        .unwrap()
    };
    let warming = input.packet(req(19 * 48000)).unwrap();
    assert_eq!(&warming[..4], b"BAW1");
    let ready = input.packet(req(19 * 48000)).unwrap();
    assert_eq!(&ready[..4], b"BAP1");
    let outside = input.packet(req(1_000_000_000_000i64)).unwrap();
    let n = u32::from_le_bytes(outside[4..8].try_into().unwrap()) as usize;
    assert!(outside[8 + n..].iter().all(|v| *v == 0));
}
