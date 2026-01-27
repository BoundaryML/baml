//! Cycle detection for type aliases and classes at the TIR level.
//!
//! This module validates that type aliases and classes don't form infinite dependency
//! cycles. It distinguishes between structural recursion (through maps/lists, which is
//! allowed) and non-structural recursion (which is an error).
//!
//! This validation happens at the TIR level (after type resolution) rather than HIR
//! because:
//! 1. It requires resolved types to properly detect cycles (e.g., `RecAlias` → Recursive)
//! 2. Uses position-independent identifiers (`ErrorLocation::TypeItem`) for incrementality
//! 3. It's validation about type structure, not syntax structure

use std::collections::{HashMap, HashSet};

use baml_base::Name;
use baml_compiler_diagnostics::TypeError;
use baml_compiler_hir::{ErrorLocation, TirContext};

use crate::Ty;

/// Type alias for TIR type errors (position-independent).
type TirTypeError = TypeError<TirContext<Ty>>;

/// Tarjan's algorithm for finding strongly connected components (cycles).
fn find_cycles<T>(graph: &HashMap<T, HashSet<T>>) -> Vec<Vec<T>>
where
    T: Clone + Eq + std::hash::Hash + Ord,
{
    struct TarjanState<T> {
        indices: HashMap<T, usize>,
        lowlinks: HashMap<T, usize>,
        on_stack: HashSet<T>,
        stack: Vec<T>,
        index: usize,
    }

    impl<T: Clone + Eq + std::hash::Hash> TarjanState<T> {
        fn new() -> Self {
            Self {
                indices: HashMap::new(),
                lowlinks: HashMap::new(),
                on_stack: HashSet::new(),
                stack: Vec::new(),
                index: 0,
            }
        }
    }

    fn tarjan_visit<T>(
        node: &T,
        graph: &HashMap<T, HashSet<T>>,
        state: &mut TarjanState<T>,
        components: &mut Vec<Vec<T>>,
    ) where
        T: Clone + Eq + std::hash::Hash,
    {
        state.indices.insert(node.clone(), state.index);
        state.lowlinks.insert(node.clone(), state.index);
        state.index += 1;
        state.stack.push(node.clone());
        state.on_stack.insert(node.clone());

        if let Some(successors) = graph.get(node) {
            for successor in successors {
                if !state.indices.contains_key(successor) {
                    tarjan_visit(successor, graph, state, components);
                    let successor_lowlink = state.lowlinks[successor];
                    let node_lowlink = state.lowlinks.get_mut(node).unwrap();
                    *node_lowlink = (*node_lowlink).min(successor_lowlink);
                } else if state.on_stack.contains(successor) {
                    let successor_index = state.indices[successor];
                    let node_lowlink = state.lowlinks.get_mut(node).unwrap();
                    *node_lowlink = (*node_lowlink).min(successor_index);
                }
            }
        }

        let node_lowlink = state.lowlinks[node];
        let node_index = state.indices[node];
        if node_lowlink == node_index {
            let mut component = Vec::new();
            loop {
                let w = state.stack.pop().unwrap();
                state.on_stack.remove(&w);
                component.push(w.clone());
                if w == *node {
                    break;
                }
            }
            components.push(component);
        }
    }

    let mut state = TarjanState::new();
    let mut components = Vec::new();

    for node in graph.keys() {
        if !state.indices.contains_key(node) {
            tarjan_visit(node, graph, &mut state, &mut components);
        }
    }

    // Filter to only cycles (SCCs with more than one node, or self-loops)
    components
        .into_iter()
        .filter(|component| {
            match component.len().cmp(&1) {
                std::cmp::Ordering::Greater => true,
                std::cmp::Ordering::Equal => {
                    // Check for self-loop
                    let node = &component[0];
                    graph
                        .get(node)
                        .map(|deps| deps.contains(node))
                        .unwrap_or(false)
                }
                std::cmp::Ordering::Less => false,
            }
        })
        .map(|mut cycle| {
            // Sort to get deterministic output starting with min element
            cycle.sort();
            cycle
        })
        .collect()
}

/// Extract all type alias dependencies from a resolved type.
/// Returns (non-structural deps, structural deps) where structural means through maps/lists.
fn extract_type_alias_deps(ty: &Ty) -> (HashSet<Name>, HashSet<Name>) {
    fn visit(
        ty: &Ty,
        non_structural_deps: &mut HashSet<Name>,
        structural_deps: &mut HashSet<Name>,
        in_structural_context: bool,
    ) {
        match ty {
            Ty::TypeAlias(fqn) => {
                // Type aliases in Ty are kept unexpanded - this is what we want to track
                let name = fqn.name.clone();
                if in_structural_context {
                    structural_deps.insert(name);
                } else {
                    non_structural_deps.insert(name);
                }
            }
            Ty::Optional(inner) => {
                // Optional doesn't make it structural for type aliases
                visit(
                    inner,
                    non_structural_deps,
                    structural_deps,
                    in_structural_context,
                );
            }
            Ty::List(inner) => {
                // List makes it structural
                visit(inner, non_structural_deps, structural_deps, true);
            }
            Ty::Map { key, value } => {
                // Map makes it structural
                visit(key, non_structural_deps, structural_deps, true);
                visit(value, non_structural_deps, structural_deps, true);
            }
            Ty::Union(variants) => {
                for variant in variants {
                    visit(
                        variant,
                        non_structural_deps,
                        structural_deps,
                        in_structural_context,
                    );
                }
            }
            // Classes, enums, primitives, and literals don't create alias dependencies
            _ => {}
        }
    }

    let mut non_structural_deps = HashSet::new();
    let mut structural_deps = HashSet::new();

    visit(ty, &mut non_structural_deps, &mut structural_deps, false);
    (non_structural_deps, structural_deps)
}

struct GraphResult {
    graph: HashMap<Name, HashSet<Name>>,
    structural_edges: HashSet<(Name, Name)>,
}

/// Build a graph of type alias dependencies, tracking which edges are structural.
///
/// This is position-independent - works only with the resolved types, no file access.
fn build_type_alias_graph(type_aliases: &HashMap<Name, Ty>) -> GraphResult {
    let mut graph: HashMap<Name, HashSet<Name>> = HashMap::new();
    let mut structural_edges: HashSet<(Name, Name)> = HashSet::new();

    for (alias_name, ty) in type_aliases {
        let (mut non_structural_deps, structural_deps) = extract_type_alias_deps(ty);

        // Mark structural edges (we need to iterate before consuming structural_deps)
        for dep in &structural_deps {
            structural_edges.insert((alias_name.clone(), dep.clone()));
        }

        // Combine all dependencies for the graph (move structural_deps to avoid clone)
        non_structural_deps.extend(structural_deps);

        // Add to graph (move the combined deps to avoid clone)
        graph.insert(alias_name.clone(), non_structural_deps);
    }

    GraphResult {
        graph,
        structural_edges,
    }
}

/// Extract class dependencies from a resolved type for class cycle detection.
/// Only considers required (non-optional) dependencies.
/// Type aliases in Ty are kept unexpanded, so we need to resolve them.
fn extract_class_deps(ty: &Ty, type_aliases: &HashMap<Name, Ty>) -> HashSet<Name> {
    fn visit(
        ty: &Ty,
        deps: &mut HashSet<Name>,
        optional: bool,
        in_list_or_map: bool,
        type_aliases: &HashMap<Name, Ty>,
        visiting: &mut HashSet<Name>,
    ) {
        match ty {
            Ty::Class(fqn) => {
                // Only add if not optional and not in list/map
                if !optional && !in_list_or_map {
                    deps.insert(fqn.name.clone());
                }
            }
            Ty::TypeAlias(fqn) => {
                // Type aliases are kept unexpanded in Ty - resolve them
                if !optional && !in_list_or_map {
                    let name = &fqn.name;
                    // Prevent infinite recursion on cyclic type aliases
                    if !visiting.contains(name) {
                        if let Some(alias_ty) = type_aliases.get(name) {
                            visiting.insert(name.clone());
                            visit(
                                alias_ty,
                                deps,
                                optional,
                                in_list_or_map,
                                type_aliases,
                                visiting,
                            );
                            visiting.remove(name);
                        }
                    }
                }
            }
            Ty::Optional(inner) => {
                // Optional breaks cycles
                visit(inner, deps, true, in_list_or_map, type_aliases, visiting);
            }
            Ty::List(inner) => {
                // Lists break cycles - mark as in structural context
                visit(inner, deps, optional, true, type_aliases, visiting);
            }
            Ty::Map { key, value } => {
                // Maps break cycles - mark as in structural context
                visit(key, deps, optional, true, type_aliases, visiting);
                visit(value, deps, optional, true, type_aliases, visiting);
            }
            Ty::Union(variants) => {
                // For unions, we need to check if ALL variants lead to the same class
                let mut union_deps = Vec::new();
                for variant in variants {
                    let mut variant_deps = HashSet::new();
                    visit(
                        variant,
                        &mut variant_deps,
                        optional,
                        in_list_or_map,
                        type_aliases,
                        visiting,
                    );
                    union_deps.push(variant_deps);
                }

                // Only add deps if all variants lead to same single class
                if !union_deps.is_empty() {
                    let first = &union_deps[0];
                    if first.len() == 1 && union_deps.iter().all(|d| d == first) {
                        deps.extend(first.iter().cloned());
                    }
                }
            }
            // Primitives, literals, enums, etc. don't create class dependencies
            _ => {}
        }
    }

    let mut deps = HashSet::new();
    let mut visiting = HashSet::new();

    visit(ty, &mut deps, false, false, type_aliases, &mut visiting);
    deps
}

/// Build a graph of class dependencies (only required fields).
///
/// This is position-independent - works only with the resolved types.
fn build_class_graph(
    class_field_types: &HashMap<Name, HashMap<Name, Ty>>,
    type_aliases: &HashMap<Name, Ty>,
) -> HashMap<Name, HashSet<Name>> {
    let mut graph: HashMap<Name, HashSet<Name>> = HashMap::new();

    for (class_name, fields) in class_field_types {
        let mut deps = HashSet::new();

        for field_ty in fields.values() {
            // Extract class dependencies from the resolved type
            let field_deps = extract_class_deps(field_ty, type_aliases);
            deps.extend(field_deps);
        }

        graph.insert(class_name.clone(), deps);
    }

    graph
}

/// Format a cycle path for error messages.
///
/// For cycles with more than one element, shows the complete cycle by adding
/// the first element at the end: "A -> B -> C -> A"
fn format_cycle_path(cycle: &[Name]) -> String {
    if cycle.len() == 1 {
        // Self-referential cycle: "A"
        cycle[0].to_string()
    } else {
        // Multi-element cycle: show complete path back to start
        let mut path: Vec<String> = cycle.iter().map(std::string::ToString::to_string).collect();
        path.push(cycle[0].to_string()); // Add first element at end to close the cycle
        path.join(" -> ")
    }
}

/// Validate type alias cycles.
///
/// Returns type errors with position-independent locations (`ErrorLocation::TypeItem`).
pub fn validate_type_alias_cycles(type_aliases: &HashMap<Name, Ty>) -> Vec<TirTypeError> {
    let GraphResult {
        graph,
        structural_edges,
    } = build_type_alias_graph(type_aliases);

    // Find all cycles
    let mut cycles = find_cycles(&graph);

    // Sort cycles for deterministic output
    cycles.sort();

    let mut errors = Vec::new();

    for cycle in cycles {
        // Check if this cycle has at least one structural edge (goes through map/list)
        // If so, the cycle is allowed because the structural type provides a base case
        let mut has_structural_edge = false;
        for i in 0..cycle.len() {
            let from = &cycle[i];
            let to = &cycle[(i + 1) % cycle.len()];
            if structural_edges.contains(&(from.clone(), to.clone())) {
                has_structural_edge = true;
                break;
            }
        }

        // Only report cycles without any structural edges as errors
        if !has_structural_edge {
            let cycle_path = format_cycle_path(&cycle);
            let first_in_cycle = cycle[0].clone();

            errors.push(TypeError::AliasCycle {
                cycle_path,
                location: ErrorLocation::TypeItem(first_in_cycle),
            });
        }
    }

    errors
}

/// Validate class cycles.
///
/// Returns type errors with position-independent locations (`ErrorLocation::TypeItem`).
pub fn validate_class_cycles(
    class_field_types: &HashMap<Name, HashMap<Name, Ty>>,
    type_aliases: &HashMap<Name, Ty>,
) -> Vec<TirTypeError> {
    let graph = build_class_graph(class_field_types, type_aliases);

    // Find all cycles
    let mut cycles = find_cycles(&graph);

    // Sort cycles for deterministic output
    cycles.sort();

    let mut errors = Vec::new();

    for cycle in cycles {
        let cycle_path = format_cycle_path(&cycle);
        let first_in_cycle = cycle[0].clone();

        errors.push(TypeError::ClassCycle {
            cycle_path,
            location: ErrorLocation::TypeItem(first_in_cycle),
        });
    }

    errors
}
