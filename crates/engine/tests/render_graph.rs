use bonaparte_engine::{GraphError, NodeKind, RenderGraph};
use bonaparte_model::{BlendMode, FrameRate, Layer, LayerKind, Project, Time};

#[test]
fn test_render_graph_deterministic_tie_breaking() {
    let mut g = RenderGraph::new();
    // Add 4 independent source nodes
    let n0 = g.add_node(NodeKind::Pass { name: "n0".into() }, vec![]);
    let n1 = g.add_node(NodeKind::Pass { name: "n1".into() }, vec![]);
    let n2 = g.add_node(NodeKind::Pass { name: "n2".into() }, vec![]);
    let n3 = g.add_node(NodeKind::Pass { name: "n3".into() }, vec![]);

    let order1 = g.topological_order().unwrap();
    let order2 = g.topological_order().unwrap();

    // Determinism check: same graph must produce exact same topological order
    assert_eq!(order1, order2);
    // Tie breaking rule: smallest NodeId first
    assert_eq!(order1, vec![n0, n1, n2, n3]);
}

#[test]
fn test_render_graph_cycle_detection() {
    let mut g = RenderGraph::new();
    let n0 = g.add_node(NodeKind::Pass { name: "0".into() }, vec![]);
    let n1 = g.add_node(NodeKind::Pass { name: "1".into() }, vec![]);
    let n2 = g.add_node(NodeKind::Pass { name: "2".into() }, vec![]);

    // 0 -> 1 -> 2 -> 0 (cycle)
    g.connect(n0, n1).unwrap();
    g.connect(n1, n2).unwrap();
    g.connect(n2, n0).unwrap();

    let res = g.topological_order();
    assert_eq!(res, Err(GraphError::Cycle));
}

#[test]
fn test_build_from_comp_with_complex_hierarchy() {
    let mut p = Project::new("DAG Test");
    let c = p.create_comp("main", 1920, 1080, FrameRate::FPS_30, Time(100));

    // L1: Base background
    let l1 = p.insert_layer(
        c,
        Layer::new(
            "bg",
            LayerKind::Solid { color: [0.1; 4] },
            Time::ZERO,
            Time(100),
        ),
    );

    // L2: Parent layer
    let mut l2 = Layer::new(
        "parent",
        LayerKind::Solid { color: [0.5; 4] },
        Time::ZERO,
        Time(100),
    );
    l2.blend_mode = BlendMode::Screen;
    let l2_id = p.insert_layer(c, l2);

    // L3: Child layer parented to L2
    let mut l3 = Layer::new_rect("child", [1.0; 4], Time::ZERO, Time(100));
    l3.parent = Some(l2_id);
    l3.blend_mode = BlendMode::Add;
    let l3_id = p.insert_layer(c, l3);

    let graph = RenderGraph::build_from_comp(&p, c, Time::ZERO).unwrap();
    let order = graph.topological_order().unwrap();

    // Verify all nodes are accounted for in topological order
    assert_eq!(order.len(), graph.len());

    let _ = (l1, l3_id);
}
