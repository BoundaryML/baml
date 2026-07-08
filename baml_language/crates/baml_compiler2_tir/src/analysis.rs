//! Generic two-pass direct+transitive analysis framework.
//!
//! Provides a reusable cache for computing per-node "facts" that propagate
//! transitively through a dependency graph. The canonical first consumer is
//! exception propagation analysis, but the framework is domain-agnostic.
//!
//! Moved from `baml_compiler_analysis` crate which was deleted in Phase 5
//! of the compiler2 migration.

use std::collections::{BTreeMap, BTreeSet};

/// A dependency graph annotated with per-node direct facts.
///
/// `N` is the node identifier type (must be `Ord` for deterministic ordering).
/// `F` is the fact type (must be `Ord` for set operations).
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
    pub fn add_node(&mut self, node: N, facts: BTreeSet<F>) {
        self.direct.insert(node.clone(), facts);
        self.edges.entry(node).or_default();
    }

    /// Register a node with no direct facts.
    pub fn add_node_empty(&mut self, node: N) {
        self.direct.entry(node.clone()).or_default();
        self.edges.entry(node).or_default();
    }

    /// Add a directed dependency edge: `from` depends on `to`.
    pub fn add_edge(&mut self, from: N, to: N) {
        self.direct.entry(from.clone()).or_default();
        self.direct.entry(to.clone()).or_default();
        self.edges.entry(to.clone()).or_default();
        self.edges.entry(from).or_default().insert(to);
    }

    /// Consume the graph and compute the transitive closure of facts.
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

/// The output of a two-pass analysis.
#[derive(Debug, Clone)]
pub struct AnalysisResult<N: Ord, F: Ord> {
    /// Per-node direct facts (unchanged from input).
    direct: BTreeMap<N, BTreeSet<F>>,
    /// Per-node transitive facts (direct ∪ propagated from dependencies).
    transitive: BTreeMap<N, BTreeSet<F>>,
}

impl<N: Ord, F: Ord> AnalysisResult<N, F> {
    /// Direct facts for `node`.
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

    /// Facts that are *only* transitive.
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

fn propagate<N: Ord + Clone, F: Ord + Clone>(
    direct: &BTreeMap<N, BTreeSet<F>>,
    edges: &BTreeMap<N, BTreeSet<N>>,
    sccs: &[Vec<N>],
) -> BTreeMap<N, BTreeSet<F>> {
    let mut node_to_scc: BTreeMap<&N, usize> = BTreeMap::new();
    for (scc_idx, component) in sccs.iter().enumerate() {
        for node in component {
            node_to_scc.insert(node, scc_idx);
        }
    }

    let mut scc_facts: Vec<BTreeSet<F>> = Vec::with_capacity(sccs.len());

    for (scc_idx, component) in sccs.iter().enumerate() {
        let mut facts = BTreeSet::new();
        for node in component {
            if let Some(df) = direct.get(node) {
                facts.extend(df.iter().cloned());
            }
        }

        let mut visited_succs: BTreeSet<usize> = BTreeSet::new();
        for node in component {
            if let Some(deps) = edges.get(node) {
                for dep in deps {
                    if let Some(&dep_scc) = node_to_scc.get(dep) {
                        if dep_scc != scc_idx && visited_succs.insert(dep_scc) {
                            facts.extend(scc_facts[dep_scc].iter().cloned());
                        }
                    }
                }
            }
        }

        scc_facts.push(facts);
    }

    let mut result = BTreeMap::new();
    for (scc_idx, component) in sccs.iter().enumerate() {
        for node in component {
            result.insert(node.clone(), scc_facts[scc_idx].clone());
        }
    }

    result
}

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
