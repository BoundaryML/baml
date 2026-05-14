//! Graph diff engine for comparing two `ControlFlowGraph` snapshots.
//!
//! Uses a three-pass matching algorithm:
//! 1. Exact `logFilterKey` match
//! 2. `(nodeType, calleeName)` fallback for unmatched call nodes
//! 3. Positional `(nodeType, label)` fallback within matched parents

use std::collections::{HashMap, HashSet};

use super::{ControlFlowGraph, Node, NodeId, NodeType, build_children_map};

/// Result of comparing two control flow graphs.
///
/// Node IDs reference nodes in the original `base` and `head` graphs —
/// the diff is a lightweight report, not a copy.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphDiff {
    /// Nodes present in `head` but not in `base`.
    pub added: Vec<NodeId>,
    /// Nodes present in `base` but not in `head`.
    pub removed: Vec<NodeId>,
    /// Matched node pairs where metadata differs: `(base_id, head_id)`.
    pub modified: Vec<(NodeId, NodeId)>,
    /// Matched node pairs where metadata is identical: `(base_id, head_id)`.
    pub unchanged: Vec<(NodeId, NodeId)>,
}

/// Compare two nodes on semantically meaningful metadata.
///
/// Excludes: `id` (unstable sequential), `parent_node_id` (changes on reparent),
/// `log_filter_key` (breaks on 6 of 12 scenarios), `source_expr` (arena index),
/// `source_span` (file positions).
fn nodes_metadata_equal(base: &Node, head: &Node) -> bool {
    base.label == head.label
        && base.node_type == head.node_type
        && base.llm_client == head.llm_client
        && base.callee_name == head.callee_name
        && base.is_container == head.is_container
}

/// Compare two control flow graph snapshots and classify every node.
pub fn compute_graph_diff(base: &ControlFlowGraph, head: &ControlFlowGraph) -> GraphDiff {
    let mut matched_base: HashSet<NodeId> = HashSet::new();
    let mut matched_head: HashSet<NodeId> = HashSet::new();
    let mut modified: Vec<(NodeId, NodeId)> = Vec::new();
    let mut unchanged: Vec<(NodeId, NodeId)> = Vec::new();

    // Build lookup: logFilterKey → NodeId for base graph
    let base_by_key: HashMap<&str, NodeId> = base
        .nodes
        .iter()
        .map(|(&id, node)| (node.log_filter_key.as_str(), id))
        .collect();

    // Track which base nodes were matched to which head nodes, for use in
    // pass 2's parent-preference logic
    let mut base_to_head_parent: HashMap<NodeId, NodeId> = HashMap::new();

    // --- Pass 1: Exact logFilterKey match ---
    for (&head_id, head_node) in &head.nodes {
        if let Some(&base_id) = base_by_key.get(head_node.log_filter_key.as_str()) {
            if matched_base.contains(&base_id) {
                continue; // Already matched (duplicate key edge case)
            }
            matched_base.insert(base_id);
            matched_head.insert(head_id);
            base_to_head_parent.insert(base_id, head_id);

            let base_node = &base.nodes[&base_id];
            if nodes_metadata_equal(base_node, head_node) {
                unchanged.push((base_id, head_id));
            } else {
                modified.push((base_id, head_id));
            }
        }
    }

    // --- Pass 2: (nodeType, calleeName) fallback for unmatched call nodes ---
    // Build index of unmatched base nodes by (nodeType, calleeName)
    let mut base_by_callee: HashMap<(&NodeType, &str), Vec<NodeId>> = HashMap::new();
    for (&base_id, base_node) in &base.nodes {
        if matched_base.contains(&base_id) {
            continue;
        }
        if let Some(ref callee) = base_node.callee_name {
            base_by_callee
                .entry((&base_node.node_type, callee.as_str()))
                .or_default()
                .push(base_id);
        }
    }

    for (&head_id, head_node) in &head.nodes {
        if matched_head.contains(&head_id) {
            continue;
        }
        if let Some(ref callee) = head_node.callee_name {
            let key = (&head_node.node_type, callee.as_str());
            if let Some(candidates) = base_by_callee.get_mut(&key) {
                if candidates.is_empty() {
                    continue;
                }
                // Prefer candidate whose parent was already matched to this
                // node's parent (via pass 1), otherwise take the first
                let chosen_idx = if candidates.len() > 1 {
                    candidates
                        .iter()
                        .position(|&base_id| {
                            let base_parent = base.nodes[&base_id].parent_node_id;
                            let head_parent = head_node.parent_node_id;
                            // Check if base_parent was matched to head_parent
                            match (base_parent, head_parent) {
                                (Some(bp), Some(hp)) => base_to_head_parent.get(&bp) == Some(&hp),
                                (None, None) => true,
                                _ => false,
                            }
                        })
                        .unwrap_or(0)
                } else {
                    0
                };
                let base_id = candidates.remove(chosen_idx);
                matched_base.insert(base_id);
                matched_head.insert(head_id);
                base_to_head_parent.insert(base_id, head_id);

                // calleeName matched but other metadata may differ (label, reparenting, etc.)
                let base_node = &base.nodes[&base_id];
                if nodes_metadata_equal(base_node, head_node) {
                    unchanged.push((base_id, head_id));
                } else {
                    modified.push((base_id, head_id));
                }
            }
        }
    }

    // --- Pass 3: Positional (nodeType, label) fallback within matched parents ---
    let base_children = build_children_map(&base.nodes);
    let head_children = build_children_map(&head.nodes);

    // Collect all matched pairs so far for parent-based child lookup
    let matched_pairs: Vec<(NodeId, NodeId)> =
        unchanged.iter().chain(modified.iter()).copied().collect();

    // Include root-level matching (nodes with no parent)
    let root_base: Vec<NodeId> = base
        .nodes
        .keys()
        .filter(|id| base.nodes[*id].parent_node_id.is_none() && !matched_base.contains(id))
        .copied()
        .collect();
    let root_head: Vec<NodeId> = head
        .nodes
        .keys()
        .filter(|id| head.nodes[*id].parent_node_id.is_none() && !matched_head.contains(id))
        .copied()
        .collect();

    positional_match_children(
        &root_base,
        &root_head,
        base,
        head,
        &mut matched_base,
        &mut matched_head,
        &mut modified,
        &mut unchanged,
    );

    for (base_parent, head_parent) in &matched_pairs {
        let base_kids: Vec<NodeId> = base_children
            .get(base_parent)
            .map(|kids| {
                kids.iter()
                    .filter(|id| !matched_base.contains(id))
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        let head_kids: Vec<NodeId> = head_children
            .get(head_parent)
            .map(|kids| {
                kids.iter()
                    .filter(|id| !matched_head.contains(id))
                    .copied()
                    .collect()
            })
            .unwrap_or_default();

        positional_match_children(
            &base_kids,
            &head_kids,
            base,
            head,
            &mut matched_base,
            &mut matched_head,
            &mut modified,
            &mut unchanged,
        );
    }

    // --- Collect unmatched as added/removed ---
    let removed: Vec<NodeId> = base
        .nodes
        .keys()
        .filter(|id| !matched_base.contains(id))
        .copied()
        .collect();
    let added: Vec<NodeId> = head
        .nodes
        .keys()
        .filter(|id| !matched_head.contains(id))
        .copied()
        .collect();

    GraphDiff {
        added,
        removed,
        modified,
        unchanged,
    }
}

/// Match unmatched children by `(nodeType, label)` — used in pass 3.
#[allow(clippy::too_many_arguments)]
fn positional_match_children(
    base_kids: &[NodeId],
    head_kids: &[NodeId],
    base: &ControlFlowGraph,
    head: &ControlFlowGraph,
    matched_base: &mut HashSet<NodeId>,
    matched_head: &mut HashSet<NodeId>,
    modified: &mut Vec<(NodeId, NodeId)>,
    unchanged: &mut Vec<(NodeId, NodeId)>,
) {
    // Index base children by (nodeType, label)
    let mut base_by_type_label: HashMap<(&NodeType, &str), Vec<NodeId>> = HashMap::new();
    for &base_id in base_kids {
        let node = &base.nodes[&base_id];
        base_by_type_label
            .entry((&node.node_type, node.label.as_str()))
            .or_default()
            .push(base_id);
    }

    for &head_id in head_kids {
        if matched_head.contains(&head_id) {
            continue;
        }
        let head_node = &head.nodes[&head_id];
        let key = (&head_node.node_type, head_node.label.as_str());
        if let Some(candidates) = base_by_type_label.get_mut(&key) {
            if let Some(pos) = candidates.iter().position(|id| !matched_base.contains(id)) {
                let base_id = candidates[pos];
                matched_base.insert(base_id);
                matched_head.insert(head_id);

                let base_node = &base.nodes[&base_id];
                if nodes_metadata_equal(base_node, head_node) {
                    unchanged.push((base_id, head_id));
                } else {
                    modified.push((base_id, head_id));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control_flow::{Edge, NodeType};

    /// Construct a non-call node.
    fn make_node(
        id: u32,
        parent: Option<u32>,
        label: &str,
        node_type: NodeType,
        key: &str,
    ) -> Node {
        Node {
            id: NodeId::new(id),
            parent_node_id: parent.map(NodeId::new),
            log_filter_key: key.to_string(),
            label: label.to_string(),
            source_expr: None,
            node_type,
            llm_client: None,
            callee_name: None,
            source_span: None,
            is_container: false,
        }
    }

    /// Construct a call-site `OtherScope` node with `callee_name`.
    fn make_call_node(id: u32, parent: Option<u32>, label: &str, callee: &str, key: &str) -> Node {
        let mut node = make_node(id, parent, label, NodeType::OtherScope, key);
        node.callee_name = Some(callee.to_string());
        node
    }

    /// Build a `ControlFlowGraph` from nodes and edge pairs.
    fn build_graph(nodes: Vec<Node>, edges: Vec<(u32, u32)>) -> ControlFlowGraph {
        let mut graph = ControlFlowGraph::default();
        for node in nodes {
            graph.nodes.insert(node.id, node);
        }
        for (src, dst) in edges {
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
        graph
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Add a function call
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_1_add_function_call() {
        // Base: Pipeline -> Summarize(text)
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Summarize(text)",
                    "Summarize",
                    "Pipeline|root:0|scope:summarize-text:0",
                ),
            ],
            vec![(0, 1)],
        );

        // Head: Pipeline -> Summarize(text) -> EditSummary(summary)
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Summarize(text)",
                    "Summarize",
                    "Pipeline|root:0|scope:summarize-text:0",
                ),
                make_call_node(
                    2,
                    Some(0),
                    "EditSummary(summary)",
                    "EditSummary",
                    "Pipeline|root:0|scope:editsummary-summary:1",
                ),
            ],
            vec![(0, 1), (1, 2)],
        );

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.added.len(), 1, "one node added");
        assert_eq!(diff.removed.len(), 0, "no nodes removed");
        assert_eq!(diff.modified.len(), 0, "no nodes modified");
        assert_eq!(diff.unchanged.len(), 2, "root + Summarize unchanged");

        let added_node = &head.nodes[&diff.added[0]];
        assert_eq!(added_node.callee_name.as_deref(), Some("EditSummary"));
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Remove a function call
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_2_remove_function_call() {
        // Base: Pipeline -> StepA -> StepB -> StepC
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "StepA(text)",
                    "StepA",
                    "Pipeline|root:0|scope:stepa-text:0",
                ),
                make_call_node(
                    2,
                    Some(0),
                    "StepB(a)",
                    "StepB",
                    "Pipeline|root:0|scope:stepb-a:1",
                ),
                make_call_node(
                    3,
                    Some(0),
                    "StepC(b)",
                    "StepC",
                    "Pipeline|root:0|scope:stepc-b:2",
                ),
            ],
            vec![(0, 1), (1, 2), (2, 3)],
        );

        // Head: Pipeline -> StepA -> StepC(a) -- StepB removed, StepC label changed
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "StepA(text)",
                    "StepA",
                    "Pipeline|root:0|scope:stepa-text:0",
                ),
                // StepC's ordinal shifted and label changed (arg changed from b to a)
                make_call_node(
                    2,
                    Some(0),
                    "StepC(a)",
                    "StepC",
                    "Pipeline|root:0|scope:stepc-a:1",
                ),
            ],
            vec![(0, 1), (1, 2)],
        );

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.removed.len(), 1, "StepB removed");
        assert_eq!(diff.added.len(), 0, "no nodes added");

        let removed_node = &base.nodes[&diff.removed[0]];
        assert_eq!(removed_node.callee_name.as_deref(), Some("StepB"));

        // StepC matched via calleeName fallback (pass 2) -- label changed so it's modified
        assert_eq!(diff.modified.len(), 1, "StepC modified (label changed)");
        let (base_id, head_id) = diff.modified[0];
        assert_eq!(base.nodes[&base_id].callee_name.as_deref(), Some("StepC"));
        assert_eq!(head.nodes[&head_id].label, "StepC(a)");

        // Root + StepA unchanged
        assert_eq!(diff.unchanged.len(), 2);
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Rename a variable (cosmetic change)
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_3_rename_variable() {
        // Base: Summarize(text)
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Summarize(text)",
                    "Summarize",
                    "Pipeline|root:0|scope:summarize-text:0",
                ),
            ],
            vec![(0, 1)],
        );

        // Head: Summarize(input) -- label changed, calleeName stable, logFilterKey changed
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Summarize(input)",
                    "Summarize",
                    "Pipeline|root:0|scope:summarize-input:0",
                ),
            ],
            vec![(0, 1)],
        );

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.added.len(), 0);
        assert_eq!(diff.removed.len(), 0);
        // Summarize matched via calleeName (pass 2, since logFilterKey changed)
        assert_eq!(diff.modified.len(), 1, "Summarize modified (label changed)");
        assert_eq!(diff.unchanged.len(), 1, "root unchanged");

        let (base_id, _) = diff.modified[0];
        assert_eq!(
            base.nodes[&base_id].callee_name.as_deref(),
            Some("Summarize")
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Add a branch (if/else)
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_4_add_branch() {
        // Base: Route -> Process(text)
        let base = build_graph(
            vec![
                make_node(0, None, "Route", NodeType::FunctionRoot, "Route|root:0"),
                make_call_node(
                    1,
                    Some(0),
                    "Process(text)",
                    "Process",
                    "Route|root:0|scope:process-text:0",
                ),
            ],
            vec![(0, 1)],
        );

        // Head: Route -> OtherScope wrapper -> BranchGroup -> BranchArm(if) -> QuickProcess
        //                                                  -> BranchArm(else) -> Process(text)
        let head = build_graph(
            vec![
                make_node(0, None, "Route", NodeType::FunctionRoot, "Route|root:0"),
                make_node(
                    1,
                    Some(0),
                    "let result = ...",
                    NodeType::OtherScope,
                    "Route|root:0|scope:let-result:0",
                ),
                make_node(
                    2,
                    Some(1),
                    "if (fast)",
                    NodeType::BranchGroup,
                    "Route|root:0|scope:let-result:0|bg:if-fast:0",
                ),
                make_node(
                    3,
                    Some(2),
                    "if (fast)",
                    NodeType::BranchArm,
                    "Route|root:0|scope:let-result:0|bg:if-fast:0|arm:if-fast:0",
                ),
                make_call_node(
                    4,
                    Some(3),
                    "QuickProcess(text)",
                    "QuickProcess",
                    "Route|root:0|scope:let-result:0|bg:if-fast:0|arm:if-fast:0|scope:quickprocess-text:0",
                ),
                make_node(
                    5,
                    Some(2),
                    "else",
                    NodeType::BranchArm,
                    "Route|root:0|scope:let-result:0|bg:if-fast:0|arm:else:1",
                ),
                // Process(text) reparented under else arm -- logFilterKey changed entirely
                make_call_node(
                    6,
                    Some(5),
                    "Process(text)",
                    "Process",
                    "Route|root:0|scope:let-result:0|bg:if-fast:0|arm:else:1|scope:process-text:0",
                ),
            ],
            vec![(0, 1), (1, 2), (2, 3), (3, 4), (2, 5), (5, 6)],
        );

        let diff = compute_graph_diff(&base, &head);

        // Process(text) matched via calleeName fallback -- label same but reparented
        // so it's "unchanged" by metadata (label, nodeType, calleeName all match)
        let process_in_unchanged = diff.unchanged.iter().any(|(base_id, head_id)| {
            base.nodes[base_id].callee_name.as_deref() == Some("Process")
                && head.nodes[head_id].callee_name.as_deref() == Some("Process")
        });
        assert!(
            process_in_unchanged,
            "Process should be matched (unchanged metadata)"
        );

        // Root matched via logFilterKey (unchanged)
        assert!(
            diff.unchanged
                .iter()
                .any(|(base_id, _)| { base.nodes[base_id].node_type == NodeType::FunctionRoot }),
            "FunctionRoot should be unchanged"
        );

        // 5 new nodes: wrapper OtherScope, BranchGroup, 2 BranchArms, QuickProcess
        assert_eq!(diff.added.len(), 5, "5 structural nodes added");
        assert_eq!(diff.removed.len(), 0, "no nodes removed");
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Add a branch arm (else-if)
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_5_add_branch_arm() {
        // Base: BranchGroup with 2 arms (if + else)
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Classify",
                    NodeType::FunctionRoot,
                    "Classify|root:0",
                ),
                make_node(
                    1,
                    Some(0),
                    "let result = ...",
                    NodeType::OtherScope,
                    "Classify|root:0|scope:let-result:0",
                ),
                make_node(
                    2,
                    Some(1),
                    "if (is_short(text))",
                    NodeType::BranchGroup,
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0",
                ),
                make_node(
                    3,
                    Some(2),
                    "if (is_short(text))",
                    NodeType::BranchArm,
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:if-is-short-text:0",
                ),
                make_call_node(
                    4,
                    Some(3),
                    "ShortSummary(text)",
                    "ShortSummary",
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:if-is-short-text:0|scope:shortsummary-text:0",
                ),
                make_node(
                    5,
                    Some(2),
                    "else",
                    NodeType::BranchArm,
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:else:1",
                ),
                make_call_node(
                    6,
                    Some(5),
                    "LongSummary(text)",
                    "LongSummary",
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:else:1|scope:longsummary-text:0",
                ),
            ],
            vec![(0, 1), (1, 2), (2, 3), (3, 4), (2, 5), (5, 6)],
        );

        // Head: Same + new else-if arm inserted between if and else
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Classify",
                    NodeType::FunctionRoot,
                    "Classify|root:0",
                ),
                make_node(
                    1,
                    Some(0),
                    "let result = ...",
                    NodeType::OtherScope,
                    "Classify|root:0|scope:let-result:0",
                ),
                make_node(
                    2,
                    Some(1),
                    "if (is_short(text))",
                    NodeType::BranchGroup,
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0",
                ),
                make_node(
                    3,
                    Some(2),
                    "if (is_short(text))",
                    NodeType::BranchArm,
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:if-is-short-text:0",
                ),
                make_call_node(
                    4,
                    Some(3),
                    "ShortSummary(text)",
                    "ShortSummary",
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:if-is-short-text:0|scope:shortsummary-text:0",
                ),
                // New else-if arm
                make_node(
                    7,
                    Some(2),
                    "else if (is_medium(text))",
                    NodeType::BranchArm,
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:else-if-is-medium-text:1",
                ),
                make_call_node(
                    8,
                    Some(7),
                    "MediumSummary(text)",
                    "MediumSummary",
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:else-if-is-medium-text:1|scope:mediumsummary-text:0",
                ),
                // else arm ordinal shifted from :1 to :2
                make_node(
                    5,
                    Some(2),
                    "else",
                    NodeType::BranchArm,
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:else:2",
                ),
                make_call_node(
                    6,
                    Some(5),
                    "LongSummary(text)",
                    "LongSummary",
                    "Classify|root:0|scope:let-result:0|bg:if-is-short-text:0|arm:else:2|scope:longsummary-text:0",
                ),
            ],
            vec![
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 4),
                (2, 7),
                (7, 8),
                (2, 5),
                (5, 6),
            ],
        );

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.added.len(), 2, "new arm + MediumSummary added");
        assert_eq!(diff.removed.len(), 0, "no nodes removed");

        // Existing arms + calls should be matched (some may be modified due to ordinal shift in logFilterKey)
        let total_matched = diff.unchanged.len() + diff.modified.len();
        assert_eq!(total_matched, 7, "all 7 base nodes matched");
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Reorder sibling calls
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_6_reorder_siblings() {
        // Base: Alpha -> Beta -> Gamma
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Alpha(text)",
                    "Alpha",
                    "Pipeline|root:0|scope:alpha-text:0",
                ),
                make_call_node(
                    2,
                    Some(0),
                    "Beta(a)",
                    "Beta",
                    "Pipeline|root:0|scope:beta-a:1",
                ),
                make_call_node(
                    3,
                    Some(0),
                    "Gamma(b)",
                    "Gamma",
                    "Pipeline|root:0|scope:gamma-b:2",
                ),
            ],
            vec![(0, 1), (1, 2), (2, 3)],
        );

        // Head: Beta -> Alpha -> Gamma (reordered, labels changed due to arg changes)
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Beta(text)",
                    "Beta",
                    "Pipeline|root:0|scope:beta-text:0",
                ),
                make_call_node(
                    2,
                    Some(0),
                    "Alpha(b)",
                    "Alpha",
                    "Pipeline|root:0|scope:alpha-b:1",
                ),
                make_call_node(
                    3,
                    Some(0),
                    "Gamma(a)",
                    "Gamma",
                    "Pipeline|root:0|scope:gamma-a:2",
                ),
            ],
            vec![(0, 1), (1, 2), (2, 3)],
        );

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.added.len(), 0, "no nodes added");
        assert_eq!(diff.removed.len(), 0, "no nodes removed");
        // All three calls matched via calleeName (pass 2) -- labels all changed
        assert_eq!(
            diff.modified.len(),
            3,
            "all 3 calls modified (labels changed)"
        );
        assert_eq!(diff.unchanged.len(), 1, "root unchanged");

        // Verify calleeName matching
        for (base_id, head_id) in &diff.modified {
            assert_eq!(
                base.nodes[base_id].callee_name, head.nodes[head_id].callee_name,
                "matched nodes should have same calleeName"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Add header sections
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_7_add_header_section() {
        // Base: Root -> Summarize -> GenerateDraft (flat)
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Summarize(text)",
                    "Summarize",
                    "Pipeline|root:0|scope:summarize-text:0",
                ),
                make_call_node(
                    2,
                    Some(0),
                    "GenerateDraft(summary)",
                    "GenerateDraft",
                    "Pipeline|root:0|scope:generatedraft-summary:1",
                ),
            ],
            vec![(0, 1), (1, 2)],
        );

        // Head: Root -> Header("Analysis") -> Summarize, Header("Generation") -> GenerateDraft
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_node(
                    1,
                    Some(0),
                    "Analysis",
                    NodeType::HeaderContextEnter,
                    "Pipeline|root:0|hdr:analysis:0",
                ),
                // Summarize reparented under header -- logFilterKey changed
                make_call_node(
                    2,
                    Some(1),
                    "Summarize(text)",
                    "Summarize",
                    "Pipeline|root:0|hdr:analysis:0|scope:summarize-text:0",
                ),
                make_node(
                    3,
                    Some(0),
                    "Generation",
                    NodeType::HeaderContextEnter,
                    "Pipeline|root:0|hdr:generation:1",
                ),
                make_call_node(
                    4,
                    Some(3),
                    "GenerateDraft(summary)",
                    "GenerateDraft",
                    "Pipeline|root:0|hdr:generation:1|scope:generatedraft-summary:0",
                ),
            ],
            vec![(0, 1), (1, 2), (0, 3), (3, 4)],
        );

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.added.len(), 2, "2 headers added");
        assert_eq!(diff.removed.len(), 0, "no nodes removed");

        // Both calls matched via calleeName (labels unchanged, metadata equal)
        let summarize_matched = diff.unchanged.iter().any(|(b, h)| {
            base.nodes[b].callee_name.as_deref() == Some("Summarize")
                && head.nodes[h].callee_name.as_deref() == Some("Summarize")
        });
        let draft_matched = diff.unchanged.iter().any(|(b, h)| {
            base.nodes[b].callee_name.as_deref() == Some("GenerateDraft")
                && head.nodes[h].callee_name.as_deref() == Some("GenerateDraft")
        });
        assert!(summarize_matched, "Summarize matched via calleeName");
        assert!(draft_matched, "GenerateDraft matched via calleeName");
    }

    // -----------------------------------------------------------------------
    // Scenario 8: Change LLM client
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_8_change_llm_client() {
        // Base: LLM function with GPT4
        let mut base_node = make_node(
            0,
            None,
            "Summarize",
            NodeType::LlmFunction,
            "Summarize|root:0",
        );
        base_node.llm_client = Some("GPT4".to_string());
        let base = build_graph(vec![base_node], vec![]);

        // Head: Same function, different client
        let mut head_node = make_node(
            0,
            None,
            "Summarize",
            NodeType::LlmFunction,
            "Summarize|root:0",
        );
        head_node.llm_client = Some("Claude".to_string());
        let head = build_graph(vec![head_node], vec![]);

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.added.len(), 0);
        assert_eq!(diff.removed.len(), 0);
        assert_eq!(diff.modified.len(), 1, "LLM client changed");
        assert_eq!(diff.unchanged.len(), 0);

        // Matched via exact logFilterKey (pass 1)
        let (base_id, head_id) = diff.modified[0];
        assert_eq!(base.nodes[&base_id].llm_client.as_deref(), Some("GPT4"));
        assert_eq!(head.nodes[&head_id].llm_client.as_deref(), Some("Claude"));
    }

    // -----------------------------------------------------------------------
    // Scenario 9: Convert expression function to LLM function
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_9_convert_to_llm_function() {
        // Base: expr function with Split -> Loop -> SummarizeChunk -> Combine
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Summarize",
                    NodeType::FunctionRoot,
                    "Summarize|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "Split(text)",
                    "Split",
                    "Summarize|root:0|scope:split-text:0",
                ),
                make_node(
                    2,
                    Some(0),
                    "for (let chunk in chunks)",
                    NodeType::Loop,
                    "Summarize|root:0|loop:for-let-chunk-in-chunks:0",
                ),
                make_call_node(
                    3,
                    Some(2),
                    "SummarizeChunk(chunk)",
                    "SummarizeChunk",
                    "Summarize|root:0|loop:for-let-chunk-in-chunks:0|scope:summarizechunk-chunk:0",
                ),
                make_call_node(
                    4,
                    Some(0),
                    "Combine(summaries)",
                    "Combine",
                    "Summarize|root:0|scope:combine-summaries:1",
                ),
            ],
            vec![(0, 1), (1, 2), (2, 3), (3, 4)],
        );

        // Head: single LlmFunction node (complete replacement)
        let mut llm_node = make_node(
            0,
            None,
            "Summarize",
            NodeType::LlmFunction,
            "Summarize|root:0",
        );
        llm_node.llm_client = Some("GPT4".to_string());
        let head = build_graph(vec![llm_node], vec![]);

        let diff = compute_graph_diff(&base, &head);
        // FunctionRoot -> LlmFunction: logFilterKey matches but nodeType changed -> modified
        // All other base nodes have no match -> removed
        assert_eq!(
            diff.removed.len(),
            4,
            "Split, Loop, SummarizeChunk, Combine removed"
        );
        assert_eq!(diff.added.len(), 0, "LlmFunction matched to FunctionRoot");
        assert_eq!(diff.modified.len(), 1, "root matched but nodeType changed");
        assert_eq!(diff.unchanged.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Scenario 10: Wrap a call in a loop
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_10_wrap_in_loop() {
        // Base: Process -> Transform(items)
        let base = build_graph(
            vec![
                make_node(0, None, "Process", NodeType::FunctionRoot, "Process|root:0"),
                make_call_node(
                    1,
                    Some(0),
                    "Transform(items)",
                    "Transform",
                    "Process|root:0|scope:transform-items:0",
                ),
            ],
            vec![(0, 1)],
        );

        // Head: Process -> Loop -> Transform(item) + Combine(results)
        let head = build_graph(
            vec![
                make_node(0, None, "Process", NodeType::FunctionRoot, "Process|root:0"),
                make_node(
                    1,
                    Some(0),
                    "for (let item in items)",
                    NodeType::Loop,
                    "Process|root:0|loop:for-let-item-in-items:0",
                ),
                // Transform reparented under loop, label changed
                make_call_node(
                    2,
                    Some(1),
                    "Transform(item)",
                    "Transform",
                    "Process|root:0|loop:for-let-item-in-items:0|scope:transform-item:0",
                ),
                make_call_node(
                    3,
                    Some(0),
                    "Combine(results)",
                    "Combine",
                    "Process|root:0|scope:combine-results:0",
                ),
            ],
            vec![(0, 1), (1, 2), (2, 3)],
        );

        let diff = compute_graph_diff(&base, &head);
        assert_eq!(diff.removed.len(), 0, "no nodes removed");
        assert_eq!(diff.added.len(), 2, "Loop + Combine added");

        // Transform matched via calleeName (pass 2) -- label changed
        assert_eq!(
            diff.modified.len(),
            1,
            "Transform modified (label + reparented)"
        );
        let (base_id, head_id) = diff.modified[0];
        assert_eq!(
            base.nodes[&base_id].callee_name.as_deref(),
            Some("Transform")
        );
        assert_eq!(
            head.nodes[&head_id].callee_name.as_deref(),
            Some("Transform")
        );

        assert_eq!(diff.unchanged.len(), 1, "root unchanged");
    }

    // -----------------------------------------------------------------------
    // Scenario 11: Rename a called function
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_11_rename_called_function() {
        // Base: Pipeline -> OldName(text)
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "OldName(text)",
                    "OldName",
                    "Pipeline|root:0|scope:oldname-text:0",
                ),
            ],
            vec![(0, 1)],
        );

        // Head: Pipeline -> NewName(text) -- calleeName changed
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "Pipeline",
                    NodeType::FunctionRoot,
                    "Pipeline|root:0",
                ),
                make_call_node(
                    1,
                    Some(0),
                    "NewName(text)",
                    "NewName",
                    "Pipeline|root:0|scope:newname-text:0",
                ),
            ],
            vec![(0, 1)],
        );

        let diff = compute_graph_diff(&base, &head);
        // Both logFilterKey and calleeName changed -- only positional match possible
        // Since both are OtherScope children of root, neither label nor calleeName matches
        // -> removed + added (no reliable signal to pair them)
        assert_eq!(diff.removed.len(), 1, "OldName removed");
        assert_eq!(diff.added.len(), 1, "NewName added");
        assert_eq!(diff.unchanged.len(), 1, "root unchanged");
    }

    // -----------------------------------------------------------------------
    // Scenario 12: Nested structural changes
    // -----------------------------------------------------------------------
    #[test]
    fn test_scenario_12_nested_structural_changes() {
        // Base: ContentPipeline with headers, GenerateDraft + EditDraft under "Generate"
        let base = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "ContentPipeline",
                    NodeType::FunctionRoot,
                    "ContentPipeline|root:0",
                ),
                make_node(
                    1,
                    Some(0),
                    "Analyze",
                    NodeType::HeaderContextEnter,
                    "ContentPipeline|root:0|hdr:analyze:0",
                ),
                make_call_node(
                    2,
                    Some(1),
                    "Summarize(text)",
                    "Summarize",
                    "ContentPipeline|root:0|hdr:analyze:0|scope:summarize-text:0",
                ),
                make_node(
                    3,
                    Some(0),
                    "Generate",
                    NodeType::HeaderContextEnter,
                    "ContentPipeline|root:0|hdr:generate:1",
                ),
                make_call_node(
                    4,
                    Some(3),
                    "GenerateDraft(summary)",
                    "GenerateDraft",
                    "ContentPipeline|root:0|hdr:generate:1|scope:generatedraft-summary:0",
                ),
                make_call_node(
                    5,
                    Some(3),
                    "EditDraft(draft)",
                    "EditDraft",
                    "ContentPipeline|root:0|hdr:generate:1|scope:editdraft-draft:1",
                ),
            ],
            vec![(0, 1), (1, 2), (0, 3), (3, 4), (4, 5)],
        );

        // Head: EditDraft removed, GenerateDraft wrapped in conditional
        let head = build_graph(
            vec![
                make_node(
                    0,
                    None,
                    "ContentPipeline",
                    NodeType::FunctionRoot,
                    "ContentPipeline|root:0",
                ),
                make_node(
                    1,
                    Some(0),
                    "Analyze",
                    NodeType::HeaderContextEnter,
                    "ContentPipeline|root:0|hdr:analyze:0",
                ),
                make_call_node(
                    2,
                    Some(1),
                    "Summarize(text)",
                    "Summarize",
                    "ContentPipeline|root:0|hdr:analyze:0|scope:summarize-text:0",
                ),
                make_node(
                    3,
                    Some(0),
                    "Generate",
                    NodeType::HeaderContextEnter,
                    "ContentPipeline|root:0|hdr:generate:1",
                ),
                make_node(
                    6,
                    Some(3),
                    "let draft = ...",
                    NodeType::OtherScope,
                    "ContentPipeline|root:0|hdr:generate:1|scope:let-draft:0",
                ),
                make_node(
                    7,
                    Some(6),
                    "if (is_long(summary))",
                    NodeType::BranchGroup,
                    "ContentPipeline|root:0|hdr:generate:1|scope:let-draft:0|bg:if-is-long-summary:0",
                ),
                make_node(
                    8,
                    Some(7),
                    "if (is_long(summary))",
                    NodeType::BranchArm,
                    "ContentPipeline|root:0|hdr:generate:1|scope:let-draft:0|bg:if-is-long-summary:0|arm:if-is-long-summary:0",
                ),
                make_call_node(
                    9,
                    Some(8),
                    "GenerateLongDraft(summary)",
                    "GenerateLongDraft",
                    "ContentPipeline|root:0|hdr:generate:1|scope:let-draft:0|bg:if-is-long-summary:0|arm:if-is-long-summary:0|scope:generatelongdraft-summary:0",
                ),
                make_node(
                    10,
                    Some(7),
                    "else",
                    NodeType::BranchArm,
                    "ContentPipeline|root:0|hdr:generate:1|scope:let-draft:0|bg:if-is-long-summary:0|arm:else:1",
                ),
                // GenerateDraft reparented deeper
                make_call_node(
                    11,
                    Some(10),
                    "GenerateDraft(summary)",
                    "GenerateDraft",
                    "ContentPipeline|root:0|hdr:generate:1|scope:let-draft:0|bg:if-is-long-summary:0|arm:else:1|scope:generatedraft-summary:0",
                ),
            ],
            vec![
                (0, 1),
                (1, 2),
                (0, 3),
                (3, 6),
                (6, 7),
                (7, 8),
                (8, 9),
                (7, 10),
                (10, 11),
            ],
        );

        let diff = compute_graph_diff(&base, &head);

        // "Analyze" section entirely unchanged
        let analyze_unchanged = diff
            .unchanged
            .iter()
            .any(|(b, _)| base.nodes[b].label == "Analyze");
        assert!(analyze_unchanged, "Analyze header should be unchanged");

        let summarize_unchanged = diff
            .unchanged
            .iter()
            .any(|(b, _)| base.nodes[b].callee_name.as_deref() == Some("Summarize"));
        assert!(summarize_unchanged, "Summarize should be unchanged");

        // EditDraft removed
        assert!(
            diff.removed
                .iter()
                .any(|id| { base.nodes[id].callee_name.as_deref() == Some("EditDraft") }),
            "EditDraft should be removed"
        );

        // GenerateDraft matched via calleeName -- metadata unchanged
        let gd_matched = diff.unchanged.iter().any(|(b, h)| {
            base.nodes[b].callee_name.as_deref() == Some("GenerateDraft")
                && head.nodes[h].callee_name.as_deref() == Some("GenerateDraft")
        });
        assert!(gd_matched, "GenerateDraft should be matched via calleeName");

        // New structural nodes added
        assert!(
            diff.added.len() >= 4,
            "at least wrapper, branch group, 2 arms, GenerateLongDraft added"
        );
    }
}
