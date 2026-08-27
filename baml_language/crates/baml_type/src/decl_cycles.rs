//! Type-alias recursion detection and declaration cycle diagnostics.
//!
//! Subtyping and equivalence live in the canonical algebra
//! ([`crate::normalize`], driven by a a [`crate::normalize::TypeContext`] context); this
//! module holds the two things that are *not* part of that relation:
//!
//! - recursive-alias detection (now `ResolvedAliases::from_aliases` in
//!   `baml_type`): which type aliases are self-referential, so
//!   runtime lowering ([`crate::ResolvedAliases`]) knows which to keep opaque
//!   rather than expand.
//! - [`find_invalid_alias_cycles`] / [`find_invalid_class_cycles`]: the
//!   ill-founded-recursion diagnostics (a type alias or class whose definition
//!   depends on itself with no indirection through a container).

use std::collections::{HashMap, HashSet};

use baml_base::Name;

use crate::{QualifiedTypeName, Ty};

// ═══════════════════════════════════════════════════════════════════════════
// RECURSIVE ALIAS DETECTION
// ═══════════════════════════════════════════════════════════════════════════

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
/// point via empty container). `Union` (including a nullable `T | null`) is
/// pass-through — it does NOT create structural context.
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
            Ty::TypeAlias(qn, _) if aliases.contains_key(qn) => {
                if in_structural {
                    structural.insert(qn.clone());
                } else {
                    non_structural.insert(qn.clone());
                }
            }
            Ty::List(inner, _) => {
                // List provides structural guard (can be empty)
                visit(inner, aliases, non_structural, structural, true);
            }
            Ty::Map { key, value, .. } => {
                // Map provides structural guard (can be empty)
                visit(key, aliases, non_structural, structural, true);
                visit(value, aliases, non_structural, structural, true);
            }
            Ty::Union(members, _) => {
                // Union passes through the structural context
                for m in members {
                    visit(m, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::Class(_, type_args, _) => {
                // Nominal type_args are pass-through for cycle classification.
                // User-defined nominal types are not structural guards like List/Map,
                // so their generic arguments inherit the surrounding context.
                for t in type_args {
                    visit(t, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::Interface(_, type_args, associated_bindings, _) => {
                for t in type_args {
                    visit(t, aliases, non_structural, structural, in_structural);
                }
                for (_, ty) in associated_bindings {
                    visit(ty, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => {
                visit(base, aliases, non_structural, structural, in_structural);
                for ty in interface.tys() {
                    visit(ty, aliases, non_structural, structural, in_structural);
                }
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for param in params {
                    visit(
                        &param.ty,
                        aliases,
                        non_structural,
                        structural,
                        in_structural,
                    );
                }
                visit(ret, aliases, non_structural, structural, in_structural);
                visit(throws, aliases, non_structural, structural, in_structural);
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
        nodes.sort_by_key(std::string::ToString::to_string);

        for node in &nodes {
            if tarjan.state[node].index == Self::UNVISITED {
                tarjan.strong_connect(node);
            }
        }

        // Sort components by first element for deterministic output.
        tarjan
            .components
            .sort_by(|a, b| a[0].to_string().cmp(&b[0].to_string()));

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
        successors.sort_by_key(std::string::ToString::to_string);

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
                    .min_by(|(_, a), (_, b)| a.to_string().cmp(&b.to_string()))
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
        // Only add if the field is truly required.
        Ty::Class(qn, _, _) if !optional && !in_list_or_map && class_fields.contains_key(qn) => {
            deps.insert(qn.clone());
        }
        Ty::Class(_, _, _) => {}
        // Resolve through type aliases (only if still required context).
        Ty::TypeAlias(qn, _) if !optional && !in_list_or_map && !visiting.contains(qn) => {
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
        Ty::TypeAlias(_, _) => {}
        // `T?` lowers to `Union([T, Null])` (canary removed the `Ty::Optional`
        // variant), so optionals are handled by the `Ty::Union` arm below —
        // which already yields no hard dependency (Null breaks it).
        Ty::List(inner, _) => {
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
        Ty::Map { key, value, .. } => {
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
        Ty::Union(members, _) => {
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
        cycle[0].to_string()
    } else {
        let mut path: Vec<String> = cycle.iter().map(std::string::ToString::to_string).collect();
        path.push(cycle[0].to_string());
        path.join(" -> ")
    }
}

#[cfg(test)]
mod tests {
    use baml_base::TyAttr;

    use super::*;

    fn qn(name: &str) -> QualifiedTypeName {
        QualifiedTypeName::new(Name::new("test"), vec![], Name::new(name))
    }

    fn type_alias(name: &str) -> Ty {
        Ty::TypeAlias(qn(name), TyAttr::default())
    }

    #[test]
    fn direct_self_reference_is_recursive() {
        // `type List = null | List`
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("List"),
            Ty::Union(
                vec![
                    Ty::Null {
                        attr: TyAttr::default(),
                    },
                    type_alias("List"),
                ],
                TyAttr::default(),
            ),
        );

        assert!(
            crate::ResolvedAliases::from_aliases(aliases.clone())
                .recursive
                .contains(&qn("List"))
        );
    }

    #[test]
    fn non_recursive_alias_is_not_marked() {
        // `type MyInt = int`
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("MyInt"),
            Ty::Int {
                attr: TyAttr::default(),
            },
        );

        assert!(
            !crate::ResolvedAliases::from_aliases(aliases.clone())
                .recursive
                .contains(&qn("MyInt"))
        );
    }

    #[test]
    fn recursion_through_class_type_arg_is_detected() {
        // `type A = Box<A>` — recursion goes through a class generic argument.
        // Cycle detection must descend into class type_args or it would miss this
        // and expansion would recurse forever.
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("A"),
            Ty::Class(qn("Box"), vec![type_alias("A")], TyAttr::default()),
        );

        assert!(
            crate::ResolvedAliases::from_aliases(aliases.clone())
                .recursive
                .contains(&qn("A")),
            "expected `type A = Box<A>` to be detected as recursive"
        );
    }

    #[test]
    fn recursion_through_interface_type_arg_is_detected() {
        // `type A = BoxLike<A>` — recursion through an interface generic argument,
        // detected just like class generic arguments.
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("A"),
            Ty::Interface(
                qn("BoxLike"),
                vec![type_alias("A")],
                vec![],
                TyAttr::default(),
            ),
        );

        assert!(
            crate::ResolvedAliases::from_aliases(aliases.clone())
                .recursive
                .contains(&qn("A")),
            "expected `type A = BoxLike<A>` to be detected as recursive"
        );
    }
}
