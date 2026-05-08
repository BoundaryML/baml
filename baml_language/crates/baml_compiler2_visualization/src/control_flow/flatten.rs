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

/// Produces a visualization-ready graph by:
/// 1. Hoisting branch arms with labeled fan-out edges (adapted Pass 2)
/// 2. Renaming `_` branch arm labels to `"default"`
/// 3. Inlining branch-arm containers so arms are represented as edge labels
/// 4. Computing `is_container` for each node
///
/// Does NOT run Pass 1 (`remove_implicit_nodes`) or Pass 3
/// (`inline_branch_arms_and_scopes`) — the playground needs all nodes
/// visible. Branch arms are the exception: `ReactFlow` already renders branch
/// structure as labeled fan-out edges, so keeping them as group containers
/// creates redundant nested boxes.
pub fn prepare_control_flow_graph_for_visualization(graph: &ControlFlowGraph) -> ControlFlowGraph {
    struct BranchGroupInfo {
        node_id: NodeId,
        parent: Option<NodeId>,
        depth: usize,
        branch_children: Vec<NodeId>,
        successors: Vec<NodeId>,
    }

    let mut graph = graph.clone();

    // ── Step 1: Hoist branch arms with labeled fan-out edges ──────────
    // Adapted from hoist_branch_arms — same deepest-first BranchGroup
    // iteration, but fan-out edges carry the arm's label.

    let children_map = build_children_map(&graph.nodes);
    let mut groups: Vec<BranchGroupInfo> = Vec::new();
    for (node_id, node) in &graph.nodes {
        if node.node_type != NodeType::BranchGroup {
            continue;
        }
        let branch_children: Vec<NodeId> = children_map
            .get(node_id)
            .map(|c| {
                c.iter()
                    .copied()
                    .filter(|cid| {
                        graph
                            .nodes
                            .get(cid)
                            .is_some_and(|n| n.node_type == NodeType::BranchArm)
                    })
                    .collect()
            })
            .unwrap_or_default();
        if branch_children.is_empty() {
            continue;
        }
        let successors: Vec<NodeId> = graph
            .edges_by_src
            .get(node_id)
            .map(|edges| edges.iter().map(|e| e.dst).collect())
            .unwrap_or_default();
        groups.push(BranchGroupInfo {
            node_id: *node_id,
            parent: node.parent_node_id,
            depth: node_depth(*node_id, &graph.nodes),
            branch_children,
            successors,
        });
    }

    // Process deepest groups first (handles nested conditionals)
    groups.sort_by_key(|g| std::cmp::Reverse(g.depth));

    for info in &groups {
        // Remove BranchGroup's outgoing edges
        graph.edges_by_src.shift_remove(&info.node_id);

        // Copy BranchGroup's successors onto each arm (deduplicating)
        for arm_id in &info.branch_children {
            let entry = graph.edges_by_src.entry(*arm_id).or_default();
            let mut existing: std::collections::HashSet<NodeId> =
                entry.iter().map(|e| e.dst).collect();
            for succ in &info.successors {
                if existing.insert(*succ) {
                    entry.push(Edge {
                        src: *arm_id,
                        dst: *succ,
                        label: None,
                    });
                }
            }
        }

        // Reparent each arm to the BranchGroup's parent
        for arm_id in &info.branch_children {
            if let Some(arm_node) = graph.nodes.get_mut(arm_id) {
                arm_node.parent_node_id = info.parent;
            }
        }

        // Create labeled fan-out edges BranchGroup → BranchArm
        let mut fan_out_edges = Vec::new();
        for arm_id in &info.branch_children {
            let arm_label = graph.nodes.get(arm_id).map(|n| n.label.clone());
            fan_out_edges.push(Edge {
                src: info.node_id,
                dst: *arm_id,
                label: arm_label,
            });
        }
        graph.edges_by_src.insert(info.node_id, fan_out_edges);
    }

    // ── Step 2: Rename "_" to "default" on BranchArm labels and edges ─
    for node in graph.nodes.values_mut() {
        if node.node_type == NodeType::BranchArm && node.label == "_" {
            node.label = "default".to_string();
        }
    }
    for edges in graph.edges_by_src.values_mut() {
        for edge in edges.iter_mut() {
            if edge.label.as_deref() == Some("_") {
                edge.label = Some("default".to_string());
            }
        }
    }

    // ── Step 3: Inline branch arms ───────────────────────────────────
    inline_branch_arms_for_visualization(&mut graph);

    // ── Step 4: Compute is_container ─────────────────────────────────
    // A node is a container if other nodes reference it as parent,
    // UNLESS it's a BranchGroup (those are diamond dispatch points,
    // not layout containers).
    let parent_set: std::collections::HashSet<NodeId> = graph
        .nodes
        .values()
        .filter_map(|n| n.parent_node_id)
        .collect();
    for node in graph.nodes.values_mut() {
        node.is_container =
            parent_set.contains(&node.id) && node.node_type != NodeType::BranchGroup;
    }

    graph
}

fn inline_branch_arms_for_visualization(graph: &mut ControlFlowGraph) {
    let arm_ids: Vec<NodeId> = graph
        .nodes
        .values()
        .filter(|node| node.node_type == NodeType::BranchArm)
        .map(|node| node.id)
        .collect();

    let mut children_map = build_children_map(&graph.nodes);
    for arm_id in arm_ids {
        if !graph.nodes.contains_key(&arm_id) {
            continue;
        }

        let has_children = children_map
            .get(&arm_id)
            .is_some_and(|children| !children.is_empty());

        // Keep empty/synthetic arms as visible leaf nodes (for example the
        // implicit `else` arm on an `if` without an explicit else block).
        if has_children && inline_node(graph, arm_id, &children_map) {
            children_map = build_children_map(&graph.nodes);
        }
    }
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
        NodeType::FunctionRoot | NodeType::LlmFunction | NodeType::HeaderContextEnter => true,
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
    groups.sort_by_key(|g| std::cmp::Reverse(g.depth));

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
                            label: None,
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
                label: None,
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
        let mut children_map = build_children_map(&next.nodes);
        let candidates = collect_candidates(&next.nodes, &children_map);

        if candidates.is_empty() {
            break;
        }

        let mut changed = false;
        for candidate_id in candidates {
            if !next.nodes.contains_key(&candidate_id) {
                continue;
            }

            if inline_node(&mut next, candidate_id, &children_map) {
                changed = true;
                children_map = build_children_map(&next.nodes);
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
                    label: None,
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
            llm_client: None,
            callee_name: None,
            source_span: None,
            is_container: false,
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
                label: None,
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

    // -- Visualization prep tests --

    /// Builds a graph mimicking `IfElseWithHeaders`:
    /// ```text
    /// FunctionRoot (0)
    ///   └─ Header "Check flag" (1)
    ///        └─ BranchGroup "if (flag)" (2)
    ///             ├─ BranchArm "if (flag)" (3)
    ///             │    └─ Header "True branch" (4)
    ///             │         └─ OtherScope "yes" (5)
    ///             └─ BranchArm "else" (6)
    ///                  └─ Header "False branch" (7)
    ///                       └─ OtherScope "no" (8)
    /// ```
    /// Sequential edges within linear scopes: 1→2 (header's child)
    /// No edges between branch arms (`BranchGroup` has non-linear children).
    /// Within each arm: 3→4, 4→5, 6→7, 7→8 (single-child linear chains)
    fn build_if_else_with_headers() -> ControlFlowGraph {
        let mut graph = ControlFlowGraph::default();

        let root = Node::root(NodeId::new(0), "f|root:0", "IfElseWithHeaders");
        let header_check = make_node(1, Some(0), "Check flag", NodeType::HeaderContextEnter);
        let branch_group = make_node(2, Some(1), "if (flag)", NodeType::BranchGroup);
        let arm_if = make_node(3, Some(2), "if (flag)", NodeType::BranchArm);
        let header_true = make_node(4, Some(3), "True branch", NodeType::HeaderContextEnter);
        let leaf_yes = make_node(5, Some(4), "yes", NodeType::OtherScope);
        let arm_else = make_node(6, Some(2), "else", NodeType::BranchArm);
        let header_false = make_node(7, Some(6), "False branch", NodeType::HeaderContextEnter);
        let leaf_no = make_node(8, Some(7), "no", NodeType::OtherScope);

        for n in [
            root,
            header_check,
            branch_group,
            arm_if,
            header_true,
            leaf_yes,
            arm_else,
            header_false,
            leaf_no,
        ] {
            graph.nodes.insert(n.id, n);
        }

        // Sequential edges within linear scopes:
        // Header "Check flag" has one child: BranchGroup
        // (no edge needed — it's the first child, so no "prev")
        // BranchArm "if" → Header "True"
        // (no edge — single child)
        // Header "True" → leaf "yes"
        // (no edge — single child)
        // Same for else arm.

        // But let's add realistic edges that the AST builder would create
        // if there were multiple children. For this test, the raw graph
        // has NO edges — all structure is parent-child. This matches the
        // actual AST builder output for simple single-child scopes.

        graph
    }

    #[test]
    fn viz_prep_reparents_arms_under_header() {
        let graph = build_if_else_with_headers();
        let result = prepare_control_flow_graph_for_visualization(&graph);

        // Branch arms are visualization-only edge labels, not rendered boxes.
        assert!(
            result.nodes.get(&NodeId::new(3)).is_none(),
            "BranchArm 'if' should be removed from visualization graph"
        );
        assert!(
            result.nodes.get(&NodeId::new(6)).is_none(),
            "BranchArm 'else' should be removed from visualization graph"
        );

        // Branch contents are reparented to the BranchGroup's parent.
        let header_true = result.nodes.get(&NodeId::new(4)).unwrap();
        assert_eq!(
            header_true.parent_node_id,
            Some(NodeId::new(1)),
            "Header inside true arm should be reparented to Header (id=1)"
        );
        let header_false = result.nodes.get(&NodeId::new(7)).unwrap();
        assert_eq!(
            header_false.parent_node_id,
            Some(NodeId::new(1)),
            "Header inside else arm should be reparented to Header (id=1)"
        );

        // BranchGroup itself stays under the header
        let bg = result.nodes.get(&NodeId::new(2)).unwrap();
        assert_eq!(
            bg.parent_node_id,
            Some(NodeId::new(1)),
            "BranchGroup should remain under Header (id=1)"
        );
    }

    #[test]
    fn viz_prep_creates_labeled_fan_out_edges() {
        let graph = build_if_else_with_headers();
        let result = prepare_control_flow_graph_for_visualization(&graph);

        // BranchGroup (id=2) should have fan-out edges to arm contents.
        let bg_edges = result
            .edges_by_src
            .get(&NodeId::new(2))
            .expect("BranchGroup should have outgoing edges");

        assert_eq!(bg_edges.len(), 2, "BranchGroup should have 2 fan-out edges");

        // Check destinations and labels
        let mut edges_sorted: Vec<_> = bg_edges.iter().collect();
        edges_sorted.sort_by_key(|e| e.dst.raw());

        assert_eq!(edges_sorted[0].dst, NodeId::new(4));
        assert_eq!(
            edges_sorted[0].label.as_deref(),
            Some("if (flag)"),
            "Fan-out edge to true-arm contents should carry arm label"
        );

        assert_eq!(edges_sorted[1].dst, NodeId::new(7));
        assert_eq!(
            edges_sorted[1].label.as_deref(),
            Some("else"),
            "Fan-out edge to else-arm contents should carry arm label"
        );
    }

    #[test]
    fn viz_prep_is_container_correct_for_headers() {
        let graph = build_if_else_with_headers();
        let result = prepare_control_flow_graph_for_visualization(&graph);

        // FunctionRoot (id=0): has children → is_container=true
        assert!(
            result.nodes.get(&NodeId::new(0)).unwrap().is_container,
            "FunctionRoot should be a container"
        );

        // Header "Check flag" (id=1): has children (BranchGroup + reparented arm contents) → true
        assert!(
            result.nodes.get(&NodeId::new(1)).unwrap().is_container,
            "Header 'Check flag' should be a container"
        );

        // BranchGroup (id=2): explicitly excluded → is_container=false
        assert!(
            !result.nodes.get(&NodeId::new(2)).unwrap().is_container,
            "BranchGroup should NOT be a container (diamond dispatch)"
        );

        // Header "True branch" (id=4): has child leaf → true
        assert!(
            result.nodes.get(&NodeId::new(4)).unwrap().is_container,
            "Header 'True branch' should be a container (has child leaf)"
        );

        // Leaf "yes" (id=5): no children → false
        assert!(
            !result.nodes.get(&NodeId::new(5)).unwrap().is_container,
            "Leaf 'yes' should NOT be a container"
        );

        // Header "False branch" (id=7): has child leaf → true
        assert!(
            result.nodes.get(&NodeId::new(7)).unwrap().is_container,
            "Header 'False branch' should be a container (has child leaf)"
        );

        // Leaf "no" (id=8): no children → false
        assert!(
            !result.nodes.get(&NodeId::new(8)).unwrap().is_container,
            "Leaf 'no' should NOT be a container"
        );
    }

    #[test]
    fn viz_prep_all_edge_endpoints_are_valid_nodes() {
        let graph = build_if_else_with_headers();
        let result = prepare_control_flow_graph_for_visualization(&graph);

        for (src, edges) in &result.edges_by_src {
            assert!(
                result.nodes.contains_key(src),
                "Edge source {src} is not a valid node"
            );
            for edge in edges {
                assert!(
                    result.nodes.contains_key(&edge.dst),
                    "Edge destination {} (from source {}) is not a valid node",
                    edge.dst,
                    src
                );
            }
        }
    }

    /// Test with a more complex graph: sequential headers, then branch inside second header.
    /// This catches edge propagation issues when `BranchGroup` has successors.
    ///
    /// ```text
    /// FunctionRoot (0)
    ///   └─ Header "Setup" (1)
    ///   │    └─ OtherScope "x = 1" (2)
    ///   └─ Header "Process" (3)
    ///        └─ BranchGroup "if (x)" (4)
    ///             ├─ BranchArm "true" (5)
    ///             └─ BranchArm "false" (6)
    ///        └─ OtherScope "done" (7)  ← successor after the if/else
    /// ```
    /// Edges: 1→3 (sequential headers), 4→7 (`BranchGroup` to successor)
    /// Within Header "Process": 4 and 7 are sequential children.
    #[test]
    fn viz_prep_successor_edges_propagated_to_arms() {
        let mut graph = ControlFlowGraph::default();

        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let h_setup = make_node(1, Some(0), "Setup", NodeType::HeaderContextEnter);
        let leaf_x = make_node(2, Some(1), "x = 1", NodeType::OtherScope);
        let h_process = make_node(3, Some(0), "Process", NodeType::HeaderContextEnter);
        let bg = make_node(4, Some(3), "if (x)", NodeType::BranchGroup);
        let arm_true = make_node(5, Some(4), "true", NodeType::BranchArm);
        let arm_false = make_node(6, Some(4), "false", NodeType::BranchArm);
        let leaf_done = make_node(7, Some(3), "done", NodeType::OtherScope);

        for n in [
            root, h_setup, leaf_x, h_process, bg, arm_true, arm_false, leaf_done,
        ] {
            graph.nodes.insert(n.id, n);
        }

        // Sequential edges:
        add_edge(&mut graph, 1, 3); // Setup → Process (at FunctionRoot level)
        add_edge(&mut graph, 4, 7); // BranchGroup → done (at Header "Process" level)

        let result = prepare_control_flow_graph_for_visualization(&graph);

        // Empty arms remain visible so implicit/synthetic branches are not
        // hidden. The BranchGroup fans out to those arm leaf nodes.
        let bg_edges = result.edges_by_src.get(&NodeId::new(4));
        let bg_edges: Vec<_> = bg_edges.map(|es| es.iter().collect()).unwrap_or_default();
        let bg_edge_dsts: Vec<u32> = bg_edges.iter().map(|e| e.dst.raw()).collect();
        let bg_edge_labels: Vec<_> = bg_edges.iter().filter_map(|e| e.label.as_deref()).collect();
        assert_eq!(
            bg_edge_dsts,
            vec![5, 6],
            "BranchGroup should fan out to empty arm nodes, got: {bg_edge_dsts:?}"
        );
        assert_eq!(
            bg_edge_labels,
            vec!["true", "false"],
            "BranchGroup fan-out edges should preserve arm labels"
        );

        assert!(
            result.nodes.get(&NodeId::new(5)).is_some(),
            "empty true arm should stay visible"
        );
        assert!(
            result.nodes.get(&NodeId::new(6)).is_some(),
            "empty false arm should stay visible"
        );
        assert_eq!(
            result.nodes.get(&NodeId::new(5)).unwrap().parent_node_id,
            Some(NodeId::new(3))
        );
        assert_eq!(
            result.nodes.get(&NodeId::new(6)).unwrap().parent_node_id,
            Some(NodeId::new(3))
        );

        let arm_true_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(5))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert!(
            arm_true_dsts.contains(&7),
            "true arm should flow to successor 7, got: {arm_true_dsts:?}"
        );
        let arm_false_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(6))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert!(
            arm_false_dsts.contains(&7),
            "false arm should flow to successor 7, got: {arm_false_dsts:?}"
        );

        // Header "Process" should be a container (has BranchGroup + arms + successor)
        assert!(result.nodes.get(&NodeId::new(3)).unwrap().is_container);
    }

    #[test]
    fn viz_prep_successor_edges_propagated_to_non_empty_arms() {
        let mut graph = ControlFlowGraph::default();

        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let h_process = make_node(1, Some(0), "Process", NodeType::HeaderContextEnter);
        let bg = make_node(2, Some(1), "if (x)", NodeType::BranchGroup);
        let arm_true = make_node(3, Some(2), "true", NodeType::BranchArm);
        let leaf_true = make_node(4, Some(3), "work", NodeType::OtherScope);
        let arm_false = make_node(5, Some(2), "false", NodeType::BranchArm);
        let leaf_false = make_node(6, Some(5), "fallback", NodeType::OtherScope);
        let leaf_done = make_node(7, Some(1), "done", NodeType::OtherScope);

        for n in [
            root, h_process, bg, arm_true, leaf_true, arm_false, leaf_false, leaf_done,
        ] {
            graph.nodes.insert(n.id, n);
        }

        add_edge(&mut graph, 2, 7); // BranchGroup → done successor

        let result = prepare_control_flow_graph_for_visualization(&graph);

        // BranchGroup fans out directly to branch contents, not BranchArm boxes.
        let bg_edges = result.edges_by_src.get(&NodeId::new(2)).unwrap();
        let bg_edge_dsts: Vec<u32> = bg_edges.iter().map(|e| e.dst.raw()).collect();
        let bg_edge_labels: Vec<_> = bg_edges.iter().filter_map(|e| e.label.as_deref()).collect();
        assert_eq!(bg_edge_dsts, vec![4, 6]);
        assert_eq!(bg_edge_labels, vec!["true", "false"]);

        // Each branch content should still flow to successor (7).
        let true_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(4))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert!(
            true_dsts.contains(&7),
            "true branch content should flow to successor 7, got: {true_dsts:?}"
        );
        let false_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(6))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert!(
            false_dsts.contains(&7),
            "false branch content should flow to successor 7, got: {false_dsts:?}"
        );

        assert!(result.nodes.get(&NodeId::new(3)).is_none());
        assert!(result.nodes.get(&NodeId::new(5)).is_none());
        assert_eq!(
            result.nodes.get(&NodeId::new(4)).unwrap().parent_node_id,
            Some(NodeId::new(1))
        );
        assert_eq!(
            result.nodes.get(&NodeId::new(6)).unwrap().parent_node_id,
            Some(NodeId::new(1))
        );
    }

    #[test]
    fn viz_prep_underscore_renamed_to_default() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let bg = make_node(1, Some(0), "match", NodeType::BranchGroup);
        let arm1 = make_node(2, Some(1), "1", NodeType::BranchArm);
        let arm_wildcard = make_node(3, Some(1), "_", NodeType::BranchArm);
        let done = make_node(4, Some(0), "done", NodeType::OtherScope);

        for n in [root, bg, arm1, arm_wildcard, done] {
            graph.nodes.insert(n.id, n);
        }
        add_edge(&mut graph, 1, 4);

        let result = prepare_control_flow_graph_for_visualization(&graph);

        // Empty branch-arm nodes remain visible, with wildcard renamed.
        assert_eq!(result.nodes.get(&NodeId::new(3)).unwrap().label, "default");
        let bg_edges = result.edges_by_src.get(&NodeId::new(1)).unwrap();
        assert!(
            bg_edges
                .iter()
                .any(|e| e.dst == NodeId::new(3) && e.label.as_deref() == Some("default")),
            "Wildcard branch should become a default-labeled edge to the branch node"
        );
    }
}
