use bonaparte_engine::{
    render_comp, render_tile, render_viewport, render_viewport_tiles, NoMedia, Rect, Tile,
    TileGrid, TILE_SIZE,
};
use bonaparte_model::{FrameRate, Layer, LayerKind, Project, StaticTransform, Time};

#[test]
fn test_tile_grid_geometry_and_counts() {
    let grid_4k = TileGrid::new(3840, 2160);
    assert_eq!(grid_4k.grid_width(), 15);
    assert_eq!(grid_4k.grid_height(), 9);
    assert_eq!(grid_4k.tile_count(), 135);

    let grid_small = TileGrid::new(100, 100);
    assert_eq!(grid_small.grid_width(), 1);
    assert_eq!(grid_small.grid_height(), 1);
    assert_eq!(grid_small.tile_count(), 1);
    let rect = grid_small.tile_rect(Tile { x: 0, y: 0 });
    assert_eq!(rect, Rect::new(0.0, 0.0, 100.0, 100.0));
}

#[test]
fn test_tile_ram_bound_and_render_tile_execution() {
    let mut p = Project::new("Tile Memory Test");
    let c = p.create_comp("big_canvas", 3840, 2160, FrameRate::FPS_30, Time(100));
    p.comps.get_mut(&c).unwrap().background = [0.1, 0.2, 0.3, 1.0];

    // Center red solid
    let mut layer = Layer::new(
        "red",
        LayerKind::Solid {
            color: [1.0, 0.0, 0.0, 1.0],
        },
        Time::ZERO,
        Time(100),
    );
    layer.transform = StaticTransform::default();
    p.insert_layer(c, layer);

    // Render a single tile: memory is guaranteed 256x256x4 bytes = 256KB, NOT a 32MB 4K buffer!
    let tile_frame = render_tile(&p, c, Tile { x: 2, y: 2 }, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(tile_frame.width, TILE_SIZE);
    assert_eq!(tile_frame.height, TILE_SIZE);
    assert_eq!(tile_frame.rgba.len(), (256 * 256 * 4) as usize);

    // Test render_viewport_tiles directly
    let vp_tiles = render_viewport_tiles(&p, c, 512, 512, 256, 256, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(vp_tiles.len(), 1);
    assert_eq!(vp_tiles[0].tile, Tile { x: 2, y: 2 });

    // Pixel inside tile must be red
    let px = tile_frame.pixel(128, 128);
    assert!(px[0] > 0.9, "Tile pixel should be red");
}

#[test]
fn test_viewport_stitching_matches_full_frame() {
    let mut p = Project::new("Stitch Test");
    let c = p.create_comp("comp", 600, 400, FrameRate::FPS_30, Time(100));
    p.comps.get_mut(&c).unwrap().background = [0.05, 0.05, 0.05, 1.0];

    // Layer 1: Green shape at center (300, 200) of size 200x200
    let mut green = Layer::new_rect(
        "green",
        [0.0, 1.0, 0.0, 1.0],
        Time::ZERO,
        Time(100),
    );
    green.transform.scale = [33.33, 50.0];
    p.insert_layer(c, green);

    // Render viewport covering entire canvas (0, 0, 600, 400)
    let full_frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    let viewport_frame = render_viewport(&p, c, 0, 0, 600, 400, Time::ZERO, &NoMedia).unwrap();

    assert_eq!(full_frame.width, viewport_frame.width);
    assert_eq!(full_frame.height, viewport_frame.height);

    // Check pixel-by-pixel equivalence across multiple sample points
    for y in (0..400).step_by(25) {
        for x in (0..600).step_by(25) {
            let p1 = full_frame.pixel(x, y);
            let p2 = viewport_frame.pixel(x, y);
            for ch in 0..4 {
                assert!(
                    (p1[ch] - p2[ch]).abs() < 0.02,
                    "Pixel mismatch at ({x}, {y}) ch {ch}: full={:?} viewport={:?}",
                    p1,
                    p2
                );
            }
        }
    }
}
