use std::{
    cmp,
    collections::{HashMap, HashSet, VecDeque},
};

use either::Either;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{FieldType, TypeExpId, WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

/// Validates if there's a cycle in the dependency graph.
pub(super) fn validate(ctx: &mut Context<'_>) {
    // First, build a graph of all the "required" dependencies represented as an
    // adjacency list. We're only going to consider type dependencies that can
    // actually cause infinite recursion. Unions and optionals can stop the
    // recursion at any point, so they don't have to be part of the "dependency"
    // graph because technically an optional field doesn't "depend" on anything,
    // it can just be null.
    let dependency_graph = HashMap::from_iter(ctx.db.walk_classes().map(|class| {
        let expr_block = &ctx.db.ast()[class.id];

        // TODO: There's already a hash set that returns "dependencies" in
        // the DB, it shoudn't be necessary to traverse all the fields here
        // again and build yet another graph, we need to refactor
        // .dependencies() or add a new method that returns not only the
        // dependency name but also field arity. The arity could be computed at
        // the same time as the dependencies hash set. Code is here:
        //
        // baml-lib/parser-database/src/types/mod.rs
        // fn visit_class()
        let mut dependencies = HashSet::new();

        for field in &expr_block.fields {
            if let Some(field_type) = &field.expr {
                insert_required_deps(field_type, ctx, &mut dependencies);
            }
        }

        (class.id, dependencies)
    }));

    for component in Tarjan::components(&dependency_graph) {
        let cycle = component
            .iter()
            .map(|id| ctx.db.ast()[*id].name().to_string())
            .collect::<Vec<_>>()
            .join(" -> ");

        ctx.push_error(DatamodelError::new_validation_error(
            &format!("These classes form a dependency cycle: {}", cycle),
            ctx.db.ast()[component[0]].span().clone(),
        ));
    }
}

/// Inserts all the required dependencies of a field into the given set.
///
/// Recursively deals with unions of unions. Can be implemented iteratively with
/// a while loop and a stack/queue if this ends up being slow / inefficient or
/// it reaches stack overflows with large inputs.
fn insert_required_deps(field: &FieldType, ctx: &Context<'_>, deps: &mut HashSet<TypeExpId>) {
    match field {
        FieldType::Symbol(arity, ident, _) if arity.is_required() => {
            if let Some(Either::Left(class)) = ctx.db.find_type_by_str(ident.name()) {
                deps.insert(class.id);
            }
        }

        FieldType::Union(arity, field_types, _, _) if arity.is_required() => {
            for f in field_types {
                insert_required_deps(f, ctx, deps);
            }
        }

        _ => {}
    }
}

/// Dependency graph represented as an adjacency list.
type Graph = HashMap<TypeExpId, HashSet<TypeExpId>>;

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
struct Tarjan<'g> {
    /// Ref to the depdenency graph.
    graph: &'g Graph,
    /// Node number counter.
    index: usize,
    /// Nodes are placed on a stack in the order in which they are visited.
    stack: Vec<TypeExpId>,
    /// State of each node.
    state: HashMap<TypeExpId, NodeState>,
    /// Strongly connected components.
    components: Vec<Vec<TypeExpId>>,
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
    pub fn components(graph: &'g Graph) -> Vec<Vec<TypeExpId>> {
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

        for node in tarjans.graph.keys() {
            if tarjans.state[node].index == Self::UNVISITED {
                tarjans.strong_connect(*node);
            }
        }

        tarjans.components
    }

    /// Recursive DFS.
    ///
    /// This is where the "algorithm" runs. Again, could be implemented
    /// iteratively if needed at some point.
    fn strong_connect(&mut self, node_id: TypeExpId) {
        // Initialize node state. This node has not yet been visited so we don't
        // have to grab the state from the hash map. And if we did, then we'd
        // have to fight the borrow checker by taking mut refs and unique refs
        // over and over again as needed (which requires hashing the same entry
        // many times and is not as readable).
        let mut node = NodeState {
            index: self.index,
            low_link: self.index,
            on_stack: true,
        };

        // Increment index and push node to stack.
        self.index += 1;
        self.stack.push(node_id);

        // Visit neighbors to find strongly connected components.
        for successor_id in &self.graph[&node_id] {
            // Grab owned state to circumvent borrow checker.
            let mut successor = *&self.state[successor_id];
            if successor.index == Self::UNVISITED {
                // Make sure state is updated before the recursive call.
                self.state.insert(node_id, node);
                self.strong_connect(*successor_id);
                // Grab updated state after recursive call.
                successor = *&self.state[successor_id];
                node.low_link = cmp::min(node.low_link, successor.low_link);
            } else if successor.on_stack {
                node.low_link = cmp::min(node.low_link, successor.index);
            }
        }

        // Update state in case we haven't already. We store this in a hash map
        // so we have to run the hashing algorithm every time we update the
        // state. Keep it to a minimum :)
        self.state.insert(node_id, node);

        if node.low_link == node.index {
            let mut component = Vec::new();

            while let Some(successor_id) = self.stack.pop() {
                // This should not fail since all nodes should be stored in
                // the state hash map.
                if let Some(successor) = self.state.get_mut(&successor_id) {
                    successor.on_stack = false;
                }

                component.push(successor_id);

                if successor_id == node_id {
                    break;
                }
            }

            // Path should be shown as parent -> child.
            // TODO: The start node is random because hash maps. A simple fix
            // is to consider that the cycle starts at the node with the
            // smallest ID.
            component.reverse();

            self.components.push(component);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use internal_baml_schema_ast::ast::TypeExpId;

    use super::Tarjan;

    fn type_exp_ids(ids: &[u32]) -> impl Iterator<Item = TypeExpId> + '_ {
        ids.iter().copied().map(TypeExpId::from)
    }

    fn graph(from: &[(u32, &[u32])]) -> HashMap<TypeExpId, HashSet<TypeExpId>> {
        HashMap::from_iter(from.iter().map(|(node, successors)| {
            (TypeExpId::from(*node), type_exp_ids(&successors).collect())
        }))
    }

    fn expected_components(components: &[&[u32]]) -> Vec<Vec<TypeExpId>> {
        components
            .iter()
            .map(|ids| type_exp_ids(ids).collect())
            .collect()
    }

    /// Ignores the graph cycle path.
    ///
    /// The graph is stored in a HashMap so Tarjan's algorithm will not always
    /// follow the same path due to random state. We can't use Vecs to compare
    /// determinstically so we'll just ignore the cycle path and compare the
    /// nodes that form the cycle.
    ///
    /// TODO: Implement the fix mentioned in the implementation and this won't
    /// be necessary.
    fn ignore_path(components: Vec<Vec<TypeExpId>>) -> Vec<HashSet<TypeExpId>> {
        components
            .into_iter()
            .map(|v| v.into_iter().collect())
            .collect()
    }

    fn assert_eq_components(actual: Vec<Vec<TypeExpId>>, expected: Vec<Vec<TypeExpId>>) {
        assert_eq!(ignore_path(actual), ignore_path(expected));
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

        assert_eq_components(
            Tarjan::components(&graph),
            expected_components(&[&[0, 1, 2], &[5, 6], &[3, 4], &[7]]),
        );
    }
}
