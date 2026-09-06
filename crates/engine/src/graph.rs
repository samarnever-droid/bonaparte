//! The render graph: a dependency-sorted set of passes (~500 lines by
//! design — ARCHITECTURE.md: owned, not adopted, so the memory architecture
//! lives in our code).
//!
//! A node is a unit of GPU work: a source (layer pixels) or an effect pass
//! (shader + params). Edges flow from inputs to consumers. Evaluation order
//! is a topological sort; cycles are a hard error.

use bonaparte_model::{BlendMode, CompId, LayerId, LayerKind, Project, Property, Time};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// What a node does. `Source` produces pixels (layer raster / footage frame /
/// generator plugin output); `Effect` transforms its inputs into one output;
/// `Composite` combines two inputs via a blend mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Source {
        /// Which layer produced this, for cache invalidation.
        layer: LayerId,
    },
    PreComp {
        /// Nested composition source.
        comp: CompId,
        time: Time,
    },
    Effect {
        /// Plugin manifest id, e.g. "builtin.glow".
        effect: String,
        /// Parameter values for this node instance, by param id.
        params: Vec<(String, EffectParamValue)>,
    },
    Composite {
        layer: LayerId,
        blend_mode: BlendMode,
    },
    Pass {
        name: String,
    },
}

/// Runtime value for one effect parameter (mirrors `ParamKind` payloads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectParamValue {
    Slider(f32),
    Color([f32; 4]),
    Point([f32; 2]),
    Checkbox(bool),
    Dropdown(usize),
}

#[derive(Debug, Clone, thiserror::Error, PartialEq)]
pub enum GraphError {
    #[error("cycle detected in render graph")]
    Cycle,
    #[error("node {consumer:?} references missing input {input:?}")]
    MissingInput { consumer: NodeId, input: NodeId },
    #[error("composition {0:?} not found")]
    CompNotFound(CompId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    /// Nodes whose outputs this node consumes, in sampling order.
    pub inputs: Vec<NodeId>,
    /// Properties of the source layer this node depends on — the cache
    /// invalidation set (evaluate → track dirty → re-render this node only).
    pub depends_on: Vec<Property>,
}

/// A dependency-sorted render graph for one frame of one comp.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RenderGraph {
    nodes: Vec<Node>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_id(&self) -> NodeId {
        NodeId(self.nodes.len() as u32)
    }

    pub fn add_node(&mut self, kind: NodeKind, depends_on: Vec<Property>) -> NodeId {
        let id = self.next_id();
        self.nodes.push(Node {
            id,
            kind,
            inputs: Vec::new(),
            depends_on,
        });
        id
    }

    /// Wire `input` into `consumer`'s input list. Validates existence here
    /// so `topological_order` can assume a well-formed edge set.
    pub fn connect(&mut self, input: NodeId, consumer: NodeId) -> Result<(), GraphError> {
        if input.0 as usize >= self.nodes.len() || consumer.0 as usize >= self.nodes.len() {
            return Err(GraphError::MissingInput { consumer, input });
        }
        self.nodes[consumer.0 as usize].inputs.push(input);
        Ok(())
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Kahn's algorithm with deterministic tie-breaking (BTreeSet of smallest NodeId first).
    /// Returns the topologically sorted NodeIds or GraphError::Cycle if a cycle exists.
    pub fn topological_order(&self) -> Result<Vec<NodeId>, GraphError> {
        let n = self.nodes.len();
        let mut indegree = vec![0usize; n];
        let mut consumers: Vec<Vec<usize>> = vec![Vec::new(); n];

        for node in &self.nodes {
            for &input in &node.inputs {
                if input.0 as usize >= n {
                    return Err(GraphError::MissingInput {
                        consumer: node.id,
                        input,
                    });
                }
                indegree[node.id.0 as usize] += 1;
                consumers[input.0 as usize].push(node.id.0 as usize);
            }
        }

        let mut ready: BTreeSet<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);

        while let Some(&i) = ready.iter().next() {
            ready.remove(&i);
            order.push(NodeId(i as u32));
            for &c in &consumers[i] {
                indegree[c] -= 1;
                if indegree[c] == 0 {
                    ready.insert(c);
                }
            }
        }

        if order.len() != n {
            return Err(GraphError::Cycle);
        }
        Ok(order)
    }

    /// Automated DAG builder from `Comp` layers, transforms, blend modes, and effects at `time`.
    pub fn build_from_comp(
        project: &Project,
        comp_id: CompId,
        time: Time,
    ) -> Result<Self, GraphError> {
        let comp = project
            .comp(comp_id)
            .ok_or(GraphError::CompNotFound(comp_id))?;
        let mut graph = Self::new();

        // 1. Cycle detection on parent hierarchy
        for layer in comp.layers.values() {
            if comp.has_parent_cycle(layer.id, layer.parent) {
                return Err(GraphError::Cycle);
            }
        }

        // 2. Base canvas background clear node
        let mut current_backdrop = graph.add_node(
            NodeKind::Pass {
                name: "background_clear".into(),
            },
            vec![],
        );

        let mut layer_source_nodes: BTreeMap<LayerId, NodeId> = BTreeMap::new();

        // 3. Create source nodes for visible layers
        for &layer_id in &comp.layer_order {
            let layer = &comp.layers[&layer_id];
            if !layer.visible_at(time) {
                continue;
            }

            let source_node = match &layer.kind {
                LayerKind::PreComp { comp: child_id } => {
                    // Check self-reference cycle
                    if *child_id == comp_id {
                        return Err(GraphError::Cycle);
                    }
                    let child_time = time - layer.start;
                    graph.add_node(
                        NodeKind::PreComp {
                            comp: *child_id,
                            time: child_time,
                        },
                        vec![
                            Property::Position,
                            Property::Scale,
                            Property::Rotation,
                            Property::Opacity,
                        ],
                    )
                }
                _ => graph.add_node(
                    NodeKind::Source { layer: layer.id },
                    vec![
                        Property::Position,
                        Property::Scale,
                        Property::Rotation,
                        Property::Opacity,
                    ],
                ),
            };

            layer_source_nodes.insert(layer.id, source_node);
        }

        // 4. Wire transform hierarchy (parent layer -> child layer)
        for &layer_id in &comp.layer_order {
            let layer = &comp.layers[&layer_id];
            if !layer.visible_at(time) {
                continue;
            }
            if let Some(parent_id) = layer.parent {
                if let (Some(&parent_node), Some(&child_node)) = (
                    layer_source_nodes.get(&parent_id),
                    layer_source_nodes.get(&layer_id),
                ) {
                    graph.connect(parent_node, child_node)?;
                }
            }
        }

        // 5. Wire compositing chain in bottom-to-top order
        for &layer_id in &comp.layer_order {
            let layer = &comp.layers[&layer_id];
            if !layer.visible_at(time) {
                continue;
            }
            let mut source_node = if matches!(layer.kind, LayerKind::Adjustment {}) {
                current_backdrop
            } else {
                layer_source_nodes[&layer.id]
            };
            for effect in layer.effects.iter().filter(|e| e.enabled) {
                let params = effect
                    .evaluated_params(time)
                    .into_iter()
                    .map(|(key, value)| {
                        use bonaparte_model::EffectValue as V;
                        let value = match value {
                            V::Float(v) => EffectParamValue::Slider(v),
                            V::Color(v) => EffectParamValue::Color(v),
                            V::Point(v) => EffectParamValue::Point(v),
                            V::Bool(v) => EffectParamValue::Checkbox(v),
                            V::Index(v) => EffectParamValue::Dropdown(v),
                        };
                        (key, value)
                    })
                    .collect();
                let node = graph.add_node(
                    NodeKind::Effect {
                        effect: effect.effect_id.clone(),
                        params,
                    },
                    vec![],
                );
                graph.connect(source_node, node)?;
                source_node = node;
            }

            // Composite node takes backdrop and layer source as inputs
            let comp_node = graph.add_node(
                NodeKind::Composite {
                    layer: layer.id,
                    blend_mode: layer.blend_mode,
                },
                vec![Property::Opacity],
            );

            graph.connect(current_backdrop, comp_node)?;
            graph.connect(source_node, comp_node)?;
            current_backdrop = comp_node;
        }

        // 6. Verify DAG is acyclic and topologically sound
        graph.topological_order()?;

        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_orders_inputs_before_consumers() {
        let mut g = RenderGraph::new();
        let src = g.add_node(
            NodeKind::Source {
                layer: bonaparte_model::LayerId(1),
            },
            vec![Property::Opacity],
        );
        let blur = g.add_node(
            NodeKind::Effect {
                effect: "builtin.blur".into(),
                params: vec![("radius".into(), EffectParamValue::Slider(8.0))],
            },
            vec![],
        );
        let glow = g.add_node(
            NodeKind::Effect {
                effect: "builtin.glow".into(),
                params: vec![],
            },
            vec![],
        );
        g.connect(src, blur).unwrap();
        g.connect(blur, glow).unwrap();
        let order = g.topological_order().unwrap();
        let pos = |id: NodeId| order.iter().position(|&n| n == id).unwrap();
        assert!(pos(src) < pos(blur));
        assert!(pos(blur) < pos(glow));
    }

    #[test]
    fn cycle_is_detected_not_hung() {
        let mut g = RenderGraph::new();
        let a = g.add_node(
            NodeKind::Effect {
                effect: "a".into(),
                params: vec![],
            },
            vec![],
        );
        let b = g.add_node(
            NodeKind::Effect {
                effect: "b".into(),
                params: vec![],
            },
            vec![],
        );
        g.connect(a, b).unwrap();
        g.connect(b, a).unwrap();
        assert!(matches!(g.topological_order(), Err(GraphError::Cycle)));
    }

    #[test]
    fn build_from_comp_creates_valid_topological_dag() {
        let mut p = Project::new("Comp Graph Test");
        let c = p.create_comp(
            "main",
            1920,
            1080,
            bonaparte_model::FrameRate::FPS_30,
            Time(100),
        );

        let l1 = p.insert_layer(
            c,
            bonaparte_model::Layer::new(
                "l1",
                LayerKind::Solid { color: [1.0; 4] },
                Time::ZERO,
                Time(100),
            ),
        );
        let mut l2 = bonaparte_model::Layer::new(
            "l2",
            LayerKind::Solid { color: [0.5; 4] },
            Time::ZERO,
            Time(100),
        );
        l2.parent = Some(l1);
        l2.blend_mode = BlendMode::Multiply;
        let l2_id = p.insert_layer(c, l2);

        let g = RenderGraph::build_from_comp(&p, c, Time::ZERO).unwrap();
        let order = g.topological_order().unwrap();

        assert!(!order.is_empty());
        let _ = l2_id;
    }
}
