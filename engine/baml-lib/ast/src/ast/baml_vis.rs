use std::collections::{HashMap, HashSet};

/// Config: maximum number of direct children (markdown + nested) a non-branching
/// container may have to be flattened into a linear sequence instead of a subgraph.
///
/// For example, with `1` (default), containers with 0 or 1 child are flattened.
/// Top-level scopes are never flattened if they have any children, regardless of this value.
const MAX_CHILDREN_TO_FLATTEN: usize = 1;
// Toggle: render function call nodes (e.g., SummarizeVideo, CreatePR) alongside headers.
const SHOW_CALL_NODES: bool = false;

use baml_types::BamlMap;
use internal_baml_diagnostics::SerializedSpan;
use serde_json;

use super::{
    header_collector::{HeaderLabelKind, Hid},
    Ast, HeaderCollector, HeaderIndex, RenderableHeader, ScopeId,
};

/// A Mermaid flowchart (LR) generator focused on headers and control-flow-like structure.
///
/// It renders nested connections as Mermaid subgraphs: the header that owns a nested scope
/// becomes a subgraph container (titled with the header text) and the nested scope headers
/// are rendered inside the container. Sibling elements are connected linearly with `-->`.
/// Connections never cross container boundaries; containers themselves are the units that
/// connect to other elements.
#[derive(Debug, Default)]
pub struct BamlVisDiagramGenerator;

impl BamlVisDiagramGenerator {
    /// Generate a Mermaid flowchart (LR) showing headers as linear steps and
    /// nested scopes as subgraphs.
    pub fn generate_headers_flowchart(ast: &Ast) -> String {
        let index = HeaderCollector::collect(ast);
        let builder = GraphBuilder::new(&index, BuilderConfig::default());
        let (graph, span_map) = builder.build();
        MermaidRenderer::render(&graph, Direction::TD, false, span_map)
    }

    /// Back-compat API used by the example. `use_fancy` toggles optional cosmetic styling.
    pub fn generate_with_styling(ast: &Ast, use_fancy: bool) -> String {
        let index = HeaderCollector::collect(ast);
        let builder = GraphBuilder::new(&index, BuilderConfig::default());
        let (graph, span_map) = builder.build();
        MermaidRenderer::render(&graph, Direction::TD, use_fancy, span_map)
    }
}

// ===== Renderer-agnostic graph model and builder =====

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum Direction {
    TD,
    LR,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum NodeKind {
    Header(Hid, Option<SerializedSpan>),
    Decision(Hid, Option<SerializedSpan>),
    Call { header: Hid, callee: String },
}

#[derive(Debug, Clone)]
struct Node {
    id: String,
    label: String,
    kind: NodeKind,
    cluster: Option<String>,
}

#[derive(Debug, Clone)]
struct Edge {
    from: String,
    to: String,
}

#[derive(Debug, Clone)]
struct Cluster {
    id: String,
    label: String,
    parent: Option<String>,
}

#[derive(Debug, Default)]
struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    clusters: Vec<Cluster>,
}

#[derive(Debug, Clone, Copy)]
struct BuilderConfig {
    show_call_nodes: bool,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            show_call_nodes: SHOW_CALL_NODES,
        }
    }
}

struct GraphBuilder<'a> {
    index: &'a HeaderIndex,
    cfg: BuilderConfig,
    graph: Graph,
    next_node: u32,
    next_cluster: u32,
    by_hid: HashMap<Hid, &'a RenderableHeader>,
    idstr_to_hid: HashMap<&'a str, Hid>,
    md_children: HashMap<Hid, Vec<Hid>>,
    has_md_parent: HashSet<Hid>,
    nested_children: HashMap<Hid, Vec<Hid>>,
    scope_root: HashMap<ScopeId, Hid>,
    nested_targets: HashSet<Hid>,
    header_entry: HashMap<Hid, String>,
    header_exits: HashMap<Hid, Vec<String>>,
    // we're going to need stable iteration in snapshot tests.
    span_map: BamlMap<String, SerializedSpan>,
    call_node_cache: HashMap<(Hid, String), String>,
}

impl<'a> GraphBuilder<'a> {
    fn new(index: &'a HeaderIndex, cfg: BuilderConfig) -> Self {
        let mut b = Self {
            index,
            cfg,
            graph: Graph::default(),
            next_node: 0,
            next_cluster: 0,
            by_hid: HashMap::new(),
            idstr_to_hid: HashMap::new(),
            md_children: HashMap::new(),
            has_md_parent: HashSet::new(),
            nested_children: HashMap::new(),
            scope_root: HashMap::new(),
            nested_targets: HashSet::new(),
            header_entry: HashMap::new(),
            header_exits: HashMap::new(),
            span_map: BamlMap::new(),
            call_node_cache: HashMap::new(),
        };
        b.precompute();
        b
    }

    fn precompute(&mut self) {
        for h in &self.index.headers {
            self.by_hid.insert(h.hid, h);
            self.idstr_to_hid.insert(&h.id, h.hid);
            self.scope_root.entry(h.scope).or_insert(h.hid);
        }
        for h in &self.index.headers {
            if let Some(pid) = &h.parent_id {
                if let Some(&ph) = self.idstr_to_hid.get(pid.as_str()) {
                    if let Some(parent) = self.by_hid.get(&ph) {
                        if parent.scope == h.scope {
                            self.md_children.entry(ph).or_default().push(h.hid);
                            self.has_md_parent.insert(h.hid);
                        }
                    }
                }
            }
        }
        for (p, c) in self.index.nested_edges_hid_iter() {
            if let (Some(ph), Some(ch)) = (self.index.get_by_hid(*p), self.index.get_by_hid(*c)) {
                if ph.scope != ch.scope {
                    self.nested_children.entry(*p).or_default().push(*c);
                    self.nested_targets.insert(*c);
                }
            }
        }
    }

    // Compute a tuple position key for stable ordering comparisons
    fn pos_tuple(&self, hid: Hid) -> (String, usize) {
        let h = self.by_hid[&hid];
        (
            h.span.file.path_buf().to_string_lossy().into_owned(),
            h.span.start,
        )
    }

    // Merge two already-ordered lists by source position, preserving internal order
    fn merge_by_pos(&self, md: &[Hid], nested: &[Hid]) -> Vec<Hid> {
        let mut i = 0;
        let mut j = 0;
        let mut out: Vec<Hid> = Vec::with_capacity(md.len() + nested.len());
        while i < md.len() || j < nested.len() {
            if j == nested.len()
                || (i < md.len() && self.pos_tuple(md[i]) <= self.pos_tuple(nested[j]))
            {
                out.push(md[i]);
                i += 1;
            } else {
                out.push(nested[j]);
                j += 1;
            }
        }
        out
    }

    fn build(mut self) -> (Graph, BamlMap<String, SerializedSpan>) {
        let mut tops: Vec<(String, usize, usize, ScopeId)> = Vec::new();
        let mut seen_scopes: HashSet<ScopeId> = HashSet::new();
        for h in &self.index.headers {
            if seen_scopes.insert(h.scope) {
                let root_hid = self.scope_root[&h.scope];
                if !self.nested_targets.contains(&root_hid) {
                    let root = self.by_hid[&root_hid];
                    tops.push((
                        root.span.file.path_buf().to_string_lossy().into_owned(),
                        root.span.start,
                        root.span.end,
                        h.scope,
                    ));
                }
            }
        }
        tops.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

        let mut visited_scopes: HashSet<ScopeId> = HashSet::new();
        for (_, _, _, scope) in tops {
            self.build_scope_sequence(scope, &mut visited_scopes, None);
        }
        (self.graph, self.span_map)
    }

    fn build_scope_sequence(
        &mut self,
        scope: ScopeId,
        visited_scopes: &mut HashSet<ScopeId>,
        parent_cluster: Option<String>,
    ) {
        if !visited_scopes.insert(scope) {
            return;
        }
        let items: Vec<Hid> = self
            .index
            .headers_in_scope_iter(scope)
            .filter(|h| !self.has_md_parent.contains(&h.hid))
            .map(|h| h.hid)
            .collect();

        let mut prev_exits: Option<Vec<String>> = None;
        for hid in items {
            let (entry, exits) = self.build_header(hid, visited_scopes, parent_cluster.clone());
            if let Some(prev) = prev_exits.take() {
                for e in prev {
                    self.graph.edges.push(Edge {
                        from: e,
                        to: entry.clone(),
                    });
                }
            }
            prev_exits = Some(exits);
        }
    }

    fn build_header(
        &mut self,
        hid: Hid,
        visited_scopes: &mut HashSet<ScopeId>,
        parent_cluster: Option<String>,
    ) -> (String, Vec<String>) {
        if let Some(entry) = self.header_entry.get(&hid).cloned() {
            let exits = self
                .header_exits
                .get(&hid)
                .cloned()
                .unwrap_or_else(|| vec![entry.clone()]);
            return (entry, exits);
        }
        let header = self.by_hid[&hid];
        let md_children = self.md_children.get(&hid).cloned().unwrap_or_default();
        let nested_children = self.nested_children.get(&hid).cloned().unwrap_or_default();
        let has_md = !md_children.is_empty();
        let has_nested = !nested_children.is_empty();
        let is_branching = header.label_kind == HeaderLabelKind::If;

        if !has_md && !has_nested {
            let node_id = self.new_node_id();
            let span = SerializedSpan::serialize(&header.span);
            self.span_map.insert(node_id.clone(), span.clone());
            self.graph.nodes.push(Node {
                id: node_id.clone(),
                label: header.title.to_string(),
                kind: NodeKind::Header(hid, Some(span)),
                cluster: parent_cluster.clone(),
            });
            self.header_entry.insert(hid, node_id.clone());
            self.header_exits.insert(hid, vec![node_id.clone()]);
            if self.cfg.show_call_nodes {
                self.render_calls_for_header(hid, &node_id, parent_cluster.clone());
            }
            return (node_id.clone(), vec![node_id]);
        }

        let total_children = md_children.len() + nested_children.len();
        let mut single_nested_child_has_multiple_items = false;
        if nested_children.len() == 1 {
            let child_root = self.by_hid[&nested_children[0]];
            let count = self.index.headers_in_scope_iter(child_root.scope).count();
            single_nested_child_has_multiple_items = count > 1;
        }
        let should_flatten = !is_branching
            && total_children <= MAX_CHILDREN_TO_FLATTEN
            && !(nested_children.len() == 1 && single_nested_child_has_multiple_items);

        if should_flatten {
            let node_id = self.new_node_id();
            let span = SerializedSpan::serialize(&header.span);
            self.span_map.insert(node_id.clone(), span.clone());
            let kind = if is_branching {
                NodeKind::Decision(hid, Some(span.clone()))
            } else {
                NodeKind::Header(hid, Some(span.clone()))
            };
            self.graph.nodes.push(Node {
                id: node_id.clone(),
                label: header.title.to_string(),
                kind,
                cluster: parent_cluster.clone(),
            });
            self.header_entry.insert(hid, node_id.clone());
            let mut exits = vec![node_id.clone()];
            if md_children.len() == 1 {
                let (c_entry, c_exits) =
                    self.build_header(md_children[0], visited_scopes, parent_cluster.clone());
                self.graph.edges.push(Edge {
                    from: node_id.clone(),
                    to: c_entry.clone(),
                });
                exits = c_exits;
            } else if nested_children.len() == 1 {
                let child_root_hid = nested_children[0];
                let child_scope = self.by_hid[&child_root_hid].scope;
                self.build_scope_sequence(child_scope, visited_scopes, parent_cluster.clone());
                let (c_entry, c_exits) =
                    self.build_header(child_root_hid, visited_scopes, parent_cluster.clone());
                self.graph.edges.push(Edge {
                    from: node_id.clone(),
                    to: c_entry.clone(),
                });
                exits = c_exits;
            }
            self.header_exits.insert(hid, exits.clone());
            return (node_id, exits);
        }

        if is_branching {
            let cluster_id = self.new_cluster_id();
            self.graph.clusters.push(Cluster {
                id: cluster_id.clone(),
                label: header.title.to_string(),
                parent: parent_cluster.clone(),
            });

            let decision_id = self.new_node_id();
            let span = SerializedSpan::serialize(&header.span);
            self.span_map.insert(decision_id.clone(), span.clone());
            self.graph.nodes.push(Node {
                id: decision_id.clone(),
                label: header.title.to_string(),
                kind: NodeKind::Decision(hid, Some(span)),
                cluster: Some(cluster_id.clone()),
            });
            self.header_entry.insert(hid, decision_id.clone());
            if self.cfg.show_call_nodes {
                self.render_calls_for_header(hid, &decision_id, Some(cluster_id.clone()));
            }

            let mut branch_exits: Vec<String> = Vec::new();
            for child_root_hid in nested_children.iter() {
                let child_scope = self.by_hid[child_root_hid].scope;
                self.build_scope_sequence(child_scope, visited_scopes, Some(cluster_id.clone()));
                let (entry, exits) =
                    self.build_header(*child_root_hid, visited_scopes, Some(cluster_id.clone()));
                self.graph.edges.push(Edge {
                    from: decision_id.clone(),
                    to: entry.clone(),
                });
                if self.cfg.show_call_nodes {
                    self.render_calls_for_header(*child_root_hid, &entry, Some(cluster_id.clone()));
                }
                branch_exits.extend(exits);
            }

            let mut md_ids_only: Vec<String> = Vec::with_capacity(md_children.len());
            for ch in md_children.iter() {
                let (rep_id, _exits) =
                    self.build_header(*ch, visited_scopes, Some(cluster_id.clone()));
                md_ids_only.push(rep_id);
            }
            if let Some(first_md) = md_ids_only.first().cloned() {
                if !branch_exits.is_empty() {
                    for e in branch_exits.iter() {
                        self.graph.edges.push(Edge {
                            from: e.clone(),
                            to: first_md.clone(),
                        });
                    }
                } else {
                    self.graph.edges.push(Edge {
                        from: decision_id.clone(),
                        to: first_md.clone(),
                    });
                }
            }
            for win in md_ids_only.windows(2) {
                let (a, b) = (&win[0], &win[1]);
                self.graph.edges.push(Edge {
                    from: a.clone(),
                    to: b.clone(),
                });
            }
            let outward = if let Some(last) = md_ids_only.last().cloned() {
                vec![last]
            } else {
                branch_exits
            };
            self.header_exits.insert(hid, outward.clone());
            return (decision_id, outward);
        }

        let cluster_id = self.new_cluster_id();
        self.graph.clusters.push(Cluster {
            id: cluster_id.clone(),
            label: header.title.to_string(),
            parent: parent_cluster.clone(),
        });

        // Merge markdown children and direct nested roots, preserving each list's internal order
        let items_merged: Vec<Hid> = self.merge_by_pos(&md_children, &nested_children);

        let mut first_rep: Option<String> = None;
        let mut prev_exits: Option<Vec<String>> = None;
        for child_hid in items_merged.into_iter() {
            // If this child is a direct nested root for the current container, prebuild its scope
            // inside this container's cluster and use the scope's final exits.
            let mut prebuilt_scope_last_exits: Option<Vec<String>> = None;
            if nested_children.contains(&child_hid) {
                let child_scope = self.by_hid[&child_hid].scope;
                self.build_scope_sequence(child_scope, visited_scopes, Some(cluster_id.clone()));
                let scope_items: Vec<Hid> = self
                    .index
                    .headers_in_scope_iter(child_scope)
                    .filter(|h| !self.has_md_parent.contains(&h.hid))
                    .map(|h| h.hid)
                    .collect();
                if let Some(&last_in_scope) = scope_items.last() {
                    if let Some(ex) = self.header_exits.get(&last_in_scope).cloned() {
                        prebuilt_scope_last_exits = Some(ex);
                    }
                }
            }

            let (entry, mut exits) =
                self.build_header(child_hid, visited_scopes, Some(cluster_id.clone()));
            if let Some(scope_exits) = prebuilt_scope_last_exits.take() {
                exits = scope_exits;
            }
            if first_rep.is_none() {
                first_rep = Some(entry.clone());
            }
            if let Some(prev) = prev_exits.take() {
                for e in prev {
                    self.graph.edges.push(Edge {
                        from: e,
                        to: entry.clone(),
                    });
                }
            }
            prev_exits = Some(exits);
        }
        let entry = first_rep.unwrap_or_else(|| {
            // Create a placeholder node if the container is empty.
            let node_id = self.new_node_id();
            let span = SerializedSpan::serialize(&header.span);
            self.span_map.insert(node_id.clone(), span.clone());
            self.graph.nodes.push(Node {
                id: node_id.clone(),
                label: header.title.to_string(),
                kind: NodeKind::Header(hid, Some(span)),
                cluster: Some(cluster_id.clone()),
            });
            node_id
        });
        let exits = prev_exits.unwrap_or_else(|| vec![entry.clone()]);
        self.header_entry.insert(hid, entry.clone());
        self.header_exits.insert(hid, exits.clone());
        (entry, exits)
    }

    fn render_calls_for_header(&mut self, hid: Hid, header_rep_id: &str, cluster: Option<String>) {
        if let Some(callees) = self.index.header_calls.get(&hid) {
            for callee in callees {
                if let Some(cached_id) = self.call_node_cache.get(&(hid, callee.clone())) {
                    self.graph.edges.push(Edge {
                        from: cached_id.clone(),
                        to: header_rep_id.to_string(),
                    });
                    continue;
                }
                let call_node_id = self.new_node_id();
                self.graph.nodes.push(Node {
                    id: call_node_id.clone(),
                    label: callee.clone(),
                    kind: NodeKind::Call {
                        header: hid,
                        callee: callee.clone(),
                    },
                    cluster: cluster.clone(),
                });
                self.graph.edges.push(Edge {
                    from: call_node_id.clone(),
                    to: header_rep_id.to_string(),
                });
                self.call_node_cache
                    .insert((hid, callee.clone()), call_node_id);
            }
        }
    }

    fn new_node_id(&mut self) -> String {
        let id = format!("n{}", self.next_node);
        self.next_node += 1;
        id
    }
    fn new_cluster_id(&mut self) -> String {
        let id = format!("sg{}", self.next_cluster);
        self.next_cluster += 1;
        id
    }
}

struct MermaidRenderer;

impl MermaidRenderer {
    fn render(
        graph: &Graph,
        direction: Direction,
        use_fancy: bool,
        span_map: BamlMap<String, SerializedSpan>,
    ) -> String {
        let mut out: Vec<String> = Vec::new();
        // out.push("---".to_string());
        // out.push("config:".to_string());
        // out.push("  layout: elk".to_string());
        // out.push("---".to_string());
        // out.push("".to_string());
        match direction {
            Direction::TD => out.push("flowchart TD".to_string()),
            Direction::LR => out.push("flowchart LR".to_string()),
        }
        if use_fancy {
            out.push("classDef loopContainer shape:processes,fill:#e0f7fa,stroke:#006064,stroke-width:2px,color:#000".to_string());
            out.push(
                "classDef decisionNode fill:#fff3e0,stroke:#ef6c00,stroke-width:2px,color:#000"
                    .to_string(),
            );
            if SHOW_CALL_NODES {
                out.push("".to_string());
                out.push(
                    "classDef callNode fill:#fffde7,stroke:#f9a825,stroke-width:2px,color:#000"
                        .to_string(),
                );
            }
        }

        let mut children_by_parent: BamlMap<Option<String>, Vec<&Cluster>> = BamlMap::new();
        for c in &graph.clusters {
            children_by_parent
                .entry(c.parent.clone())
                .or_default()
                .push(c);
        }
        let mut nodes_by_cluster: BamlMap<Option<String>, Vec<&Node>> = BamlMap::new();
        for n in &graph.nodes {
            nodes_by_cluster
                .entry(n.cluster.clone())
                .or_default()
                .push(n);
        }

        fn emit(
            out: &mut Vec<String>,
            cluster: Option<&Cluster>,
            children_by_parent: &BamlMap<Option<String>, Vec<&Cluster>>,
            nodes_by_cluster: &BamlMap<Option<String>, Vec<&Node>>,
            use_fancy: bool,
            indent: usize,
        ) {
            let indent_str = " ".repeat(indent);
            let key = cluster.map(|c| c.id.clone());
            let key_opt = key.clone();
            if let Some(c) = cluster {
                out.push(format!(
                    "{}subgraph {}[\"{}\"]",
                    indent_str,
                    c.id,
                    escape_label(&c.label)
                ));
                out.push(format!("{indent_str}  direction LR"));
            }
            if let Some(nodes) = nodes_by_cluster.get(&key_opt) {
                for n in nodes {
                    match &n.kind {
                        NodeKind::Decision(_, _) => {
                            out.push(format!(
                                "{}  {}{{\"{}\"}}",
                                indent_str,
                                n.id,
                                escape_label(&n.label)
                            ));
                            // Always emit decisionNode class line to match expected output
                            out.push(format!("{}  class {} decisionNode;", indent_str, n.id));
                        }
                        NodeKind::Call { .. } => {
                            out.push(format!(
                                "{}  {}[\"{}\"]",
                                indent_str,
                                n.id,
                                escape_label(&n.label)
                            ));
                            if use_fancy && SHOW_CALL_NODES {
                                out.push(format!("{}  class {} callNode;", indent_str, n.id));
                            }
                        }
                        NodeKind::Header(_, _) => {
                            out.push(format!(
                                "{}  {}[\"{}\"]",
                                indent_str,
                                n.id,
                                escape_label(&n.label)
                            ));
                        }
                    }
                }
            }
            if let Some(children) = children_by_parent.get(&key_opt) {
                for ch in children {
                    emit(
                        out,
                        Some(ch),
                        children_by_parent,
                        nodes_by_cluster,
                        use_fancy,
                        indent + 2,
                    );
                }
            }
            if cluster.is_some() {
                out.push(format!("{indent_str}end"));
            }
        }
        emit(
            &mut out,
            None,
            &children_by_parent,
            &nodes_by_cluster,
            use_fancy,
            0,
        );

        let mut emitted: HashSet<(String, String)> = HashSet::new();
        for e in &graph.edges {
            if emitted.insert((e.from.clone(), e.to.clone())) {
                out.push(format!("  {} --> {}", e.from, e.to));
            }
        }
        // Click lines disabled for tests
        if !span_map.is_empty() {
            if let Ok(json) = serde_json::to_string(&span_map) {
                out.push(format!("%%__BAML_SPANMAP__={json}"));
            }
            for rep_id in span_map.keys() {
                out.push(format!(
                    "  click {rep_id} call bamlMermaidNodeClick() \"Go to source\""
                ));
            }
        }
        out.join("\n")
    }
}

#[inline]
fn escape_label(s: &str) -> String {
    s.replace('"', "&quot;")
}
