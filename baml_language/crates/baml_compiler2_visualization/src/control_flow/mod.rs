//! Control flow visualization graph for BAML functions.
//!
//! Graph types, builder infrastructure, the AST-based graph builder, the LLM
//! graph builder, and the three-pass flattening pipeline.

mod flatten;
mod from_ast;

use std::{collections::HashMap, fmt};

pub use flatten::flatten_control_flow_graph;
pub use from_ast::build_control_flow_graph_from_ast;
use indexmap::IndexMap;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Opaque node identifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u32);

impl NodeId {
    pub fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub fn raw(&self) -> u32 {
        self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Segment of a log-filter key path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PathSegment {
    FunctionRoot { ordinal: u16 },
    Header { slug: String, ordinal: u16 },
    BranchGroup { slug: String, ordinal: u16 },
    BranchArm { slug: String, ordinal: u16 },
    Loop { slug: String, ordinal: u16 },
    OtherScope { slug: String, ordinal: u16 },
}

/// The type of a visualization node.
#[derive(Clone, Debug)]
pub enum NodeType {
    FunctionRoot,
    HeaderContextEnter,
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

/// A node in the control flow visualization graph.
#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    pub parent_node_id: Option<NodeId>,
    pub log_filter_key: String,
    pub label: String,
    /// Raw arena index referencing the source expression that produced this node.
    ///
    /// Interpretation depends on which builder created the graph:
    /// - VIR builder: index into `ExprBody.exprs` (convert via `ExprId::into_raw().into_u32()`)
    /// - AST builder: not yet set (always `None`)
    pub source_expr: Option<u32>,
    pub node_type: NodeType,
}

impl Node {
    pub fn new(
        id: NodeId,
        parent_node_id: Option<NodeId>,
        log_filter_key: impl Into<String>,
        label: impl Into<String>,
        source_expr: Option<u32>,
        node_type: NodeType,
    ) -> Self {
        Self {
            id,
            parent_node_id,
            log_filter_key: log_filter_key.into(),
            label: label.into(),
            source_expr,
            node_type,
        }
    }

    pub fn root(id: NodeId, log_filter_key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(
            id,
            None,
            log_filter_key,
            label,
            None,
            NodeType::FunctionRoot,
        )
    }
}

/// A directed edge in the visualization graph.
#[derive(Clone, Debug)]
pub struct Edge {
    pub src: NodeId,
    pub dst: NodeId,
}

/// The control flow visualization graph.
#[derive(Clone, Debug, Default)]
pub struct ControlFlowGraph {
    pub nodes: IndexMap<NodeId, Node>,
    pub edges_by_src: IndexMap<NodeId, Vec<Edge>>,
}

// ---------------------------------------------------------------------------
// Graph builder accumulator
// ---------------------------------------------------------------------------

pub struct GraphAccumulator {
    nodes: IndexMap<NodeId, Node>,
    edges: Vec<Edge>,
    next_node_id: u32,
}

impl Default for GraphAccumulator {
    fn default() -> Self {
        Self {
            nodes: IndexMap::new(),
            edges: Vec::new(),
            next_node_id: 0,
        }
    }
}

impl GraphAccumulator {
    pub fn allocate_id(&mut self) -> NodeId {
        let id = NodeId::new(self.next_node_id);
        self.next_node_id += 1;
        id
    }

    pub fn add_node(&mut self, node: Node) {
        self.nodes.insert(node.id, node);
    }

    pub fn add_edge(&mut self, src: NodeId, dst: NodeId) {
        self.edges.push(Edge { src, dst });
    }

    pub fn finish(self) -> ControlFlowGraph {
        let mut edges_by_src: IndexMap<NodeId, Vec<Edge>> = IndexMap::new();
        for edge in self.edges {
            edges_by_src.entry(edge.src).or_default().push(edge);
        }
        ControlFlowGraph {
            nodes: self.nodes,
            edges_by_src,
        }
    }
}

// ---------------------------------------------------------------------------
// Frame stack (scope tracking during traversal)
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub struct FrameCounters {
    header: u16,
    branch_group: u16,
    branch_arm: u16,
    loop_node: u16,
    other_scope: u16,
}

pub enum CounterKind {
    Header,
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

impl FrameCounters {
    pub fn next(&mut self, kind: &CounterKind) -> u16 {
        match kind {
            CounterKind::Header => {
                let c = self.header;
                self.header += 1;
                c
            }
            CounterKind::BranchGroup => {
                let c = self.branch_group;
                self.branch_group += 1;
                c
            }
            CounterKind::BranchArm => {
                let c = self.branch_arm;
                self.branch_arm += 1;
                c
            }
            CounterKind::Loop => {
                let c = self.loop_node;
                self.loop_node += 1;
                c
            }
            CounterKind::OtherScope => {
                let c = self.other_scope;
                self.other_scope += 1;
                c
            }
        }
    }
}

pub enum FrameEntry {
    FunctionRoot,
    Header { level: u8 },
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

impl FrameEntry {
    /// Whether children that are peers in a code sequence are "linear"
    /// (connected by sequential edges).
    pub fn children_are_linear(&self) -> bool {
        !matches!(self, FrameEntry::BranchGroup)
    }
}

pub struct Frame {
    pub entry: FrameEntry,
    pub node_id: NodeId,
    pub lexical_segment: Option<PathSegment>,
    pub counters: FrameCounters,
    pub last_linear_child: Option<NodeId>,
}

impl Frame {
    pub fn new(
        entry: FrameEntry,
        node_id: NodeId,
        lexical_segment: Option<PathSegment>,
    ) -> Self {
        Self {
            entry,
            node_id,
            lexical_segment,
            counters: FrameCounters::default(),
            last_linear_child: None,
        }
    }

    pub fn next_ordinal(&mut self, kind: &CounterKind) -> u16 {
        self.counters.next(kind)
    }
}

// ---------------------------------------------------------------------------
// LLM function graph builder
// ---------------------------------------------------------------------------

/// Build a simple 2-node control flow graph for an LLM function.
///
/// Produces: `FunctionRoot` -> `OtherScope("LLM client: <client_name>")`.
pub fn build_llm_control_flow_graph(function_name: &str, client_name: &str) -> ControlFlowGraph {
    let mut graph = GraphAccumulator::default();
    let root_id = graph.allocate_id();
    let root_segment = PathSegment::FunctionRoot { ordinal: 0 };
    let root_key = encode_segments(function_name, std::slice::from_ref(&root_segment));
    graph.add_node(Node::root(root_id, root_key, function_name));

    let slug = slug_or_default("llm", "llm");
    let segment = PathSegment::OtherScope { slug, ordinal: 0 };
    let log_filter_key = encode_segments(function_name, &[root_segment, segment]);
    let scope_id = graph.allocate_id();
    let node = Node::new(
        scope_id,
        Some(root_id),
        log_filter_key,
        format!("LLM client: {client_name}"),
        None,
        NodeType::OtherScope,
    );
    graph.add_node(node);
    graph.add_edge(root_id, scope_id);

    graph.finish()
}

// ---------------------------------------------------------------------------
// Utility functions
// ---------------------------------------------------------------------------

pub fn slugify(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

pub fn slug_or_default(label: &str, default: &str) -> String {
    let candidate = slugify(label);
    if candidate.is_empty() {
        default.to_string()
    } else {
        candidate
    }
}

pub fn describe_node_type(node_type: &NodeType) -> &'static str {
    match node_type {
        NodeType::FunctionRoot => "function",
        NodeType::HeaderContextEnter => "header",
        NodeType::BranchGroup => "branch-group",
        NodeType::BranchArm => "branch-arm",
        NodeType::Loop => "loop",
        NodeType::OtherScope => "other-scope",
    }
}

pub fn encode_segments(function: &str, segments: &[PathSegment]) -> String {
    let mut encoded = String::from(function);
    for segment in segments {
        encoded.push('|');
        match segment {
            PathSegment::FunctionRoot { ordinal } => {
                encoded.push_str("root:");
                encoded.push_str(&ordinal.to_string());
            }
            PathSegment::Header { slug, ordinal } => {
                encoded.push_str("hdr:");
                encoded.push_str(slug);
                encoded.push(':');
                encoded.push_str(&ordinal.to_string());
            }
            PathSegment::BranchGroup { slug, ordinal } => {
                encoded.push_str("bg:");
                encoded.push_str(slug);
                encoded.push(':');
                encoded.push_str(&ordinal.to_string());
            }
            PathSegment::BranchArm { slug, ordinal } => {
                encoded.push_str("arm:");
                encoded.push_str(slug);
                encoded.push(':');
                encoded.push_str(&ordinal.to_string());
            }
            PathSegment::Loop { slug, ordinal } => {
                encoded.push_str("loop:");
                encoded.push_str(slug);
                encoded.push(':');
                encoded.push_str(&ordinal.to_string());
            }
            PathSegment::OtherScope { slug, ordinal } => {
                encoded.push_str("scope:");
                encoded.push_str(slug);
                encoded.push(':');
                encoded.push_str(&ordinal.to_string());
            }
        }
    }
    encoded
}

// ---------------------------------------------------------------------------
// Helpers used by flatten module
// ---------------------------------------------------------------------------

pub(crate) fn build_children_map(nodes: &IndexMap<NodeId, Node>) -> HashMap<NodeId, Vec<NodeId>> {
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for node in nodes.values() {
        if let Some(parent) = node.parent_node_id {
            children.entry(parent).or_default().push(node.id);
        }
    }
    children
}

pub(crate) fn node_depth(node_id: NodeId, nodes: &IndexMap<NodeId, Node>) -> usize {
    let mut depth = 0;
    let mut current = Some(node_id);
    while let Some(id) = current {
        depth += 1;
        current = nodes.get(&id).and_then(|node| node.parent_node_id);
    }
    depth
}

// ---------------------------------------------------------------------------
// Display for ControlFlowGraph (for snapshot tests)
// ---------------------------------------------------------------------------

impl fmt::Display for ControlFlowGraph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Nodes:")?;
        for (id, node) in &self.nodes {
            let parent = node
                .parent_node_id
                .map(|p| p.to_string())
                .unwrap_or_else(|| "-".to_string());
            writeln!(
                f,
                "  [{id}] parent={parent} type={} label={:?}",
                describe_node_type(&node.node_type),
                node.label
            )?;
        }
        writeln!(f, "Edges:")?;
        for edges in self.edges_by_src.values() {
            for edge in edges {
                writeln!(f, "  {} -> {}", edge.src, edge.dst)?;
            }
        }
        Ok(())
    }
}
