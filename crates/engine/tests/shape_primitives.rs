use bonaparte_engine::{render_comp, FrameView, MediaFrames};
use bonaparte_model::{FrameRate, Layer, MediaId, Project, Time};

struct NoMedia;
impl MediaFrames for NoMedia {
    fn frame_rgba(&self, _media: MediaId, _time: Time) -> Option<FrameView<'_>> {
        None
    }
}

#[test]
fn test_circle_shape_rasterization() {
    let mut p = Project::new("Circle Test");
    let c = p.create_comp("comp", 100, 100, FrameRate::FPS_30, Time(100));
    p.comps.get_mut(&c).unwrap().background = [0.0, 0.0, 0.0, 1.0]; // Black background

    // Circle of size 60x60 in center (50, 50), radius is 30
    let mut circle = Layer::new_circle("my_circle", [1.0, 0.0, 0.0, 1.0], Time::ZERO, Time(100));
    circle.transform.scale = [60.0, 60.0];
    p.insert_layer(c, circle);

    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();

    // 1. Center of circle at (50, 50) must be pure red
    let center_px = frame.pixel(50, 50);
    assert!(center_px[0] > 0.9, "Circle center (50, 50) should be red, got: {:?}", center_px);
    assert!(center_px[1] < 0.1 && center_px[2] < 0.1);

    // 2. Point at (50, 30) (distance 20 from center <= 30) must be red
    let inside_px = frame.pixel(50, 30);
    assert!(inside_px[0] > 0.9, "Point (50, 30) inside circle radius should be red, got: {:?}", inside_px);

    // 3. Corner of the bounding box at (25, 25): distance from center is sqrt(25^2 + 25^2) = 35.35 > 30!
    // In a rectangle this would be inside, but in a CIRCLE it MUST be outside (background black)!
    let corner_px = frame.pixel(25, 25);
    assert!(corner_px[0] < 0.1, "Bounding box corner (25, 25) must be outside circle (black), got: {:?}", corner_px);

    // 4. Outer point (10, 10) must be pure black
    let outer_px = frame.pixel(10, 10);
    assert!(outer_px[0] < 0.1, "Outer point (10, 10) must be black, got: {:?}", outer_px);
}

#[test]
fn test_rounded_rect_shape_rasterization() {
    let mut p = Project::new("Rect Test");
    let c = p.create_comp("comp", 100, 100, FrameRate::FPS_30, Time(100));
    p.comps.get_mut(&c).unwrap().background = [0.0, 0.0, 0.0, 1.0];

    let mut rrect = Layer::new_rect(
        "my_rect",
        [0.0, 1.0, 0.0, 1.0], // Green
        Time::ZERO,
        Time(100),
    );
    rrect.transform.scale = [60.0, 60.0];
    p.insert_layer(c, rrect);

    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();

    // Center (50, 50) must be green
    let center_px = frame.pixel(50, 50);
    assert!(center_px[1] > 0.9, "Center should be green");

    // Corner at (25, 25) is inside the 60x60 rect [20, 80]x[20, 80] -> must be green!
    let corner_px = frame.pixel(25, 25);
    assert!(corner_px[1] > 0.9, "Rect corner at (25, 25) must be green, got: {:?}", corner_px);
}
