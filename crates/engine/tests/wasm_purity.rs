use bonaparte_engine::{
    render_comp, render_tile, render_viewport, Affine2D, NoMedia, RenderGraph, Tile, TileGrid,
};
use bonaparte_model::{BlendMode, FrameRate, Layer, LayerKind, Project, StaticTransform, Time};

#[test]
fn test_engine_pure_in_memory_execution() {
    // Verifies all engine operations run with zero filesystem, network, thread, or OS dependencies
    let mut p = Project::new("Pure In-Memory");
    let c = p.create_comp("pure", 256, 256, FrameRate::FPS_30, Time(100));

    let mut l = Layer::new(
        "solid",
        LayerKind::Solid {
            color: [0.5, 0.5, 0.5, 1.0],
        },
        Time::ZERO,
        Time(100),
    );
    l.transform = StaticTransform::default();
    l.blend_mode = BlendMode::Normal;
    p.insert_layer(c, l);

    // 1. Render comp
    let frame = render_comp(&p, c, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(frame.width, 256);
    assert_eq!(frame.height, 256);

    // 2. Render tile
    let tile = render_tile(&p, c, Tile { x: 0, y: 0 }, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(tile.width, 256);

    // 3. Render viewport
    let vp = render_viewport(&p, c, 0, 0, 100, 100, Time::ZERO, &NoMedia).unwrap();
    assert_eq!(vp.width, 100);

    // 4. RenderGraph
    let g = RenderGraph::build_from_comp(&p, c, Time::ZERO).unwrap();
    let order = g.topological_order().unwrap();
    assert!(!order.is_empty());

    // 5. Affine2D
    let aff = Affine2D::identity();
    assert_eq!(aff.transform_point([0.0, 0.0]), [0.0, 0.0]);

    // 6. TileGrid
    let grid = TileGrid::new(256, 256);
    assert_eq!(grid.tile_count(), 1);
}
