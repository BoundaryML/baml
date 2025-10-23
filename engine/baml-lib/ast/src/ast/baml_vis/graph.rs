/// Renderer-agnostic graph model and builder
use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use baml_types::BamlMap;
use indexmap::IndexMap;
use internal_baml_diagnostics::SerializedSpan;

use super::header_collector::{HeaderIndex, HeaderLabelKind, Hid, RenderableHeader, ScopeId};

/// Config: maximum number of direct children (markdown + nested) a non-branching
/// container may have to be flattened into a linear sequence instead of a subgraph.
///
/// With the current value (`0`), any container with visible children retains its own cluster.
/// Top-level scopes are never flattened if they have any children, regardless of this value.
const MAX_CHILDREN_TO_FLATTEN: usize = 0;

pub fn build<'index>(
    index: &'index HeaderIndex,
    config: BuilderConfig,
) -> (Graph<'index>, BamlMap<NodeId, SerializedSpan>) {
    let pre = Prelude::from_index(index);
    let builder = GraphBuilder::new(index, &pre, config);
    let (graph, span_map) = builder.build();
    (graph, span_map)
}

use super::{graph, SHOW_CALL_NODES};

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Direction {
    TD,
    LR,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum NodeKind<'index> {
    Header(Hid, Option<SerializedSpan>),
    Decision(Hid, Option<SerializedSpan>),
    Call { header: Hid, callee: &'index str },
}

#[derive(Debug, Clone)]
pub struct Node<'index> {
    pub id: NodeId,
    pub label: &'index str,
    pub kind: NodeKind<'index>,
    pub cluster: Option<ClusterId>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub from: NodeId,
    pub to: NodeId,
}
#[derive(Debug, Clone)]
pub struct Cluster<'index> {
    pub id: ClusterId,
    pub label: &'index str,
    pub parent: Option<ClusterId>,
}

#[derive(Debug, Default)]
pub struct Graph<'index> {
    pub nodes: Vec<Node<'index>>,
    pub edges: Vec<Edge>,
    pub clusters: Vec<Cluster<'index>>,
}

impl<'index> Graph<'index> {
    // NOTE: since the graph is currently a tree, there should be no need for having the node id
    // before constructing it.
    // Only reason we use a builder function is because `Nodes` store their own id.
    // Since we grab `&mut self` anyway & don't give it to the function, it won't be able to
    // add more nodes to the tree before it has inserted this one.
    pub fn add_node(&mut self, make: impl FnOnce(NodeId) -> Node<'index>) -> NodeId {
        let node_id = NodeId(self.nodes.len() as u32);
        self.nodes.push(make(node_id));
        node_id
    }

    pub fn add_cluster(&mut self, make: impl FnOnce(ClusterId) -> Cluster<'index>) -> ClusterId {
        let id = ClusterId(self.clusters.len() as u32);
        self.clusters.push(make(id));
        id
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BuilderConfig {
    show_call_nodes: bool,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            show_call_nodes: SHOW_CALL_NODES,
        }
    }
}

struct GraphBuilder<'index, 'pre> {
    index: &'index HeaderIndex,
    cfg: BuilderConfig,
    graph: Graph<'index>,
    // TODO: add Prelude ref directly.
    by_hid: &'pre HashMap<Hid, &'index RenderableHeader>,
    md_children: &'pre HashMap<Hid, Vec<Hid>>,
    has_md_parent: &'pre HashSet<Hid>,
    nested_children: &'pre HashMap<Hid, Vec<Hid>>,
    nested_targets: &'pre HashSet<Hid>,

    // TODO: check visit order
    header_entry: HashMap<Hid, NodeId>,
    header_exits: HashMap<Hid, Vec<NodeId>>,
    // we're going to need stable iteration in snapshot tests.
    span_map: BamlMap<NodeId, SerializedSpan>,
}

// NOTE: this could be part of HeaderIndex
/// Cached, precomuted data used by graph builder.
struct Prelude<'index> {
    // NOTE: this could be a Box<[&'index RenderableHeader]>. Doesn't matter much.
    /// Map to reference by `Hid`, since [`HeaderIndex::headers`] has a different order.
    by_hid: HashMap<Hid, &'index RenderableHeader>,
    /// Nodes that have markdown header children, & their respective children.
    md_children: HashMap<Hid, Vec<Hid>>,
    /// Set of nodes that have a markdown header parent.
    has_md_parent: HashSet<Hid>,
    /// For nested edges, the ones that cross a code scope.
    nested_children: HashMap<Hid, Vec<Hid>>,
    /// The set of all [`Hid`] that are nested children.
    nested_targets: HashSet<Hid>,
}

impl<'index> Prelude<'index> {
    pub fn from_index(index: &'index HeaderIndex) -> Self {
        let mut by_hid = HashMap::new();

        let mut idstr_to_hid = HashMap::new();
        for h in &index.headers {
            by_hid.insert(h.hid, h);
            idstr_to_hid.insert(h.id.as_str(), h.hid);
        }

        let mut md_children: HashMap<_, Vec<_>> = HashMap::new();
        let mut has_md_parent = HashSet::new();
        for h in &index.headers {
            if let Some(pid) = &h.parent_id {
                if let Some(&ph) = idstr_to_hid.get(pid.as_str()) {
                    let parent = by_hid[&ph];
                    if parent.scope == h.scope {
                        md_children.entry(ph).or_default().push(h.hid);
                        has_md_parent.insert(h.hid);
                    }
                }
            }
        }

        let mut nested_children: HashMap<_, Vec<_>> = HashMap::new();
        let mut nested_targets: HashSet<_> = HashSet::new();

        for (p, c) in nested_scope_edges(index, &by_hid) {
            nested_children.entry(p).or_default().push(c);
            nested_targets.insert(c);
        }

        Self {
            by_hid,
            md_children,
            has_md_parent,
            nested_children,
            nested_targets,
        }
    }
}

impl<'index, 'pre> GraphBuilder<'index, 'pre> {
    pub fn new(index: &'index HeaderIndex, pre: &'pre Prelude<'index>, cfg: BuilderConfig) -> Self {
        Self {
            index,
            cfg,
            graph: Graph::default(),
            by_hid: &pre.by_hid,
            md_children: &pre.md_children,
            has_md_parent: &pre.has_md_parent,
            nested_children: &pre.nested_children,
            nested_targets: &pre.nested_targets,
            header_entry: HashMap::new(),
            header_exits: HashMap::new(),
            span_map: BamlMap::new(),
        }
    }

    pub fn build(mut self) -> (Graph<'index>, BamlMap<NodeId, SerializedSpan>) {
        let scope_root = build_scope_roots(self.index);

        let top_scopes = self.index.scopes().filter(|scope| {
            let root_hid = &scope_root[scope];
            !self.nested_targets.contains(root_hid)
        });

        let filtered_headers = self.classify_and_filter_headers();

        dbg!(&self.index.headers);
        dbg!(&filtered_headers);

        for scope in top_scopes {
            self.build_scope_sequence(scope, None, &filtered_headers.actions);
        }

        self.add_scope_edges();

        if self.cfg.show_call_nodes {
            self.add_header_calls();
        }

        (self.graph, self.span_map)
    }

    /// Returns an iterator of (child, parent) pairs, relating each component of the scope to the
    /// previous in the list. First component is related to `parent_hid`.
    fn link_scope_children<'iter>(
        &'iter self,
        scope: ScopeId,
        parent_hid: Hid,
    ) -> impl Iterator<Item = (Hid, Hid)> + 'iter {
        let mut non_md_headers = self
            .index
            .headers_in_scope_iter(scope)
            .map(|h| h.hid)
            .filter(|hid| !self.has_md_parent.contains(hid));

        let first = non_md_headers.next();

        first
            .map(move |first| {
                // we relate each subsequent header to the previous one in its scope
                let paired_rest = non_md_headers.scan(first, |prev, next| {
                    Some((next, std::mem::replace(prev, next)))
                });

                [(first, parent_hid)].into_iter().chain(paired_rest)
            })
            .into_iter()
            .flatten()
    }

    // NOTE: right now, algorithm switches between visiting scopes & visiting headers because
    // headers are not linked to their parent headers via scopes.

    /// Returns a map of actions to take on filtered headers. Iterating the map will result ina
    /// pre-order traversal on the headers.
    fn classify_and_filter_headers(&self) -> FilteredHeaders {
        let mut actions = IndexMap::new();

        // for non-root & non-filtered nodes, their parent.
        let mut parent = HashMap::new();

        let classify_nonfiltered = |header: &RenderableHeader, parent: &mut HashMap<Hid, Hid>| {
            let mut add_all_children = || {
                let md_children = self
                    .md_children
                    .get(&header.hid)
                    .into_iter()
                    .flatten()
                    .copied();
                let nested_children = self
                    .nested_children
                    .get(&header.hid)
                    .into_iter()
                    .flatten()
                    .copied();
                let parent_hid = header.hid;
                let md_children = md_children.map(|child| (child, parent_hid));

                let scope_entries = nested_children.flat_map(|child| {
                    let scope = self.by_hid[&child].scope;

                    self.link_scope_children(scope, parent_hid)
                });

                parent.extend(md_children.chain(scope_entries));
            };

            // If any of the children lists is Some(), then it is nonempty. So unwrap_or(&[]) does
            // not remove information.

            let md_children = self
                .md_children
                .get(&header.hid)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let nested_children = self
                .nested_children
                .get(&header.hid)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            dbg!(md_children, nested_children);

            if matches!((md_children, nested_children), ([], [])) {
                HeaderAction::Empty
            } else if header.label_kind == HeaderLabelKind::If {
                // mark children as visited.
                add_all_children();
                HeaderAction::If
            } else {
                let single_nested_child_has_multiple_items = nested_children.len() == 1 && {
                    let child_root = self.by_hid[&nested_children[0]];
                    let count = self.index.headers_in_scope_iter(child_root.scope).count();
                    count > 1
                };

                let total_children = md_children.len() + nested_children.len();

                #[allow(clippy::absurd_extreme_comparisons)]
                let should_flatten = total_children <= MAX_CHILDREN_TO_FLATTEN
                    && !single_nested_child_has_multiple_items;

                if should_flatten {
                    // implicitly make filtered removed by not adding a parent to them

                    if md_children.len() == 1 {
                        parent.insert(md_children[0], header.hid);
                    } else if nested_children.len() == 1 {
                        let scope = self.by_hid[&nested_children[0]].scope;
                        parent.extend(self.link_scope_children(scope, header.hid));
                    }

                    HeaderAction::Flattened
                } else {
                    add_all_children();
                    HeaderAction::Nonempty
                }
            }
        };

        // forward action classification & filtering.

        for root in self.index.headers.iter().filter(|h| h.parent_id.is_none()) {
            actions.insert(root.hid, classify_nonfiltered(root, &mut parent));
        }

        for header in &self.index.headers {
            // no parent => filtered.
            let Some(parent_id) = parent.get(&header.hid).copied() else {
                continue;
            };

            // to allow for scope sequences within filtered context, if a visited header is the first of its scope, the entire scope is
            // marked as visited.

            let mut scope_iter = self.index.headers_in_scope_iter(header.scope);

            let is_first_of_scope = scope_iter
                .next()
                .is_some_and(|first| first.hid == header.hid);

            if is_first_of_scope {
                parent.extend(scope_iter.map(|header| (header.hid, parent_id)));
            }

            actions.insert(header.hid, classify_nonfiltered(header, &mut parent));
        }

        FilteredHeaders { actions, parent }
    }

    /// Links the items in each scope in source order, showing execution order.
    fn add_scope_edges(&mut self) {
        for scope in self.index.scopes() {
            // TODO: I think we can remove collect()?
            let mut items = self
                .index
                .headers_in_scope_iter(scope)
                .filter(|h| !self.has_md_parent.contains(&h.hid))
                .map(|h| h.hid);

            let Some(first) = items.next() else {
                continue;
            };

            let prev_pairs =
                items.scan(first, |prev, cur| Some((cur, std::mem::replace(prev, cur))));

            for (hid, prev_hid) in prev_pairs {
                let entry = self.header_entry[&hid];
                let prev_exits = &self.header_exits[&prev_hid];
                // NOTE: `extend` on a loop. TL;DR: each exits vec is pretty small.
                // More thorough explanation at the end of `build_header`.
                self.graph.edges.extend(
                    prev_exits
                        .iter()
                        .copied()
                        .map(|e| Edge { from: e, to: entry }),
                );
            }
        }
    }

    fn build_scope_sequence(
        &mut self,
        scope: ScopeId,
        parent_cluster: Option<ClusterId>,
        filtered: &IndexMap<Hid, HeaderAction>,
    ) {
        let items = self
            .index
            .headers_in_scope_iter(scope)
            .filter(|h| !self.has_md_parent.contains(&h.hid))
            .map(|h| h.hid);

        // post-order: build headers inside scope. <- parent_cluster
        for hid in items {
            self.build_header(hid, parent_cluster, filtered);
        }
    }

    fn build_header(
        &mut self,
        hid: Hid,
        parent_cluster: Option<ClusterId>,
        filtered: &IndexMap<Hid, HeaderAction>,
    ) {
        // pre-order/post-order notation:
        // <pre/post>-order: <name> [[output] <- <dependency list>]

        let header = self.by_hid[&hid];

        assert!(
            !self.header_entry.contains_key(&hid),
            "header graph is a tree - no cycles!"
        );
        let md_children = self.md_children.get(&hid).map(Vec::as_slice).unwrap_or(&[]);
        let nested_children = self
            .nested_children
            .get(&hid)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        match filtered[&hid] {
            HeaderAction::Empty => {
                let span = SerializedSpan::serialize(&header.span);
                let node_id = self.graph.add_node(|node_id| Node {
                    id: node_id,
                    label: header.title.as_ref(),
                    kind: NodeKind::Header(hid, Some(span.clone())),
                    cluster: parent_cluster,
                });
                self.span_map.insert(node_id, span);
                self.header_entry.insert(hid, node_id);
                self.header_exits.insert(hid, vec![node_id]);
            }
            HeaderAction::If => {
                // pre-order: assign cluster ids: cluster_id <- header, parent_cluster
                let cluster_id = self.graph.add_cluster(|cluster_id| Cluster {
                    id: cluster_id,
                    label: header.title.as_ref(),
                    parent: parent_cluster,
                });

                // pre-order: insert entry <- header, cluster_id
                let span = SerializedSpan::serialize(&header.span);
                let decision_id = self.graph.add_node(|decision_id| Node {
                    id: decision_id,
                    label: header.title.as_ref(),
                    kind: NodeKind::Decision(hid, Some(span.clone())),
                    cluster: Some(cluster_id),
                });
                self.span_map.insert(decision_id, span);
                self.header_entry.insert(hid, decision_id);

                // post-order: build scope sequence for scope & build header
                // header_entry, header_exit <- cluster_id.
                // For some reason it can't work without a visited_scopes? That's pre-order data.
                for child_root_id in nested_children {
                    let child_scope = self.by_hid[child_root_id].scope;
                    self.build_scope_sequence(child_scope, Some(cluster_id), filtered);
                }

                // post-order: build header for each of the markdown children <- cluster_id.
                // Same doubt wrt visited_scopes.
                for child in md_children {
                    self.build_header(*child, Some(cluster_id), filtered);
                }

                // unordered: add edges from decision id to children <- decision_id
                self.graph
                    .edges
                    .extend(nested_children.iter().map(|child_root_id| Edge {
                        from: decision_id,
                        to: self.header_entry[child_root_id],
                    }));

                // post-order: collect branch exits <- header_exits
                let branch_exits: Vec<_> = nested_children
                    .iter()
                    .flat_map(|child_hid| &self.header_exits[child_hid])
                    .copied()
                    .collect();

                let md_ids_only: Vec<_> =
                    md_children.iter().map(|ch| self.header_entry[ch]).collect();

                if let Some(first_md) = md_ids_only.first().copied() {
                    if !branch_exits.is_empty() {
                        for e in branch_exits.iter() {
                            self.graph.edges.push(Edge {
                                from: *e,
                                to: first_md,
                            });
                        }
                    } else {
                        self.graph.edges.push(Edge {
                            from: decision_id,
                            to: first_md,
                        });
                    }
                }
                self.graph.edges.extend(md_ids_only.windows(2).map(|win| {
                    let from = win[0];
                    let to = win[1];
                    Edge { from, to }
                }));
                let outward = if let Some(last) = md_ids_only.last().copied() {
                    vec![last]
                } else {
                    branch_exits
                };
                self.header_exits.insert(hid, outward.clone());
            }
            HeaderAction::Nonempty => {
                // pre-order: assign cluster id cluster_id <- header, parent_cluster

                let cluster_id = self.graph.add_cluster(|cluster_id| Cluster {
                    id: cluster_id,
                    label: header.title.as_ref(),
                    // TODO: clone() on copy
                    parent: parent_cluster,
                });

                // post-order: build scope sequence <- cluster_id, visited_scopes. Only for direct nested
                // children.
                for child_hid in nested_children {
                    let child_scope = self.by_hid[child_hid].scope;
                    self.build_scope_sequence(child_scope, Some(cluster_id), filtered);
                }

                // post-order: build header for markdown children (scope sequence in nested already visits nested
                // children)
                // <- cluster_id
                for &child_hid in md_children {
                    self.build_header(child_hid, Some(cluster_id), filtered);
                }

                // Merge markdown children and direct nested roots, preserving each list's internal order
                let items_merged: Vec<_> =
                    merge_by_pos(self.by_hid, md_children, nested_children).collect();

                // We should have at least one item, since empty children are handled separately.
                let first_rep = self.header_entry[&items_merged[0]];

                // unordered: create edges <- header_exits.
                // Left scan for choosing exits, although choosing fn is not expensive so it can be
                // executed twice.

                let choose_exits_hid = |child_hid| {
                    // NOTE: since nested children Hids are marked, can we use pre?
                    let prebuilt_scope_last_exits = if nested_children.contains(&child_hid) {
                        let child_scope = self.by_hid[&child_hid].scope;
                        let maybe_last_in_scope = self
                            .index
                            .headers_in_scope_iter(child_scope)
                            .rev()
                            .map(|h| h.hid)
                            .find(|h| !self.has_md_parent.contains(h));

                        maybe_last_in_scope.filter(|l| self.header_exits.contains_key(l))
                    } else {
                        None
                    };

                    prebuilt_scope_last_exits.unwrap_or(child_hid)
                };

                // NOTE: using Hid instead of direct reference since otherwise we lock writes to
                // `self.header_exits`. We know that invalidating the reference that we hold is not possible
                // but we cannot communicate this to Rust.
                let mut prev_exits_hid = choose_exits_hid(items_merged[0]);

                for &child_hid in &items_merged[1..] {
                    let entry = self.header_entry[&child_hid];

                    let exits_hid = choose_exits_hid(child_hid);
                    let prev = std::mem::replace(&mut prev_exits_hid, exits_hid);

                    // NOTE: using `extend` in a loop. Since most nodes will only have 1 or 2 children,
                    // impact is covered by exponential allocation.
                    //
                    // The full iterator is `FlatMap`, it doesn't implement `ExactSizedIterator`
                    // and doesn't have a reliable size_hint, so the perf fix here is to precalculate
                    // the total exit count with a .iter().map().sum().
                    let edges_prev = self.header_exits[&prev].iter().copied().map(|exit| Edge {
                        from: exit,
                        to: entry,
                    });

                    self.graph.edges.extend(edges_prev);
                }

                let entry = first_rep;
                self.header_entry.insert(hid, entry);
                let exits = self.header_exits[&prev_exits_hid].to_owned();
                self.header_exits.insert(hid, exits);
            }
            HeaderAction::Flattened => {
                // pre-order add & assign node id to header
                let span = SerializedSpan::serialize(&header.span);
                let node_id = self.graph.add_node(|node_id| Node {
                    id: node_id,
                    label: header.title.as_ref(),
                    kind: NodeKind::Header(hid, Some(span.clone())),
                    cluster: parent_cluster,
                });

                self.span_map.insert(node_id, span.clone());
                self.header_entry.insert(hid, node_id);

                // TODO: classify child & what to do inside flattened, like scope sequence needed or
                // not.

                // post-order: build scope sequence for child scope, only when flatten is single child.
                // This may be implicit if the child is marked as visited.

                // post-order: run children headers
                if md_children.len() == 1 {
                    self.build_header(md_children[0], parent_cluster, filtered);
                } else if nested_children.len() == 1 {
                    let child_root_hid = nested_children[0];
                    let child_scope = self.by_hid[&child_root_hid].scope;
                    // build_scope_sequence already calls build_header for the entries inside the
                    // scope, including the child_root_hid
                    self.build_scope_sequence(child_scope, parent_cluster, filtered);
                }

                // unordered: add edges <- node_id, children id

                if md_children.len() == 1 {
                    let c_entry = self.header_entry[&md_children[0]];

                    self.graph.edges.push(Edge {
                        from: node_id,
                        to: c_entry,
                    });
                } else if nested_children.len() == 1 {
                    let c_entry = self.header_entry[&nested_children[0]];
                    self.graph.edges.push(Edge {
                        from: node_id,
                        to: c_entry,
                    });
                }

                // post-order: copy exits from children.
                // NOTE: we could reference them, but the vecs are pretty small.
                // TODO: If each tree node only has one preceding parent, we can extract them (move) instead.
                let exits = if md_children.len() == 1 {
                    self.header_exits[&md_children[0]].to_owned()
                } else if nested_children.len() == 1 {
                    self.header_exits[&nested_children[0]].to_owned()
                } else {
                    vec![node_id]
                };

                self.header_exits.insert(hid, exits);
            }
        }
    }

    /// Visits the headers that have an assigned node id, & for each inserts & links a call node
    /// if the header is in [`HeaderIndex::header_calls`]
    fn add_header_calls(&mut self) {
        let entries_with_calls = self.header_entry.iter().filter_map(|(hid, rep_id)| {
            self.index
                .header_calls
                .get(hid)
                .map(|callees| (*hid, *rep_id, callees))
        });

        for (hid, rep_id, callees) in entries_with_calls {
            let cluster = self.graph.nodes[rep_id.0 as usize].cluster;
            for callee in callees {
                let call_node_id = self.graph.add_node(|call_node_id| Node {
                    id: call_node_id,
                    label: callee.as_str(),
                    kind: NodeKind::Call {
                        header: hid,
                        callee,
                    },
                    cluster,
                });
                self.graph.edges.push(Edge {
                    from: call_node_id,
                    to: rep_id,
                });
            }
        }
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ClusterId(u32);

#[derive(Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeId(u32);

impl serde::Serialize for NodeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl std::fmt::Debug for ClusterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}
impl std::fmt::Debug for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "n{}", self.0)
    }
}

impl std::fmt::Display for ClusterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "sg{}", self.0)
    }
}

fn build_scope_roots(index: &HeaderIndex) -> HashMap<ScopeId, Hid> {
    // iterate the headers by scope order. The first one to appear is the scope root.
    let mut scope_root = HashMap::new();
    for h in &index.headers {
        scope_root.entry(h.scope).or_insert(h.hid);
    }
    scope_root
}

/// Edges that cross a scope enter, i.e not just header changes.
fn nested_scope_edges<'iter>(
    index: &'iter HeaderIndex,
    by_hid: &'iter HashMap<Hid, &'iter RenderableHeader>,
) -> impl Iterator<Item = (Hid, Hid)> + 'iter {
    index
        .nested_edges_hid_iter()
        .filter(|(p, c)| by_hid[p].scope != by_hid[c].scope)
        .copied()
}

/// Compute a tuple position key for stable ordering comparisons
fn pos_tuple<'index>(
    by_hid: &HashMap<Hid, &'index RenderableHeader>,
    hid: Hid,
) -> (&'index Path, usize) {
    let h = by_hid[&hid];
    (h.span.file.path_buf().as_ref(), h.span.start)
}

/// Merge two already-ordered lists by source position, preserving internal order
fn merge_by_pos<'index, 'iter>(
    by_hid: &'iter HashMap<Hid, &'index RenderableHeader>,
    lhs: &'iter [Hid],
    rhs: &'iter [Hid],
) -> impl Iterator<Item = Hid> + 'iter
where
    'index: 'iter,
{
    return State { by_hid, lhs, rhs };

    // implement iterator manually for size hint + exact size.
    struct State<'index, 'iter> {
        by_hid: &'iter HashMap<Hid, &'index RenderableHeader>,
        lhs: &'iter [Hid],
        rhs: &'iter [Hid],
    }

    impl ExactSizeIterator for State<'_, '_> {
        fn len(&self) -> usize {
            self.lhs.len() + self.rhs.len()
        }
    }

    impl Iterator for State<'_, '_> {
        type Item = Hid;

        // exact sized
        fn size_hint(&self) -> (usize, Option<usize>) {
            let len = self.len();
            (len, Some(len))
        }

        fn next(&mut self) -> Option<Self::Item> {
            match (self.lhs, self.rhs) {
                ([l, lrest @ ..], [r, rrest @ ..]) => Some(
                    if pos_tuple(self.by_hid, *l) <= pos_tuple(self.by_hid, *r) {
                        self.lhs = lrest;
                        *l
                    } else {
                        self.rhs = rrest;
                        *r
                    },
                ),
                ([l, rest @ ..], []) => {
                    self.lhs = rest;
                    Some(*l)
                }
                ([], [r, rest @ ..]) => {
                    self.rhs = rest;
                    Some(*r)
                }
                ([], []) => None,
            }
        }
    }
}

/// An internal action to take on a header. This is stored because there are both pre-order &
/// post-order visits that need this information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderAction {
    Empty,
    Nonempty,
    Flattened,
    If,
}

#[derive(Debug)]
struct FilteredHeaders {
    /// Map of actions to take on filtered headers. Iterating the map will result ina
    /// pre-order traversal on the headers.
    actions: IndexMap<Hid, HeaderAction>,

    #[allow(dead_code)] // FIXME: remove this
    // Given a header, tracks its parent, even across scope boundaries.
    parent: HashMap<Hid, Hid>,
}
