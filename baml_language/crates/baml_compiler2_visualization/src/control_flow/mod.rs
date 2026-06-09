//! Control flow visualization graph for BAML functions.
//!
//! Graph types, builder infrastructure, the AST-based graph builder, the LLM
//! graph builder, and the three-pass flattening pipeline.

mod flatten;
mod from_ast;

use std::{collections::HashMap, fmt};

pub use flatten::{flatten_control_flow_graph, prepare_control_flow_graph_for_visualization};
pub use from_ast::{STMT_SOURCE_EXPR_TAG, build_control_flow_graph_from_ast};
use indexmap::IndexMap;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

/// Opaque node identifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PathSegment {
    FunctionRoot { ordinal: u16 },
    Header { slug: String, ordinal: u16 },
    BranchGroup { slug: String, ordinal: u16 },
    BranchArm { slug: String, ordinal: u16 },
    Loop { slug: String, ordinal: u16 },
    OtherScope { slug: String, ordinal: u16 },
}

/// The type of a visualization node.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeType {
    FunctionRoot,
    LlmFunction,
    HeaderContextEnter,
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

/// Source range for a graph node.
///
/// Line and column values are 0-indexed LSP/VS Code positions. `end_*` is
/// intentionally derived from an end offset that has already been expanded by
/// the caller, so clients can select the whole span directly.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    pub file_id: u32,
    pub file_path: String,
    pub start_offset: u32,
    pub end_offset: u32,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

/// A node in the control flow visualization graph.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: NodeId,
    pub parent_node_id: Option<NodeId>,
    pub log_filter_key: String,
    pub label: String,
    /// Raw arena index referencing the source expression that produced this node.
    ///
    /// Interpretation depends on which builder created the graph:
    /// - VIR builder: index into `ExprBody.exprs` (convert via `ExprId::into_raw().into_u32()`)
    /// - AST builder: index into `AstSourceMap`, with the high bit tagging statement spans
    pub source_expr: Option<u32>,
    pub node_type: NodeType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_client: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callee_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_span: Option<SourceSpan>,
    #[serde(default)]
    pub is_container: bool,
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
            llm_client: None,
            callee_name: None,
            source_span: None,
            is_container: false,
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

    #[must_use]
    pub fn with_llm_client(mut self, client_name: impl Into<String>) -> Self {
        self.llm_client = Some(client_name.into());
        self
    }

    #[must_use]
    pub fn with_callee_name(mut self, callee_name: impl Into<String>) -> Self {
        self.callee_name = Some(callee_name.into());
        self
    }
}

/// A directed edge in the visualization graph.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    pub src: NodeId,
    pub dst: NodeId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The control flow visualization graph.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
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
        self.edges.push(Edge {
            src,
            dst,
            label: None,
        });
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
    pub fn new(entry: FrameEntry, node_id: NodeId, lexical_segment: Option<PathSegment>) -> Self {
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

/// Build a single semantic node for a declarative LLM function.
///
/// The desugared render/build/call functions are implementation details, so the
/// playground graph should surface only the top-level LLM call.
pub fn build_llm_control_flow_graph(function_name: &str, client_name: &str) -> ControlFlowGraph {
    let mut graph = GraphAccumulator::default();
    let llm_id = graph.allocate_id();
    let root_segment = PathSegment::FunctionRoot { ordinal: 0 };
    let root_key = encode_segments(function_name, std::slice::from_ref(&root_segment));
    let node = Node::new(
        llm_id,
        None,
        root_key,
        function_name,
        None,
        NodeType::LlmFunction,
    )
    .with_llm_client(client_name);
    graph.add_node(node);

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
        NodeType::LlmFunction => "llm-function",
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
