use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use super::{build_children_map, collect_subtree, node_depth};
use crate::control_flow::{ControlFlowVisualization, Edge, Node, NodeId, NodeType};

/// Pass 3: flatten BranchArm / OtherScope nodes so header nodes appear at the correct depth.
pub fn flatten_branch_arms_and_scopes(viz: &ControlFlowVisualization) -> ControlFlowVisualization {
    let children_map = build_children_map(&viz.nodes);
    let mut flatten_targets: HashMap<NodeId, FlattenInfo> = HashMap::new();

    for node in viz.nodes.values() {
        if !matches!(node.node_type, NodeType::BranchArm | NodeType::OtherScope) {
            continue;
        }

        let children = match children_map.get(&node.id) {
            Some(children) if !children.is_empty() => children.clone(),
            _ => continue,
        };

        let entry_child = children.iter().copied().find(|child_id| {
            viz.nodes
                .get(child_id)
                .map(|child| matches!(child.node_type, NodeType::HeaderContextEnter))
                .unwrap_or(false)
        });

        if entry_child.is_none() {
            continue;
        }

        let subtree = collect_subtree(node.id, &children_map);
        let successors = viz
            .edges_by_src
            .get(&node.id)
            .map(|edges| edges.iter().map(|edge| edge.dst).collect())
            .unwrap_or_default();
        let exit_nodes = find_exit_nodes(&subtree, &viz.edges_by_src);

        flatten_targets.insert(
            node.id,
            FlattenInfo {
                node_id: node.id,
                parent: node.parent_node_id,
                entry_child,
                successors,
                children,
                subtree,
                exit_nodes,
                depth: node_depth(node.id, &viz.nodes),
            },
        );
    }

    if flatten_targets.is_empty() {
        return viz.clone();
    }

    let mut nodes = viz.nodes.clone();
    let mut edges_by_src: IndexMap<NodeId, Vec<Edge>> = IndexMap::new();
    let mut targets_sorted: Vec<_> = flatten_targets.values().cloned().collect();
    targets_sorted.sort_by(|a, b| b.depth.cmp(&a.depth));

    for info in &targets_sorted {
        let resolved_parent = resolve_flatten_parent(&viz.nodes, info);
        if let Some(children) = children_map.get(&info.node_id) {
            for child_id in children.iter().copied() {
                if let Some(child_node) = nodes.get_mut(&child_id) {
                    child_node.parent_node_id = resolved_parent;
                }
            }
        }
        nodes.shift_remove(&info.node_id);
    }

    for (src, edges) in viz.edges_by_src.iter() {
        if flatten_targets.contains_key(src) {
            continue;
        }
        let mut updated: Vec<Edge> = Vec::new();
        for edge in edges {
            if let Some(info) = flatten_targets.get(&edge.dst) {
                if let Some(entry_child) = info.entry_child {
                    if !updated.iter().any(|e| e.dst == entry_child) {
                        updated.push(Edge {
                            src: *src,
                            dst: entry_child,
                        });
                    }
                }
            } else {
                updated.push(edge.clone());
            }
        }
        if !updated.is_empty() {
            edges_by_src.insert(*src, updated);
        }
    }

    for info in flatten_targets.values() {
        if info.successors.is_empty() {
            continue;
        }
        for exit in &info.exit_nodes {
            let entry = edges_by_src.entry(*exit).or_default();
            for succ in &info.successors {
                if !entry.iter().any(|edge| edge.dst == *succ) {
                    entry.push(Edge {
                        src: *exit,
                        dst: *succ,
                    });
                }
            }
        }
    }

    ControlFlowVisualization {
        nodes,
        edges_by_src,
    }
}

fn resolve_flatten_parent(nodes: &IndexMap<NodeId, Node>, info: &FlattenInfo) -> Option<NodeId> {
    let node = match nodes.get(&info.node_id) {
        Some(node) => node,
        None => return info.parent,
    };

    match node.node_type {
        NodeType::BranchArm => info
            .parent
            .and_then(|branch_group| nodes.get(&branch_group))
            .and_then(|group| group.parent_node_id),
        _ => info.parent,
    }
}

#[derive(Clone)]
struct FlattenInfo {
    node_id: NodeId,
    parent: Option<NodeId>,
    entry_child: Option<NodeId>,
    successors: Vec<NodeId>,
    children: Vec<NodeId>,
    subtree: HashSet<NodeId>,
    exit_nodes: Vec<NodeId>,
    depth: usize,
}

fn find_exit_nodes(subtree: &HashSet<NodeId>, edges: &IndexMap<NodeId, Vec<Edge>>) -> Vec<NodeId> {
    let mut result = Vec::new();
    for node_id in subtree {
        match edges.get(node_id) {
            Some(list) => {
                if list.iter().all(|edge| subtree.contains(&edge.dst)) {
                    result.push(*node_id);
                }
            }
            None => result.push(*node_id),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use internal_baml_core::ast::Span;

    use super::*;

    fn make_node(id: u32, parent: Option<u32>, node_type: NodeType, label: &str) -> Node {
        Node {
            id: NodeId::new(id),
            parent_node_id: parent.map(NodeId::new),
            lexical_id: format!("f|{id}"),
            label: label.to_string(),
            span: Span::fake(),
            node_type,
        }
    }

    fn branch_scope_viz() -> ControlFlowVisualization {
        let mut viz = ControlFlowVisualization::default();
        let root = Node::root(NodeId::new(0), "f|root:0".to_string(), Span::fake(), "root");
        let group = make_node(1, Some(0), NodeType::BranchGroup, "if");
        let arm_with_header = make_node(2, Some(1), NodeType::BranchArm, "arm-1");
        let arm_without = make_node(3, Some(1), NodeType::BranchArm, "arm-2");
        let header1 = make_node(4, Some(2), NodeType::HeaderContextEnter, "Header 1");
        let header2 = make_node(5, Some(4), NodeType::HeaderContextEnter, "Header 2");
        let after = make_node(6, Some(0), NodeType::HeaderContextEnter, "After");

        viz.nodes.insert(root.id, root);
        viz.nodes.insert(group.id, group);
        viz.nodes.insert(arm_with_header.id, arm_with_header);
        viz.nodes.insert(arm_without.id, arm_without);
        viz.nodes.insert(header1.id, header1);
        viz.nodes.insert(header2.id, header2);
        viz.nodes.insert(after.id, after.clone());

        viz.edges_by_src.insert(
            NodeId::new(0),
            vec![Edge {
                src: NodeId::new(0),
                dst: NodeId::new(1),
            }],
        );
        viz.edges_by_src.insert(
            NodeId::new(1),
            vec![
                Edge {
                    src: NodeId::new(1),
                    dst: NodeId::new(2),
                },
                Edge {
                    src: NodeId::new(1),
                    dst: NodeId::new(3),
                },
            ],
        );
        viz.edges_by_src.insert(
            NodeId::new(2),
            vec![Edge {
                src: NodeId::new(2),
                dst: NodeId::new(6),
            }],
        );
        viz.edges_by_src.insert(
            NodeId::new(4),
            vec![Edge {
                src: NodeId::new(4),
                dst: NodeId::new(5),
            }],
        );

        viz
    }

    #[test]
    fn branch_arms_with_headers_are_flattened() {
        let viz = branch_scope_viz();
        let flattened = flatten_branch_arms_and_scopes(&viz);

        assert!(!flattened.nodes.contains_key(&NodeId::new(2)));
        assert!(flattened.nodes.contains_key(&NodeId::new(3)));

        let header1 = flattened.nodes.get(&NodeId::new(4)).unwrap();
        assert_eq!(header1.parent_node_id, Some(NodeId::new(0)));

        let group_edges = flattened.edges_by_src.get(&NodeId::new(1)).unwrap();
        let destinations: Vec<_> = group_edges.iter().map(|edge| edge.dst.raw()).collect();
        assert!(destinations.contains(&4));
        assert!(destinations.contains(&3));

        let header2_edges = flattened
            .edges_by_src
            .get(&NodeId::new(5))
            .expect("header2 edges");
        assert!(header2_edges.iter().any(|edge| edge.dst == NodeId::new(6)));
    }

    #[test]
    fn other_scope_with_header_is_flattened() {
        let mut viz = ControlFlowVisualization::default();
        let root = Node::root(NodeId::new(0), "f|root:0".to_string(), Span::fake(), "root");
        let header = make_node(1, Some(0), NodeType::HeaderContextEnter, "Start");
        let scope = make_node(2, Some(1), NodeType::OtherScope, "scope");
        let nested_header = make_node(3, Some(2), NodeType::HeaderContextEnter, "Inner");
        viz.nodes.insert(root.id, root);
        viz.nodes.insert(header.id, header);
        viz.nodes.insert(scope.id, scope);
        viz.nodes.insert(nested_header.id, nested_header);

        let flattened = flatten_branch_arms_and_scopes(&viz);
        assert!(!flattened.nodes.contains_key(&NodeId::new(2)));
        let child = flattened.nodes.get(&NodeId::new(3)).unwrap();
        assert_eq!(child.parent_node_id, Some(NodeId::new(1)));
    }
}
