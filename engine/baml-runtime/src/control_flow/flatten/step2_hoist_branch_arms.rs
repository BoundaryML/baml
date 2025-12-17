use std::collections::HashSet;

use super::{build_children_map, node_depth};
use crate::control_flow::{ControlFlowVisualization, Edge, NodeId, NodeType};

/// Pass 2: hoist BranchArm nodes and wire their edges directly.
pub fn hoist_branch_arms(viz: &ControlFlowVisualization) -> ControlFlowVisualization {
    let children = build_children_map(&viz.nodes);
    let mut next = viz.clone();

    #[derive(Clone)]
    struct BranchGroupInfo {
        node_id: NodeId,
        parent: Option<NodeId>,
        depth: usize,
        branch_children: Vec<NodeId>,
        successors: Vec<NodeId>,
    }

    let mut groups: Vec<BranchGroupInfo> = viz
        .nodes
        .values()
        .filter_map(|node| {
            if !matches!(node.node_type, NodeType::BranchGroup) {
                return None;
            }

            let branch_children: Vec<NodeId> = children
                .get(&node.id)
                .into_iter()
                .flat_map(|list| list.iter().copied())
                .filter(|child_id| {
                    viz.nodes
                        .get(child_id)
                        .map(|child| matches!(child.node_type, NodeType::BranchArm))
                        .unwrap_or(false)
                })
                .collect();

            if branch_children.is_empty() {
                return None;
            }

            let successors: Vec<NodeId> = viz
                .edges_by_src
                .get(&node.id)
                .map(|edges| edges.iter().map(|edge| edge.dst).collect())
                .unwrap_or_default();

            Some(BranchGroupInfo {
                node_id: node.id,
                parent: node.parent_node_id,
                depth: node_depth(node.id, &viz.nodes),
                branch_children,
                successors,
            })
        })
        .collect();

    groups.sort_by(|a, b| b.depth.cmp(&a.depth));

    for info in groups {
        // Step 2: move outgoing edges from the branch group onto each arm.
        next.edges_by_src.shift_remove(&info.node_id);
        if !info.successors.is_empty() {
            for child in &info.branch_children {
                let entry = next.edges_by_src.entry(*child).or_default();
                let mut existing: HashSet<NodeId> = entry.iter().map(|edge| edge.dst).collect();
                for succ in &info.successors {
                    if existing.insert(*succ) {
                        entry.push(Edge {
                            src: *child,
                            dst: *succ,
                        });
                    }
                }
            }
        }

        // Step 3: hoist branch arms and create BranchGroup -> BranchArm edges.
        for child in &info.branch_children {
            if let Some(node) = next.nodes.get_mut(child) {
                node.parent_node_id = info.parent;
            }
        }

        let mut group_edges: Vec<Edge> = Vec::new();
        for child in &info.branch_children {
            group_edges.push(Edge {
                src: info.node_id,
                dst: *child,
            });
        }
        next.edges_by_src.insert(info.node_id, group_edges);
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
            log_filter_key: "f|bg".to_string(),
            label: "if".to_string(),
            span: Span::fake(),
            node_type: NodeType::BranchGroup,
        };
        let arm1 = Node {
            id: NodeId::new(2),
            parent_node_id: Some(group.id),
            log_filter_key: "f|arm1".to_string(),
            label: "arm1".to_string(),
            span: Span::fake(),
            node_type: NodeType::BranchArm,
        };
        let arm2 = Node {
            id: NodeId::new(3),
            parent_node_id: Some(group.id),
            log_filter_key: "f|arm2".to_string(),
            label: "arm2".to_string(),
            span: Span::fake(),
            node_type: NodeType::BranchArm,
        };
        let after = Node {
            id: NodeId::new(4),
            parent_node_id: Some(root.id),
            log_filter_key: "f|after".to_string(),
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
        let expanded = hoist_branch_arms(&viz);
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

        let arm1 = expanded.nodes.get(&NodeId::new(2)).expect("arm1 node");
        let arm2 = expanded.nodes.get(&NodeId::new(3)).expect("arm2 node");
        assert_eq!(arm1.parent_node_id, Some(NodeId::new(0)));
        assert_eq!(arm2.parent_node_id, Some(NodeId::new(0)));
        let group = expanded.nodes.get(&NodeId::new(1)).expect("group node");
        assert_eq!(group.parent_node_id, Some(NodeId::new(0)));
    }

    #[test]
    fn branch_group_without_successors_has_no_arm_edges() {
        let mut viz = branch_group_viz();
        viz.edges_by_src.shift_remove(&NodeId::new(1));

        let expanded = hoist_branch_arms(&viz);

        let group_edges = expanded
            .edges_by_src
            .get(&NodeId::new(1))
            .expect("branch group edges");
        let dsts: Vec<_> = group_edges.iter().map(|edge| edge.dst.raw()).collect();
        assert_eq!(dsts, vec![2, 3]);

        assert!(expanded
            .edges_by_src
            .get(&NodeId::new(2))
            .map(|edges| edges.is_empty())
            .unwrap_or(true));
        assert!(expanded
            .edges_by_src
            .get(&NodeId::new(3))
            .map(|edges| edges.is_empty())
            .unwrap_or(true));
    }
}
