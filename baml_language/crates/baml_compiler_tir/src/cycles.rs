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
//!
//! ## Implementation
//!
//! Uses Tarjan's strongly connected components algorithm, adapted from engine/tarjan.rs.
//! The algorithm finds all cycles in a directed graph efficiently in O(V + E) time.

use std::{
    cmp,
    collections::{HashMap, HashSet},
};

use baml_base::Name;
use baml_compiler_diagnostics::TypeError;
use baml_compiler_hir::{ErrorLocation, TirContext};

use crate::Ty;

/// Type alias for TIR type errors (position-independent).
type TirTypeError = TypeError<TirContext<Ty>>;

/// Dependency graph represented as an adjacency list.
type Graph<V> = HashMap<V, HashSet<V>>;

// ============================================================================
// TARJAN'S ALGORITHM
// ============================================================================

/// State of each node for Tarjan's algorithm.
#[derive(Clone, Copy)]
struct NodeState {
    /// Node unique index.
    index: usize,
    /// Low link value.
    ///
    /// Represents the smallest index of any node on the stack known to be
    /// reachable from `self` through `self`'s DFS subtree.
    low_link: usize,
    /// Whether the node is on the stack.
    on_stack: bool,
}

/// Tarjan's strongly connected components algorithm implementation.
///
/// This algorithm finds and returns all the cycles in a graph. Read more about
/// it [here](https://en.wikipedia.org/wiki/Tarjan%27s_strongly_connected_components_algorithm).
///
/// Adapted from engine/baml-lib/parser-database/src/tarjan.rs with modifications
/// for use with `Name` instead of opaque IDs.
struct Tarjan<'g> {
    /// Ref to the dependency graph.
    graph: &'g Graph<Name>,
    /// Node number counter.
    index: usize,
    /// Nodes are placed on a stack in the order in which they are visited.
    stack: Vec<Name>,
    /// State of each node.
    state: HashMap<Name, NodeState>,
    /// Strongly connected components (cycles).
    components: Vec<Vec<Name>>,
}

impl<'g> Tarjan<'g> {
    /// Unvisited node marker.
    ///
    /// Technically we should use [`Option<usize>`] and [`None`] for
    /// [`NodeState::index`] and [`NodeState::low_link`] but that would require
    /// some ugly and repetitive [`Option::unwrap`] calls. [`usize::MAX`] won't
    /// be reached as an index anyway, the algorithm will stack overflow much
    /// sooner than that :/
    const UNVISITED: usize = usize::MAX;

    /// Public entry point for the algorithm.
    ///
    /// Loops through all the nodes in the graph and visits them if they haven't
    /// been visited already. When the algorithm is done, [`Self::components`]
    /// will contain all the cycles in the graph.
    fn components(graph: &'g Graph<Name>) -> Vec<Vec<Name>> {
        let mut tarjan = Self {
            graph,
            index: 0,
            stack: Vec::new(),
            state: graph
                .keys()
                .map(|node| {
                    let state = NodeState {
                        index: Self::UNVISITED,
                        low_link: Self::UNVISITED,
                        on_stack: false,
                    };

                    (node.clone(), state)
                })
                .collect(),
            components: Vec::new(),
        };

        // Always start at the same node to avoid randomness in the cycle path.
        // Sort nodes to ensure deterministic traversal order.
        let mut nodes: Vec<_> = graph.keys().cloned().collect();
        nodes.sort();

        for node in nodes {
            if tarjan.state[&node].index == Self::UNVISITED {
                tarjan.strong_connect(&node);
            }
        }

        // Sort components by the first element in each cycle (which is already
        // sorted as well). This should get rid of all the randomness caused by
        // hash maps and hash sets.
        tarjan.components.sort_by(|a, b| a[0].cmp(&b[0]));

        tarjan.components
    }

    /// Recursive DFS.
    ///
    /// This is where the "algorithm" runs. Could be implemented iteratively if
    /// needed at some point.
    fn strong_connect(&mut self, node_id: &Name) {
        // Initialize node state. This node has not yet been visited so we don't
        // have to grab the state from the hash map. And if we did, then we'd
        // have to fight the borrow checker by taking mut refs and read-only
        // refs over and over again as needed (which requires hashing the same
        // entry many times and is not as readable).
        let mut node = NodeState {
            index: self.index,
            low_link: self.index,
            on_stack: true,
        };

        // Increment index and push node to stack.
        self.index += 1;

        // Update state. We store this in a hash map
        // so we have to run the hashing algorithm every time we update the
        // state. Keep it to a minimum :)
        self.state.insert(node_id.clone(), node);
        self.stack.push(node_id.clone());

        // Sort successors to ensure deterministic traversal order.
        // HashSet iteration is non-deterministic, so we must sort to avoid
        // reporting different cycle paths on each run.
        let mut successors: Vec<_> = self.graph[node_id].iter().collect();
        successors.sort();

        // Visit neighbors to find strongly connected components.
        for successor_id in successors {
            // Grab owned state to circumvent borrow checker.
            let mut successor = self.state[successor_id];
            if successor.index == Self::UNVISITED {
                self.strong_connect(successor_id);
                // Grab updated state after recursive call.
                successor = self.state[successor_id];
                node.low_link = cmp::min(node.low_link, successor.low_link);
            } else if successor.on_stack {
                node.low_link = cmp::min(node.low_link, successor.index);
            }
        }

        // Re-insert the node's state as the state might have changed.
        self.state.insert(node_id.clone(), node);

        // Root node of a strongly connected component.
        if node.low_link == node.index {
            let mut component = Vec::new();

            while let Some(parent_id) = self.stack.pop() {
                // This should not fail since all nodes should be stored in
                // the state hash map.
                if let Some(parent) = self.state.get_mut(&parent_id) {
                    parent.on_stack = false;
                }

                component.push(parent_id.clone());

                if &parent_id == node_id {
                    break;
                }
            }

            // Path should be shown as parent -> child not child -> parent.
            component.reverse();

            // Find index of minimum element in the component.
            //
            // The cycle path is not computed deterministically because the
            // graph is stored in a hash map, so random state will cause the
            // traversal algorithm to start at different nodes each time.
            //
            // Therefore, to avoid reporting errors to the user differently
            // every time, we'll use a simple deterministic way to determine
            // the start node of a cycle.
            //
            // Basically, the start node will always be the smallest type name in
            // the cycle. That gets rid of the random state.
            let min_index = component
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.cmp(b))
                .map(|(i, _)| i);

            // We have a cycle if the component contains more than one node or
            // it contains a single node that points to itself. Otherwise it's
            // just a normal node with no cycles whatsoever, so we'll skip it.
            if component.len() > 1
                || (component.len() == 1 && self.graph[node_id].contains(node_id))
            {
                if let Some(index) = min_index {
                    component.rotate_left(index);
                    self.components.push(component);
                }
            }
        }
    }
}

// ============================================================================
// TYPE ALIAS CYCLE DETECTION
// ============================================================================

/// Extract all type alias dependencies from a resolved type.
///
/// Returns `(non-structural deps, structural deps)` where structural means
/// through maps/lists (which are allowed to be recursive).
fn extract_type_alias_deps(ty: &Ty) -> (HashSet<Name>, HashSet<Name>) {
    fn visit(
        ty: &Ty,
        non_structural_deps: &mut HashSet<Name>,
        structural_deps: &mut HashSet<Name>,
        in_structural_context: bool,
    ) {
        match ty {
            Ty::TypeAlias(fqn, _) => {
                // Type aliases in Ty are kept unexpanded - this is what we want to track.
                // They stay as names and never expand, preventing infinite recursion.
                let name = fqn.name.clone();
                if in_structural_context {
                    structural_deps.insert(name);
                } else {
                    non_structural_deps.insert(name);
                }
            }
            Ty::Optional(inner, _) => {
                // Optional doesn't make it structural for type aliases
                visit(
                    inner,
                    non_structural_deps,
                    structural_deps,
                    in_structural_context,
                );
            }
            Ty::List(inner, _) => {
                // List makes it structural - provides termination via []
                visit(inner, non_structural_deps, structural_deps, true);
            }
            Ty::Map { key, value, .. } => {
                // Map makes it structural - provides termination via {}
                visit(key, non_structural_deps, structural_deps, true);
                visit(value, non_structural_deps, structural_deps, true);
            }
            Ty::Union(variants, _) => {
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

/// Result of building a type alias dependency graph.
struct GraphResult {
    /// The full dependency graph (all edges).
    graph: Graph<Name>,
    /// Edges that go through structural types (List/Map).
    /// These edges indicate cycles that are allowed.
    structural_edges: HashSet<(Name, Name)>,
}

/// Build a graph of type alias dependencies, tracking which edges are structural.
///
/// This is position-independent - works only with the resolved types, no file access.
fn build_type_alias_graph(type_aliases: &HashMap<Name, Ty>) -> GraphResult {
    let mut graph: Graph<Name> = HashMap::new();
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

/// Validate type alias cycles.
///
/// Returns type errors with position-independent locations (`ErrorLocation::TypeItem`).
///
/// # Algorithm
///
/// 1. Build dependency graph from type aliases
/// 2. Find all cycles using Tarjan's algorithm
/// 3. Filter out cycles that have at least one "structural" edge (through List/Map)
/// 4. Report remaining cycles as errors
///
/// # Example
///
/// ```text
/// type A = B       // Error: A -> B -> A (no structural edge)
/// type B = A
///
/// type C = D[]     // OK: C -> D -> C but goes through List
/// type D = C
/// ```
pub fn validate_type_alias_cycles(type_aliases: &HashMap<Name, Ty>) -> Vec<TirTypeError> {
    let GraphResult {
        graph,
        structural_edges,
    } = build_type_alias_graph(type_aliases);

    // Find all cycles using Tarjan's algorithm
    let cycles = Tarjan::components(&graph);

    let mut errors = Vec::new();

    for cycle in cycles {
        // Check if this cycle has at least one structural edge (goes through map/list).
        // If so, the cycle is allowed because the structural type provides a base case
        // for termination (empty list or empty map).
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

// ============================================================================
// CLASS CYCLE DETECTION
// ============================================================================

/// Extract class dependencies from a resolved type for class cycle detection.
///
/// Only considers **required** (non-optional) dependencies. Optional fields
/// and fields inside lists/maps don't create hard dependencies because they
/// can be absent or empty.
///
/// Type aliases in Ty are kept unexpanded, so we need to resolve them
/// recursively to find the actual class dependencies.
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
            Ty::Class(fqn, _) => {
                // Only add if not optional and not in list/map.
                // Optional and structural contexts break the hard dependency.
                if !optional && !in_list_or_map {
                    deps.insert(fqn.display_name());
                }
            }
            Ty::TypeAlias(fqn, _) => {
                // Type aliases are kept unexpanded in Ty - resolve them.
                // But only if this is a required field (not optional, not in list/map).
                if !optional && !in_list_or_map {
                    let name = &fqn.name;
                    // Prevent infinite recursion on cyclic type aliases using a visiting set
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
            Ty::Optional(inner, _) => {
                // Optional breaks cycles - field can be absent
                visit(inner, deps, true, in_list_or_map, type_aliases, visiting);
            }
            Ty::List(inner, _) => {
                // Lists break cycles - can be empty
                visit(inner, deps, optional, true, type_aliases, visiting);
            }
            Ty::Map { key, value, .. } => {
                // Maps break cycles - can be empty
                visit(key, deps, optional, true, type_aliases, visiting);
                visit(value, deps, optional, true, type_aliases, visiting);
            }
            Ty::Union(variants, _) => {
                // For unions, we need to check if ALL variants lead to the same class.
                // If they do, then that class is a hard dependency. Otherwise, the
                // union can be satisfied by choosing a variant without that class.
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
) -> Graph<Name> {
    let mut graph: Graph<Name> = HashMap::new();

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

/// Validate class cycles.
///
/// Returns type errors with position-independent locations (`ErrorLocation::TypeItem`).
///
/// # Algorithm
///
/// 1. Build dependency graph from class field types
/// 2. Find all cycles using Tarjan's algorithm
/// 3. Report all cycles as errors (classes can't be recursive at all)
///
/// # Example
///
/// ```text
/// class User {
///     post Post     // Error: User -> Post -> User
/// }
///
/// class Post {
///     author User
/// }
/// ```
///
/// Unlike type aliases, classes **cannot** be recursive even through lists/maps,
/// because classes represent data structures that must be finitely constructed.
pub fn validate_class_cycles(
    class_field_types: &HashMap<Name, HashMap<Name, Ty>>,
    type_aliases: &HashMap<Name, Ty>,
) -> Vec<TirTypeError> {
    let graph = build_class_graph(class_field_types, type_aliases);

    // Find all cycles using Tarjan's algorithm
    let cycles = Tarjan::components(&graph);

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

// ============================================================================
// HELPERS
// ============================================================================

/// Format a cycle path for error messages.
///
/// For cycles with more than one element, shows the complete cycle by adding
/// the first element at the end: "A -> B -> C -> A"
///
/// For single-element cycles (self-loops), just shows: "A"
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
