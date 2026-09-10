use bonaparte_audio::*;
use bonaparte_model::*;
use std::sync::Arc;
struct Bank(Arc<PcmSource>);
impl AudioSources for Bank {
    fn source(&self, _: MediaId) -> Option<Arc<dyn AudioSource>> {
        Some(self.0.clone())
    }
}
fn scene(channels: u16, samples: Vec<f32>) -> (Project, CompId, Bank) {
    let frames = samples.len() / channels as usize;
    let bank = Bank(Arc::new(PcmSource { channels, samples }));
    let mut p = Project::new("Audio");
    let c = p.create_comp("Main", 8, 8, FrameRate::FPS_24, Time(240000));
    let media = p.insert_media(MediaAsset {
        id: MediaId(0),
        name: "Audio".into(),
        path: None,
        kind: MediaKind::Audio {
            duration: Time(frames as i64 * 5 / 2),
        },
        embedded: None,
        audio: Some(EmbeddedAudio {
            data_base64: Arc::from("AAAA"),
            astra_chunks: None,
            beat_grid: None,
            kaya_words: None,
            sha256: "0".repeat(64),
            frames: frames as u64,
            channels,
            sample_rate: 48000,
            original_sample_rate: 48000,
            original_channels: channels,
            codec: "pcm".into(),
            peak: 1.0,
        }),
        slot: None,
        alias: None,
        perception: None,
        video: None,
        footage: None,
    });
    let mut t = AudioTrack::new("track-1", "Source");
    t.clips
        .push(AudioClip::new("clip-1", "Clip", media, frames as u64));
    p.comp_mut(c).unwrap().audio.tracks.push(t);
    (p, c, bank)
}
#[test]
fn mono_uses_equal_power_pan_and_stereo_center_is_unchanged() {
    let (p, c, b) = scene(1, vec![1.0; 8]);
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 8, 48000)
        .unwrap();
    assert!((out.samples[0] - 0.5f32.sqrt()).abs() < 1e-6);
    assert_eq!(out.samples[0], out.samples[1]);
    let (p, c, b) = scene(2, vec![0.2, -0.7, 0.5, 0.1]);
    assert_eq!(
        MixPlan::new(&p, c, &b)
            .unwrap()
            .render(0, 2, 48000)
            .unwrap()
            .samples,
        b.0.samples
    );
}
#[test]
fn source_offsets_gaps_and_overlapping_clips_mix_at_sample_boundaries() {
    let (mut p, c, b) = scene(2, (0..40).map(|i| i as f32 / 100.0).collect());
    let clip = &mut p.comp_mut(c).unwrap().audio.tracks[0].clips[0];
    clip.start_frame = 3;
    clip.source_offset = 4.0;
    clip.duration_frames = 5;
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 10, 48000)
        .unwrap();
    assert_eq!(&out.samples[..6], &[0.0; 6]);
    assert_eq!(&out.samples[6..16], &b.0.samples[8..18]);
    assert_eq!(&out.samples[16..], &[0.0; 4]);
    let mut copy = p.comp(c).unwrap().audio.tracks[0].clips[0].clone();
    copy.id = "clip-2".into();
    p.comp_mut(c).unwrap().audio.tracks[0].clips.push(copy);
    let mixed = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 10, 48000)
        .unwrap();
    assert_eq!(mixed.samples[6], out.samples[6] * 2.0);
}
#[test]
fn moving_and_splitting_keep_varispeed_fades_and_automation_identical() {
    let (mut p, c, b) = scene(
        2,
        (0..4000)
            .map(|i| ((i as f64 * 0.1).sin() * 0.5) as f32)
            .collect(),
    );
    let clip = &mut p.comp_mut(c).unwrap().audio.tracks[0].clips[0];
    clip.duration_frames = 1100;
    clip.rate = 1.25;
    clip.fade_in = Some(AudioFade {
        start: 0,
        end: 900,
        shape: FadeShape::EqualPower,
    });
    clip.fade_out = Some(AudioFade {
        start: 300,
        end: 1100,
        shape: FadeShape::Linear,
    });
    clip.gain_points = vec![
        AudioPoint {
            frame: 0,
            value: -12.0,
        },
        AudioPoint {
            frame: 1000,
            value: 0.0,
        },
    ];
    clip.pan_points = vec![
        AudioPoint {
            frame: 0,
            value: -0.6,
        },
        AudioPoint {
            frame: 1100,
            value: 0.5,
        },
    ];
    let original = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 1200, 48000)
        .unwrap()
        .samples;
    let (a, mut right) = p.comp(c).unwrap().audio.tracks[0].clips[0]
        .split(417, "clip-2".into())
        .unwrap();
    right.start_frame += 0;
    p.comp_mut(c).unwrap().audio.tracks[0].clips = vec![a, right];
    let split = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 1200, 48000)
        .unwrap()
        .samples;
    for (a, b) in original.iter().zip(split) {
        assert!((a - b).abs() < 1e-6);
    }
}
#[test]
fn chunks_and_arbitrary_seek_ranges_are_sample_identical() {
    let (mut p, c, b) = scene(
        2,
        (0..4000)
            .map(|i| ((i as f64 * 0.08).sin() * 0.4) as f32)
            .collect(),
    );
    let clip = &mut p.comp_mut(c).unwrap().audio.tracks[0].clips[0];
    clip.duration_frames = 1200;
    clip.rate = 1.33;
    clip.pan_points = vec![
        AudioPoint {
            frame: 0,
            value: -1.0,
        },
        AudioPoint {
            frame: 1200,
            value: 1.0,
        },
    ];
    let plan = MixPlan::new(&p, c, &b).unwrap();
    let full = plan.render(0, 1000, 44100).unwrap().samples;
    let mut parts = plan.render(0, 337, 44100).unwrap().samples;
    parts.extend(plan.render(337, 663, 44100).unwrap().samples);
    assert_eq!(full, parts);
    assert_eq!(
        &full[246..824],
        &plan.render(123, 289, 44100).unwrap().samples
    );
}
#[test]
fn reverse_and_mute_solo_are_real_mix_operations() {
    let (mut p, c, b) = scene(2, vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6]);
    let clip = &mut p.comp_mut(c).unwrap().audio.tracks[0].clips[0];
    clip.rate = -1.0;
    clip.source_offset = 2.0;
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 3, 48000)
        .unwrap();
    assert_eq!(out.samples, vec![0.5, 0.6, 0.3, 0.4, 0.1, 0.2]);
    let mut other = AudioTrack::new("track-2", "Solo");
    other.solo = true;
    p.comp_mut(c).unwrap().audio.tracks.push(other);
    assert!(MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 3, 48000)
        .unwrap()
        .samples
        .iter()
        .all(|v| *v == 0.0));
    p.comp_mut(c).unwrap().audio.tracks[1].muted = true;
    assert_ne!(
        MixPlan::new(&p, c, &b)
            .unwrap()
            .render(0, 3, 48000)
            .unwrap()
            .samples,
        vec![0.0; 6]
    );
}
#[test]
fn unclipped_float_mix_reports_overload_and_gain_reduces_it() {
    let (mut p, c, b) = scene(2, vec![0.8; 20]);
    let clip = &mut p.comp_mut(c).unwrap().audio.tracks[0].clips[0];
    clip.gain_db = 6.0;
    let hot = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 10, 48000)
        .unwrap();
    assert!(hot.meter.clipped_samples > 0);
    assert!(hot.samples[0] > 1.0);
    p.comp_mut(c).unwrap().audio.gain_db = -12.0;
    let safe = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 10, 48000)
        .unwrap();
    assert_eq!(safe.meter.clipped_samples, 0);
    assert!((safe.meter.peak[0] - 0.40095).abs() < 0.001);
}
#[test]
fn arrangement_edits_are_atomic_undoable_and_reject_bad_source_ranges() {
    let (mut p, c, _) = scene(1, vec![0.1; 100]);
    let before = p.comp(c).unwrap().audio.clone();
    let mut next = before.clone();
    next.tracks[0].gain_db = -6.0;
    let mut h = History::new();
    h.commit(
        &mut p,
        Op::SetCompAudio {
            comp: c,
            audio: next.clone(),
        },
    )
    .unwrap();
    assert_eq!(p.comp(c).unwrap().audio, next);
    h.undo(&mut p).unwrap();
    assert_eq!(p.comp(c).unwrap().audio, before);
    h.redo(&mut p).unwrap();
    next.tracks[0].clips[0].source_offset = 10000.0;
    assert!(h
        .commit(
            &mut p,
            Op::SetCompAudio {
                comp: c,
                audio: next
            }
        )
        .is_err());
    assert_eq!(p.comp(c).unwrap().audio.tracks[0].gain_db, -6.0);
}
#[test]
fn audio_time_is_not_quantized_to_video_frames() {
    assert_eq!(audio_frames(Time(5)), 2);
    assert_eq!(audio_duration(Time(3)), 2);
    let (mut p, c, b) = scene(2, vec![1.0, 0.0]);
    p.comp_mut(c).unwrap().audio.tracks[0].clips[0].start_frame = 1;
    assert_eq!(
        MixPlan::new(&p, c, &b)
            .unwrap()
            .render(0, 3, 48000)
            .unwrap()
            .samples,
        vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn downsampling_suppresses_out_of_band_aliasing() {
    let samples = (0..4096)
        .flat_map(|i| {
            let v = (2.0 * std::f64::consts::PI * 20000.0 * i as f64 / 48000.0).sin() as f32;
            [v, v]
        })
        .collect();
    let (mut p, c, b) = scene(2, samples);
    let clip = &mut p.comp_mut(c).unwrap().audio.tracks[0].clips[0];
    clip.rate = 2.0;
    clip.duration_frames = 1800;
    let block = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(100, 1000, 48000)
        .unwrap();
    assert!(
        block.meter.rms[0] < 0.001,
        "alias RMS {}",
        block.meter.rms[0]
    );
}
#[test]
fn nested_audio_obeys_comp_time_and_visibility_not_visual_opacity() {
    let (mut p, c, b) = scene(2, vec![0.3; 200]);
    let audio = p.comp(c).unwrap().audio.clone();
    p.comp_mut(c).unwrap().audio = AudioArrangement::default();
    let child = p.create_comp("Nested", 8, 8, FrameRate::FPS_24, Time(120000));
    p.comp_mut(child).unwrap().audio = audio;
    let mut layer = Layer::new(
        "Precomp",
        LayerKind::PreComp { comp: child },
        Time(5000),
        Time(120000),
    );
    layer.transform.opacity = 0.0;
    let id = p.insert_layer(c, layer);
    let block = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 2200, 48000)
        .unwrap();
    assert!(block.samples[..4000].iter().all(|v| *v == 0.0));
    assert_eq!(block.samples[4000], 0.3);
    p.layer_mut(c, id).unwrap().visible = false;
    assert!(MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 2200, 48000)
        .unwrap()
        .samples
        .iter()
        .all(|v| *v == 0.0));
}
#[test]
fn repeated_precomp_graph_validation_does_not_expand_exponentially() {
    let mut p = Project::new("Repeated DAG");
    let mut child = p.create_comp("Leaf", 8, 8, FrameRate::FPS_24, Time(120000));
    for _ in 0..10 {
        let next = p.create_comp("Parent", 8, 8, FrameRate::FPS_24, Time(120000));
        for _ in 0..16 {
            p.insert_layer(
                next,
                Layer::new(
                    "Instance",
                    LayerKind::PreComp { comp: child },
                    Time::ZERO,
                    Time(120000),
                ),
            );
        }
        child = next;
    }
    p.validate().unwrap();
    assert!(!p.has_audio(child));
}
