use bonaparte_audio::*;
use bonaparte_model::*;
use std::sync::Arc;
struct Bank(Arc<PcmSource>);
impl AudioSources for Bank {
    fn source(&self, _: MediaId) -> Option<Arc<dyn AudioSource>> {
        Some(self.0.clone())
    }
}
fn fixture(samples: Vec<f32>) -> (Project, CompId, Bank) {
    let frames = samples.len() / 2;
    let mut p = Project::new("Processing");
    let c = p.create_comp("Main", 8, 8, FrameRate::FPS_24, Time(240000));
    let id = p.insert_media(MediaAsset {
        id: MediaId(0),
        name: "Input".into(),
        path: None,
        kind: MediaKind::Audio {
            duration: Time(frames as i64 * 5 / 2),
        },
        embedded: None,
        audio: Some(EmbeddedAudio {
            data_base64: Arc::from("AAAA"),
            sha256: "0".repeat(64),
            frames: frames as u64,
            channels: 2,
            sample_rate: 48000,
            original_sample_rate: 48000,
            original_channels: 2,
            codec: "pcm".into(),
            peak: 1.0,
        }),
        slot: None,
        alias: None,
        perception: None,
        video: None,
    });
    let mut t = AudioTrack::new("track", "Input");
    t.clips
        .push(AudioClip::new("clip", "Input", id, frames as u64));
    p.comp_mut(c).unwrap().audio.tracks.push(t);
    (
        p,
        c,
        Bank(Arc::new(PcmSource {
            channels: 2,
            samples,
        })),
    )
}
fn sine(hz: f64, n: usize) -> Vec<f32> {
    (0..n)
        .flat_map(|i| {
            let v = (0.2 * (2.0 * std::f64::consts::PI * hz * i as f64 / 48000.0).sin()) as f32;
            [v, v]
        })
        .collect()
}
#[test]
fn flat_enabled_eq_is_neutral() {
    let (p, c, b) = fixture(sine(1200.0, 24000));
    let clean = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 20000, 48000)
        .unwrap();
    let mut p = p;
    p.comp_mut(c).unwrap().audio.tracks[0].processing.eq.enabled = true;
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 20000, 48000)
        .unwrap();
    assert_eq!(clean.samples, out.samples);
}
#[test]
fn eq_bell_gain_and_highpass_are_measurable() {
    let (mut p, c, b) = fixture(sine(1000.0, 48000));
    let eq = &mut p.comp_mut(c).unwrap().audio.tracks[0].processing.eq;
    eq.enabled = true;
    eq.mid_hz = 1000.0;
    eq.mid_db = 6.0;
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(10000, 20000, 48000)
        .unwrap();
    let ratio = out.meter.rms[0] as f64 / (0.2 / 2.0f64.sqrt());
    assert!((20.0 * ratio.log10() - 6.0).abs() < 0.1);
    let (mut p, c, b) = fixture(sine(30.0, 48000));
    let eq = &mut p.comp_mut(c).unwrap().audio.tracks[0].processing.eq;
    eq.enabled = true;
    eq.highpass_hz = 180.0;
    assert!(
        MixPlan::new(&p, c, &b)
            .unwrap()
            .render(10000, 20000, 48000)
            .unwrap()
            .meter
            .rms[0]
            < 0.005
    );
}
#[test]
fn compressor_reduces_steady_level_and_links_stereo() {
    let (mut p, c, b) = fixture((0..72000).flat_map(|_| [0.8, 0.2]).collect());
    let cmp = &mut p.comp_mut(c).unwrap().audio.tracks[0].processing.compressor;
    cmp.enabled = true;
    cmp.threshold_db = -18.0;
    cmp.ratio = 4.0;
    cmp.knee_db = 0.0;
    cmp.attack_ms = 5.0;
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(48000, 4096, 48000)
        .unwrap();
    let expected = db_gain(-18.0 + (20.0 * 0.8f64.log10() + 18.0) / 4.0);
    assert!((out.samples[0] as f64 - expected).abs() < 0.001);
    assert!((out.samples[0] / out.samples[1] - 4.0).abs() < 0.0001);
    assert!(out.processing.iter().any(|m| m.reduction_db > 10.0));
}
#[test]
fn limiter_enforces_sample_ceiling_without_shifting_the_impulse() {
    let mut samples = vec![0.0; 12000];
    samples[4000 * 2] = 2.0;
    samples[4000 * 2 + 1] = -1.5;
    let (mut p, c, b) = fixture(samples);
    p.comp_mut(c).unwrap().audio.limiter.enabled = true;
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 6000, 48000)
        .unwrap();
    let first = out
        .samples
        .chunks_exact(2)
        .position(|v| v[0].abs() > 0.01)
        .unwrap();
    assert_eq!(first, 4000);
    assert!(out.meter.peak[0] <= db_gain(-1.0) as f32 + 0.000001);
    assert_eq!(out.compensated_latency, 240);
}
#[test]
fn routing_and_post_strip_sends_sum_without_duplicate_dry_paths() {
    let (mut p, c, b) = fixture(vec![0.25; 24000]);
    let mut a = AudioBus::new("bus-a", "A");
    a.gain_db = -6.020599913279624;
    let z = AudioBus::new("bus-b", "B");
    let audio = &mut p.comp_mut(c).unwrap().audio;
    audio.buses = vec![a, z];
    audio.tracks[0].output = Some("bus-a".into());
    audio.tracks[0].sends.push(AudioSend {
        bus: "bus-b".into(),
        gain_db: -6.020599913279624,
        enabled: true,
    });
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 1000, 48000)
        .unwrap();
    assert!((out.samples[0] - 0.25).abs() < 0.000001);
    assert_eq!(out.processing.iter().filter(|m| m.kind == "bus").count(), 2);
}
#[test]
fn processed_seek_and_chunk_boundaries_match_contiguous_output() {
    let (mut p, c, b) = fixture(sine(450.0, 48000));
    let a = &mut p.comp_mut(c).unwrap().audio;
    a.processing.eq.enabled = true;
    a.processing.eq.low_db = 5.0;
    a.processing.compressor.enabled = true;
    a.processing.compressor.threshold_db = -24.0;
    a.limiter.enabled = true;
    let plan = MixPlan::new(&p, c, &b).unwrap();
    let all = plan.render(0, 36000, 48000).unwrap().samples;
    let mut pieces = plan.render(0, 777, 48000).unwrap().samples;
    pieces.extend(plan.render(777, 15000, 48000).unwrap().samples);
    pieces.extend(plan.render(15777, 20223, 48000).unwrap().samples);
    assert_eq!(all, pieces);
    assert_eq!(
        &all[7770..15200],
        plan.render(3885, 3715, 48000).unwrap().samples
    );
    let other = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(3885, 3715, 48000)
        .unwrap()
        .samples;
    assert_eq!(&all[7770..15200], other);
}
#[test]
fn nested_limiter_routes_are_latency_aligned() {
    let mut samples = vec![0.0; 12000];
    samples[2000 * 2] = 0.2;
    samples[2000 * 2 + 1] = 0.2;
    let (mut p, c, b) = fixture(samples);
    let audio = p.comp(c).unwrap().audio.clone();
    let child = p.create_comp("Child", 8, 8, FrameRate::FPS_24, Time(240000));
    p.comp_mut(child).unwrap().audio = audio;
    p.comp_mut(child).unwrap().audio.limiter.enabled = true;
    p.insert_layer(
        c,
        Layer::new(
            "Nested",
            LayerKind::PreComp { comp: child },
            Time::ZERO,
            Time(240000),
        ),
    );
    p.comp_mut(c).unwrap().audio.limiter.enabled = true;
    let out = MixPlan::new(&p, c, &b)
        .unwrap()
        .render(0, 6000, 48000)
        .unwrap();
    assert_eq!(
        out.samples.chunks_exact(2).position(|s| s[0] > 0.01),
        Some(2000)
    );
    assert!((out.samples[4000] - 0.4).abs() < 0.000001);
    assert_eq!(out.compensated_latency, 480);
}
#[test]
fn invalid_eq_and_feedback_routes_fail_atomically() {
    let (mut p, c, _) = fixture(vec![0.1; 200]);
    let original = p.comp(c).unwrap().audio.clone();
    let mut next = original.clone();
    let mut a = AudioBus::new("a", "A");
    a.output = Some("b".into());
    let mut b = AudioBus::new("b", "B");
    b.sends.push(AudioSend {
        bus: "a".into(),
        gain_db: -6.0,
        enabled: true,
    });
    next.buses = vec![a, b];
    let mut history = History::new();
    assert!(history
        .commit(
            &mut p,
            Op::SetCompAudio {
                comp: c,
                audio: next
            }
        )
        .is_err());
    assert_eq!(p.comp(c).unwrap().audio, original);
    let mut next = original;
    next.processing.eq.mid_q = f64::NAN;
    assert!(history
        .commit(
            &mut p,
            Op::SetCompAudio {
                comp: c,
                audio: next
            }
        )
        .is_err());
}
