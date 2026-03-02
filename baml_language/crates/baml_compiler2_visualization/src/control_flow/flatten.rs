//! Three-pass flattening pipeline for control flow visualization graphs.
//!
//! Ported from `engine/baml-runtime/src/control_flow/flatten/`.
//!
//! 1. `remove_implicit_nodes` — prune nodes without header ancestry
//! 2. `hoist_branch_arms` — reparent arms and fan out edges
//! 3. `inline_branch_arms_and_scopes` — remove wrapper containers

use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use super::{ControlFlowGraph, Edge, Node, NodeId, NodeType, build_children_map, node_depth};

// ---------------------------------------------------------------------------
// Public pipeline entry point
// ---------------------------------------------------------------------------

/// Run the three-pass flattening pipeline.
pub fn flatten_control_flow_graph(graph: &ControlFlowGraph) -> ControlFlowGraph {
    let pass_one = remove_implicit_nodes(graph);
    let pass_two = hoist_branch_arms(&pass_one);
    inline_branch_arms_and_scopes(&pass_two)
}

// ===========================================================================
// Pass 1: Remove implicit nodes
// ===========================================================================

fn remove_implicit_nodes(graph: &ControlFlowGraph) -> ControlFlowGraph {
    let children = build_children_map(&graph.nodes);
    let mut memo: HashMap<NodeId, bool> = HashMap::new();
    for node in graph.nodes.values() {
        compute_has_header(node.id, &graph.nodes, &children, &mut memo);
    }

    let mut keep: HashSet<NodeId> = HashSet::new();
    for node in graph.nodes.values() {
        if should_keep(node, &graph.nodes, &memo) {
            keep.insert(node.id);
        }
    }

    filter_graph(graph, &keep)
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

    let Some(node) = nodes.get(&node_id) else {
        memo.insert(node_id, false);
        return false;
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

fn filter_graph(graph: &ControlFlowGraph, keep: &HashSet<NodeId>) -> ControlFlowGraph {
    let mut nodes = IndexMap::new();
    for (id, node) in &graph.nodes {
        if keep.contains(id) {
            nodes.insert(*id, node.clone());
        }
    }

    let mut edges_by_src: IndexMap<NodeId, Vec<Edge>> = IndexMap::new();
    for (src, edges) in &graph.edges_by_src {
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

    ControlFlowGraph {
        nodes,
        edges_by_src,
    }
}

// ===========================================================================
// Pass 2: Hoist branch arms
// ===========================================================================

fn hoist_branch_arms(graph: &ControlFlowGraph) -> ControlFlowGraph {
    struct BranchGroupInfo {
        node_id: NodeId,
        parent: Option<NodeId>,
        depth: usize,
        branch_children: Vec<NodeId>,
        successors: Vec<NodeId>,
    }

    let children = build_children_map(&graph.nodes);
    let mut next = graph.clone();

    let mut groups: Vec<BranchGroupInfo> = graph
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
                    graph
                        .nodes
                        .get(child_id)
                        .map(|child| matches!(child.node_type, NodeType::BranchArm))
                        .unwrap_or(false)
                })
                .collect();

            if branch_children.is_empty() {
                return None;
            }

            let successors: Vec<NodeId> = graph
                .edges_by_src
                .get(&node.id)
                .map(|edges| edges.iter().map(|edge| edge.dst).collect())
                .unwrap_or_default();

            Some(BranchGroupInfo {
                node_id: node.id,
                parent: node.parent_node_id,
                depth: node_depth(node.id, &graph.nodes),
                branch_children,
                successors,
            })
        })
        .collect();

    // Process deepest first
    groups.sort_by(|a, b| b.depth.cmp(&a.depth));

    for info in groups {
        // Move outgoing edges from branch group onto each arm
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

        // Hoist branch arms and create BranchGroup -> BranchArm edges
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

// ===========================================================================
// Pass 3: Inline BranchArm and OtherScope nodes
// ===========================================================================

fn inline_branch_arms_and_scopes(graph: &ControlFlowGraph) -> ControlFlowGraph {
    let mut next = graph.clone();

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
    graph: &mut ControlFlowGraph,
    node_id: NodeId,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
) -> bool {
    let parent = {
        let Some(node) = graph.nodes.get(&node_id) else {
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
        .filter(|child_id| graph.nodes.contains_key(child_id))
        .collect();

    if direct_children.is_empty() {
        return false;
    }

    let entry_node = direct_children[0];
    let exit_nodes = collect_exit_nodes(node_id, children_map, graph);

    reparent_children(graph, parent, &direct_children);
    redirect_incoming_edges(graph, node_id, entry_node);
    let outgoing = graph
        .edges_by_src
        .shift_remove(&node_id)
        .unwrap_or_default();
    fan_out_outgoing_edges(graph, &exit_nodes, &outgoing);

    graph.nodes.shift_remove(&node_id);
    true
}

fn collect_exit_nodes(
    candidate_id: NodeId,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
    graph: &ControlFlowGraph,
) -> Vec<NodeId> {
    let mut exits = Vec::new();
    if let Some(children) = children_map.get(&candidate_id) {
        for child in children {
            collect_exit_nodes_recursive(*child, children_map, graph, &mut exits);
        }
    }
    exits
}

fn collect_exit_nodes_recursive(
    node_id: NodeId,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
    graph: &ControlFlowGraph,
    exits: &mut Vec<NodeId>,
) {
    if let Some(children) = children_map.get(&node_id) {
        for child in children {
            collect_exit_nodes_recursive(*child, children_map, graph, exits);
        }
    }

    let has_outgoing = graph
        .edges_by_src
        .get(&node_id)
        .map(|edges| !edges.is_empty())
        .unwrap_or(false);

    if !has_outgoing {
        exits.push(node_id);
    }
}

fn reparent_children(
    graph: &mut ControlFlowGraph,
    new_parent: Option<NodeId>,
    children: &[NodeId],
) {
    for child_id in children {
        if let Some(child) = graph.nodes.get_mut(child_id) {
            child.parent_node_id = new_parent;
        }
    }
}

fn redirect_incoming_edges(graph: &mut ControlFlowGraph, old_target: NodeId, new_target: NodeId) {
    for edges in graph.edges_by_src.values_mut() {
        for edge in edges.iter_mut() {
            if edge.dst == old_target {
                edge.dst = new_target;
            }
        }
    }
}

fn fan_out_outgoing_edges(graph: &mut ControlFlowGraph, exits: &[NodeId], outgoing: &[Edge]) {
    if exits.is_empty() || outgoing.is_empty() {
        return;
    }

    for exit in exits {
        let entry = graph.edges_by_src.entry(*exit).or_default();
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

    // DFS from roots first
    for node in nodes.values().filter(|node| node.parent_node_id.is_none()) {
        dfs_candidates(node.id, nodes, children_map, &mut visited, &mut ordered);
    }

    // Then any orphans
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

    // Post-order: add after children
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

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: u32, parent: Option<u32>, label: &str, node_type: NodeType) -> Node {
        Node {
            id: NodeId::new(id),
            parent_node_id: parent.map(NodeId::new),
            log_filter_key: format!("f|{id}"),
            label: label.to_string(),
            source_expr: None,
            node_type,
        }
    }

    fn add_edge(graph: &mut ControlFlowGraph, src: u32, dst: u32) {
        graph
            .edges_by_src
            .entry(NodeId::new(src))
            .or_default()
            .push(Edge {
                src: NodeId::new(src),
                dst: NodeId::new(dst),
            });
    }

    // -- Pass 1 tests --

    #[test]
    fn pass1_drops_branch_group_without_headers() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "root");
        let header = make_node(1, Some(0), "header", NodeType::HeaderContextEnter);
        let branch_group = make_node(2, Some(1), "if", NodeType::BranchGroup);
        let branch_arm = make_node(3, Some(2), "arm1", NodeType::BranchArm);
        graph.nodes.insert(root.id, root);
        graph.nodes.insert(header.id, header.clone());
        graph.nodes.insert(branch_group.id, branch_group);
        graph.nodes.insert(branch_arm.id, branch_arm);
        let filtered = remove_implicit_nodes(&graph);
        assert!(filtered.nodes.contains_key(&header.id));
        assert!(!filtered.nodes.contains_key(&NodeId::new(2)));
        assert!(!filtered.nodes.contains_key(&NodeId::new(3)));
    }

    #[test]
    fn pass1_keeps_all_arms_when_one_has_header() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "root");
        let header = make_node(1, Some(0), "header", NodeType::HeaderContextEnter);
        let bg = make_node(2, Some(1), "if", NodeType::BranchGroup);
        let arm_with = make_node(3, Some(2), "arm-with", NodeType::BranchArm);
        let arm_without = make_node(4, Some(2), "arm-without", NodeType::BranchArm);
        let nested = make_node(5, Some(3), "inner", NodeType::HeaderContextEnter);

        graph.nodes.insert(root.id, root);
        graph.nodes.insert(header.id, header);
        graph.nodes.insert(bg.id, bg);
        graph.nodes.insert(arm_with.id, arm_with);
        graph.nodes.insert(arm_without.id, arm_without);
        graph.nodes.insert(nested.id, nested);

        let filtered = remove_implicit_nodes(&graph);
        assert!(filtered.nodes.contains_key(&NodeId::new(2)));
        assert!(filtered.nodes.contains_key(&NodeId::new(3)));
        assert!(filtered.nodes.contains_key(&NodeId::new(4)));
    }

    // -- Pass 2 tests --

    #[test]
    fn pass2_hoists_arms() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "root");
        let group = make_node(1, Some(0), "if", NodeType::BranchGroup);
        let arm1 = make_node(2, Some(1), "arm1", NodeType::BranchArm);
        let arm2 = make_node(3, Some(1), "arm2", NodeType::BranchArm);
        let after = make_node(4, Some(0), "after", NodeType::HeaderContextEnter);
        graph.nodes.insert(root.id, root);
        graph.nodes.insert(group.id, group);
        graph.nodes.insert(arm1.id, arm1);
        graph.nodes.insert(arm2.id, arm2);
        graph.nodes.insert(after.id, after);
        add_edge(&mut graph, 0, 1);
        add_edge(&mut graph, 1, 4);

        let expanded = hoist_branch_arms(&graph);

        // BranchGroup -> both arms
        let group_edges = expanded
            .edges_by_src
            .get(&NodeId::new(1))
            .expect("group edges");
        let dsts: Vec<_> = group_edges.iter().map(|e| e.dst.raw()).collect();
        assert_eq!(dsts, vec![2, 3]);

        // Arms get successor edges
        assert!(
            expanded
                .edges_by_src
                .get(&NodeId::new(2))
                .unwrap()
                .iter()
                .any(|e| e.dst == NodeId::new(4))
        );
        assert!(
            expanded
                .edges_by_src
                .get(&NodeId::new(3))
                .unwrap()
                .iter()
                .any(|e| e.dst == NodeId::new(4))
        );

        // Arms reparented
        assert_eq!(
            expanded.nodes.get(&NodeId::new(2)).unwrap().parent_node_id,
            Some(NodeId::new(0))
        );
    }

    // -- Pipeline test --

    #[test]
    fn pipeline_runs_all_passes() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "root");
        graph.nodes.insert(root.id, root);
        let flattened = flatten_control_flow_graph(&graph);
        assert_eq!(1, flattened.nodes.len());
    }
}
