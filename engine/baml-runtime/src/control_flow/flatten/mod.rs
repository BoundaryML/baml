use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::control_flow::{ControlFlowVisualization, Edge, Node, NodeId};

mod step1_remove_implicit;
mod step2_hoist_branch_arms;
mod step3_inline_implicit_nodes;

pub use step1_remove_implicit::remove_implicit_nodes;
pub use step2_hoist_branch_arms::hoist_branch_arms;
pub use step3_inline_implicit_nodes::inline_branch_arms_and_scopes;

/// Result of flattening a `ControlFlowVisualization`.
#[derive(Clone, Debug, Default)]
pub struct FlattenedControlFlowVisualization {
    pub nodes: IndexMap<NodeId, Node>,
    pub edges_by_src: IndexMap<NodeId, Vec<Edge>>,
}

impl From<ControlFlowVisualization> for FlattenedControlFlowVisualization {
    fn from(value: ControlFlowVisualization) -> Self {
        Self {
            nodes: value.nodes,
            edges_by_src: value.edges_by_src,
        }
    }
}

impl From<FlattenedControlFlowVisualization> for ControlFlowVisualization {
    fn from(value: FlattenedControlFlowVisualization) -> Self {
        ControlFlowVisualization {
            nodes: value.nodes,
            edges_by_src: value.edges_by_src,
        }
    }
}

/// Run the three-pass flattening pipeline described in goal5b.
pub fn flatten_control_flow(viz: &ControlFlowVisualization) -> FlattenedControlFlowVisualization {
    let pass_one = remove_implicit_nodes(viz);
    let pass_two = hoist_branch_arms(&pass_one);
    let pass_three = inline_branch_arms_and_scopes(&pass_two);
    pass_three.into()
}

pub(crate) fn build_children_map(nodes: &IndexMap<NodeId, Node>) -> HashMap<NodeId, Vec<NodeId>> {
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for node in nodes.values() {
        if let Some(parent) = node.parent_node_id {
            children.entry(parent).or_default().push(node.id);
        }
    }
    children
}

pub(crate) fn collect_subtree(
    root: NodeId,
    children: &HashMap<NodeId, Vec<NodeId>>,
) -> HashSet<NodeId> {
    let mut stack = vec![root];
    let mut result = HashSet::new();
    while let Some(id) = stack.pop() {
        if !result.insert(id) {
            continue;
        }
        if let Some(child_ids) = children.get(&id) {
            stack.extend(child_ids.iter().copied());
        }
    }
    result
}

pub(crate) fn node_depth(node_id: NodeId, nodes: &IndexMap<NodeId, Node>) -> usize {
    let mut depth = 0;
    let mut current = Some(node_id);
    while let Some(id) = current {
        depth += 1;
        current = nodes.get(&id).and_then(|node| node.parent_node_id);
    }
    depth
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ast::Span;

    use super::*;

    fn sample_viz() -> ControlFlowVisualization {
        let mut viz = ControlFlowVisualization::default();
        let root_id = NodeId::new(0);
        viz.nodes.insert(
            root_id,
            Node::root(
                root_id,
                "root|root:0".to_string(),
                Span::fake(),
                "root".to_string(),
            ),
        );
        viz
    }

    #[test]
    fn flatten_pipeline_runs_all_passes() {
        let viz = sample_viz();
        let flattened = flatten_control_flow(&viz);
        assert_eq!(1, flattened.nodes.len());
    }
}
