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
/// 1. Pruning to required nodes — only headers (`//#`), LLM functions/calls,
///    structure that contains them (an annotated if-branch or loop body pulls
///    in the whole if / loop), and early returns inside rendered scopes
/// 2. Hoisting branch arms with labeled fan-out edges (adapted Pass 2)
/// 3. Renaming `_` branch arm labels to `"default"`
/// 4. Inlining branch-arm containers so arms are represented as edge labels
/// 5. Computing `is_container` for each node
pub fn prepare_control_flow_graph_for_visualization(graph: &ControlFlowGraph) -> ControlFlowGraph {
    struct BranchGroupInfo {
        node_id: NodeId,
        parent: Option<NodeId>,
        depth: usize,
        branch_children: Vec<NodeId>,
        successors: Vec<NodeId>,
    }

    // ── Step 0: Prune to header/LLM-anchored nodes ─────────────────────
    let keep = compute_required_nodes(graph);
    let mut graph = filter_graph(graph, &keep);

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
    let keep = compute_required_nodes(graph);
    filter_graph(graph, &keep)
}

/// A node that renders no matter where it appears: header comment nodes
/// (`//#`) and LLM functions / calls to LLM functions.
fn is_always_rendered(node: &Node) -> bool {
    matches!(
        node.node_type,
        NodeType::HeaderContextEnter | NodeType::LlmFunction
    ) || node.llm_client.is_some()
}

/// Compute the set of nodes that must be rendered:
/// - the function root,
/// - header comment nodes and LLM functions / LLM calls (always),
/// - any structural node (loop, branch group, scope) whose subtree contains
///   one of the above — a `//#` inside an if-branch or loop body forces the
///   whole if / loop to render,
/// - every arm of a rendered branch group (one annotated branch extracts all
///   branches),
/// - return nodes whose enclosing rendered scope survives, so early returns
///   stay visible.
fn compute_required_nodes(graph: &ControlFlowGraph) -> HashSet<NodeId> {
    let children = build_children_map(&graph.nodes);
    let mut memo: HashMap<NodeId, bool> = HashMap::new();
    for node in graph.nodes.values() {
        compute_has_anchor(node.id, &graph.nodes, &children, &mut memo);
    }

    let mut keep: HashSet<NodeId> = HashSet::new();
    for node in graph.nodes.values() {
        let required = matches!(node.node_type, NodeType::FunctionRoot)
            || *memo.get(&node.id).unwrap_or(&false);
        if required {
            keep.insert(node.id);
        }
    }

    // A `//#` header placed directly above an `if`/`match` annotates the whole
    // branch: keep any BranchGroup that is a direct child of a kept header so
    // every arm renders (via the arm loop below), even when no arm contains its
    // own anchor.
    let mut header_branch_groups: Vec<NodeId> = Vec::new();
    for node in graph.nodes.values() {
        if !matches!(node.node_type, NodeType::BranchGroup) {
            continue;
        }
        let Some(parent_id) = node.parent_node_id else {
            continue;
        };
        let parent_is_kept_header = keep.contains(&parent_id)
            && matches!(
                graph.nodes.get(&parent_id).map(|p| &p.node_type),
                Some(NodeType::HeaderContextEnter)
            );
        if parent_is_kept_header {
            header_branch_groups.push(node.id);
        }
    }
    for id in header_branch_groups {
        keep.insert(id);
    }

    // All arms of a kept branch group render, even header-less ones.
    for node in graph.nodes.values() {
        if !matches!(node.node_type, NodeType::BranchArm) {
            continue;
        }
        if let Some(parent_id) = node.parent_node_id {
            let parent_is_kept_group = keep.contains(&parent_id)
                && matches!(
                    graph.nodes.get(&parent_id).map(|p| &p.node_type),
                    Some(NodeType::BranchGroup)
                );
            if parent_is_kept_group {
                keep.insert(node.id);
            }
        }
    }

    // Early returns render whenever their enclosing rendered scope renders.
    // Keep any wrapper nodes needed to connect the return back to that scope.
    for node in graph.nodes.values() {
        if !matches!(node.node_type, NodeType::Return) {
            continue;
        }

        let Some(parent_id) = node.parent_node_id else {
            keep.insert(node.id);
            continue;
        };

        let mut current = Some(parent_id);
        let mut path = Vec::new();
        let mut seen = HashSet::new();
        while let Some(ancestor_id) = current {
            if !seen.insert(ancestor_id) {
                break;
            }
            path.push(ancestor_id);
            if keep.contains(&ancestor_id) {
                keep.insert(node.id);
                keep.extend(path);
                break;
            }
            current = graph.nodes.get(&ancestor_id).and_then(|n| n.parent_node_id);
        }
    }

    keep
}

/// Whether `node_id` or any of its descendants is an always-rendered node
/// (header / LLM).
fn compute_has_anchor(
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

    let mut result = is_always_rendered(node);
    if let Some(child_ids) = children.get(&node_id) {
        for child in child_ids {
            if compute_has_anchor(*child, nodes, children, memo) {
                result = true;
                break;
            }
        }
    }

    memo.insert(node_id, result);
    result
}

/// Drop all nodes outside `keep`, splicing sequential edges through removed
/// nodes so surviving siblings stay connected (kept → dropped → kept becomes
/// kept → kept).
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
        let mut filtered: Vec<Edge> = Vec::new();
        let mut seen: HashSet<NodeId> = HashSet::new();
        for edge in edges {
            if keep.contains(&edge.dst) {
                if seen.insert(edge.dst) {
                    filtered.push(edge.clone());
                }
            } else {
                // Walk through the dropped node(s) to the next kept nodes.
                for dst in kept_successors_through(edge.dst, graph, keep) {
                    if seen.insert(dst) {
                        filtered.push(Edge {
                            src: *src,
                            dst,
                            label: None,
                        });
                    }
                }
            }
        }
        if !filtered.is_empty() {
            edges_by_src.insert(*src, filtered);
        }
    }

    ControlFlowGraph {
        nodes,
        edges_by_src,
    }
}

/// Starting from a dropped node, follow outgoing edges through other dropped
/// nodes and return the kept nodes reachable that way.
fn kept_successors_through(
    start: NodeId,
    graph: &ControlFlowGraph,
    keep: &HashSet<NodeId>,
) -> Vec<NodeId> {
    let mut result = Vec::new();
    let mut visited: HashSet<NodeId> = HashSet::new();
    let mut stack = vec![start];
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        if keep.contains(&id) {
            result.push(id);
            continue;
        }
        if let Some(edges) = graph.edges_by_src.get(&id) {
            for edge in edges {
                stack.push(edge.dst);
            }
        }
    }
    result
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

    if !has_outgoing && !is_terminal_subtree(node_id, children_map, graph) {
        exits.push(node_id);
    }
}

/// Whether control flow always leaves the function inside this subtree: the
/// node is a `return`, or a container all of whose children are terminal.
/// Terminal subtrees must never fan out to whatever comes after an inlined
/// container — e.g. a header whose only content is an early return.
fn is_terminal_subtree(
    node_id: NodeId,
    children_map: &HashMap<NodeId, Vec<NodeId>>,
    graph: &ControlFlowGraph,
) -> bool {
    let Some(node) = graph.nodes.get(&node_id) else {
        return false;
    };
    if matches!(node.node_type, NodeType::Return) {
        return true;
    }
    match children_map.get(&node_id) {
        Some(children) if !children.is_empty() => children
            .iter()
            .all(|child| is_terminal_subtree(*child, children_map, graph)),
        _ => false,
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
            callee_names: Vec::new(),
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
    fn pass1_keeps_branch_group_directly_under_header() {
        // A `//#` header directly above an if/match annotates the whole branch:
        // the branch group and all arms survive even with no anchor inside them.
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
        assert!(filtered.nodes.contains_key(&NodeId::new(2)));
        assert!(filtered.nodes.contains_key(&NodeId::new(3)));
    }

    #[test]
    fn pass1_drops_branch_group_without_anchor_outside_header() {
        // A branch group with no anchor that is NOT directly under a header is
        // still pruned — only headers / LLM calls force a branch to render.
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "root");
        let branch_group = make_node(1, Some(0), "if", NodeType::BranchGroup);
        let branch_arm = make_node(2, Some(1), "arm1", NodeType::BranchArm);
        graph.nodes.insert(root.id, root);
        graph.nodes.insert(branch_group.id, branch_group);
        graph.nodes.insert(branch_arm.id, branch_arm);
        let filtered = remove_implicit_nodes(&graph);
        assert!(!filtered.nodes.contains_key(&NodeId::new(1)));
        assert!(!filtered.nodes.contains_key(&NodeId::new(2)));
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

        // Plain leaves "yes" (5) and "no" (8) are not header/LLM anchored →
        // pruned from the visualization graph.
        assert!(
            result.nodes.get(&NodeId::new(5)).is_none(),
            "Plain leaf 'yes' should be pruned"
        );
        assert!(
            result.nodes.get(&NodeId::new(8)).is_none(),
            "Plain leaf 'no' should be pruned"
        );

        // With their leaves pruned, the branch headers become leaf nodes.
        assert!(
            !result.nodes.get(&NodeId::new(4)).unwrap().is_container,
            "Header 'True branch' should be a leaf after pruning"
        );
        assert!(
            !result.nodes.get(&NodeId::new(7)).unwrap().is_container,
            "Header 'False branch' should be a leaf after pruning"
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
    ///   │    └─ OtherScope "x = 1" (2)      ← pruned (no header/LLM anchor)
    ///   └─ Header "Process" (3)
    ///        └─ BranchGroup "if (x)" (4)    ← kept: header inside true arm
    ///             ├─ BranchArm "true" (5)
    ///             │    └─ Header "Work" (8)
    ///             └─ BranchArm "false" (6)  ← kept: sibling arm of annotated arm
    ///        └─ Header "Done" (7)           ← successor after the if/else
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
        let h_done = make_node(7, Some(3), "Done", NodeType::HeaderContextEnter);
        let h_work = make_node(8, Some(5), "Work", NodeType::HeaderContextEnter);

        for n in [
            root, h_setup, leaf_x, h_process, bg, arm_true, arm_false, h_done, h_work,
        ] {
            graph.nodes.insert(n.id, n);
        }

        // Sequential edges:
        add_edge(&mut graph, 1, 3); // Setup → Process (at FunctionRoot level)
        add_edge(&mut graph, 4, 7); // BranchGroup → Done (at Header "Process" level)

        let result = prepare_control_flow_graph_for_visualization(&graph);

        // The plain leaf under "Setup" is pruned.
        assert!(
            result.nodes.get(&NodeId::new(2)).is_none(),
            "plain OtherScope leaf should be pruned"
        );

        // The annotated arm is inlined (it has content); the empty false arm
        // stays visible. Fan-out goes to the arm content / the empty arm.
        let bg_edges = result.edges_by_src.get(&NodeId::new(4));
        let bg_edges: Vec<_> = bg_edges.map(|es| es.iter().collect()).unwrap_or_default();
        let bg_edge_dsts: Vec<u32> = bg_edges.iter().map(|e| e.dst.raw()).collect();
        let bg_edge_labels: Vec<_> = bg_edges.iter().filter_map(|e| e.label.as_deref()).collect();
        assert_eq!(
            bg_edge_dsts,
            vec![8, 6],
            "BranchGroup should fan out to arm content and empty arm, got: {bg_edge_dsts:?}"
        );
        assert_eq!(
            bg_edge_labels,
            vec!["true", "false"],
            "BranchGroup fan-out edges should preserve arm labels"
        );

        assert!(
            result.nodes.get(&NodeId::new(5)).is_none(),
            "non-empty true arm should be inlined"
        );
        assert!(
            result.nodes.get(&NodeId::new(6)).is_some(),
            "empty false arm should stay visible"
        );
        assert_eq!(
            result.nodes.get(&NodeId::new(8)).unwrap().parent_node_id,
            Some(NodeId::new(3))
        );
        assert_eq!(
            result.nodes.get(&NodeId::new(6)).unwrap().parent_node_id,
            Some(NodeId::new(3))
        );

        let work_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(8))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert!(
            work_dsts.contains(&7),
            "true arm content should flow to successor 7, got: {work_dsts:?}"
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
        let leaf_true = make_node(4, Some(3), "work", NodeType::HeaderContextEnter);
        let arm_false = make_node(5, Some(2), "false", NodeType::BranchArm);
        let leaf_false = make_node(6, Some(5), "fallback", NodeType::HeaderContextEnter);
        let leaf_done = make_node(7, Some(1), "done", NodeType::HeaderContextEnter);

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
        let h_one = make_node(5, Some(2), "Handle one", NodeType::HeaderContextEnter);
        let arm_wildcard = make_node(3, Some(1), "_", NodeType::BranchArm);
        let done = make_node(4, Some(0), "Done", NodeType::HeaderContextEnter);

        for n in [root, bg, arm1, h_one, arm_wildcard, done] {
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

    // -- Pruning tests (header / LLM anchoring) --

    #[test]
    fn viz_prep_prunes_unannotated_structure() {
        // if/loop/call nodes with no header or LLM call anywhere inside are
        // dropped; only the root survives.
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let bg = make_node(1, Some(0), "if (x)", NodeType::BranchGroup);
        let arm1 = make_node(2, Some(1), "if (x)", NodeType::BranchArm);
        let arm2 = make_node(3, Some(1), "else", NodeType::BranchArm);
        let lp = make_node(4, Some(0), "while (y)", NodeType::Loop);
        let call = make_node(5, Some(0), "Helper(x)", NodeType::OtherScope);
        for n in [root, bg, arm1, arm2, lp, call] {
            graph.nodes.insert(n.id, n);
        }
        add_edge(&mut graph, 1, 4);
        add_edge(&mut graph, 4, 5);

        let result = prepare_control_flow_graph_for_visualization(&graph);
        let ids: Vec<u32> = result.nodes.keys().map(NodeId::raw).collect();
        assert_eq!(ids, vec![0], "only the function root should survive");
    }

    #[test]
    fn viz_prep_keeps_llm_calls_without_headers() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let llm_call = make_node(1, Some(0), "Summarize(text)", NodeType::OtherScope)
            .with_llm_client("openai/gpt-4o");
        let plain_call = make_node(2, Some(0), "Helper(x)", NodeType::OtherScope);
        for n in [root, llm_call, plain_call] {
            graph.nodes.insert(n.id, n);
        }
        add_edge(&mut graph, 1, 2);

        let result = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            result.nodes.contains_key(&NodeId::new(1)),
            "LLM call must always render"
        );
        assert!(
            !result.nodes.contains_key(&NodeId::new(2)),
            "plain call should be pruned"
        );
    }

    #[test]
    fn viz_prep_header_inside_loop_keeps_loop() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let lp = make_node(1, Some(0), "for (item in items)", NodeType::Loop);
        let bg = make_node(2, Some(1), "if (x)", NodeType::BranchGroup);
        let arm1 = make_node(3, Some(2), "if (x)", NodeType::BranchArm);
        let header = make_node(4, Some(3), "Annotated", NodeType::HeaderContextEnter);
        let arm2 = make_node(5, Some(2), "else", NodeType::BranchArm);
        for n in [root, lp, bg, arm1, header, arm2] {
            graph.nodes.insert(n.id, n);
        }

        let result = prepare_control_flow_graph_for_visualization(&graph);
        assert!(
            result.nodes.contains_key(&NodeId::new(1)),
            "loop containing a header (transitively) must render"
        );
        assert!(
            result.nodes.contains_key(&NodeId::new(2)),
            "if containing a header must render"
        );
        assert!(
            result.nodes.contains_key(&NodeId::new(5)),
            "the unannotated sibling branch must render too"
        );
        assert!(result.nodes.contains_key(&NodeId::new(4)));
    }

    #[test]
    fn viz_prep_splices_edges_through_pruned_nodes() {
        // Header → plain call → Header: dropping the call must not disconnect
        // the headers.
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let h1 = make_node(1, Some(0), "Step 1", NodeType::HeaderContextEnter);
        let call = make_node(2, Some(0), "Helper(x)", NodeType::OtherScope);
        let h2 = make_node(3, Some(0), "Step 2", NodeType::HeaderContextEnter);
        for n in [root, h1, call, h2] {
            graph.nodes.insert(n.id, n);
        }
        add_edge(&mut graph, 1, 2);
        add_edge(&mut graph, 2, 3);

        let result = prepare_control_flow_graph_for_visualization(&graph);
        assert!(!result.nodes.contains_key(&NodeId::new(2)));
        let h1_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(1))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert_eq!(
            h1_dsts,
            vec![3],
            "edge must be spliced through the pruned call node"
        );
    }

    #[test]
    fn viz_prep_return_in_branch_does_not_reach_successor() {
        // Header
        //   └─ BranchGroup "if (x)" ── successor Header "Done"
        //        ├─ arm "if (x)" └─ Return "return 1"
        //        └─ arm "else"   └─ Header "Continue"
        // The returning arm must NOT flow to "Done".
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let h = make_node(1, Some(0), "Process", NodeType::HeaderContextEnter);
        let bg = make_node(2, Some(1), "if (x)", NodeType::BranchGroup);
        let arm_ret = make_node(3, Some(2), "if (x)", NodeType::BranchArm);
        let ret = make_node(4, Some(3), "return 1", NodeType::Return);
        let arm_else = make_node(5, Some(2), "else", NodeType::BranchArm);
        let h_cont = make_node(6, Some(5), "Continue", NodeType::HeaderContextEnter);
        let h_done = make_node(7, Some(1), "Done", NodeType::HeaderContextEnter);
        for n in [root, h, bg, arm_ret, ret, arm_else, h_cont, h_done] {
            graph.nodes.insert(n.id, n);
        }
        add_edge(&mut graph, 2, 7); // BranchGroup → Done successor

        let result = prepare_control_flow_graph_for_visualization(&graph);

        assert!(
            result.nodes.contains_key(&NodeId::new(4)),
            "return inside a rendered arm must stay visible"
        );
        let ret_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(4))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert!(
            ret_dsts.is_empty(),
            "return node must not flow to the successor, got: {ret_dsts:?}"
        );
        let cont_dsts: Vec<u32> = result
            .edges_by_src
            .get(&NodeId::new(6))
            .map(|es| es.iter().map(|e| e.dst.raw()).collect())
            .unwrap_or_default();
        assert!(
            cont_dsts.contains(&7),
            "non-returning arm should flow to the successor, got: {cont_dsts:?}"
        );
    }

    #[test]
    fn pass1_keeps_wrapper_path_for_nested_return_under_header() {
        let mut graph = ControlFlowGraph::default();
        let root = Node::root(NodeId::new(0), "f|root:0", "func");
        let header = make_node(1, Some(0), "Process", NodeType::HeaderContextEnter);
        let wrapper = make_node(2, Some(1), "let x = if ...", NodeType::OtherScope);
        let bg = make_node(3, Some(2), "if (x)", NodeType::BranchGroup);
        let arm = make_node(4, Some(3), "if (x)", NodeType::BranchArm);
        let ret = make_node(5, Some(4), "return 1", NodeType::Return);
        for n in [root, header, wrapper, bg, arm, ret] {
            graph.nodes.insert(n.id, n);
        }

        let filtered = remove_implicit_nodes(&graph);

        for id in [1, 2, 3, 4, 5] {
            assert!(
                filtered.nodes.contains_key(&NodeId::new(id)),
                "expected node {id} to survive so the nested return stays visible"
            );
        }
    }
}
