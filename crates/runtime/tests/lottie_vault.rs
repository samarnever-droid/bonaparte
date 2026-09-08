//! Lottie (Bodymovin) import — the motion-graphics interchange format —
//! lands as ONE assembled group with real keyframes, plus the Vault
//! (global asset folder) round trip.

use bonaparte_engine::reference::render_comp;
use bonaparte_model::*;
use bonaparte_runtime::{decode_embedded_frames, EditorSession};
use serde_json::json;

const LOTTIE: &str = r#"{
  "v": "5.7.4", "fr": 30, "ip": 0, "op": 60, "w": 400, "h": 300, "nm": "Bounce",
  "layers": [
    {
      "ty": 4, "nm": "Ball", "ind": 1, "ip": 0, "op": 60, "st": 0,
      "ks": {
        "o": { "a": 0, "k": 90 },
        "p": { "a": 1, "k": [
          { "t": 0, "s": [100, 60, 0], "o": {"x": 0.4, "y": 0}, "i": {"x": 0.6, "y": 1} },
          { "t": 30, "s": [300, 240, 0], "o": {"x": 0.4, "y": 0}, "i": {"x": 0.6, "y": 1} },
          { "t": 60, "s": [100, 60, 0] }
        ]},
        "a": { "a": 0, "k": [0, 0, 0] },
        "s": { "a": 0, "k": [100, 100, 100] }
      },
      "shapes": [
        { "ty": "el", "nm": "dot", "p": { "a": 0, "k": [0, 0] }, "s": { "a": 0, "k": [80, 80] } },
        { "ty": "fl", "nm": "fill", "c": { "a": 0, "k": [1, 0.3, 0.2, 1] }, "o": { "a": 0, "k": 100 } }
      ]
    },
    { "ty": 5, "nm": "Text", "ind": 2, "ip": 0, "op": 60, "st": 0, "ks": {} }
  ]
}"#;

fn session() -> EditorSession {
    let mut p = Project::new("Lottie");
    p.create_comp("Main", 400, 300, FrameRate::FPS_30, Time(TICKS_PER_SEC));
    EditorSession::new(p).unwrap()
}

#[test]
fn lottie_imports_assembled_with_keyframes_and_renders() {
    let mut s = session();
    let reply = s
        .command(
            "import_lottie",
            json!({"name": "bounce.lottie", "json": LOTTIE, "compId": 1}),
        )
        .expect("lottie import");
    assert_eq!(reply["layers"].as_u64().unwrap(), 1);

    // ONE assembled group in the parent comp.
    let comp = s.project.comp(CompId(1)).unwrap();
    assert_eq!(comp.layer_order.len(), 1);
    let group = &comp.layers[&comp.layer_order[0]];
    let LayerKind::PreComp { comp: child_id } = &group.kind else {
        panic!("expected an assembled PreComp group");
    };
    assert_eq!(
        group.transform.scale,
        [100.0, 100.0],
        "400×300 fits a 400×300 comp"
    );
    let child = s.project.comp(*child_id).expect("sub comp");
    assert_eq!(child.width, 400);
    assert_eq!(child.height, 300);
    assert_eq!(child.layer_order.len(), 1, "text skipped with a report");
    assert_eq!(reply["skipped"].as_u64().unwrap(), 1);
    assert!(reply["skippedKinds"].as_str().unwrap().contains("text"));

    // The ball carries REAL animation: 3 position keys, 1 s apart.
    let ball = &child.layers[&child.layer_order[0]];
    assert_eq!(ball.transform.opacity, 0.9);
    let track = ball.tracks.get(&Property::Position).expect("position keys");
    assert_eq!(track.keys.len(), 3);
    assert_eq!(track.keys[0].value, PropValue::Vec2([-100.0, -90.0]));
    assert_eq!(track.keys[1].value, PropValue::Vec2([100.0, 90.0]));
    assert_eq!(track.keys[1].time.0, TICKS_PER_SEC);
    assert!(matches!(track.keys[0].easing, Easing::Bezier { .. }));

    // The engine renders the imported animation at both ends.
    let frames = decode_embedded_frames(&s.project).unwrap();
    for t in [Time::ZERO, Time(TICKS_PER_SEC)] {
        let rendered = render_comp(&s.project, CompId(1), t, &frames);
        assert!(rendered.is_ok(), "render at {:?}: {:?}", t, rendered.err());
    }
}

#[test]
fn lottie_rejects_non_lottie_and_reports_atomically() {
    let mut s = session();
    let before = s.command("save_project", json!({})).unwrap().to_string();
    assert!(s
        .command(
            "import_lottie",
            json!({"name": "x.json", "json": "{\"a\":1}", "compId": 1})
        )
        .is_err());
    assert_eq!(
        s.command("save_project", json!({})).unwrap().to_string(),
        before,
        "failed import must not mutate"
    );
}

#[test]
fn vault_round_trip_through_commands() {
    let dir = std::env::temp_dir().join(format!(
        "bonaparte-vault-cmd-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::env::set_var("BONAPARTE_VAULT", &dir);
    let mut s = session();
    let b64 = "aGVsbG8gdmF1bHQ="; // "hello vault"
    let reply = s
        .command(
            "vault_save",
            json!({"folder": "logos", "name": "acme.svg", "dataBase64": b64}),
        )
        .unwrap();
    assert_eq!(reply["saved"], "acme.svg");
    let listing = s.command("vault_list", json!({})).unwrap();
    let logos = listing["folders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "logos")
        .unwrap();
    assert_eq!(logos["entries"].as_array().unwrap().len(), 1);
    let read = s
        .command("vault_read", json!({"folder": "logos", "name": "acme.svg"}))
        .unwrap();
    assert_eq!(read["dataBase64"], b64);
    assert!(s
        .command("vault_read", json!({"folder": "logos", "name": "nope.svg"}))
        .is_err());
}
