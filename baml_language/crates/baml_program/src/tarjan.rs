//! Tarjan's strongly connected components algorithm for cycle detection.
//!
//! This is used to detect cycles in BAML types that reference each other
//! recursively, enabling proper handling of recursive types in output format
//! rendering.

use std::cmp;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// Dependency graph represented as an adjacency list.
pub type Graph<V> = HashMap<V, HashSet<V>>;

/// State of each node for Tarjan's algorithm.
#[derive(Clone, Copy)]
struct NodeState {
    /// Node unique index (discovery order).
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
pub struct Tarjan<'g, V> {
    /// Reference to the dependency graph.
    graph: &'g Graph<V>,
    /// Node number counter.
    index: usize,
    /// Nodes are placed on a stack in the order in which they are visited.
    stack: Vec<V>,
    /// State of each node.
    state: HashMap<V, NodeState>,
    /// Strongly connected components (cycles).
    components: Vec<Vec<V>>,
}

impl<'g, V: Eq + Ord + Hash + Clone> Tarjan<'g, V> {
    /// Unvisited node marker.
    ///
    /// Technically we should use `Option<usize>` and `None` for
    /// `NodeState::index` and `NodeState::low_link` but that would require
    /// some ugly and repetitive `Option::unwrap` calls. `usize::MAX` won't
    /// be reached as an index anyway.
    const UNVISITED: usize = usize::MAX;

    /// Find all strongly connected components (cycles) in the graph.
    ///
    /// Returns a list of cycles, where each cycle is a list of nodes.
    /// Single nodes without self-references are filtered out.
    pub fn components(graph: &'g Graph<V>) -> Vec<Vec<V>> {
        let mut tarjans = Self {
            graph,
            index: 0,
            stack: Vec::new(),
            state: HashMap::from_iter(graph.keys().map(|node| {
                (
                    node.clone(),
                    NodeState {
                        index: Self::UNVISITED,
                        low_link: Self::UNVISITED,
                        on_stack: false,
                    },
                )
            })),
            components: Vec::new(),
        };

        // Sort nodes for deterministic results
        let mut nodes: Vec<V> = graph.keys().cloned().collect();
        nodes.sort();

        for node in nodes {
            if tarjans.state[&node].index == Self::UNVISITED {
                tarjans.strong_connect(node);
            }
        }

        // Sort components by first element for determinism
        tarjans.components.sort_by(|a, b| a[0].cmp(&b[0]));
        tarjans.components
    }

    /// Recursive DFS that detects cycle roots.
    fn strong_connect(&mut self, node_id: V) {
        let mut node = NodeState {
            index: self.index,
            low_link: self.index,
            on_stack: true,
        };

        self.index += 1;
        self.state.insert(node_id.clone(), node);
        self.stack.push(node_id.clone());

        // Visit successors (sorted for determinism)
        if let Some(successors_set) = self.graph.get(&node_id) {
            let mut successors: Vec<V> = successors_set.iter().cloned().collect();
            successors.sort();

            for successor_id in successors {
                let mut successor = self.state[&successor_id];
                if successor.index == Self::UNVISITED {
                    self.strong_connect(successor_id.clone());
                    successor = self.state[&successor_id];
                    node.low_link = cmp::min(node.low_link, successor.low_link);
                } else if successor.on_stack {
                    node.low_link = cmp::min(node.low_link, successor.index);
                }
            }
        }

        self.state.insert(node_id.clone(), node);

        // Check if this is the root of an SCC
        if node.low_link == node.index {
            let mut component = Vec::new();

            while let Some(parent_id) = self.stack.pop() {
                if let Some(parent) = self.state.get_mut(&parent_id) {
                    parent.on_stack = false;
                }
                let is_root = parent_id == node_id;
                component.push(parent_id);
                if is_root {
                    break;
                }
            }

            component.reverse();

            // Find index of minimum element in the component for determinism.
            // The cycle path is not computed deterministically because the
            // graph is stored in a hash map, so we rotate to start at the
            // smallest element.
            let min_index = component
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.cmp(b))
                .map(|(i, _)| i);

            // Only include actual cycles (>1 node or self-referential)
            let is_self_referential = self
                .graph
                .get(&node_id)
                .map(|deps| deps.contains(&node_id))
                .unwrap_or(false);

            if component.len() > 1 || (component.len() == 1 && is_self_referential) {
                if let Some(index) = min_index {
                    component.rotate_left(index);
                    self.components.push(component);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_cycle() {
        // A -> B -> C -> A
        let graph: Graph<&str> = HashMap::from([
            ("A", HashSet::from(["B"])),
            ("B", HashSet::from(["C"])),
            ("C", HashSet::from(["A"])),
        ]);
        let cycles = Tarjan::components(&graph);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0].len(), 3);
        assert!(cycles[0].contains(&"A"));
        assert!(cycles[0].contains(&"B"));
        assert!(cycles[0].contains(&"C"));
    }

    #[test]
    fn test_self_referential() {
        // A -> A
        let graph: Graph<&str> = HashMap::from([("A", HashSet::from(["A"]))]);
        let cycles = Tarjan::components(&graph);
        assert_eq!(cycles.len(), 1);
        assert_eq!(cycles[0], vec!["A"]);
    }

    #[test]
    fn test_no_cycles() {
        // A -> B -> C (no back edge)
        let graph: Graph<&str> = HashMap::from([
            ("A", HashSet::from(["B"])),
            ("B", HashSet::from(["C"])),
            ("C", HashSet::new()),
        ]);
        let cycles = Tarjan::components(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_multiple_cycles() {
        // Two separate cycles: A <-> B, C <-> D
        let graph: Graph<&str> = HashMap::from([
            ("A", HashSet::from(["B"])),
            ("B", HashSet::from(["A"])),
            ("C", HashSet::from(["D"])),
            ("D", HashSet::from(["C"])),
        ]);
        let cycles = Tarjan::components(&graph);
        assert_eq!(cycles.len(), 2);
    }

    #[test]
    fn test_complex_graph() {
        // Graph with multiple interconnected cycles
        // 0 -> 1 -> 2 -> 0 (cycle 1)
        // 3 -> 1, 3 -> 2, 3 -> 4
        // 4 -> 5, 4 -> 3 (cycle 2: 3 <-> 4)
        // 5 -> 2, 5 -> 6
        // 6 -> 5 (cycle 3: 5 <-> 6)
        // 7 -> 4, 7 -> 6, 7 -> 7 (self-cycle)
        let graph: Graph<u32> = HashMap::from([
            (0, HashSet::from([1])),
            (1, HashSet::from([2])),
            (2, HashSet::from([0])),
            (3, HashSet::from([1, 2, 4])),
            (4, HashSet::from([5, 3])),
            (5, HashSet::from([2, 6])),
            (6, HashSet::from([5])),
            (7, HashSet::from([4, 6, 7])),
        ]);

        let cycles = Tarjan::components(&graph);
        assert_eq!(cycles.len(), 4);

        // Verify each expected cycle exists
        let cycle_sets: Vec<HashSet<u32>> = cycles.iter().map(|c| c.iter().copied().collect()).collect();
        assert!(cycle_sets.contains(&HashSet::from([0, 1, 2])));
        assert!(cycle_sets.contains(&HashSet::from([3, 4])));
        assert!(cycle_sets.contains(&HashSet::from([5, 6])));
        assert!(cycle_sets.contains(&HashSet::from([7])));
    }

    #[test]
    fn test_empty_graph() {
        let graph: Graph<&str> = HashMap::new();
        let cycles = Tarjan::components(&graph);
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_single_node_no_self_ref() {
        let graph: Graph<&str> = HashMap::from([("A", HashSet::new())]);
        let cycles = Tarjan::components(&graph);
        assert!(cycles.is_empty());
    }
}
