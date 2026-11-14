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

use std::collections::{HashMap, HashSet};

use baml_base::Name;

use crate::{DeclName, Ty};

// ═══════════════════════════════════════════════════════════════════════════
// RECURSIVE ALIAS DETECTION
// ═══════════════════════════════════════════════════════════════════════════

// ═══════════════════════════════════════════════════════════════════════════
// INVALID CYCLE DETECTION (Tarjan's SCC + structural edges)
// ═══════════════════════════════════════════════════════════════════════════
//
// Mirrors the approach in `baml_compiler_tir/src/cycles.rs`:
// 2. Find SCCs via Tarjan's algorithm (deterministic ordering).

///
///
/// Returns a set of qualified type names that should receive cycle diagnostics.
pub fn find_invalid_alias_cycles(aliases: &HashMap<DeclName, Ty>) -> HashSet<DeclName> {
    let graph = build_alias_graph(aliases);

    // 2. Find SCCs via Tarjan's (deterministic, only real cycles)
    let sccs = Tarjan::components(&graph);

    let productive = productive_aliases(aliases);

    let mut invalid = HashSet::new();
    for scc in &sccs {
        if !scc.iter().all(|name| productive.contains(name)) {
            for name in scc {
                invalid.insert(name.clone());
            }
        }
    }

    invalid
}

fn build_alias_graph(aliases: &HashMap<DeclName, Ty>) -> HashMap<DeclName, HashSet<DeclName>> {
    aliases
        .iter()
        .map(|(alias_name, ty)| {
            let (mut deps, guarded) = extract_type_alias_deps(ty, aliases);
            deps.extend(guarded);
            (alias_name.clone(), deps)
        })
        .collect()
}

fn productive_aliases(aliases: &HashMap<DeclName, Ty>) -> HashSet<DeclName> {
    fn productive(ty: &Ty, aliases: &HashMap<DeclName, Ty>, known: &HashSet<DeclName>) -> bool {
        match ty {
            Ty::TypeAlias(qn) => !aliases.contains_key(qn) || known.contains(qn),
            Ty::Union(members) => members.iter().any(|m| productive(m, aliases, known)),
            Ty::Tuple(elements) => elements.iter().all(|e| productive(e, aliases, known)),
            Ty::Never => false,
            _ => true,
        }
    }

    let mut known = HashSet::new();
    loop {
        let found: Vec<DeclName> = aliases
            .iter()
            .filter(|(name, ty)| !known.contains(*name) && productive(ty, aliases, &known))
            .map(|(name, _)| name.clone())
            .collect();
        if found.is_empty() {
            return known;
        }
        known.extend(found);
    }
}

/// Extract type alias dependencies from a resolved type.
///
fn extract_type_alias_deps(
    ty: &Ty,
    aliases: &HashMap<DeclName, Ty>,
) -> (HashSet<DeclName>, HashSet<DeclName>) {
    fn visit(
        ty: &Ty,
        aliases: &HashMap<DeclName, Ty>,
        unguarded: &mut HashSet<DeclName>,
        guarded: &mut HashSet<DeclName>,
        in_guard: bool,
    ) {
        match ty {
            Ty::TypeAlias(qn) if aliases.contains_key(qn) => {
                if in_guard {
                    guarded.insert(qn.clone());
                } else {
                    unguarded.insert(qn.clone());
                }
            }
            Ty::List(inner) => {
                visit(inner, aliases, unguarded, guarded, true);
            }
            Ty::Map { key, value, .. } => {
                visit(key, aliases, unguarded, guarded, true);
                visit(value, aliases, unguarded, guarded, true);
            }
            Ty::Union(members) => {
                for m in members {
                    visit(m, aliases, unguarded, guarded, in_guard);
                }
            }
            Ty::Tuple(elements) => {
                for e in elements {
                    visit(e, aliases, unguarded, guarded, true);
                }
            }
            Ty::Class(_, type_args) => {
                // Nominal type_args are pass-through for cycle classification.
                // so their generic arguments inherit the surrounding context.
                for t in type_args {
                    visit(t, aliases, unguarded, guarded, in_guard);
                }
            }
            Ty::Interface(_, type_args, associated_bindings) => {
                for t in type_args {
                    visit(t, aliases, unguarded, guarded, in_guard);
                }
                for (_, ty) in associated_bindings {
                    visit(ty, aliases, unguarded, guarded, in_guard);
                }
            }
            Ty::AssociatedTypeProjection {
                base, interface, ..
            } => {
                visit(base, aliases, unguarded, guarded, in_guard);
                for ty in interface.tys() {
                    visit(ty, aliases, unguarded, guarded, in_guard);
                }
            }
            Ty::Function {
                params,
                ret,
                throws,
                ..
            } => {
                for param in params {
                    visit(&param.ty, aliases, unguarded, guarded, in_guard);
                }
                visit(ret, aliases, unguarded, guarded, in_guard);
                visit(throws, aliases, unguarded, guarded, in_guard);
            }
            _ => {}
        }
    }

    let mut unguarded = HashSet::new();
    let mut guarded = HashSet::new();
    visit(ty, aliases, &mut unguarded, &mut guarded, false);
    (unguarded, guarded)
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
    graph: &'g HashMap<DeclName, HashSet<DeclName>>,
    index: usize,
    stack: Vec<DeclName>,
    state: HashMap<DeclName, NodeState>,
    components: Vec<Vec<DeclName>>,
}

impl<'g> Tarjan<'g> {
    const UNVISITED: usize = usize::MAX;

    fn components(graph: &'g HashMap<DeclName, HashSet<DeclName>>) -> Vec<Vec<DeclName>> {
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
        nodes.sort();

        for node in &nodes {
            if tarjan.state[node].index == Self::UNVISITED {
                tarjan.strong_connect(node);
            }
        }

        // Sort components by first element for deterministic output.
        tarjan.components.sort_by(|a, b| a[0].cmp(&b[0]));

        tarjan.components
    }

    fn strong_connect(&mut self, node_id: &DeclName) {
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
        successors.sort();

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
                    .min_by(|(_, a), (_, b)| a.cmp(b))
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

/// A class cycle: the classes participating, in cycle order.
pub struct ClassCycleInfo {
    /// All class names in this cycle.
    pub members: Vec<DeclName>,
}

impl ClassCycleInfo {
    /// The cycle as a path, `A -> B -> A`, with each head spelled by `render`
    /// (a compile-time head has no spelling of its own; the caller supplies
    /// the viewpoint).
    pub fn cycle_path(&self, render: impl Fn(&DeclName) -> String) -> String {
        match self.members.as_slice() {
            [only] => render(only),
            members => members
                .iter()
                .chain(members.first())
                .map(render)
                .collect::<Vec<_>>()
                .join(" -> "),
        }
    }
}

/// Find all classes that participate in unconstructable required-field cycles.
///
/// A "required" field is one that is NOT optional, NOT a list, and NOT a map.
/// Optional/list/map fields can be null/empty, breaking the construction chain.
///
/// Returns a list of `ClassCycleInfo`, one per SCC found.
pub fn find_invalid_class_cycles(
    class_fields: &HashMap<DeclName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<DeclName, Ty>,
) -> Vec<ClassCycleInfo> {
    let graph = build_class_graph(class_fields, type_aliases);
    let sccs = Tarjan::components(&graph);

    sccs.into_iter()
        .map(|scc| ClassCycleInfo { members: scc })
        .collect()
}

/// Build a dependency graph of classes based on required field types.
fn build_class_graph(
    class_fields: &HashMap<DeclName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<DeclName, Ty>,
) -> HashMap<DeclName, HashSet<DeclName>> {
    let mut graph: HashMap<DeclName, HashSet<DeclName>> = HashMap::new();

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
    class_fields: &HashMap<DeclName, Vec<(Name, Ty)>>,
    type_aliases: &HashMap<DeclName, Ty>,
    deps: &mut HashSet<DeclName>,
    optional: bool,
    in_list_or_map: bool,
    visiting: &mut HashSet<DeclName>,
) {
    match ty {
        // Only add if the field is truly required.
        Ty::Class(qn, _) if !optional && !in_list_or_map && class_fields.contains_key(qn) => {
            deps.insert(qn.clone());
        }
        Ty::Class(_, _) => {}
        // Resolve through type aliases (only if still required context).
        Ty::TypeAlias(qn) if !optional && !in_list_or_map && !visiting.contains(qn) => {
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
        Ty::TypeAlias(_) => {}
        // `T?` lowers to `Union([T, Null])` (canary removed the `Ty::Optional`
        // variant), so optionals are handled by the `Ty::Union` arm below —
        // which already yields no hard dependency (Null breaks it).
        Ty::List(inner) => {
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
        Ty::Tuple(elements) => {
            for element in elements {
                extract_required_class_deps(
                    element,
                    class_fields,
                    type_aliases,
                    deps,
                    optional,
                    in_list_or_map,
                    visiting,
                );
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn qn(name: &str) -> DeclName {
        crate::test_roots::new(Name::new("test"), vec![], Name::new(name))
    }

    fn type_alias(name: &str) -> Ty {
        Ty::TypeAlias(qn(name))
    }

    #[test]
    fn direct_self_reference_is_recursive() {
        // `type List = null | List`
        let mut aliases = HashMap::new();
        aliases.insert(
            qn("List"),
            Ty::Union(Box::new([Ty::Null, type_alias("List")])),
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
        aliases.insert(qn("MyInt"), Ty::Int);

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
        aliases.insert(qn("A"), Ty::Class(qn("Box"), Box::new([type_alias("A")])));

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
            Ty::Interface(qn("BoxLike"), Box::new([type_alias("A")]), Box::new([])),
        );

        assert!(
            crate::ResolvedAliases::from_aliases(aliases.clone())
                .recursive
                .contains(&qn("A")),
            "expected `type A = BoxLike<A>` to be detected as recursive"
        );
    }

    fn tuple(elements: Vec<Ty>) -> Ty {
        Ty::Tuple(elements.into_boxed_slice())
    }

    fn optional(ty: Ty) -> Ty {
        Ty::Union(Box::new([ty, Ty::Null]))
    }

    fn invalid(decls: &[(&str, Ty)]) -> Vec<String> {
        let aliases: HashMap<DeclName, Ty> = decls
            .iter()
            .map(|(name, ty)| (qn(name), ty.clone()))
            .collect();
        let mut names: Vec<String> = find_invalid_alias_cycles(&aliases)
            .iter()
            .map(|name| name.name().to_string())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn tuple_recursion_terminated_by_null_is_valid() {
        assert!(
            invalid(&[("Chain", tuple(vec![Ty::Int, optional(type_alias("Chain"))]))]).is_empty()
        );
        assert!(
            invalid(&[("Chain", optional(tuple(vec![Ty::Int, type_alias("Chain")])))]).is_empty()
        );
        assert!(
            invalid(&[(
                "T",
                tuple(vec![Ty::Int, Ty::List(Box::new(type_alias("T")))])
            )])
            .is_empty()
        );
    }

    #[test]
    fn tuple_recursion_without_a_finite_value_is_invalid() {
        assert_eq!(
            invalid(&[("T", tuple(vec![Ty::Int, type_alias("T")]))]),
            ["T"]
        );
        assert_eq!(
            invalid(&[("Bad", tuple(vec![type_alias("Bad"), optional(Ty::Int)]))]),
            ["Bad"]
        );
        assert_eq!(
            invalid(&[(
                "T",
                tuple(vec![type_alias("T"), Ty::List(Box::new(type_alias("T")))])
            )]),
            ["T"]
        );
    }

    #[test]
    fn mutual_tuple_recursion_through_null_is_valid() {
        assert!(
            invalid(&[
                ("A", tuple(vec![Ty::Int, type_alias("B")])),
                ("B", optional(type_alias("A"))),
            ])
            .is_empty()
        );
        assert_eq!(
            invalid(&[
                ("A", tuple(vec![Ty::Int, type_alias("B")])),
                ("B", tuple(vec![type_alias("A"), Ty::String])),
            ]),
            ["A", "B"]
        );
    }

    #[test]
    fn vacuous_self_member_beside_a_finite_one_is_valid() {
        assert!(invalid(&[("A", optional(type_alias("A")))]).is_empty());
        assert_eq!(invalid(&[("A", type_alias("A"))]), ["A"]);
    }

    #[test]
    fn class_cycle_follows_tuple_elements() {
        let required: HashMap<DeclName, Vec<(Name, Ty)>> = HashMap::from([(
            qn("A"),
            vec![(
                Name::new("x"),
                tuple(vec![Ty::Int, Ty::Class(qn("A"), Box::new([]))]),
            )],
        )]);
        assert_eq!(
            find_invalid_class_cycles(&required, &HashMap::new()).len(),
            1
        );
        let optional_field: HashMap<DeclName, Vec<(Name, Ty)>> = HashMap::from([(
            qn("A"),
            vec![(
                Name::new("x"),
                tuple(vec![Ty::Int, optional(Ty::Class(qn("A"), Box::new([])))]),
            )],
        )]);
        assert!(find_invalid_class_cycles(&optional_field, &HashMap::new()).is_empty());
    }
}
