use bonaparte_engine::{render_comp, FrameView, MediaFrames, RenderError};
use bonaparte_model::{FrameRate, Layer, LayerKind, MediaId, Project, Time};

struct TestMedia;
impl MediaFrames for TestMedia {
    fn frame_rgba(&self, media: MediaId, _time: Time) -> Option<FrameView<'_>> {
        if media == MediaId(42) {
            static PIXELS: [u8; 16] = [
                0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255,
            ];
            Some(FrameView {
                width: 2,
                height: 2,
                rgba: &PIXELS,
            })
        } else {
            None
        }
    }
}

#[test]
fn test_all_five_layer_kinds_render() {
    let mut p = Project::new("All Layers");

    // 1. Child Comp for PreComp
    let child_comp = p.create_comp("child", 20, 20, FrameRate::FPS_30, Time(100));
    p.comps.get_mut(&child_comp).unwrap().background = [0.0, 1.0, 1.0, 1.0]; // Cyan background

    // 2. Main Comp
    let main_comp = p.create_comp("main", 100, 100, FrameRate::FPS_30, Time(100));
    p.comps.get_mut(&main_comp).unwrap().background = [0.0, 0.0, 0.0, 1.0];

    // Layer 1: Solid (Blue)
    let solid = Layer::new(
        "solid",
        LayerKind::Solid {
            color: [0.0, 0.0, 1.0, 1.0],
        },
        Time::ZERO,
        Time(100),
    );
    p.insert_layer(main_comp, solid);

    // Layer 2: Shape (Yellow rectangle in center)
    let mut shape = Layer::new_rect("shape", [1.0, 1.0, 0.0, 1.0], Time::ZERO, Time(100));
    shape.transform.scale = [30.0, 30.0]; // 30x30 in center
    p.insert_layer(main_comp, shape);

    // Layer 3: Footage (Green 2x2 image)
    let mut footage = Layer::new(
        "footage",
        LayerKind::Footage { media: MediaId(42) },
        Time::ZERO,
        Time(100),
    );
    footage.transform.position = [25.0, 0.0]; // shift right
    footage.transform.scale = [10.0, 10.0];
    p.insert_layer(main_comp, footage);

    // Layer 4: Text ("HI")
    let mut text_layer = Layer::new(
        "text",
        LayerKind::Text {
            style: Default::default(),
            text: "HI".into(),
            size: 16.0,
        },
        Time::ZERO,
        Time(100),
    );
    text_layer.transform.position = [-25.0, 0.0]; // shift left
    p.insert_layer(main_comp, text_layer);

    // Layer 5: PreComp
    let mut precomp_layer = Layer::new(
        "precomp",
        LayerKind::PreComp { comp: child_comp },
        Time::ZERO,
        Time(100),
    );
    precomp_layer.transform.position = [0.0, 25.0]; // shift down
    precomp_layer.transform.scale = [20.0, 20.0];
    p.insert_layer(main_comp, precomp_layer);

    let frame = render_comp(&p, main_comp, Time::ZERO, &TestMedia).unwrap();
    assert_eq!(frame.width, 100);
    assert_eq!(frame.height, 100);

    // Corner (0, 0) sees base solid blue
    let corner = frame.pixel(0, 0);
    assert!(corner[2] > 0.9, "Corner should be solid blue");

    // Center (50, 50) sees yellow shape
    let center = frame.pixel(50, 50);
    assert!(
        center[0] > 0.9 && center[1] > 0.9,
        "Center should be yellow"
    );

    // Down at (50, 75) sees child comp cyan (green + blue)
    let precomp_px = frame.pixel(50, 75);
    assert!(
        precomp_px[1] > 0.9 && precomp_px[2] > 0.9,
        "PreComp region should be cyan"
    );
}

#[test]
fn test_precomp_depth_limit() {
    let mut p = Project::new("Deep PreComps");
    let mut last_comp = p.create_comp("C0", 10, 10, FrameRate::FPS_30, Time(100));

    // Create a chain of 40 nested precomps (limit is 32)
    for i in 1..=40 {
        let comp = p.create_comp(format!("C{i}"), 10, 10, FrameRate::FPS_30, Time(100));
        let layer = Layer::new(
            format!("L{i}"),
            LayerKind::PreComp { comp: last_comp },
            Time::ZERO,
            Time(100),
        );
        p.insert_layer(comp, layer);
        last_comp = comp;
    }

    let res = render_comp(&p, last_comp, Time::ZERO, &TestMedia);
    assert!(matches!(res, Err(RenderError::PreCompDepthLimit(_))));
}
