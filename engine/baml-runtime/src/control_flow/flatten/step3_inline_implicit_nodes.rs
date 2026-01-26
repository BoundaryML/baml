use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use super::build_children_map;
use crate::control_flow::{ControlFlowVisualization, Edge, Node, NodeId, NodeType};

/// Pass 3: inline `BranchArm` and `OtherScope` nodes so headers become siblings.
pub fn inline_branch_arms_and_scopes(viz: &ControlFlowVisualization) -> ControlFlowVisualization {
    let mut next = viz.clone();

    loop {
        let children_map = build_children_map(&next.nodes);
        let candidates = collect_candidates(&next.nodes, &children_map);

        if candidates.is_empty() {
            break;
        }

        let mut changed = false;
        for candidate_id in candidates {
            if !next.nodes.contains_key(&candidate_id) {
                continue;
            }

            let children_map = build_children_map(&next.nodes);
            if inline_node(&mut next, candidate_id, &children_map) {
                changed = true;
            }
        }

        if !changed {
            break;
        }
    }

    next
}

fn inline_node(
    viz: &mut ControlFlowVisualization,
    node_id: NodeId,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
) -> bool {
    let parent = {
        let Some(node) = viz.nodes.get(&node_id) else {
            return false;
        };
        if !matches!(node.node_type, NodeType::BranchArm | NodeType::OtherScope) {
            return false;
        }
        node.parent_node_id
    };

    let direct_children: Vec<NodeId> = children_map
        .get(&node_id)
        .into_iter()
        .flat_map(|children| children.iter().copied())
        .filter(|child_id| viz.nodes.contains_key(child_id))
        .collect();

    if direct_children.is_empty() {
        return false;
    }

    let entry_node = direct_children[0];
    let exit_nodes = collect_exit_nodes(node_id, children_map, viz);

    reparent_children(viz, parent, &direct_children);
    redirect_incoming_edges(viz, node_id, entry_node);
    let outgoing = viz.edges_by_src.shift_remove(&node_id).unwrap_or_default();
    fan_out_outgoing_edges(viz, &exit_nodes, &outgoing);

    viz.nodes.shift_remove(&node_id);
    true
}

fn collect_exit_nodes(
    candidate_id: NodeId,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
    viz: &ControlFlowVisualization,
) -> Vec<NodeId> {
    let mut exits = Vec::new();
    if let Some(children) = children_map.get(&candidate_id) {
        for child in children {
            collect_exit_nodes_recursive(*child, children_map, viz, &mut exits);
        }
    }
    exits
}

fn collect_exit_nodes_recursive(
    node_id: NodeId,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
    viz: &ControlFlowVisualization,
    exits: &mut Vec<NodeId>,
) {
    if let Some(children) = children_map.get(&node_id) {
        for child in children {
            collect_exit_nodes_recursive(*child, children_map, viz, exits);
        }
    }

    let has_outgoing = viz
        .edges_by_src
        .get(&node_id)
        .map(|edges| !edges.is_empty())
        .unwrap_or(false);

    if !has_outgoing {
        exits.push(node_id);
    }
}

fn reparent_children(
    viz: &mut ControlFlowVisualization,
    new_parent: Option<NodeId>,
    children: &[NodeId],
) {
    for child_id in children {
        if let Some(child) = viz.nodes.get_mut(child_id) {
            child.parent_node_id = new_parent;
        }
    }
}

fn redirect_incoming_edges(
    viz: &mut ControlFlowVisualization,
    old_target: NodeId,
    new_target: NodeId,
) {
    for edges in viz.edges_by_src.values_mut() {
        for edge in edges.iter_mut() {
            if edge.dst == old_target {
                edge.dst = new_target;
            }
        }
    }
}

fn fan_out_outgoing_edges(viz: &mut ControlFlowVisualization, exits: &[NodeId], outgoing: &[Edge]) {
    if exits.is_empty() || outgoing.is_empty() {
        return;
    }

    for exit in exits {
        let entry = viz.edges_by_src.entry(*exit).or_default();
        let mut existing: HashSet<NodeId> = entry.iter().map(|edge| edge.dst).collect();
        for edge in outgoing {
            if existing.insert(edge.dst) {
                entry.push(Edge {
                    src: *exit,
                    dst: edge.dst,
                });
            }
        }
    }
}

fn collect_candidates(
    nodes: &IndexMap<NodeId, Node>,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
) -> Vec<NodeId> {
    let mut visited = HashSet::new();
    let mut ordered = Vec::new();

    for node in nodes.values().filter(|node| node.parent_node_id.is_none()) {
        dfs_candidates(node.id, nodes, children_map, &mut visited, &mut ordered);
    }

    for node in nodes.values() {
        if !visited.contains(&node.id) {
            dfs_candidates(node.id, nodes, children_map, &mut visited, &mut ordered);
        }
    }

    ordered
}

fn dfs_candidates(
    node_id: NodeId,
    nodes: &IndexMap<NodeId, Node>,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
    visited: &mut HashSet<NodeId>,
    ordered: &mut Vec<NodeId>,
) {
    if !visited.insert(node_id) {
        return;
    }

    if let Some(children) = children_map.get(&node_id) {
        for child in children {
            dfs_candidates(*child, nodes, children_map, visited, ordered);
        }
    }

    if let Some(node) = nodes.get(&node_id) {
        if matches!(node.node_type, NodeType::BranchArm | NodeType::OtherScope)
            && children_map
                .get(&node_id)
                .map(|children| !children.is_empty())
                .unwrap_or(false)
        {
            ordered.push(node_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ast::Span;

    use super::*;
    use crate::control_flow::{Node, NodeType};

    fn make_node(id: u32, parent: Option<u32>, label: &str, node_type: NodeType) -> Node {
        Node {
            id: NodeId::new(id),
            parent_node_id: parent.map(NodeId::new),
            log_filter_key: format!("f|{id}"),
            label: label.to_string(),
            span: Span::fake(),
            node_type,
        }
    }

    fn add_edge(viz: &mut ControlFlowVisualization, src: u32, dst: u32) {
        viz.edges_by_src
            .entry(NodeId::new(src))
            .or_default()
            .push(Edge {
                src: NodeId::new(src),
                dst: NodeId::new(dst),
            });
    }

    #[test]
    fn inlines_branch_arms_and_other_scope() {
        // Recreate the step-2 graph from goal5b3-inline-implicit-nodes.md
        let mut viz = ControlFlowVisualization::default();
        viz.nodes.insert(
            NodeId::new(0),
            make_node(
                0,
                None,
                "BranchArmsContainSequentialHeaders",
                NodeType::FunctionRoot,
            ),
        );
        viz.nodes.insert(
            NodeId::new(1),
            make_node(1, Some(0), "Enclosing", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(2),
            make_node(2, Some(1), "if (k == 1)", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(3),
            make_node(3, Some(1), "if (k == 1)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(4),
            make_node(
                4,
                Some(3),
                "k == 1 BranchArm first",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(5),
            make_node(
                5,
                Some(3),
                "k == 1 BranchArm second",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(6),
            make_node(6, Some(1), "else if (k == 2)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(7),
            make_node(
                7,
                Some(6),
                "k == 2 BranchArm first",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(8),
            make_node(
                8,
                Some(6),
                "k == 2 BranchArm second",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(9),
            make_node(
                9,
                Some(6),
                "k == 2 BranchArm third",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(10),
            make_node(10, Some(1), "else if (k == 3)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(11),
            make_node(11, Some(1), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(12),
            make_node(12, Some(11), "Else Branch", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(13),
            make_node(13, Some(1), "After scope", NodeType::OtherScope),
        );
        viz.nodes.insert(
            NodeId::new(14),
            make_node(14, Some(13), "After", NodeType::HeaderContextEnter),
        );

        add_edge(&mut viz, 2, 3);
        add_edge(&mut viz, 2, 6);
        add_edge(&mut viz, 2, 10);
        add_edge(&mut viz, 2, 11);
        add_edge(&mut viz, 4, 5);
        add_edge(&mut viz, 7, 8);
        add_edge(&mut viz, 8, 9);
        add_edge(&mut viz, 3, 13);
        add_edge(&mut viz, 6, 13);
        add_edge(&mut viz, 10, 13);
        add_edge(&mut viz, 11, 13);

        let flattened = inline_branch_arms_and_scopes(&viz);

        assert_eq!(11, flattened.nodes.len());
        for removed in [3, 6, 11, 13] {
            assert!(
                !flattened.nodes.contains_key(&NodeId::new(removed)),
                "node {removed} should be removed"
            );
        }

        let expect_parent = |child: u32, parent: u32| {
            let node = flattened.nodes.get(&NodeId::new(child)).unwrap();
            assert_eq!(
                Some(NodeId::new(parent)),
                node.parent_node_id,
                "node {child} should be reparented to {parent}"
            );
        };
        for child in [4, 5, 7, 8, 9, 10, 12, 14] {
            expect_parent(child, 1);
        }
        assert_eq!(
            Some(NodeId::new(1)),
            flattened
                .nodes
                .get(&NodeId::new(2))
                .and_then(|node| node.parent_node_id)
        );

        let edges = |src: u32| -> Vec<u32> {
            flattened
                .edges_by_src
                .get(&NodeId::new(src))
                .map(|edges| edges.iter().map(|edge| edge.dst.raw()).collect())
                .unwrap_or_else(Vec::new)
        };

        assert_eq!(vec![4, 7, 10, 12], edges(2));
        assert_eq!(vec![5], edges(4));
        assert_eq!(vec![14], edges(5));
        assert_eq!(vec![8], edges(7));
        assert_eq!(vec![9], edges(8));
        assert_eq!(vec![14], edges(9));
        assert_eq!(vec![14], edges(10));
        assert_eq!(vec![14], edges(12));
    }

    #[test]
    fn sequential_ifs_matches_snapshot() {
        // Mirrors the pass-2 snapshot for SequentialIfs.baml
        let mut viz = ControlFlowVisualization::default();
        viz.nodes.insert(
            NodeId::new(0),
            make_node(0, None, "SequentialIfs", NodeType::FunctionRoot),
        );
        viz.nodes.insert(
            NodeId::new(1),
            make_node(1, Some(0), "Setup context", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(2),
            make_node(2, Some(1), "OtherScope", NodeType::OtherScope),
        );
        viz.nodes.insert(
            NodeId::new(3),
            make_node(
                3,
                Some(2),
                "In-scope configuration",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(4),
            make_node(4, Some(1), "if (false) #1", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(5),
            make_node(5, Some(1), "if (false)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(6),
            make_node(
                6,
                Some(5),
                "listen to SQS for S3",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(7),
            make_node(
                7,
                Some(5),
                "listen to SQS for EC2",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(8),
            make_node(8, Some(1), "else if (true)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(9),
            make_node(
                9,
                Some(8),
                "listen to Kinesis",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(10),
            make_node(10, Some(1), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(11),
            make_node(11, Some(1), "if (false) #2", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(12),
            make_node(12, Some(1), "if (false)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(13),
            make_node(13, Some(12), "listen to SQS", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(14),
            make_node(14, Some(1), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(15),
            make_node(15, Some(0), "Done", NodeType::HeaderContextEnter),
        );

        add_edge(&mut viz, 1, 15);
        add_edge(&mut viz, 2, 4);
        add_edge(&mut viz, 4, 10);
        add_edge(&mut viz, 4, 5);
        add_edge(&mut viz, 4, 8);
        add_edge(&mut viz, 5, 11);
        add_edge(&mut viz, 6, 7);
        add_edge(&mut viz, 8, 11);
        add_edge(&mut viz, 10, 11);
        add_edge(&mut viz, 11, 12);
        add_edge(&mut viz, 11, 14);

        let flattened = inline_branch_arms_and_scopes(&viz);

        for removed in [2, 5, 8, 12] {
            assert!(
                !flattened.nodes.contains_key(&NodeId::new(removed)),
                "node {removed} should be removed"
            );
        }

        for child in [3, 6, 7, 9, 13] {
            let parent = flattened
                .nodes
                .get(&NodeId::new(child))
                .and_then(|node| node.parent_node_id);
            assert_eq!(Some(NodeId::new(1)), parent, "node {child} parent mismatch");
        }

        let edges = |src: u32| -> Vec<u32> {
            let mut set: Vec<u32> = flattened
                .edges_by_src
                .get(&NodeId::new(src))
                .map(|edges| edges.iter().map(|edge| edge.dst.raw()).collect())
                .unwrap_or_else(Vec::new);
            set.sort();
            set
        };

        assert_eq!(vec![15], edges(1));
        assert_eq!(vec![6, 9, 10], edges(4));
        assert_eq!(vec![7], edges(6));
        assert_eq!(vec![11], edges(7));
        assert_eq!(vec![11], edges(9));
        assert_eq!(vec![11], edges(10));
        assert_eq!(vec![13, 14], edges(11));
        assert_eq!(vec![4], edges(3));
        assert_eq!(edges(14), Vec::<u32>::new());
        assert_eq!(edges(13), Vec::<u32>::new());
        assert_eq!(edges(15), Vec::<u32>::new());
    }

    #[test]
    fn nested_ifs_snapshot_integration() {
        // Mirrors the pass-2 snapshot for NestedIfs.baml (subset relevant to pass 3)
        let mut viz = ControlFlowVisualization::default();
        viz.nodes.insert(
            NodeId::new(0),
            make_node(0, None, "NestedIfs", NodeType::FunctionRoot),
        );
        viz.nodes.insert(
            NodeId::new(1),
            make_node(1, Some(0), "If statement 1", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(2),
            make_node(2, Some(1), "if (true)", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(3),
            make_node(3, Some(1), "if (true)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(4),
            make_node(4, Some(3), "If statement 2.1", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(5),
            make_node(5, Some(4), "if (true)", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(6),
            make_node(6, Some(4), "if (true)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(7),
            make_node(7, Some(6), "If statement 3.1", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(8),
            make_node(8, Some(7), "if (true)", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(9),
            make_node(9, Some(7), "if (true)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(10),
            make_node(10, Some(9), "True 3.1", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(11),
            make_node(11, Some(7), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(12),
            make_node(
                12,
                Some(11),
                "Third False 3.1",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(13),
            make_node(13, Some(4), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(14),
            make_node(
                14,
                Some(13),
                "If statement 3.2",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(15),
            make_node(15, Some(14), "if (true)", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(16),
            make_node(16, Some(14), "if (true)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(17),
            make_node(17, Some(16), "True 3.2", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(18),
            make_node(18, Some(14), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(19),
            make_node(
                19,
                Some(18),
                "Third False 3.2",
                NodeType::HeaderContextEnter,
            ),
        );
        viz.nodes.insert(
            NodeId::new(20),
            make_node(20, Some(1), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(21),
            make_node(21, Some(20), "if (true) outer", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(22),
            make_node(22, Some(20), "if (true)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(23),
            make_node(23, Some(22), "if (true) nested", NodeType::BranchGroup),
        );
        viz.nodes.insert(
            NodeId::new(24),
            make_node(24, Some(22), "if (true)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(25),
            make_node(25, Some(22), "else if (false)", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(26),
            make_node(26, Some(25), "Second False", NodeType::HeaderContextEnter),
        );
        viz.nodes.insert(
            NodeId::new(27),
            make_node(27, Some(22), "else", NodeType::BranchArm),
        );
        viz.nodes.insert(
            NodeId::new(28),
            make_node(28, Some(20), "else outer", NodeType::BranchArm),
        );

        add_edge(&mut viz, 2, 20);
        add_edge(&mut viz, 2, 3);
        add_edge(&mut viz, 5, 13);
        add_edge(&mut viz, 5, 6);
        add_edge(&mut viz, 8, 11);
        add_edge(&mut viz, 8, 9);
        add_edge(&mut viz, 15, 16);
        add_edge(&mut viz, 15, 18);
        add_edge(&mut viz, 21, 22);
        add_edge(&mut viz, 21, 28);
        add_edge(&mut viz, 23, 24);
        add_edge(&mut viz, 23, 25);
        add_edge(&mut viz, 23, 27);

        let flattened = inline_branch_arms_and_scopes(&viz);

        for removed in [3, 6, 9, 11, 13, 16, 18, 25] {
            assert!(
                !flattened.nodes.contains_key(&NodeId::new(removed)),
                "node {removed} should be removed"
            );
        }

        let edges = |src: u32| -> Vec<u32> {
            let mut dsts: Vec<u32> = flattened
                .edges_by_src
                .get(&NodeId::new(src))
                .map(|edges| edges.iter().map(|edge| edge.dst.raw()).collect())
                .unwrap_or_else(Vec::new);
            dsts.sort();
            dsts
        };

        assert_eq!(vec![4, 21], edges(2));
        assert_eq!(vec![7, 14], edges(5));
        assert_eq!(vec![10, 12], edges(8));
        assert_eq!(vec![17, 19], edges(15));
        assert_eq!(vec![23, 28], edges(21));
        assert_eq!(vec![24, 26, 27], edges(23));
    }
}
