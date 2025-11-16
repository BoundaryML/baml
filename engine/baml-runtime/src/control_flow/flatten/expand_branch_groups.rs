use std::collections::HashSet;

use super::build_children_map;
use crate::control_flow::{ControlFlowVisualization, Edge, NodeId, NodeType};

/// Pass 2: ensure BranchGroup / BranchArm nodes have the correct fan-out edges.
pub fn expand_branch_groups(viz: &ControlFlowVisualization) -> ControlFlowVisualization {
    let children = build_children_map(&viz.nodes);
    let mut next = viz.clone();

    for node in viz.nodes.values() {
        if !matches!(node.node_type, NodeType::BranchGroup) {
            continue;
        }

        let branch_children: Vec<NodeId> = children
            .get(&node.id)
            .into_iter()
            .flat_map(|list| list.iter().copied())
            .filter(|child_id| {
                next.nodes
                    .get(child_id)
                    .map(|child| matches!(child.node_type, NodeType::BranchArm))
                    .unwrap_or(false)
            })
            .collect();

        if branch_children.is_empty() {
            continue;
        }

        let successors: Vec<NodeId> = next
            .edges_by_src
            .get(&node.id)
            .map(|edges| edges.iter().map(|edge| edge.dst).collect())
            .unwrap_or_default();

        let mut new_edges: Vec<Edge> = Vec::new();
        for child in &branch_children {
            new_edges.push(Edge {
                src: node.id,
                dst: *child,
            });
        }
        next.edges_by_src.insert(node.id, new_edges);

        if successors.is_empty() {
            continue;
        }

        for child in branch_children {
            let entry = next.edges_by_src.entry(child).or_default();
            let mut existing: HashSet<NodeId> = entry.iter().map(|edge| edge.dst).collect();
            for succ in &successors {
                if existing.insert(*succ) {
                    entry.push(Edge {
                        src: child,
                        dst: *succ,
                    });
                }
            }
        }
    }

    next
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ast::Span;

    use super::*;
    use crate::control_flow::{Node, NodeType};

    fn branch_group_viz() -> ControlFlowVisualization {
        let mut viz = ControlFlowVisualization::default();
        let root = Node::root(NodeId::new(0), "f|root:0".to_string(), Span::fake(), "root");
        let group = Node {
            id: NodeId::new(1),
            parent_node_id: Some(root.id),
            lexical_id: "f|bg".to_string(),
            label: "if".to_string(),
            span: Span::fake(),
            node_type: NodeType::BranchGroup,
        };
        let arm1 = Node {
            id: NodeId::new(2),
            parent_node_id: Some(group.id),
            lexical_id: "f|arm1".to_string(),
            label: "arm1".to_string(),
            span: Span::fake(),
            node_type: NodeType::BranchArm,
        };
        let arm2 = Node {
            id: NodeId::new(3),
            parent_node_id: Some(group.id),
            lexical_id: "f|arm2".to_string(),
            label: "arm2".to_string(),
            span: Span::fake(),
            node_type: NodeType::BranchArm,
        };
        let after = Node {
            id: NodeId::new(4),
            parent_node_id: Some(root.id),
            lexical_id: "f|after".to_string(),
            label: "after".to_string(),
            span: Span::fake(),
            node_type: NodeType::HeaderContextEnter,
        };
        viz.nodes.insert(root.id, root);
        viz.nodes.insert(group.id, group);
        viz.nodes.insert(arm1.id, arm1);
        viz.nodes.insert(arm2.id, arm2);
        viz.nodes.insert(after.id, after);

        viz.edges_by_src.insert(
            NodeId::new(0),
            vec![Edge {
                src: NodeId::new(0),
                dst: NodeId::new(1),
            }],
        );
        viz.edges_by_src.insert(
            NodeId::new(1),
            vec![Edge {
                src: NodeId::new(1),
                dst: NodeId::new(4),
            }],
        );
        viz
    }

    #[test]
    fn branch_group_edges_are_fanned_out() {
        let viz = branch_group_viz();
        let expanded = expand_branch_groups(&viz);
        let group_edges = expanded
            .edges_by_src
            .get(&NodeId::new(1))
            .expect("branch group edges");
        let dsts: Vec<_> = group_edges.iter().map(|edge| edge.dst.raw()).collect();
        assert_eq!(dsts, vec![2, 3]);

        let arm1_edges = expanded
            .edges_by_src
            .get(&NodeId::new(2))
            .expect("arm edges");
        assert!(arm1_edges.iter().any(|edge| edge.dst == NodeId::new(4)));

        let arm2_edges = expanded
            .edges_by_src
            .get(&NodeId::new(3))
            .expect("arm edges");
        assert!(arm2_edges.iter().any(|edge| edge.dst == NodeId::new(4)));
    }
}
