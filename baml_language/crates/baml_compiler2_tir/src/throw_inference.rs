//! HIR-level throw inference for compiler2 (BEP-007).
//!
//! This runs before type inference and computes a per-function transitive throw
//! set over the call graph. Functions with declared `throws` clauses act as
//! firewalls: their declared set becomes caller-visible, replacing body-derived
//! facts for propagation.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use baml_base::Name;
use baml_compiler2_ast::{Expr, ExprBody, Literal};
use baml_compiler2_hir::{
    contributions::Definition,
    package::{PackageId, PackageItems, package_dependencies},
};

use crate::{
    lower_type_expr::{lower_type_expr_in_ns_into, qualify_def},
    ty::{PrimitiveType, Ty},
};

/// A throw fact is a `Ty`.
pub type ThrowFact = Ty;

/// Prefix for `self`-qualified call targets inside class methods.
const SELF_PREFIX: &str = "self.";
/// The literal name of the own-package alias used in qualified paths.
const ROOT_PACKAGE: &str = "root";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionThrowSets {
    pub transitive: BTreeMap<Name, BTreeSet<ThrowFact>>,
}

// Comparison-based replacement for Salsa early cutoff.
crate::impl_partial_eq_salsa_update!(FunctionThrowSets);

impl FunctionThrowSets {
    pub fn transitive_for(&self, name: &Name) -> Option<&BTreeSet<ThrowFact>> {
        self.transitive.get(name)
    }
}

#[salsa::tracked(returns(ref))]
pub fn function_throw_sets<'db>(
    db: &'db dyn crate::Db,
    package_id: PackageId<'db>,
) -> FunctionThrowSets {
    let pkg_items = baml_compiler2_ppir::package_items(db, package_id);
    // Load dependency interfaces for cross-package throw lookup
    let dep_interfaces: Vec<&crate::package_interface::PackageInterface> =
        package_dependencies(db, package_id)
            .iter()
            .map(|dep_id| crate::package_interface::package_interface(db, *dep_id))
            .collect();

    let mut graph: AnalysisGraph<Name, ThrowFact> = AnalysisGraph::new();

    let mut builder = ThrowGraphBuilder::default();

    for ns in pkg_items.namespaces.values() {
        for (short_name, def) in &ns.values {
            let Definition::Function(func_loc) = def else {
                continue;
            };

            let key = function_key(db, *func_loc, short_name);
            let item_tree = baml_compiler2_ppir::file_item_tree(db, func_loc.file(db));
            let func_data = &item_tree[func_loc.id(db)];
            builder.process(
                db,
                pkg_items,
                *func_loc,
                key,
                &func_data.generic_params,
                None,
            );
        }

        // Also process class methods, which are not in ns.values.
        for (class_name, def) in &ns.types {
            let Definition::Class(class_loc) = def else {
                continue;
            };
            let file = class_loc.file(db);
            let item_tree = baml_compiler2_ppir::file_item_tree(db, file);
            let class_data = &item_tree[class_loc.id(db)];

            for &method_id in &class_data.methods {
                let method_data = &item_tree[method_id];
                let method_name = &method_data.name;
                let func_loc = baml_compiler2_hir::loc::FunctionLoc::new(db, file, method_id);
                // Key as "ClassName.method_name" (with namespace prefix if any).
                let method_short = Name::new(format!("{class_name}.{method_name}"));
                let key = function_key(db, func_loc, &method_short);

                builder.process(
                    db,
                    pkg_items,
                    func_loc,
                    key,
                    &method_data.generic_params,
                    Some(class_name),
                );
            }
        }
    }

    // Destructure the accumulators back into locals so the downstream merge /
    // add-node / add-edge block stays byte-for-byte identical (and the
    // nodes-then-edges ordering and BTreeMap/BTreeSet iteration are preserved).
    let ThrowGraphBuilder {
        mut direct_facts,
        has_declared_contract,
        call_edges,
    } = builder;

    // Process call edges: for cross-package targets, merge their throw facts
    // into the caller's direct facts; same-package targets become graph edges
    // (added after nodes so endpoints carry their enriched direct facts).
    let mut same_pkg_edges: Vec<(Name, Name)> = Vec::new();
    for (from, targets) in &call_edges {
        if has_declared_contract.contains(from) {
            continue;
        }
        for to in targets {
            if let Some(dep_throws) = lookup_dep_throw_set(&dep_interfaces, to) {
                // Cross-package: merge dependency's transitive throw facts into caller's direct facts
                direct_facts
                    .entry(from.clone())
                    .or_default()
                    .extend(dep_throws.iter().cloned());
            } else {
                same_pkg_edges.push((from.clone(), to.clone()));
            }
        }
    }

    // Add all nodes with their (possibly enriched) direct facts
    for (key, facts) in &direct_facts {
        graph.add_node(key.clone(), facts.clone());
    }

    // Add same-package call edges
    for (from, to) in same_pkg_edges {
        graph.add_edge(from, to);
    }

    let analysis = graph.analyze();

    let mut transitive = BTreeMap::new();
    for (name, facts) in analysis.iter_transitive() {
        transitive.insert(name.clone(), facts.clone());
    }

    FunctionThrowSets { transitive }
}

/// Accumulates the direct throw facts, declared-contract flags, and call edges
/// for every callable in a package, keyed by throw-set key.
#[derive(Default)]
struct ThrowGraphBuilder {
    /// Direct facts per callable. Tracked separately so cross-package facts can
    /// be merged in before the nodes are added to the graph.
    direct_facts: BTreeMap<Name, BTreeSet<ThrowFact>>,
    has_declared_contract: BTreeSet<Name>,
    call_edges: BTreeMap<Name, BTreeSet<Name>>,
}

impl ThrowGraphBuilder {
    /// Compute the direct throw facts, declared-contract flag, and call edges
    /// for a single callable (free function or class method) and record them.
    ///
    /// `class_name` is `Some` for class methods, in which case `self.X` call
    /// targets are rewritten to `ClassName.X` so edges connect to the correct
    /// graph nodes.
    fn process<'db>(
        &mut self,
        db: &'db dyn crate::Db,
        pkg_items: &PackageItems<'db>,
        func_loc: baml_compiler2_hir::loc::FunctionLoc<'db>,
        key: Name,
        generic_params: &[Name],
        class_name: Option<&Name>,
    ) {
        let sig = baml_compiler2_ppir::function_signature(db, func_loc);
        let body = baml_compiler2_ppir::function_body(db, func_loc);
        let ns =
            baml_compiler2_hir::file_package::file_package(db, func_loc.file(db)).namespace_path;

        let declared_throws = sig.throws.as_ref().map(|te| {
            // Throws recovery ignores lowering diagnostics here; they surface
            // elsewhere during inference.
            let lowered =
                lower_type_expr_in_ns_into(db, te, pkg_items, &ns, generic_params, &mut |_diag| {});
            flatten_ty_to_facts(&lowered)
        });

        let has_contract = declared_throws.is_some();
        let expr_body = if let baml_compiler2_hir::body::FunctionBody::Expr(b) = body.as_ref() {
            Some(b)
        } else {
            None
        };
        let direct = if let Some(declared) = declared_throws {
            declared
        } else if let Some(expr_body) = expr_body {
            collect_direct_throws(db, pkg_items, &ns, expr_body)
        } else {
            BTreeSet::new()
        };

        self.direct_facts.insert(key.clone(), direct);
        if has_contract {
            self.has_declared_contract.insert(key.clone());
        }

        if let Some(expr_body) = expr_body {
            let targets = collect_call_targets(expr_body);
            let targets = match class_name {
                Some(class_name) => targets
                    .into_iter()
                    .map(|t| rewrite_self_target(&t, class_name))
                    .collect(),
                None => targets,
            };
            self.call_edges.insert(key, targets);
        }
    }
}

/// Build the throw-set lookup key for a function given its namespace path and short name.
///
/// For top-level functions the key is just the short name; for namespaced
/// functions it is `"ns1.ns2.name"`.
pub fn throw_set_key(namespace_path: &[Name], short_name: &Name) -> Name {
    if namespace_path.is_empty() {
        short_name.clone()
    } else {
        let mut parts = namespace_path.to_vec();
        parts.push(short_name.clone());
        segments_to_dotted_name(&parts)
    }
}

fn function_key<'db>(
    db: &'db dyn crate::Db,
    func: baml_compiler2_hir::loc::FunctionLoc<'db>,
    short_name: &Name,
) -> Name {
    let file = func.file(db);
    let pkg = baml_compiler2_hir::file_package::file_package(db, file);
    throw_set_key(&pkg.namespace_path, short_name)
}

fn collect_direct_throws<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    body: &ExprBody,
) -> BTreeSet<ThrowFact> {
    let mut facts = BTreeSet::new();

    for (_, expr) in body.exprs.iter() {
        if let Expr::Throw { value } = expr {
            facts.insert(throw_fact_from_expr(
                db, pkg_items, ns_context, *value, body,
            ));
        }
    }
    for (_, stmt) in body.stmts.iter() {
        if let baml_compiler2_ast::Stmt::Throw { value } = stmt {
            facts.insert(throw_fact_from_expr(
                db, pkg_items, ns_context, *value, body,
            ));
        }
    }

    // Remove facts that correspond to catch binding variable names.
    // This is a heuristic: if a binding name happens to shadow a type name,
    // the corresponding fact is suppressed.
    let catch_bindings = collect_catch_binding_names(body);
    if !catch_bindings.is_empty() {
        facts.retain(|fact| {
            let name = fact_display_name(fact);
            !catch_bindings.contains(name.as_str())
        });
    }

    facts
}

/// Get a display name for a throw fact, used for the catch binding name filter.
fn fact_display_name(fact: &Ty) -> String {
    match fact {
        Ty::Primitive(p) => p.to_string(),
        Ty::Class(qn, _) | Ty::Enum(qn) | Ty::TypeAlias(qn) => qn.to_string(),
        Ty::EnumVariant(qn, variant) => format!("{qn}.{variant}"),
        Ty::Unknown => "unknown".to_string(),
        _ => format!("{fact}"),
    }
}

fn collect_call_targets(body: &ExprBody) -> BTreeSet<Name> {
    let mut targets = BTreeSet::new();
    for (_, expr) in body.exprs.iter() {
        if let Expr::Call { callee, .. } = expr {
            if let Some(path) = crate::throws_analysis::expr_to_path_segments(*callee, body) {
                targets.insert(segments_to_dotted_name(&path));
            }
        }
    }
    targets
}

/// Join path segments into a single dotted `Name` (e.g. `["a", "b"]` -> `"a.b"`).
fn segments_to_dotted_name(segments: &[Name]) -> Name {
    Name::new(
        segments
            .iter()
            .map(Name::as_str)
            .collect::<Vec<_>>()
            .join("."),
    )
}

/// Convert a thrown expression to a `Ty` directly, using `pkg_items` to resolve
/// paths to their actual types (enum variants, classes, etc).
fn throw_fact_from_expr<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    expr_id: baml_compiler2_ast::ExprId,
    body: &ExprBody,
) -> Ty {
    match &body.exprs[expr_id] {
        Expr::Literal(Literal::String(_)) => Ty::Primitive(PrimitiveType::String),
        Expr::Literal(Literal::Int(_)) => Ty::Primitive(PrimitiveType::Int),
        Expr::Literal(Literal::Float(_)) => Ty::Primitive(PrimitiveType::Float),
        Expr::Literal(Literal::Bool(_)) => Ty::Primitive(PrimitiveType::Bool),
        Expr::Null => Ty::Primitive(PrimitiveType::Null),
        Expr::Path(_) | Expr::MemberAccess { .. } => {
            crate::throws_analysis::expr_to_path_segments(expr_id, body)
                .map(|segments| resolve_path_to_ty(db, pkg_items, ns_context, &segments))
                .unwrap_or(Ty::Unknown)
        }
        Expr::Object {
            type_name: Some(path),
            ..
        } => resolve_path_to_ty(db, pkg_items, ns_context, path.segments()),
        _ => Ty::Unknown,
    }
}

/// Rewrite a call target name from `self.X` to `ClassName.X`.
/// Other targets are returned unchanged.
fn rewrite_self_target(target: &Name, class_name: &Name) -> Name {
    let s = target.as_str();
    if let Some(rest) = s.strip_prefix(SELF_PREFIX) {
        Name::new(format!("{class_name}.{rest}"))
    } else {
        target.clone()
    }
}

/// Look up a type by namespace, preferring `ns_context`-qualified resolution
/// for bare names. When `ns_context` is non-empty and the path carries no
/// namespace of its own (`ns.is_empty()`), try the `ns_context`-qualified name
/// first, then fall back to the bare lookup; otherwise look up `ns` directly.
fn lookup_type_ns_first<'db>(
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    ns: &[Name],
    name: &Name,
) -> Option<Definition<'db>> {
    if !ns_context.is_empty() && ns.is_empty() {
        pkg_items
            .lookup_type(ns_context, name)
            .or_else(|| pkg_items.lookup_type(ns, name))
    } else {
        pkg_items.lookup_type(ns, name)
    }
}

/// Resolve a path like `["Status", "HttpError"]` or `["ns", "Status", "HttpError"]`
/// or `["Status"]` to a `Ty`.
fn resolve_path_to_ty<'db>(
    db: &'db dyn crate::Db,
    pkg_items: &PackageItems<'db>,
    ns_context: &[Name],
    segments: &[Name],
) -> Ty {
    // Try treating the last segment as an enum variant and the prefix as
    // the enum path. For bare `["Status", "HttpError"]` from a namespaced file,
    // try namespace-qualified first, then unqualified.
    if segments.len() >= 2 {
        let enum_path = &segments[..segments.len() - 1];
        let variant = &segments[segments.len() - 1];
        let enum_name = enum_path.last().expect("enum_path is non-empty");
        let enum_ns = &enum_path[..enum_path.len() - 1];
        // Try with namespace context for bare enum names
        let def = lookup_type_ns_first(pkg_items, ns_context, enum_ns, enum_name);
        if let Some(def) = def {
            if let Definition::Enum(_) = def {
                let qtn = qualify_def(db, def, enum_name);
                return Ty::EnumVariant(qtn, variant.clone());
            }
        }
    }

    // Try the full path as a type lookup. For single-segment bare names,
    // try namespace-qualified first, then unqualified.
    let name = segments.last().expect("segments is non-empty");
    let seg_ns = &segments[..segments.len() - 1];
    let def = lookup_type_ns_first(pkg_items, ns_context, seg_ns, name);
    // Cross-package fallback for qualified paths whose first segment names
    // either the literal `root` package (own-package alias) or another
    // package the resolver knows about. Mirrors `lower_type_expr_in_ns` so
    // throw-set type recovery works for `root.http.Response { ... }` literals
    // and `baml.errors.DevOther` references inside throw expressions.
    let def = def.or_else(|| {
        if segments.len() < 2 {
            return None;
        }
        if segments[0].as_str() == ROOT_PACKAGE {
            pkg_items.lookup_type(&segments[1..segments.len() - 1], name)
        } else {
            let pkg_id = PackageId::new(db, segments[0].clone());
            let pkg = baml_compiler2_ppir::package_items(db, pkg_id);
            pkg.lookup_type(&segments[1..segments.len() - 1], name)
        }
    });
    if let Some(def) = def {
        return match def {
            Definition::Class(_) => Ty::Class(qualify_def(db, def, name), vec![]),
            Definition::Enum(_) => Ty::Enum(qualify_def(db, def, name)),
            Definition::TypeAlias(_) => Ty::TypeAlias(qualify_def(db, def, name)),
            _ => Ty::Unknown,
        };
    }

    Ty::Unknown
}

fn collect_catch_binding_names(body: &ExprBody) -> HashSet<&str> {
    let mut names = HashSet::new();
    for (_, expr) in body.exprs.iter() {
        if let Expr::Catch { clauses, .. } = expr {
            for clause in clauses {
                if let Some(name) = body.patterns[clause.binding].binding_name(&body.patterns) {
                    names.insert(name.as_str());
                }
            }
        }
    }
    names
}

/// Flatten a compound `Ty` into its leaf throw facts.
/// Unions and optionals are decomposed; leaf types are kept as-is.
pub fn flatten_ty_to_facts(ty: &Ty) -> BTreeSet<ThrowFact> {
    let mut out = BTreeSet::new();
    collect_leaf_types(ty, &mut out);
    out
}

fn collect_leaf_types(ty: &Ty, out: &mut BTreeSet<Ty>) {
    match ty {
        // Compound types: decompose
        Ty::Optional(inner) => {
            collect_leaf_types(inner, out);
            out.insert(Ty::Primitive(PrimitiveType::Null));
        }
        Ty::Union(members) => {
            for member in members {
                collect_leaf_types(member, out);
            }
        }
        // Literal types: widen to primitive for throw fact purposes
        Ty::Literal(lit, _) => {
            out.insert(Ty::Primitive(PrimitiveType::from_literal(lit)));
        }
        // Bottom/void: no facts
        Ty::Never | Ty::Void => {}
        // Everything else: keep as-is
        _ => {
            out.insert(ty.clone());
        }
    }
}

/// Look up a function's transitive throw set from dependency interfaces.
fn lookup_dep_throw_set<'a>(
    dep_interfaces: &'a [&crate::package_interface::PackageInterface],
    target_name: &Name,
) -> Option<&'a BTreeSet<ThrowFact>> {
    dep_interfaces
        .iter()
        .find_map(|iface| iface.throw_sets.transitive_for(target_name))
}

/// Reject catch-everything binding types — `unknown` and unresolved `any`.
///
/// Operates on the resolved `Ty` produced by TIR's `pattern_type`. Both
/// `unknown` (an explicit `unknown` type) and `any` (an unresolved path that
/// the user typed expecting it to mean "anything") collapse to
/// `Ty::BuiltinUnknown` / `Ty::Unknown` after resolution. We can't
/// distinguish between them at this point, so the diagnostic just says
/// "unknown".
pub fn is_banned_catch_binding_type(ty: &Ty) -> Option<&'static str> {
    if matches!(ty, Ty::BuiltinUnknown | Ty::Unknown) {
        Some("unknown")
    } else {
        None
    }
}

// ── Direct+transitive analysis framework ────────────────────────────────────
//
// Generic two-pass direct+transitive analysis. Provides a reusable cache for
// computing per-node "facts" that propagate transitively through a dependency
// graph. Throw-set inference is its only consumer, so it lives here rather than
// in a standalone module.
//
// The `Tarjan` SCC core is `pub(crate)` and shared with `normalize.rs`'s
// cycle-detection passes. Each caller supplies its own determinism via the
// node/successor ordering key passed to `components_ordered` (the throws path
// uses Ord order; the cycle-detection paths key by canonical `to_string()`).

/// A dependency graph annotated with per-node direct facts.
///
/// `N` is the node identifier type (must be `Ord` for deterministic ordering).
/// `F` is the fact type (must be `Ord` for set operations).
struct AnalysisGraph<N: Ord + Clone, F: Ord + Clone> {
    /// Per-node direct facts (pass 1 output, provided by the caller).
    direct: BTreeMap<N, BTreeSet<F>>,
    /// Adjacency list: `node → {dependency₁, dependency₂, …}`.
    /// An edge from A to B means "A depends on B" (i.e., A calls B).
    edges: BTreeMap<N, BTreeSet<N>>,
}

impl<N: Ord + Clone, F: Ord + Clone> AnalysisGraph<N, F> {
    /// Create an empty graph.
    fn new() -> Self {
        Self {
            direct: BTreeMap::new(),
            edges: BTreeMap::new(),
        }
    }

    /// Register a node with its direct facts.
    fn add_node(&mut self, node: N, facts: BTreeSet<F>) {
        self.direct.insert(node.clone(), facts);
        self.edges.entry(node).or_default();
    }

    /// Add a directed dependency edge: `from` depends on `to`.
    fn add_edge(&mut self, from: N, to: N) {
        self.direct.entry(to.clone()).or_default();
        self.edges.entry(to.clone()).or_default();
        self.edges.entry(from).or_default().insert(to);
    }

    /// Consume the graph and compute the transitive closure of facts.
    fn analyze(self) -> AnalysisResult<N, F> {
        let sccs = Tarjan::components(&self.edges);
        let transitive = propagate(&self.direct, &self.edges, &sccs);
        AnalysisResult { transitive }
    }
}

/// The output of a two-pass analysis.
struct AnalysisResult<N: Ord, F: Ord> {
    /// Per-node transitive facts (direct ∪ propagated from dependencies).
    transitive: BTreeMap<N, BTreeSet<F>>,
}

impl<N: Ord, F: Ord> AnalysisResult<N, F> {
    /// Iterate over all nodes and their transitive facts in deterministic order.
    fn iter_transitive(&self) -> impl Iterator<Item = (&N, &BTreeSet<F>)> {
        self.transitive.iter()
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

/// Generic Tarjan's strongly-connected-components core.
///
/// Returns ALL components (including trivial singletons), each ordered to match
/// DFS visitation order (`component.reverse()` after stack-pop). Determinism is
/// the CALLER's responsibility: traversal visits nodes and successors in the
/// order given by `components_ordered`'s `key`. Any cycle filtering / rotation /
/// component sorting is left to the caller as a post-pass.
pub(crate) struct Tarjan<'g, N: Ord> {
    /// Successors of each node, pre-sorted by the caller-supplied key.
    sorted_succ: BTreeMap<N, Vec<&'g N>>,
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

    /// Compute SCCs visiting nodes and successors in Ord order.
    ///
    /// Equivalent to `components_ordered(edges, |n| n.clone())`, but avoids the
    /// clone since `BTreeMap`/`BTreeSet` iteration is already Ord-ordered.
    pub(crate) fn components(edges: &'g BTreeMap<N, BTreeSet<N>>) -> Vec<Vec<N>> {
        Self::components_ordered(edges, |n| n)
    }

    /// Compute SCCs visiting nodes and successors in the order induced by `key`.
    ///
    /// `key(&node)` defines the visitation order of both the outer node loop and
    /// each node's successors. Returns ALL components (no filtering, no
    /// rotation) so callers can apply their own deterministic post-pass.
    pub(crate) fn components_ordered<'k, K, FK>(
        edges: &'g BTreeMap<N, BTreeSet<N>>,
        key: FK,
    ) -> Vec<Vec<N>>
    where
        K: Ord,
        FK: Fn(&'k N) -> K,
        N: 'k,
        'g: 'k,
    {
        let mut sorted_succ: BTreeMap<N, Vec<&'g N>> = BTreeMap::new();
        for (node, succs) in edges {
            let mut succs: Vec<&'g N> = succs.iter().collect();
            succs.sort_by_key(|n| key(n));
            sorted_succ.insert(node.clone(), succs);
        }

        let mut nodes: Vec<&'g N> = edges.keys().collect();
        nodes.sort_by_key(|n| key(n));

        let mut tarjan = Self {
            sorted_succ,
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

        for node in nodes {
            if tarjan.state[node].index == Self::UNVISITED {
                tarjan.strong_connect(node);
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

        if let Some(successors) = self.sorted_succ.get(node_id) {
            for succ_id in successors.clone() {
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
