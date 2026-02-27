//! Generic two-pass direct+transitive analysis framework.
//!
//! Provides a reusable cache for computing per-node "facts" that propagate
//! transitively through a dependency graph. The canonical first consumer is
//! exception propagation analysis, but the framework is domain-agnostic.
//!
//! # Two-pass architecture
//!
//! 1. **Direct pass** — visits each node in isolation and computes its *direct*
//!    facts (e.g., "this function directly throws `ValidationError`").
//!
//! 2. **Transitive pass** — propagates facts along dependency edges until a
//!    fixed point is reached (e.g., "this function transitively throws
//!    `ValidationError` because it calls a function that does").
//!
//! # Design principles
//!
//! - **Parametric**: generic over node identity (`N`), fact type (`F`), and
//!   edge structure. No coupling to any specific analysis domain.
//! - **Deterministic**: uses [`BTreeMap`] / [`BTreeSet`] throughout for
//!   reproducible iteration order, enabling stable diagnostics and golden tests.
//! - **Cycle-safe**: the transitive pass uses Tarjan's SCC algorithm so that
//!   nodes in a cycle share the union of their facts (fixed-point semantics).
//! - **Incremental-ready**: cache keying by node identity means a future
//!   Salsa/DB integration can invalidate individual entries without full
//!   recomputation.
//!
//! # Usage
//!
//! ```rust,ignore
//! use baml_compiler_analysis::{AnalysisGraph, AnalysisResult};
//!
//! // 1. Build a graph describing nodes and their dependencies.
//! let mut graph = AnalysisGraph::new();
//! graph.add_node("main", btreeset!["ValidationError"]);
//! graph.add_node("helper", btreeset!["HttpError"]);
//! graph.add_edge("main", "helper");
//!
//! // 2. Compute the analysis.
//! let result = graph.analyze();
//!
//! // 3. Query direct or transitive facts.
//! assert_eq!(result.direct("main"), Some(&btreeset!["ValidationError"]));
//! assert!(result.transitive("main").unwrap().contains("HttpError"));
//! ```

use std::collections::{BTreeMap, BTreeSet};

// ============================================================================
// Core types
// ============================================================================

/// A dependency graph annotated with per-node direct facts.
///
/// `N` is the node identifier type (must be `Ord` for deterministic ordering).
/// `F` is the fact type (must be `Ord` for set operations).
///
/// The graph is constructed incrementally via [`add_node`](Self::add_node) and
/// [`add_edge`](Self::add_edge), then consumed by [`analyze`](Self::analyze) to
/// produce an [`AnalysisResult`].
#[derive(Debug, Clone)]
pub struct AnalysisGraph<N: Ord + Clone, F: Ord + Clone> {
    /// Per-node direct facts (pass 1 output, provided by the caller).
    direct: BTreeMap<N, BTreeSet<F>>,
    /// Adjacency list: `node → {dependency₁, dependency₂, …}`.
    /// An edge from A to B means "A depends on B" (i.e., A calls B).
    edges: BTreeMap<N, BTreeSet<N>>,
}

impl<N: Ord + Clone, F: Ord + Clone> Default for AnalysisGraph<N, F> {
    fn default() -> Self {
        Self::new()
    }
}

impl<N: Ord + Clone, F: Ord + Clone> AnalysisGraph<N, F> {
    /// Create an empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            direct: BTreeMap::new(),
            edges: BTreeMap::new(),
        }
    }

    /// Register a node with its direct facts.
    ///
    /// If the node already exists, its facts are replaced.
    pub fn add_node(&mut self, node: N, facts: BTreeSet<F>) {
        self.direct.insert(node.clone(), facts);
        // Ensure the node appears in the edge map even with no edges.
        self.edges.entry(node).or_default();
    }

    /// Register a node with no direct facts.
    ///
    /// Equivalent to `add_node(node, BTreeSet::new())`.
    pub fn add_node_empty(&mut self, node: N) {
        self.direct.entry(node.clone()).or_default();
        self.edges.entry(node).or_default();
    }

    /// Add a directed dependency edge: `from` depends on `to`.
    ///
    /// Both endpoints are implicitly registered if not already present.
    pub fn add_edge(&mut self, from: N, to: N) {
        self.direct.entry(from.clone()).or_default();
        self.direct.entry(to.clone()).or_default();
        self.edges.entry(to.clone()).or_default();
        self.edges.entry(from).or_default().insert(to);
    }

    /// Consume the graph and compute the transitive closure of facts.
    ///
    /// Returns an [`AnalysisResult`] containing both direct and transitive
    /// facts for every node in the graph.
    #[must_use]
    pub fn analyze(self) -> AnalysisResult<N, F> {
        let sccs = Tarjan::components(&self.edges);
        let transitive = propagate(&self.direct, &self.edges, &sccs);
        AnalysisResult {
            direct: self.direct,
            transitive,
        }
    }

    /// Return the number of nodes in the graph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.direct.len()
    }

    /// Return the number of edges in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.values().map(BTreeSet::len).sum()
    }
}

// ============================================================================
// Analysis result
// ============================================================================

/// The output of a two-pass analysis.
///
/// Contains both the direct facts (provided as input) and the computed
/// transitive facts (closed over dependency edges).
#[derive(Debug, Clone)]
pub struct AnalysisResult<N: Ord, F: Ord> {
    /// Per-node direct facts (unchanged from input).
    direct: BTreeMap<N, BTreeSet<F>>,
    /// Per-node transitive facts (direct ∪ propagated from dependencies).
    transitive: BTreeMap<N, BTreeSet<F>>,
}

impl<N: Ord, F: Ord> AnalysisResult<N, F> {
    /// Direct facts for `node` (what the node itself produces).
    #[must_use]
    pub fn direct(&self, node: &N) -> Option<&BTreeSet<F>> {
        self.direct.get(node)
    }

    /// Transitive facts for `node` (direct ∪ all reachable dependencies).
    #[must_use]
    pub fn transitive(&self, node: &N) -> Option<&BTreeSet<F>> {
        self.transitive.get(node)
    }

    /// Iterate over all nodes and their direct facts in deterministic order.
    pub fn iter_direct(&self) -> impl Iterator<Item = (&N, &BTreeSet<F>)> {
        self.direct.iter()
    }

    /// Iterate over all nodes and their transitive facts in deterministic order.
    pub fn iter_transitive(&self) -> impl Iterator<Item = (&N, &BTreeSet<F>)> {
        self.transitive.iter()
    }

    /// The set of all nodes in the analysis.
    pub fn nodes(&self) -> impl Iterator<Item = &N> {
        self.direct.keys()
    }

    /// Facts that are *only* transitive — present in the transitive set but
    /// not in the direct set.  Useful for diagnostics ("inherited from callee").
    #[must_use]
    pub fn inherited(&self, node: &N) -> Option<BTreeSet<&F>> {
        let trans = self.transitive.get(node)?;
        let direct = self.direct.get(node);
        Some(
            trans
                .iter()
                .filter(|f| direct.is_none_or(|d| !d.contains(f)))
                .collect(),
        )
    }
}

// ============================================================================
// Transitive propagation (pass 2)
// ============================================================================

/// Propagate facts transitively using the condensation (DAG of SCCs).
///
/// Within each SCC, all nodes share the same transitive fact set (fixed point).
/// The condensation is processed in reverse topological order so that when we
/// visit an SCC, all of its successors have already been resolved.
fn propagate<N: Ord + Clone, F: Ord + Clone>(
    direct: &BTreeMap<N, BTreeSet<F>>,
    edges: &BTreeMap<N, BTreeSet<N>>,
    sccs: &[Vec<N>],
) -> BTreeMap<N, BTreeSet<F>> {
    // Map each node to its SCC index for O(1) lookup.
    let mut node_to_scc: BTreeMap<&N, usize> = BTreeMap::new();
    for (scc_idx, component) in sccs.iter().enumerate() {
        for node in component {
            node_to_scc.insert(node, scc_idx);
        }
    }

    // Per-SCC accumulated facts, indexed by SCC index.
    let mut scc_facts: Vec<BTreeSet<F>> = Vec::with_capacity(sccs.len());

    // Our Tarjan implementation emits SCCs in reverse-topological order
    // (leaves/sinks first). Iterating forward therefore guarantees that
    // every successor SCC has already been resolved when we visit a
    // given SCC.
    for (scc_idx, component) in sccs.iter().enumerate() {
        // 1. Seed with direct facts from all nodes in this SCC.
        let mut facts = BTreeSet::new();
        for node in component {
            if let Some(df) = direct.get(node) {
                facts.extend(df.iter().cloned());
            }
        }

        // 2. Merge in transitive facts from successor SCCs.
        //    "Successors" = SCCs reachable via edges from nodes in this SCC.
        let mut visited_succs: BTreeSet<usize> = BTreeSet::new();
        for node in component {
            if let Some(deps) = edges.get(node) {
                for dep in deps {
                    if let Some(&dep_scc) = node_to_scc.get(dep) {
                        // Skip self-SCC (already seeded above) and already-visited.
                        if dep_scc != scc_idx && visited_succs.insert(dep_scc) {
                            facts.extend(scc_facts[dep_scc].iter().cloned());
                        }
                    }
                }
            }
        }

        scc_facts.push(facts);
    }

    // Fan out: every node gets its SCC's fact set.
    let mut result = BTreeMap::new();
    for (scc_idx, component) in sccs.iter().enumerate() {
        for node in component {
            result.insert(node.clone(), scc_facts[scc_idx].clone());
        }
    }

    result
}

// ============================================================================
// Tarjan's SCC algorithm
// ============================================================================
//
// Adapted from `baml_compiler_tir/src/cycles.rs` but made fully generic
// over node type and using BTreeMap/BTreeSet for deterministic output.

/// Internal state for Tarjan's algorithm.
struct Tarjan<'g, N: Ord> {
    edges: &'g BTreeMap<N, BTreeSet<N>>,
    index: usize,
    stack: Vec<N>,
    state: BTreeMap<N, NodeState>,
    components: Vec<Vec<N>>,
}

#[derive(Clone, Copy)]
struct NodeState {
    index: usize,
    low_link: usize,
    on_stack: bool,
}

impl<'g, N: Ord + Clone> Tarjan<'g, N> {
    const UNVISITED: usize = usize::MAX;

    /// Compute all SCCs. Returns components in reverse topological order
    /// (leaves/sinks first, roots last).
    fn components(edges: &'g BTreeMap<N, BTreeSet<N>>) -> Vec<Vec<N>> {
        let mut tarjan = Self {
            edges,
            index: 0,
            stack: Vec::new(),
            state: edges
                .keys()
                .map(|n| {
                    (
                        n.clone(),
                        NodeState {
                            index: Self::UNVISITED,
                            low_link: Self::UNVISITED,
                            on_stack: false,
                        },
                    )
                })
                .collect(),
            components: Vec::new(),
        };

        // Deterministic iteration order guaranteed by BTreeMap.
        let nodes: Vec<N> = edges.keys().cloned().collect();
        for node in nodes {
            if tarjan.state[&node].index == Self::UNVISITED {
                tarjan.strong_connect(&node);
            }
        }

        tarjan.components
    }

    fn strong_connect(&mut self, node_id: &N) {
        let mut node = NodeState {
            index: self.index,
            low_link: self.index,
            on_stack: true,
        };
        self.index += 1;
        self.state.insert(node_id.clone(), node);
        self.stack.push(node_id.clone());

        if let Some(successors) = self.edges.get(node_id) {
            for succ_id in successors {
                let succ = self.state[succ_id];
                if succ.index == Self::UNVISITED {
                    self.strong_connect(succ_id);
                    let succ = self.state[succ_id];
                    node.low_link = node.low_link.min(succ.low_link);
                } else if succ.on_stack {
                    node.low_link = node.low_link.min(succ.index);
                }
            }
        }

        self.state.insert(node_id.clone(), node);

        if node.low_link == node.index {
            let mut component = Vec::new();
            while let Some(top) = self.stack.pop() {
                if let Some(st) = self.state.get_mut(&top) {
                    st.on_stack = false;
                }
                let is_root = &top == node_id;
                component.push(top);
                if is_root {
                    break;
                }
            }
            component.reverse();
            self.components.push(component);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -- helpers --

    fn s(name: &str) -> String {
        name.to_string()
    }

    fn facts(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| s(n)).collect()
    }

    // ========================================================================
    // Basic direct-only (no edges)
    // ========================================================================

    #[test]
    fn isolated_nodes_direct_equals_transitive() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["x"]));
        g.add_node(s("B"), facts(&["y"]));

        let r = g.analyze();

        assert_eq!(r.direct(&s("A")), Some(&facts(&["x"])));
        assert_eq!(r.transitive(&s("A")), Some(&facts(&["x"])));
        assert_eq!(r.direct(&s("B")), Some(&facts(&["y"])));
        assert_eq!(r.transitive(&s("B")), Some(&facts(&["y"])));
    }

    // ========================================================================
    // Linear chain: A → B → C
    // ========================================================================

    #[test]
    fn linear_chain_propagation() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_node(s("C"), facts(&["c"]));
        g.add_edge(s("A"), s("B"));
        g.add_edge(s("B"), s("C"));

        let r = g.analyze();

        // C: only its own fact
        assert_eq!(r.transitive(&s("C")), Some(&facts(&["c"])));
        // B: b + c
        assert_eq!(r.transitive(&s("B")), Some(&facts(&["b", "c"])));
        // A: a + b + c
        assert_eq!(r.transitive(&s("A")), Some(&facts(&["a", "b", "c"])));

        // Direct facts unchanged
        assert_eq!(r.direct(&s("A")), Some(&facts(&["a"])));
        assert_eq!(r.direct(&s("B")), Some(&facts(&["b"])));
    }

    // ========================================================================
    // Diamond: A → B, A → C, B → D, C → D
    // ========================================================================

    #[test]
    fn diamond_graph() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_node(s("C"), facts(&["c"]));
        g.add_node(s("D"), facts(&["d"]));
        g.add_edge(s("A"), s("B"));
        g.add_edge(s("A"), s("C"));
        g.add_edge(s("B"), s("D"));
        g.add_edge(s("C"), s("D"));

        let r = g.analyze();

        assert_eq!(r.transitive(&s("D")), Some(&facts(&["d"])));
        assert_eq!(r.transitive(&s("B")), Some(&facts(&["b", "d"])));
        assert_eq!(r.transitive(&s("C")), Some(&facts(&["c", "d"])));
        assert_eq!(r.transitive(&s("A")), Some(&facts(&["a", "b", "c", "d"])));
    }

    // ========================================================================
    // Simple cycle: A → B → A
    // ========================================================================

    #[test]
    fn simple_cycle_shares_facts() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_edge(s("A"), s("B"));
        g.add_edge(s("B"), s("A"));

        let r = g.analyze();

        // Both nodes share the union of their facts.
        let expected = facts(&["a", "b"]);
        assert_eq!(r.transitive(&s("A")), Some(&expected));
        assert_eq!(r.transitive(&s("B")), Some(&expected));
    }

    // ========================================================================
    // Self-loop: A → A
    // ========================================================================

    #[test]
    fn self_loop() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["a"]));
        g.add_edge(s("A"), s("A"));

        let r = g.analyze();
        assert_eq!(r.transitive(&s("A")), Some(&facts(&["a"])));
    }

    // ========================================================================
    // Cycle with external dependency: A ↔ B → C
    // ========================================================================

    #[test]
    fn cycle_with_tail() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_node(s("C"), facts(&["c"]));
        g.add_edge(s("A"), s("B"));
        g.add_edge(s("B"), s("A"));
        g.add_edge(s("B"), s("C"));

        let r = g.analyze();

        // C is a leaf.
        assert_eq!(r.transitive(&s("C")), Some(&facts(&["c"])));
        // A and B form a cycle, both reach C.
        let expected = facts(&["a", "b", "c"]);
        assert_eq!(r.transitive(&s("A")), Some(&expected));
        assert_eq!(r.transitive(&s("B")), Some(&expected));
    }

    // ========================================================================
    // Three-node cycle: A → B → C → A
    // ========================================================================

    #[test]
    fn three_node_cycle() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_node(s("C"), facts(&["c"]));
        g.add_edge(s("A"), s("B"));
        g.add_edge(s("B"), s("C"));
        g.add_edge(s("C"), s("A"));

        let r = g.analyze();

        let expected = facts(&["a", "b", "c"]);
        assert_eq!(r.transitive(&s("A")), Some(&expected));
        assert_eq!(r.transitive(&s("B")), Some(&expected));
        assert_eq!(r.transitive(&s("C")), Some(&expected));
    }

    // ========================================================================
    // Empty graph
    // ========================================================================

    #[test]
    fn empty_graph() {
        let g: AnalysisGraph<String, String> = AnalysisGraph::new();
        let r = g.analyze();
        assert_eq!(r.nodes().count(), 0);
    }

    // ========================================================================
    // Node with no facts
    // ========================================================================

    #[test]
    fn node_with_no_direct_facts() {
        let mut g = AnalysisGraph::new();
        g.add_node_empty(s("A"));
        g.add_node(s("B"), facts(&["b"]));
        g.add_edge(s("A"), s("B"));

        let r = g.analyze();

        assert_eq!(r.direct(&s("A")), Some(&facts(&[])));
        assert_eq!(r.transitive(&s("A")), Some(&facts(&["b"])));
    }

    // ========================================================================
    // `inherited` helper
    // ========================================================================

    #[test]
    fn inherited_returns_only_transitive() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_edge(s("A"), s("B"));

        let r = g.analyze();

        let inherited = r.inherited(&s("A")).unwrap();
        // "a" is direct, so only "b" is inherited.
        assert!(inherited.contains(&s("b")));
        assert!(!inherited.contains(&s("a")));
    }

    // ========================================================================
    // Deterministic ordering (golden test)
    // ========================================================================

    #[test]
    fn deterministic_output_ordering() {
        // Build the same graph twice with different insertion orders
        // and verify identical output.
        let build = |reverse: bool| {
            let mut g = AnalysisGraph::new();
            let nodes = vec![
                (s("alpha"), facts(&["z", "a", "m"])),
                (s("beta"), facts(&["y", "b"])),
                (s("gamma"), facts(&["x"])),
                (s("delta"), facts(&["w", "d"])),
            ];

            let edges = vec![
                (s("alpha"), s("beta")),
                (s("alpha"), s("gamma")),
                (s("beta"), s("delta")),
                (s("gamma"), s("delta")),
            ];

            if reverse {
                for (n, f) in nodes.into_iter().rev() {
                    g.add_node(n, f);
                }
                for (from, to) in edges.into_iter().rev() {
                    g.add_edge(from, to);
                }
            } else {
                for (n, f) in nodes {
                    g.add_node(n, f);
                }
                for (from, to) in edges {
                    g.add_edge(from, to);
                }
            }

            g.analyze()
        };

        let r1 = build(false);
        let r2 = build(true);

        // Collect transitive facts in iteration order.
        let collect = |r: &AnalysisResult<String, String>| -> Vec<(String, Vec<String>)> {
            r.iter_transitive()
                .map(|(n, fs)| (n.clone(), fs.iter().cloned().collect()))
                .collect()
        };

        assert_eq!(collect(&r1), collect(&r2));
    }

    // ========================================================================
    // Snapshot tests (golden)
    // ========================================================================

    #[test]
    fn snapshot_linear_chain() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("main"), facts(&["ValidationError"]));
        g.add_node(s("parse_input"), facts(&["ParseError"]));
        g.add_node(s("fetch_data"), facts(&["HttpError", "TimeoutError"]));
        g.add_edge(s("main"), s("parse_input"));
        g.add_edge(s("main"), s("fetch_data"));
        g.add_edge(s("parse_input"), s("fetch_data"));

        let r = g.analyze();

        let snapshot: BTreeMap<&str, Vec<&str>> = r
            .iter_transitive()
            .map(|(n, fs)| (n.as_str(), fs.iter().map(String::as_str).collect()))
            .collect();

        insta::assert_yaml_snapshot!(snapshot, @r"
        fetch_data:
          - HttpError
          - TimeoutError
        main:
          - HttpError
          - ParseError
          - TimeoutError
          - ValidationError
        parse_input:
          - HttpError
          - ParseError
          - TimeoutError
        ");
    }

    #[test]
    fn snapshot_cycle_with_tail() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("server"), facts(&["ListenError"]));
        g.add_node(s("handler"), facts(&["HandlerPanic"]));
        g.add_node(s("middleware"), facts(&["AuthError"]));
        g.add_node(s("db"), facts(&["ConnectionError"]));
        // server ↔ handler (retry cycle)
        g.add_edge(s("server"), s("handler"));
        g.add_edge(s("handler"), s("server"));
        // handler → middleware → db
        g.add_edge(s("handler"), s("middleware"));
        g.add_edge(s("middleware"), s("db"));

        let r = g.analyze();

        let snapshot: BTreeMap<&str, Vec<&str>> = r
            .iter_transitive()
            .map(|(n, fs)| (n.as_str(), fs.iter().map(String::as_str).collect()))
            .collect();

        insta::assert_yaml_snapshot!(snapshot, @r"
        db:
          - ConnectionError
        handler:
          - AuthError
          - ConnectionError
          - HandlerPanic
          - ListenError
        middleware:
          - AuthError
          - ConnectionError
        server:
          - AuthError
          - ConnectionError
          - HandlerPanic
          - ListenError
        ");
    }

    // ========================================================================
    // Integer node IDs (demonstrates non-String generics)
    // ========================================================================

    #[test]
    fn integer_node_ids() {
        let mut g: AnalysisGraph<u32, &str> = AnalysisGraph::new();
        g.add_node(1, ["a"].into_iter().collect());
        g.add_node(2, ["b"].into_iter().collect());
        g.add_node(3, ["c"].into_iter().collect());
        g.add_edge(1, 2);
        g.add_edge(2, 3);

        let r = g.analyze();

        let expected: BTreeSet<&str> = ["a", "b", "c"].into_iter().collect();
        assert_eq!(r.transitive(&1), Some(&expected));
    }

    // ========================================================================
    // Multiple disconnected components
    // ========================================================================

    #[test]
    fn disconnected_components() {
        let mut g = AnalysisGraph::new();
        // Component 1: A → B
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_edge(s("A"), s("B"));
        // Component 2: C → D
        g.add_node(s("C"), facts(&["c"]));
        g.add_node(s("D"), facts(&["d"]));
        g.add_edge(s("C"), s("D"));

        let r = g.analyze();

        // No cross-component contamination.
        assert_eq!(r.transitive(&s("A")), Some(&facts(&["a", "b"])));
        assert_eq!(r.transitive(&s("C")), Some(&facts(&["c", "d"])));
        // Leaves only have their own facts.
        assert_eq!(r.transitive(&s("B")), Some(&facts(&["b"])));
        assert_eq!(r.transitive(&s("D")), Some(&facts(&["d"])));
    }

    // ========================================================================
    // Overlapping facts across nodes
    // ========================================================================

    #[test]
    fn overlapping_facts_deduplicated() {
        let mut g = AnalysisGraph::new();
        g.add_node(s("A"), facts(&["x", "y"]));
        g.add_node(s("B"), facts(&["y", "z"]));
        g.add_edge(s("A"), s("B"));

        let r = g.analyze();

        // A's transitive set is the union, with "y" appearing only once.
        assert_eq!(r.transitive(&s("A")), Some(&facts(&["x", "y", "z"])));
    }

    // ========================================================================
    // Implicit node creation via add_edge
    // ========================================================================

    #[test]
    fn add_edge_implicitly_creates_nodes() {
        let mut g: AnalysisGraph<String, String> = AnalysisGraph::new();
        g.add_edge(s("A"), s("B"));

        let r = g.analyze();

        // Both nodes exist with empty fact sets.
        assert_eq!(r.direct(&s("A")), Some(&facts(&[])));
        assert_eq!(r.direct(&s("B")), Some(&facts(&[])));
        assert_eq!(r.transitive(&s("A")), Some(&facts(&[])));
    }

    // ========================================================================
    // Complex SCC topology: two cycles connected by a bridge
    // ========================================================================

    #[test]
    fn two_cycles_with_bridge() {
        let mut g = AnalysisGraph::new();
        // Cycle 1: A ↔ B
        g.add_node(s("A"), facts(&["a"]));
        g.add_node(s("B"), facts(&["b"]));
        g.add_edge(s("A"), s("B"));
        g.add_edge(s("B"), s("A"));
        // Cycle 2: C ↔ D
        g.add_node(s("C"), facts(&["c"]));
        g.add_node(s("D"), facts(&["d"]));
        g.add_edge(s("C"), s("D"));
        g.add_edge(s("D"), s("C"));
        // Bridge: B → C
        g.add_edge(s("B"), s("C"));

        let r = g.analyze();

        // Cycle 2 is self-contained.
        let cycle2 = facts(&["c", "d"]);
        assert_eq!(r.transitive(&s("C")), Some(&cycle2));
        assert_eq!(r.transitive(&s("D")), Some(&cycle2));

        // Cycle 1 inherits cycle 2 through the bridge.
        let cycle1 = facts(&["a", "b", "c", "d"]);
        assert_eq!(r.transitive(&s("A")), Some(&cycle1));
        assert_eq!(r.transitive(&s("B")), Some(&cycle1));
    }
}
