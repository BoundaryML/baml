//! Type normalization and subtyping.
//!
//! Converts surface `Ty` types to an internal `StructuralTy` where all type
//! aliases are resolved. Recursive aliases are represented using Mu types with
//! equirecursive (co-inductive) subtyping.

use std::collections::{HashMap, HashSet};

use baml_base::Name;

use crate::ty::{LiteralValue, PrimitiveType, QualifiedTypeName, Ty};

// ═══════════════════════════════════════════════════════════════════════════
// PUBLIC API
// ═══════════════════════════════════════════════════════════════════════════

/// Check if `sub` is a subtype of `sup`, resolving type aliases.
pub(crate) fn is_subtype_of(sub: &Ty, sup: &Ty, aliases: &HashMap<QualifiedTypeName, Ty>) -> bool {
    let recursive = find_recursive_aliases(aliases);
    let sub_norm = normalize(sub, aliases, &recursive);
    let sup_norm = normalize(sup, aliases, &recursive);
    sub_norm.is_subtype_of(&sup_norm, &mut HashSet::new())
}

/// Find all recursive type aliases via DFS.
pub fn find_recursive_aliases(
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> HashSet<QualifiedTypeName> {
    let mut recursive = HashSet::new();
    for name in aliases.keys() {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        if has_cycle(name, aliases, &mut visited, &mut stack) {
            recursive.insert(name.clone());
        }
    }
    recursive
}

// ═══════════════════════════════════════════════════════════════════════════
// STRUCTURAL TYPE (private)
// ═══════════════════════════════════════════════════════════════════════════

/// Normalized structural type. All aliases resolved, recursion explicit.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum StructuralTy {
    // Primitives
    Int,
    Float,
    String,
    Bool,
    Null,
    Image,
    Audio,
    Video,
    Pdf,
    // Literal
    Literal(baml_base::Literal),
    // User-defined (resolved by qualified name)
    Class(QualifiedTypeName),
    Enum(QualifiedTypeName),
    EnumVariant(QualifiedTypeName, Name),
    // Constructors
    Optional(Box<StructuralTy>),
    List(Box<StructuralTy>),
    Map {
        key: Box<StructuralTy>,
        value: Box<StructuralTy>,
    },
    Union(Vec<StructuralTy>),
    Function {
        params: Vec<StructuralTy>,
        ret: Box<StructuralTy>,
    },
    // Recursion
    Mu {
        var: QualifiedTypeName,
        body: Box<StructuralTy>,
    },
    TyVar(QualifiedTypeName),
    // Special
    Never,
    Void,
    /// The explicit `unknown` keyword — top type (supertype of everything).
    BuiltinUnknown,
    Unknown,
    Error,
}

impl StructuralTy {
    /// Equirecursive subtyping with co-inductive assumptions.
    fn is_subtype_of(
        &self,
        other: &StructuralTy,
        assumptions: &mut HashSet<(StructuralTy, StructuralTy)>,
    ) -> bool {
        // Co-inductive: if we've assumed this pair, it holds
        let pair = (self.clone(), other.clone());
        if assumptions.contains(&pair) {
            return true;
        }

        // Reflexivity
        if self == other {
            return true;
        }

        // Never is the bottom type — subtype of everything
        if matches!(self, StructuralTy::Never) {
            return true;
        }

        // BuiltinUnknown is the top type — everything is a subtype of it.
        // But BuiltinUnknown itself is NOT a subtype of specific types
        // (unlike the error-recovery Unknown which is bidirectionally compatible).
        if matches!(other, StructuralTy::BuiltinUnknown) {
            return true;
        }
        // If self is BuiltinUnknown and other is not BuiltinUnknown (reflexivity
        // already handled equal case above), it's not a subtype.
        if matches!(self, StructuralTy::BuiltinUnknown) {
            return false;
        }

        // Void is only compatible with itself (handled by reflexivity above)
        if matches!(self, StructuralTy::Void) || matches!(other, StructuralTy::Void) {
            return false;
        }

        // Error recovery: Unknown/Error are compatible with anything
        if matches!(self, StructuralTy::Unknown | StructuralTy::Error)
            || matches!(other, StructuralTy::Unknown | StructuralTy::Error)
        {
            return true;
        }

        assumptions.insert(pair.clone());

        let result = match (self, other) {
            // Mu unfolding
            (StructuralTy::Mu { var, body }, other) => {
                let unfolded = substitute(body, var, self);
                unfolded.is_subtype_of(other, assumptions)
            }
            (self_ty, StructuralTy::Mu { var, body }) => {
                let unfolded = substitute(body, var, other);
                self_ty.is_subtype_of(&unfolded, assumptions)
            }

            // TyVar (inside Mu bodies)
            (StructuralTy::TyVar(v1), StructuralTy::TyVar(v2)) => v1 == v2,

            // Null <: Optional<T>
            (StructuralTy::Null, StructuralTy::Optional(_)) => true,

            // T <: Optional<T>
            (inner, StructuralTy::Optional(opt_inner)) => {
                inner.is_subtype_of(opt_inner, assumptions)
            }

            // Optional<T> <: T | null
            (StructuralTy::Optional(inner), StructuralTy::Union(types)) => {
                types.contains(&StructuralTy::Null)
                    && types.iter().any(|t| inner.is_subtype_of(t, assumptions))
            }

            // T <: T | U
            (inner, StructuralTy::Union(types)) => {
                types.iter().any(|t| inner.is_subtype_of(t, assumptions))
            }

            // Union<T1, T2> <: U iff all Ti <: U
            (StructuralTy::Union(types), other) => {
                types.iter().all(|t| t.is_subtype_of(other, assumptions))
            }

            // List covariance
            (StructuralTy::List(inner1), StructuralTy::List(inner2)) => {
                inner1.is_subtype_of(inner2, assumptions)
            }

            // Map covariance in value, invariant in key
            (
                StructuralTy::Map { key: k1, value: v1 },
                StructuralTy::Map { key: k2, value: v2 },
            ) => {
                let keys_compatible = k1 == k2
                    || matches!(k1.as_ref(), StructuralTy::Unknown | StructuralTy::Error)
                    || matches!(k2.as_ref(), StructuralTy::Unknown | StructuralTy::Error);
                keys_compatible && v1.is_subtype_of(v2, assumptions)
            }

            // Int <: Float
            (StructuralTy::Int, StructuralTy::Float) => true,

            // Literal types are subtypes of their base types
            (StructuralTy::Literal(LiteralValue::Int(_)), StructuralTy::Int) => true,
            (StructuralTy::Literal(LiteralValue::Int(_)), StructuralTy::Float) => true,
            (StructuralTy::Literal(LiteralValue::Float(_)), StructuralTy::Float) => true,
            (StructuralTy::Literal(LiteralValue::String(_)), StructuralTy::String) => true,
            (StructuralTy::Literal(LiteralValue::Bool(_)), StructuralTy::Bool) => true,

            // EnumVariant(E, V) <: Enum(E)
            (StructuralTy::EnumVariant(e, _), StructuralTy::Enum(sup_e)) => e == sup_e,

            // Function subtyping: contravariant params, covariant return
            (
                StructuralTy::Function {
                    params: params1,
                    ret: ret1,
                },
                StructuralTy::Function {
                    params: params2,
                    ret: ret2,
                },
            ) => {
                if !ret1.is_subtype_of(ret2, assumptions) {
                    return false;
                }
                if params2.len() > params1.len() {
                    return false;
                }
                for (p1, p2) in params1.iter().zip(params2.iter()) {
                    if !p2.is_subtype_of(p1, assumptions) {
                        return false;
                    }
                }
                true
            }

            _ => false,
        };

        assumptions.remove(&pair);
        result
    }
}

/// Substitute `TyVar` with replacement in type.
fn substitute(
    ty: &StructuralTy,
    var: &QualifiedTypeName,
    replacement: &StructuralTy,
) -> StructuralTy {
    match ty {
        StructuralTy::TyVar(v) if v == var => replacement.clone(),
        StructuralTy::Optional(inner) => {
            StructuralTy::Optional(Box::new(substitute(inner, var, replacement)))
        }
        StructuralTy::List(inner) => {
            StructuralTy::List(Box::new(substitute(inner, var, replacement)))
        }
        StructuralTy::Map { key, value } => StructuralTy::Map {
            key: Box::new(substitute(key, var, replacement)),
            value: Box::new(substitute(value, var, replacement)),
        },
        StructuralTy::Union(types) => StructuralTy::Union(
            types
                .iter()
                .map(|t| substitute(t, var, replacement))
                .collect(),
        ),
        StructuralTy::Function { params, ret } => StructuralTy::Function {
            params: params
                .iter()
                .map(|t| substitute(t, var, replacement))
                .collect(),
            ret: Box::new(substitute(ret, var, replacement)),
        },
        StructuralTy::Mu { var: v, body } if v != var => StructuralTy::Mu {
            var: v.clone(),
            body: Box::new(substitute(body, var, replacement)),
        },
        _ => ty.clone(),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// NORMALIZATION (private)
// ═══════════════════════════════════════════════════════════════════════════

fn normalize(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    recursive: &HashSet<QualifiedTypeName>,
) -> StructuralTy {
    let mut expanding = HashSet::new();
    normalize_impl(ty, aliases, recursive, &mut expanding)
}

fn normalize_impl(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    recursive: &HashSet<QualifiedTypeName>,
    expanding: &mut HashSet<QualifiedTypeName>,
) -> StructuralTy {
    match ty {
        Ty::Primitive(p) => match p {
            PrimitiveType::Int => StructuralTy::Int,
            PrimitiveType::Float => StructuralTy::Float,
            PrimitiveType::String => StructuralTy::String,
            PrimitiveType::Bool => StructuralTy::Bool,
            PrimitiveType::Null => StructuralTy::Null,
            PrimitiveType::Image => StructuralTy::Image,
            PrimitiveType::Audio => StructuralTy::Audio,
            PrimitiveType::Video => StructuralTy::Video,
            PrimitiveType::Pdf => StructuralTy::Pdf,
        },
        Ty::Never => StructuralTy::Never,
        Ty::Void => StructuralTy::Void,
        Ty::BuiltinUnknown => StructuralTy::BuiltinUnknown,
        Ty::Unknown => StructuralTy::Unknown,
        Ty::Error => StructuralTy::Error,
        Ty::Literal(lit, _freshness) => StructuralTy::Literal(lit.clone()),
        Ty::Class(qn) => StructuralTy::Class(qn.clone()),
        Ty::Enum(qn) => StructuralTy::Enum(qn.clone()),
        Ty::EnumVariant(qn, v) => StructuralTy::EnumVariant(qn.clone(), v.clone()),

        Ty::TypeAlias(qn) => {
            if expanding.contains(qn) {
                return StructuralTy::TyVar(qn.clone());
            }

            if let Some(alias_ty) = aliases.get(qn) {
                if recursive.contains(qn) {
                    expanding.insert(qn.clone());
                    let body = normalize_impl(alias_ty, aliases, recursive, expanding);
                    expanding.remove(qn);
                    StructuralTy::Mu {
                        var: qn.clone(),
                        body: Box::new(body),
                    }
                } else {
                    normalize_impl(alias_ty, aliases, recursive, expanding)
                }
            } else {
                StructuralTy::Error
            }
        }

        Ty::Optional(inner) => StructuralTy::Optional(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),
        Ty::List(inner) | Ty::EvolvingList(inner) => StructuralTy::List(Box::new(normalize_impl(
            inner, aliases, recursive, expanding,
        ))),
        Ty::Map(key, value) | Ty::EvolvingMap(key, value) => StructuralTy::Map {
            key: Box::new(normalize_impl(key, aliases, recursive, expanding)),
            value: Box::new(normalize_impl(value, aliases, recursive, expanding)),
        },
        Ty::Union(types) => StructuralTy::Union(
            types
                .iter()
                .map(|t| normalize_impl(t, aliases, recursive, expanding))
                .collect(),
        ),
        Ty::Function { params, ret } => StructuralTy::Function {
            params: params
                .iter()
                .map(|(_, t)| normalize_impl(t, aliases, recursive, expanding))
                .collect(),
            ret: Box::new(normalize_impl(ret, aliases, recursive, expanding)),
        },
        // `$rust_type` — opaque Rust-managed state. Treated as Unknown
        // in the structural type system (cannot be constructed or destructured
        // by user code).
        Ty::RustType => StructuralTy::Unknown,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CYCLE DETECTION
// ═══════════════════════════════════════════════════════════════════════════

fn has_cycle(
    name: &QualifiedTypeName,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    visited: &mut HashSet<QualifiedTypeName>,
    stack: &mut HashSet<QualifiedTypeName>,
) -> bool {
    if stack.contains(name) {
        return true;
    }
    if visited.contains(name) {
        return false;
    }
    visited.insert(name.clone());
    stack.insert(name.clone());
    let result = aliases
        .get(name)
        .is_some_and(|ty| ty_has_cycle(ty, aliases, visited, stack));
    stack.remove(name);
    result
}

fn ty_has_cycle(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
    visited: &mut HashSet<QualifiedTypeName>,
    stack: &mut HashSet<QualifiedTypeName>,
) -> bool {
    match ty {
        Ty::TypeAlias(qn) if aliases.contains_key(qn) => has_cycle(qn, aliases, visited, stack),
        Ty::Optional(inner) | Ty::List(inner) | Ty::EvolvingList(inner) => {
            ty_has_cycle(inner, aliases, visited, stack)
        }
        Ty::Map(key, value) | Ty::EvolvingMap(key, value) => {
            ty_has_cycle(key, aliases, visited, stack)
                || ty_has_cycle(value, aliases, visited, stack)
        }
        Ty::Union(types) => types
            .iter()
            .any(|t| ty_has_cycle(t, aliases, visited, stack)),
        Ty::Function { params, ret } => {
            params
                .iter()
                .any(|(_, t)| ty_has_cycle(t, aliases, visited, stack))
                || ty_has_cycle(ret, aliases, visited, stack)
        }
        _ => false,
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// INVALID CYCLE DETECTION (Tarjan's SCC + structural edges)
// ═══════════════════════════════════════════════════════════════════════════
//
// Mirrors the approach in `baml_compiler_tir/src/cycles.rs`:
// 1. Build a dependency graph tracking structural vs non-structural edges.
//    "Structural" means the reference goes through List or Map, which provide
//    a termination point (empty container). Optional and Union are pass-through.
// 2. Find SCCs via Tarjan's algorithm (deterministic ordering).
// 3. A cycle is valid if it has at least one structural edge within the SCC.
//    Otherwise every member gets an AliasCycle diagnostic.

/// Find all type aliases that participate in **invalid** (unguarded) cycles.
///
/// An edge is "structural" if it passes through a `List` or `Map` constructor,
/// which provides a base case for termination (empty list/map). `Optional` and
/// `Union` are pass-through — they do NOT create structural context.
///
/// A cycle is valid if at least one edge within the SCC is structural.
/// Otherwise all members are flagged as invalid.
///
/// Returns a set of qualified type names that should receive cycle diagnostics.
pub fn find_invalid_alias_cycles(
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> HashSet<QualifiedTypeName> {
    // 1. Build the graph + structural edge set
    let GraphResult {
        graph,
        structural_edges,
    } = build_alias_graph(aliases);

    // 2. Find SCCs via Tarjan's (deterministic, only real cycles)
    let sccs = Tarjan::components(&graph);

    // 3. For each SCC, check if it has at least one structural edge
    let mut invalid = HashSet::new();
    for scc in &sccs {
        let scc_set: HashSet<&QualifiedTypeName> = scc.iter().collect();
        let has_structural = structural_edges
            .iter()
            .any(|(from, to)| scc_set.contains(from) && scc_set.contains(to));

        if !has_structural {
            for name in scc {
                invalid.insert(name.clone());
            }
        }
    }

    invalid
}

/// Result of building a type alias dependency graph.
struct GraphResult {
    /// The full dependency graph (all edges, structural + non-structural).
    graph: HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>>,
    /// Edges that go through structural types (List/Map).
    structural_edges: HashSet<(QualifiedTypeName, QualifiedTypeName)>,
}

/// Build a graph of type alias dependencies, tracking which edges are structural.
fn build_alias_graph(aliases: &HashMap<QualifiedTypeName, Ty>) -> GraphResult {
    let mut graph: HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>> = HashMap::new();
    let mut structural_edges: HashSet<(QualifiedTypeName, QualifiedTypeName)> = HashSet::new();

    for (alias_name, ty) in aliases {
        let (mut non_structural, structural) = extract_type_alias_deps(ty, aliases);

        for dep in &structural {
            structural_edges.insert((alias_name.clone(), dep.clone()));
        }

        // Graph has ALL edges (structural + non-structural combined)
        non_structural.extend(structural);
        graph.insert(alias_name.clone(), non_structural);
    }

    GraphResult {
        graph,
        structural_edges,
    }
}

/// Extract type alias dependencies from a resolved type.
///
/// Returns `(non_structural_deps, structural_deps)` where structural means
/// the reference goes through `List` or `Map` (which provide a termination
/// point via empty container). `Optional` and `Union` are pass-through —
/// they do NOT create structural context.
fn extract_type_alias_deps(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> (HashSet<QualifiedTypeName>, HashSet<QualifiedTypeName>) {
    fn visit(
        ty: &Ty,
        aliases: &HashMap<QualifiedTypeName, Ty>,
        non_structural: &mut HashSet<QualifiedTypeName>,
        structural: &mut HashSet<QualifiedTypeName>,
        in_structural: bool,
    ) {
        match ty {
            Ty::TypeAlias(qn) if aliases.contains_key(qn) => {
                if in_structural {
                    structural.insert(qn.clone());
                } else {
                    non_structural.insert(qn.clone());
                }
            }
            Ty::Optional(inner) => {
                // Optional does NOT create structural context
                visit(inner, aliases, non_structural, structural, in_structural);
            }
            Ty::List(inner) | Ty::EvolvingList(inner) => {
                // List provides structural guard (can be empty)
                visit(inner, aliases, non_structural, structural, true);
            }
            Ty::Map(key, value) | Ty::EvolvingMap(key, value) => {
                // Map provides structural guard (can be empty)
                visit(key, aliases, non_structural, structural, true);
                visit(value, aliases, non_structural, structural, true);
            }
            Ty::Union(members) => {
                // Union passes through the structural context
                for m in members {
                    visit(m, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::Function { params, ret } => {
                for (_, t) in params {
                    visit(t, aliases, non_structural, structural, in_structural);
                }
                visit(ret, aliases, non_structural, structural, in_structural);
            }
            _ => {}
        }
    }

    let mut non_structural = HashSet::new();
    let mut structural = HashSet::new();
    visit(ty, aliases, &mut non_structural, &mut structural, false);
    (non_structural, structural)
}

// ── Tarjan's SCC ─────────────────────────────────────────────────────────────
//
// Adapted from `baml_compiler_tir/src/cycles.rs` — deterministic ordering
// via sorted traversal, component reversal, and rotation to minimum element.

/// State of each node for Tarjan's algorithm.
#[derive(Clone, Copy)]
struct NodeState {
    index: usize,
    low_link: usize,
    on_stack: bool,
}

/// Tarjan's strongly connected components algorithm.
///
/// Only returns real cycles (multi-node SCCs or single nodes with self-loops).
/// Components are sorted deterministically.
struct Tarjan<'g> {
    graph: &'g HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>>,
    index: usize,
    stack: Vec<QualifiedTypeName>,
    state: HashMap<QualifiedTypeName, NodeState>,
    components: Vec<Vec<QualifiedTypeName>>,
}

impl<'g> Tarjan<'g> {
    const UNVISITED: usize = usize::MAX;

    fn components(
        graph: &'g HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>>,
    ) -> Vec<Vec<QualifiedTypeName>> {
        let mut tarjan = Self {
            graph,
            index: 0,
            stack: Vec::new(),
            state: graph
                .keys()
                .map(|node| {
                    (
                        node.clone(),
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

        // Sort nodes for deterministic traversal order.
        let mut nodes: Vec<_> = graph.keys().cloned().collect();
        nodes.sort_by(|a, b| (&a.pkg, &a.name).cmp(&(&b.pkg, &b.name)));

        for node in &nodes {
            if tarjan.state[node].index == Self::UNVISITED {
                tarjan.strong_connect(node);
            }
        }

        // Sort components by first element for deterministic output.
        tarjan
            .components
            .sort_by(|a, b| (&a[0].pkg, &a[0].name).cmp(&(&b[0].pkg, &b[0].name)));

        tarjan.components
    }

    fn strong_connect(&mut self, node_id: &QualifiedTypeName) {
        let mut node = NodeState {
            index: self.index,
            low_link: self.index,
            on_stack: true,
        };
        self.index += 1;
        self.state.insert(node_id.clone(), node);
        self.stack.push(node_id.clone());

        // Sort successors for deterministic DFS order.
        let mut successors: Vec<_> = self.graph[node_id].iter().collect();
        successors.sort_by(|a, b| (&a.pkg, &a.name).cmp(&(&b.pkg, &b.name)));

        for successor_id in successors {
            let mut successor = self.state[successor_id];
            if successor.index == Self::UNVISITED {
                self.strong_connect(successor_id);
                successor = self.state[successor_id];
                node.low_link = std::cmp::min(node.low_link, successor.low_link);
            } else if successor.on_stack {
                node.low_link = std::cmp::min(node.low_link, successor.index);
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

            // Reverse: stack pop order → DFS visitation order
            component.reverse();

            // Only keep real cycles: multi-node or single node with self-loop.
            let is_cycle = component.len() > 1
                || (component.len() == 1 && self.graph[node_id].contains(node_id));

            if is_cycle {
                // Rotate to start at the lexicographically smallest element
                // for deterministic cycle paths.
                if let Some(min_idx) = component
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| (&a.pkg, &a.name).cmp(&(&b.pkg, &b.name)))
                    .map(|(i, _)| i)
                {
                    component.rotate_left(min_idx);
                }
                self.components.push(component);
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CLASS REQUIRED-FIELD CYCLE DETECTION
// ═══════════════════════════════════════════════════════════════════════════
//
// Classes with required (non-optional, non-list, non-map) fields that form
// a cycle are impossible to construct at runtime. We detect these using the
// same Tarjan's SCC infrastructure.
//
// Unlike type alias cycles, there is no "structural guard" exemption —
// every SCC found is unconditionally an error.

/// A class cycle: the names participating and a formatted path string.
pub struct ClassCycleInfo {
    /// All class names in this cycle.
    pub members: Vec<QualifiedTypeName>,
    /// Human-readable cycle path, e.g. "A -> B -> A".
    pub cycle_path: String,
}

/// Find all classes that participate in unconstructable required-field cycles.
///
/// A "required" field is one that is NOT optional, NOT a list, and NOT a map.
/// Optional/list/map fields can be null/empty, breaking the construction chain.
///
/// Returns a list of `ClassCycleInfo`, one per SCC found.
pub fn find_invalid_class_cycles(
    class_fields: &HashMap<QualifiedTypeName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<QualifiedTypeName, Ty>,
) -> Vec<ClassCycleInfo> {
    let graph = build_class_graph(class_fields, type_aliases);
    let sccs = Tarjan::components(&graph);

    sccs.into_iter()
        .map(|scc| {
            let cycle_path = format_cycle_path(&scc);
            ClassCycleInfo {
                members: scc,
                cycle_path,
            }
        })
        .collect()
}

/// Build a dependency graph of classes based on required field types.
fn build_class_graph(
    class_fields: &HashMap<QualifiedTypeName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<QualifiedTypeName, Ty>,
) -> HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>> {
    let mut graph: HashMap<QualifiedTypeName, HashSet<QualifiedTypeName>> = HashMap::new();

    // All classes must be in the graph (even if they have no required deps)
    for class_name in class_fields.keys() {
        graph.entry(class_name.clone()).or_default();
    }

    for (class_name, fields) in class_fields {
        let mut deps = HashSet::new();
        for (_field_name, field_ty) in fields {
            extract_required_class_deps(
                field_ty,
                class_fields,
                type_aliases,
                &mut deps,
                false, // not optional
                false, // not in list/map
                &mut HashSet::new(),
            );
        }
        graph.insert(class_name.clone(), deps);
    }

    graph
}

/// Extract required class dependencies from a field type.
///
/// A class reference is "required" only if it is NOT behind Optional, List,
/// or Map. Type aliases are resolved transparently.
fn extract_required_class_deps(
    ty: &Ty,
    class_fields: &HashMap<QualifiedTypeName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<QualifiedTypeName, Ty>,
    deps: &mut HashSet<QualifiedTypeName>,
    optional: bool,
    in_list_or_map: bool,
    visiting: &mut HashSet<QualifiedTypeName>,
) {
    match ty {
        Ty::Class(qn) => {
            // Only add if the field is truly required
            if !optional && !in_list_or_map && class_fields.contains_key(qn) {
                deps.insert(qn.clone());
            }
        }
        Ty::TypeAlias(qn) => {
            // Resolve through type aliases (only if still required context)
            if !optional && !in_list_or_map && !visiting.contains(qn) {
                if let Some(alias_ty) = type_aliases.get(qn) {
                    visiting.insert(qn.clone());
                    extract_required_class_deps(
                        alias_ty,
                        class_fields,
                        type_aliases,
                        deps,
                        optional,
                        in_list_or_map,
                        visiting,
                    );
                    visiting.remove(qn);
                }
            }
        }
        Ty::Optional(inner) => {
            // Optional breaks the hard dependency
            extract_required_class_deps(
                inner,
                class_fields,
                type_aliases,
                deps,
                true,
                in_list_or_map,
                visiting,
            );
        }
        Ty::List(inner) | Ty::EvolvingList(inner) => {
            // List breaks the hard dependency (can be empty)
            extract_required_class_deps(
                inner,
                class_fields,
                type_aliases,
                deps,
                optional,
                true,
                visiting,
            );
        }
        Ty::Map(key, value) | Ty::EvolvingMap(key, value) => {
            // Map breaks the hard dependency (can be empty)
            extract_required_class_deps(
                key,
                class_fields,
                type_aliases,
                deps,
                optional,
                true,
                visiting,
            );
            extract_required_class_deps(
                value,
                class_fields,
                type_aliases,
                deps,
                optional,
                true,
                visiting,
            );
        }
        Ty::Union(members) => {
            // Union: only a hard dependency if ALL variants lead to the same class.
            // If any variant provides an alternative (e.g. string), the cycle is broken.
            let mut variant_deps_list = Vec::new();
            for member in members {
                let mut variant_deps = HashSet::new();
                extract_required_class_deps(
                    member,
                    class_fields,
                    type_aliases,
                    &mut variant_deps,
                    optional,
                    in_list_or_map,
                    visiting,
                );
                variant_deps_list.push(variant_deps);
            }
            // Only add if ALL variants produce the same single dep
            if !variant_deps_list.is_empty() {
                let first = &variant_deps_list[0];
                if first.len() == 1 && variant_deps_list.iter().all(|d| d == first) {
                    deps.extend(first.iter().cloned());
                }
            }
        }
        _ => {}
    }
}

/// Format a cycle path as "A -> B -> C -> A".
fn format_cycle_path(cycle: &[QualifiedTypeName]) -> String {
    if cycle.len() == 1 {
        cycle[0].name.to_string()
    } else {
        let mut path: Vec<String> = cycle.iter().map(|qn| qn.name.to_string()).collect();
        path.push(cycle[0].name.to_string());
        path.join(" -> ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ty::Freshness;

    fn qn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("test"), Name::new(name))
    }

    fn type_alias(name: &str) -> Ty {
        Ty::TypeAlias(qn(name))
    }

    #[test]
    fn test_simple_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(qn("MyInt"), Ty::Primitive(PrimitiveType::Int));

        assert!(is_subtype_of(
            &type_alias("MyInt"),
            &Ty::Primitive(PrimitiveType::Int),
            &aliases
        ));
        assert!(is_subtype_of(
            &Ty::Primitive(PrimitiveType::Int),
            &type_alias("MyInt"),
            &aliases
        ));
    }

    #[test]
    fn test_transitive_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(qn("MyInt"), Ty::Primitive(PrimitiveType::Int));
        aliases.insert(qn("AnotherInt"), type_alias("MyInt"));

        assert!(is_subtype_of(
            &type_alias("AnotherInt"),
            &Ty::Primitive(PrimitiveType::Int),
            &aliases
        ));
        assert!(is_subtype_of(
            &type_alias("AnotherInt"),
            &type_alias("MyInt"),
            &aliases
        ));
    }

    #[test]
    fn test_union_alias() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("IntOrString"),
            Ty::Union(vec![
                Ty::Primitive(PrimitiveType::Int),
                Ty::Primitive(PrimitiveType::String),
            ]),
        );

        assert!(is_subtype_of(
            &Ty::Primitive(PrimitiveType::Int),
            &type_alias("IntOrString"),
            &aliases
        ));
        assert!(is_subtype_of(
            &Ty::Primitive(PrimitiveType::String),
            &type_alias("IntOrString"),
            &aliases
        ));
        assert!(!is_subtype_of(
            &Ty::Primitive(PrimitiveType::Bool),
            &type_alias("IntOrString"),
            &aliases
        ));
    }

    #[test]
    fn test_recursive_alias_detection() {
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("List"),
            Ty::Union(vec![Ty::Primitive(PrimitiveType::Null), type_alias("List")]),
        );

        let recursive = find_recursive_aliases(&aliases);
        assert!(recursive.contains(&qn("List")));
    }

    #[test]
    fn test_non_recursive_not_marked() {
        let mut aliases = HashMap::new();
        aliases.insert(qn("MyInt"), Ty::Primitive(PrimitiveType::Int));

        let recursive = find_recursive_aliases(&aliases);
        assert!(!recursive.contains(&qn("MyInt")));
    }

    #[test]
    fn test_never_is_bottom() {
        let aliases = HashMap::new();

        assert!(is_subtype_of(
            &Ty::Never,
            &Ty::Primitive(PrimitiveType::Int),
            &aliases
        ));
        assert!(is_subtype_of(
            &Ty::Never,
            &Ty::Primitive(PrimitiveType::String),
            &aliases
        ));
        assert!(is_subtype_of(&Ty::Never, &Ty::Class(qn("Foo")), &aliases));
        assert!(is_subtype_of(
            &Ty::Never,
            &Ty::Optional(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
    }

    #[test]
    fn test_int_subtype_of_float() {
        let aliases = HashMap::new();
        assert!(is_subtype_of(
            &Ty::Primitive(PrimitiveType::Int),
            &Ty::Primitive(PrimitiveType::Float),
            &aliases
        ));
        assert!(!is_subtype_of(
            &Ty::Primitive(PrimitiveType::Float),
            &Ty::Primitive(PrimitiveType::Int),
            &aliases
        ));
    }

    #[test]
    fn test_literal_widens() {
        let aliases = HashMap::new();
        // Fresh and Regular should both be subtypes of their base primitive
        assert!(is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Fresh),
            &Ty::Primitive(PrimitiveType::Int),
            &aliases
        ));
        assert!(is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Regular),
            &Ty::Primitive(PrimitiveType::Float),
            &aliases
        ));
        assert!(is_subtype_of(
            &Ty::Literal(LiteralValue::String("hi".into()), Freshness::Fresh),
            &Ty::Primitive(PrimitiveType::String),
            &aliases
        ));
        assert!(!is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Fresh),
            &Ty::Primitive(PrimitiveType::String),
            &aliases
        ));
        // Freshness is ignored for subtyping: Fresh(1) <: Regular(1)
        assert!(is_subtype_of(
            &Ty::Literal(LiteralValue::Int(42), Freshness::Fresh),
            &Ty::Literal(LiteralValue::Int(42), Freshness::Regular),
            &aliases
        ));
    }

    #[test]
    fn test_enum_variant_subtype_of_enum() {
        let aliases = HashMap::new();
        assert!(is_subtype_of(
            &Ty::EnumVariant(qn("Color"), Name::new("Red")),
            &Ty::Enum(qn("Color")),
            &aliases
        ));
        assert!(!is_subtype_of(
            &Ty::EnumVariant(qn("Color"), Name::new("Red")),
            &Ty::Enum(qn("Shape")),
            &aliases
        ));
    }

    #[test]
    fn test_function_covariant_return() {
        let aliases = HashMap::new();
        let f1 = Ty::Function {
            params: vec![(None, Ty::Primitive(PrimitiveType::Int))],
            ret: Box::new(Ty::Primitive(PrimitiveType::Int)),
        };
        let f2 = Ty::Function {
            params: vec![(None, Ty::Primitive(PrimitiveType::Int))],
            ret: Box::new(Ty::Primitive(PrimitiveType::Float)),
        };
        assert!(is_subtype_of(&f1, &f2, &aliases));
        assert!(!is_subtype_of(&f2, &f1, &aliases));
    }

    #[test]
    fn test_function_contravariant_params() {
        let aliases = HashMap::new();
        let f1 = Ty::Function {
            params: vec![(None, Ty::Primitive(PrimitiveType::Float))],
            ret: Box::new(Ty::Primitive(PrimitiveType::String)),
        };
        let f2 = Ty::Function {
            params: vec![(None, Ty::Primitive(PrimitiveType::Int))],
            ret: Box::new(Ty::Primitive(PrimitiveType::String)),
        };
        assert!(is_subtype_of(&f1, &f2, &aliases));
        assert!(!is_subtype_of(&f2, &f1, &aliases));
    }

    #[test]
    fn test_optional_subtyping() {
        let aliases = HashMap::new();
        // int <: int?
        assert!(is_subtype_of(
            &Ty::Primitive(PrimitiveType::Int),
            &Ty::Optional(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
        // null <: int?
        assert!(is_subtype_of(
            &Ty::Primitive(PrimitiveType::Null),
            &Ty::Optional(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
        // string NOT <: int?
        assert!(!is_subtype_of(
            &Ty::Primitive(PrimitiveType::String),
            &Ty::Optional(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
    }

    // ── Evolving container tests ────────────────────────────────────────────

    #[test]
    fn test_evolving_list_subtype_of_list() {
        let aliases = HashMap::new();
        // EvolvingList(int) <: List(int)
        assert!(is_subtype_of(
            &Ty::EvolvingList(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &Ty::List(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
        // List(int) <: EvolvingList(int)
        assert!(is_subtype_of(
            &Ty::List(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &Ty::EvolvingList(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
    }

    #[test]
    fn test_evolving_list_covariance() {
        let aliases = HashMap::new();
        // EvolvingList(int) <: List(float) (int <: float)
        assert!(is_subtype_of(
            &Ty::EvolvingList(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &Ty::List(Box::new(Ty::Primitive(PrimitiveType::Float))),
            &aliases
        ));
        // EvolvingList(string) NOT <: List(int)
        assert!(!is_subtype_of(
            &Ty::EvolvingList(Box::new(Ty::Primitive(PrimitiveType::String))),
            &Ty::List(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
    }

    #[test]
    fn test_evolving_list_never_is_bottom() {
        let aliases = HashMap::new();
        // EvolvingList(Never) <: List(int) — empty evolving is assignable anywhere
        assert!(is_subtype_of(
            &Ty::EvolvingList(Box::new(Ty::Never)),
            &Ty::List(Box::new(Ty::Primitive(PrimitiveType::Int))),
            &aliases
        ));
    }

    #[test]
    fn test_evolving_map_subtype_of_map() {
        let aliases = HashMap::new();
        // EvolvingMap(string, int) <: Map(string, int)
        assert!(is_subtype_of(
            &Ty::EvolvingMap(
                Box::new(Ty::Primitive(PrimitiveType::String)),
                Box::new(Ty::Primitive(PrimitiveType::Int)),
            ),
            &Ty::Map(
                Box::new(Ty::Primitive(PrimitiveType::String)),
                Box::new(Ty::Primitive(PrimitiveType::Int)),
            ),
            &aliases
        ));
    }

    #[test]
    fn test_make_evolving() {
        // List(Never) → EvolvingList(Never)
        assert_eq!(
            Ty::List(Box::new(Ty::Never)).make_evolving(),
            Ty::EvolvingList(Box::new(Ty::Never))
        );
        // Map(Never, Never) → EvolvingMap(Never, Never)
        assert_eq!(
            Ty::Map(Box::new(Ty::Never), Box::new(Ty::Never)).make_evolving(),
            Ty::EvolvingMap(Box::new(Ty::Never), Box::new(Ty::Never))
        );
        // Non-empty List passes through
        assert_eq!(
            Ty::List(Box::new(Ty::Primitive(PrimitiveType::Int))).make_evolving(),
            Ty::List(Box::new(Ty::Primitive(PrimitiveType::Int)))
        );
        // Non-container passes through
        assert_eq!(
            Ty::Primitive(PrimitiveType::Int).make_evolving(),
            Ty::Primitive(PrimitiveType::Int)
        );
    }

    #[test]
    fn test_evolving_display() {
        assert_eq!(Ty::EvolvingList(Box::new(Ty::Never)).to_string(), "_[]");
        assert_eq!(
            Ty::EvolvingList(Box::new(Ty::Primitive(PrimitiveType::Int))).to_string(),
            "int[] (evolving)"
        );
        assert_eq!(
            Ty::EvolvingMap(Box::new(Ty::Never), Box::new(Ty::Never)).to_string(),
            "map<_, _>"
        );
        assert_eq!(
            Ty::EvolvingMap(
                Box::new(Ty::Primitive(PrimitiveType::String)),
                Box::new(Ty::Primitive(PrimitiveType::Int))
            )
            .to_string(),
            "map<string, int> (evolving)"
        );
    }
}
