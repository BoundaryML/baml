//! Tarjan's strongly connected components algorithm for cycle detection.
//!
//! This is used in parser_database to detect cycles in BAML types
//! that reference each other recursively.

use std::{
    cmp,
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::Hash,
};

use internal_baml_ast::ast::TypeExpId;

/// Dependency graph represented as an adjacency list.
type Graph<V> = HashMap<V, HashSet<V>>;

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
/// This struct is simply bookkeeping for the algorithm, it can be implemented
/// with just function calls but the recursive one would need 6 parameters which
/// is pretty ugly.
pub struct Tarjan<'g, V> {
    /// Ref to the depdenency graph.
    graph: &'g Graph<V>,
    /// Node number counter.
    index: usize,
    /// Nodes are placed on a stack in the order in which they are visited.
    stack: Vec<V>,
    /// State of each node.
    state: HashMap<V, NodeState>,
    /// Strongly connected components.
    components: Vec<Vec<V>>,
}

// V is Copy because we mostly use opaque identifiers for class or alias IDs.
// In practice V ends up being a u32, but if for some reason this needs to
// be used with strings then we can make V Clone instead of Copy and refactor
// the code below.
impl<'g, V: Eq + Ord + Hash + Copy> Tarjan<'g, V> {
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
    pub fn components(graph: &'g Graph<V>) -> Vec<Vec<V>> {
        let mut tarjans = Self {
            graph,
            index: 0,
            stack: Vec::new(),
            state: HashMap::from_iter(graph.keys().map(|&node| {
                let state = NodeState {
                    index: Self::UNVISITED,
                    low_link: Self::UNVISITED,
                    on_stack: false,
                };

                (node, state)
            })),
            components: Vec::new(),
        };

        // Always start at the same node to avoid randomness in the cycle path,
        // although the hash sets to which the nodes point to might still cause
        // randomness but we deal with that in the strong_connect method when
        // we build the component path.
        let mut nodes = Vec::from_iter(graph.keys());
        nodes.sort();

        for node in nodes {
            if tarjans.state[node].index == Self::UNVISITED {
                tarjans.strong_connect(*node);
            }
        }

        // Sort components by the first element in each cycle (which is already
        // sorted as well). This should get rid of all the randomness caused by
        // hash maps and hash sets.
        tarjans.components.sort_by(|a, b| a[0].cmp(&b[0]));

        tarjans.components
    }

    /// Recursive DFS.
    ///
    /// This is where the "algorithm" runs. Could be implemented iteratively if
    /// needed at some point.
    fn strong_connect(&mut self, node_id: V) {
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
        self.state.insert(node_id, node);
        self.stack.push(node_id);

        // TODO: @antoniosarosi: HashSet is random, won't always iterate in the
        // same order. Fix this with IndexSet or something, we really don't want
        // to sort this every single time. Also order only matters for tests, we
        // can do `if cfg!(test)` or something.
        let mut successors = Vec::from_iter(&self.graph[&node_id]);
        successors.sort();

        // Visit neighbors to find strongly connected components.
        for successor_id in successors {
            // Grab owned state to circumvent borrow checker.
            let mut successor = self.state[successor_id];
            if successor.index == Self::UNVISITED {
                self.strong_connect(*successor_id);
                // Grab updated state after recursive call.
                successor = self.state[successor_id];
                node.low_link = cmp::min(node.low_link, successor.low_link);
            } else if successor.on_stack {
                node.low_link = cmp::min(node.low_link, successor.index);
            }
        }

        // Re-insert the node's state as the state might have changed.
        // we need to do some memory gymnastics here to avoid needing a re-insert
        // of the state every time we update it.
        self.state.insert(node_id, node);

        // Root node of a strongly connected component.
        if node.low_link == node.index {
            let mut component = Vec::new();

            while let Some(parent_id) = self.stack.pop() {
                // This should not fail since all nodes should be stored in
                // the state hash map.
                if let Some(parent) = self.state.get_mut(&parent_id) {
                    parent.on_stack = false;
                }

                component.push(parent_id);

                if parent_id == node_id {
                    break;
                }
            }

            // Path should be shown as parent -> child not child -> parent.
            component.reverse();

            // Find index of minimum element in the component.
            //
            // The cycle path is not computed deterministacally because the
            // graph is stored in a hash map, so random state will cause the
            // traversal algorithm to start at different nodes each time.
            //
            // Therefore, to avoid reporting errors to the user differently
            // every time, we'll use a simple deterministic way to determine
            // the start node of a cycle.
            //
            // Basically, the start node will always be the smallest type ID in
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
                || (component.len() == 1 && self.graph[&node_id].contains(&node_id))
            {
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
    use std::collections::{HashMap, HashSet};

    use internal_baml_ast::ast::TypeExpId;

    use super::Tarjan;

    fn type_exp_ids(ids: &[u32]) -> impl Iterator<Item = TypeExpId> + '_ {
        ids.iter().copied().map(TypeExpId::from)
    }

    fn graph(from: &[(u32, &[u32])]) -> HashMap<TypeExpId, HashSet<TypeExpId>> {
        HashMap::from_iter(
            from.iter().map(|(node, successors)| {
                (TypeExpId::from(*node), type_exp_ids(successors).collect())
            }),
        )
    }

    fn expected_components(components: &[&[u32]]) -> Vec<Vec<TypeExpId>> {
        components
            .iter()
            .map(|ids| type_exp_ids(ids).collect())
            .collect()
    }

    #[test]
    fn find_cycles_names() {
        // Define the graph using a key-value type with string literals
        let graph_data = [
            (
                "ProcessNextStepArgs",
                vec!["JSONSchemaValue", "Tool", "Message"],
            ),
            ("JSONSchemaProperty", vec!["JSONSchemaValue"]),
            ("Message", vec![]),
            ("ParameterValue", vec![]),
            ("Tool", vec!["JSONSchemaValue"]),
            ("UserCommandQuestion", vec![]),
            ("Parameter", vec![]),
            ("UserTool", vec![]),
            ("ScriptStep", vec!["JSONSchemaValue"]),
            ("JSONSchemaValue", vec!["JSONSchemaValue"]),
            ("UserCommandParameter", vec![]),
            (
                "StepDescriptionRequest",
                vec!["JSONSchemaValue", "UserTool", "ScriptStep"],
            ),
            ("GetUnstructuredContentArgs", vec![]),
            ("SummarizedToolExecutionResults", vec!["StructuredResponse"]),
            ("StructuredResponse", vec![]),
            ("StepDescriptionResponse", vec![]),
            ("GetUnstructuredContentResponse", vec![]),
            ("ToolCall", vec!["ParameterValue"]),
            ("SummarizeToolExecutionResultsArgs", vec!["Message"]),
            (
                "UserCommand",
                vec!["UserCommandQuestion", "UserCommandParameter"],
            ),
            ("ProcessNextStepResponse", vec!["ToolCall"]),
        ];

        // Transform the key-value type into an IndexMap with IndexSet
        let graph = HashMap::from_iter(
            graph_data
                .into_iter()
                .map(|(node, successors)| (node, HashSet::from_iter(successors))),
        );

        let components = Tarjan::components(&graph);
        assert_eq!(components, &[&["JSONSchemaValue"]]);
    }

    #[test]
    fn find_cycles() {
        let graph = graph(&[
            (0, &[1]),
            (1, &[2]),
            (2, &[0]),
            (3, &[1, 2, 4]),
            (4, &[5, 3]),
            (5, &[2, 6]),
            (6, &[5]),
            (7, &[4, 6, 7]),
        ]);

        assert_eq!(
            Tarjan::components(&graph),
            expected_components(&[&[0, 1, 2], &[3, 4], &[5, 6], &[7]]),
        );
    }

    #[test]
    fn no_cycles_found() {
        let graph = graph(&[
            (0, &[1]),
            (1, &[2, 3]),
            (2, &[4]),
            (3, &[5]),
            (4, &[]),
            (5, &[]),
        ]);

        assert_eq!(Tarjan::components(&graph), expected_components(&[]));
    }
}
