use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use super::build_children_map;
use crate::control_flow::{ControlFlowVisualization, Edge, Node, NodeId, NodeType};

/// Pass 1: remove implicit nodes that do not contribute headerized work.
pub fn remove_implicit_nodes(viz: &ControlFlowVisualization) -> ControlFlowVisualization {
    let children = build_children_map(&viz.nodes);
    let mut memo: HashMap<NodeId, bool> = HashMap::new();
    for node in viz.nodes.values() {
        compute_has_header(node.id, &viz.nodes, &children, &mut memo);
    }

    let mut keep: HashSet<NodeId> = HashSet::new();
    for node in viz.nodes.values() {
        if should_keep(node, &viz.nodes, &memo) {
            keep.insert(node.id);
        }
    }

    filter_viz(viz, &keep)
}

fn compute_has_header(
    node_id: NodeId,
    nodes: &IndexMap<NodeId, Node>,
    children: &HashMap<NodeId, Vec<NodeId>>,
    memo: &mut HashMap<NodeId, bool>,
) -> bool {
    if let Some(value) = memo.get(&node_id) {
        return *value;
    }

    let node = match nodes.get(&node_id) {
        Some(node) => node,
        None => {
            memo.insert(node_id, false);
            return false;
        }
    };

    let mut result = matches!(node.node_type, NodeType::HeaderContextEnter);
    if let Some(child_ids) = children.get(&node_id) {
        for child in child_ids {
            if compute_has_header(*child, nodes, children, memo) {
                result = true;
                break;
            }
        }
    }

    memo.insert(node_id, result);
    result
}

fn should_keep(
    node: &Node,
    nodes: &IndexMap<NodeId, Node>,
    has_header: &HashMap<NodeId, bool>,
) -> bool {
    match node.node_type {
        NodeType::FunctionRoot | NodeType::HeaderContextEnter => true,
        NodeType::BranchArm => {
            if *has_header.get(&node.id).unwrap_or(&false) {
                true
            } else if let Some(parent_id) = node.parent_node_id {
                matches!(
                    nodes.get(&parent_id).map(|parent| &parent.node_type),
                    Some(NodeType::BranchGroup)
                ) && *has_header.get(&parent_id).unwrap_or(&false)
            } else {
                false
            }
        }
        _ => *has_header.get(&node.id).unwrap_or(&false),
    }
}

fn filter_viz(viz: &ControlFlowVisualization, keep: &HashSet<NodeId>) -> ControlFlowVisualization {
    let mut nodes = IndexMap::new();
    for (id, node) in viz.nodes.iter() {
        if keep.contains(id) {
            nodes.insert(*id, node.clone());
        }
    }

    let mut edges_by_src: IndexMap<NodeId, Vec<Edge>> = IndexMap::new();
    for (src, edges) in viz.edges_by_src.iter() {
        if !keep.contains(src) {
            continue;
        }
        let filtered: Vec<Edge> = edges
            .iter()
            .filter(|edge| keep.contains(&edge.dst))
            .cloned()
            .collect();
        if !filtered.is_empty() {
            edges_by_src.insert(*src, filtered);
        }
    }

    ControlFlowVisualization {
        nodes,
        edges_by_src,
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ast::Span;

    use super::*;
    use crate::control_flow::{Node, NodeType};

    fn make_node(id: u32, parent: Option<u32>, label: &str, node_type: NodeType) -> Node {
        let parent_id = parent.map(NodeId::new);
        Node {
            id: NodeId::new(id),
            parent_node_id: parent_id,
            log_filter_key: format!("f|{id}"),
            label: label.to_string(),
            span: Span::fake(),
            node_type,
        }
    }

    #[test]
    fn drops_branch_group_without_headers() {
        let mut viz = ControlFlowVisualization::default();
        let root = Node::root(NodeId::new(0), "f|root:0".to_string(), Span::fake(), "root");
        let header = make_node(1, Some(0), "header", NodeType::HeaderContextEnter);
        let branch_group = make_node(2, Some(1), "if", NodeType::BranchGroup);
        let branch_arm = make_node(3, Some(2), "arm1", NodeType::BranchArm);
        viz.nodes.insert(root.id, root);
        viz.nodes.insert(header.id, header.clone());
        viz.nodes.insert(branch_group.id, branch_group);
        viz.nodes.insert(branch_arm.id, branch_arm);
        let filtered = remove_implicit_nodes(&viz);
        assert!(filtered.nodes.contains_key(&header.id));
        assert!(!filtered.nodes.contains_key(&NodeId::new(2)));
        assert!(!filtered.nodes.contains_key(&NodeId::new(3)));
    }

    #[test]
    fn keeps_all_branch_arms_when_one_has_header() {
        let mut viz = ControlFlowVisualization::default();
        let root = Node::root(NodeId::new(0), "f|root:0".to_string(), Span::fake(), "root");
        let header = make_node(1, Some(0), "header", NodeType::HeaderContextEnter);
        let branch_group = make_node(2, Some(1), "if", NodeType::BranchGroup);
        let arm_with_header = make_node(3, Some(2), "arm-with", NodeType::BranchArm);
        let arm_without_header = make_node(4, Some(2), "arm-without", NodeType::BranchArm);
        let nested_header = make_node(5, Some(3), "inner", NodeType::HeaderContextEnter);

        viz.nodes.insert(root.id, root);
        viz.nodes.insert(header.id, header);
        viz.nodes.insert(branch_group.id, branch_group);
        viz.nodes.insert(arm_with_header.id, arm_with_header);
        viz.nodes.insert(arm_without_header.id, arm_without_header);
        viz.nodes.insert(nested_header.id, nested_header);

        let filtered = remove_implicit_nodes(&viz);
        assert!(filtered.nodes.contains_key(&NodeId::new(2)));
        assert!(filtered.nodes.contains_key(&NodeId::new(3)));
        assert!(filtered.nodes.contains_key(&NodeId::new(4)));
    }
}
