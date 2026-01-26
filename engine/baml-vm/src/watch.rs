//! Implementation of the infamous @watch syntax in Baml.
//!
//! This module implements a reachability algorithm that tracks which nodes need
//! to emit when values change.
//!
//! The structure maintains reachability sets for each "watched" root, and
//! updates them when edges are added or removed.
//!
//! Modifications are expensive (link or unlink edges) but lookup is fast:
//!
//! "Given a modification to node X, do we need to notify any roots? If so,
//! which ones?".
//!
//! That's the question we'll need to answer most frequently. Knowing if we
//! have to notify at all is O(1) while collecting the exact roots that have to
//! emit scales linearly with the number of roots.
//!
//! In practice, I'd say even collecting watched roots is O(1) because no one
//! will watch a million different objects.
//!
//! So, with this efficiency, Baml code not using @watch shouldn't take much of
//! a performance hit other than the if statement constantly asking
//! "does this change trigger any emission?".
//!
//! # Graph Reachability
//!
//! The code here is based on these 2 rules:
//!
//! 1. Given any subgraph G, a node p ∈ G and a set of roots R ⊆ G, when a new
//!    subgraph L is connected to G through an edge (p, c) where c ∈ L, all the
//!    previous nodes reachable from any r ∈ R are still reachable, but new
//!    reachable nodes exist in L and potentially G if p is reachable from r.
//!    Therefore, we may not have to traverse G, but we must traverse L to
//!    propagate root reachability to all nodes of L reachable from c. If G ends
//!    up in the path and we discover new reachable nodes, then we propagate
//!    there as well.
//!
//! 2. Given any graph G, two connected nodes p, c ∈ G and a set of roots R ⊆ G,
//!    when the edge (p, c) is removed and p is reachable from any r ∈ R, then
//!    we must traverse G starting at r to rediscover reachable nodes. We cannot
//!    assume that nodes reachable from c are no longer reachable just because
//!    the path (p, c) is no longer present. The node c could still be reachable
//!    through other paths.
//!
//! It should be noted that I made up this whole math and it was never formally
//! verified. But, explained visually:
//!
//! - Rule 1
//!
//! Given these two subgraphs:
//!
//! ```text
//!        Graph G                      Graph L
//!
//!         +---+                 +---+   +---+   +---+
//!    +----| r |<---+            | a |<--| b |<--| c |
//!    |    +---+    |            +---+   +---+   +---+
//!    |      |      |              |       ʌ
//!    v      V      |              |       |
//!  +---+  +---+  +---+            |       |
//!  | x |  | p |  | y |<-----------+       |
//!  +---+  +---+  +---+                    |
//!                  |                      |
//!                  V                      |
//!                +---+                    |
//!                | z |--------------------+
//!                +---+
//! ```
//!
//! We can see that the only reachable nodes from root r are {x, p}. Therefore,
//! if this was an emittable object where r is the @watch declaration, then r
//! would only emit when x or p change.
//!
//! But, adding the edge (p, c):
//!
//! ```text
//!        Graph G                      Graph L
//!
//!         +---+                 +---+   +---+   +---+
//!    +----| r |<---+            | a |<--| b |<--| c |
//!    |    +---+    |            +---+   +---+   +---+
//!    |      |      |              |       ʌ       ʌ
//!    v      V      |              |       |       |
//!  +---+  +---+  +---+            |       |       |
//!  | x |  | p |  | y |<-----------+       |       |
//!  +---+  +---+  +---+                    |       |
//!           |      |                      |       |
//!           |      V                      |       |
//!           |    +---+                    |       |
//!           |    | z |--------------------+       |
//!           |    +---+                            |
//!           |                                     |
//!           +-------------------------------------+
//! ```
//!
//! Now the set of reachable nodes from root r is {x, p, c, b, a, y, z}. So, if
//! r was an @watch object, it would emit when any of the other nodes change.
//!
//! - Rule 2
//!
//! Given the graph above, if we remove the edge (y, z):
//!
//! ```text
//!        Graph G                      Graph L
//!
//!         +---+                 +---+   +---+   +---+
//!    +----| r |<---+            | a |<--| b |<--| c |
//!    |    +---+    |            +---+   +---+   +---+
//!    |      |      |              |       ʌ       ʌ
//!    v      V      |              |       |       |
//!  +---+  +---+  +---+            |       |       |
//!  | x |  | p |  | y |<-----------+       |       |
//!  +---+  +---+  +---+                    |       |
//!           |                             |       |
//!           |                             |       |
//!           |    +---+                    |       |
//!           |    | z |--------------------+       |
//!           |    +---+                            |
//!           |                                     |
//!           +-------------------------------------+
//! ```
//!
//! We might be tempted to say, "Ok, since z is no longer reachable from r then
//! neither are its descendants." Which false, {b, a, y} are all still reachable
//! from r through (p, c).
//!
//! Similarly, even the statement "z is no longer reachable from r" could be
//! false, if there was an edge (b, z) then removing (y, z) does nothing, z
//! would still be reachable.
//!
//! So, on removal of nodes, we have to completely recompute reachability from
//! roots. We cannot propagate locally like we do on additions.
//!
//! If there's an algorithm that can avoid that full graph traversal starting at
//! roots, I'd like to be proved wrong. But my research only ended on
//! "SCC condensation"  mixed with "edge reference counting", which is much more
//! complex than needed.
//!
//! If someone ever complains about @watch being slow, then ping me and I'll ask
//! Claude to implement SCC condensation + edge ref counting.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::{Object, ObjectIndex, ObjectPool, StackIndex, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum WatchFilter {
    Default,
    Manual,
    Paused,
    Function(ObjectIndex),
}

/// State associated with a watched root.
#[derive(Clone, Debug, PartialEq)]
pub struct RootState {
    /// Current value.
    pub value: Value,
    /// Last assigned value.
    pub last_assigned: Option<Value>,
    /// Last notified value.
    pub last_notified: Option<Value>,
    /// Channel name.
    pub channel: String,
    /// Pointer to filter function.
    pub filter: WatchFilter,
}

/// Identifies a node in the emit graph.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum NodeId {
    /// Local variable on the stack.
    LocalVar(StackIndex),
    /// Heap-allocated object.
    HeapObject(ObjectIndex),
}

/// Edge label for parent -> child relationships.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Path {
    /// Local variable binding: `let x ---binding---> value`.
    Binding,
    /// Instance field: `instance ---field---> value`.
    InstanceField(usize),
    /// Array element: `array ---index---> value`.
    ArrayIndex(usize),
    /// Map entry: `map ---key---> value`.
    MapKey(String),
}

/// The emit dependency graph with incremental reachability maintenance.
///
/// Public API consists of this set of operations:
///
/// - [`Watch::register_root`]
/// - [`Watch::unregister_root`]
/// - [`Watch::add_edge`]
/// - [`Watch::link_edge`]
/// - [`Watch::unlink_edge`]
/// - [`Watch::copy_roots_reaching`]
///
/// It should be possible to change the implementation without changing the
/// current API.
#[derive(Default, Debug)]
pub struct Watch {
    /// Forward edges: `parent -> [(path, child)]`
    children: HashMap<NodeId, HashSet<(Path, NodeId)>>,

    /// Reverse edges: `child -> [(parent, path)]`
    parents: HashMap<NodeId, HashSet<(NodeId, Path)>>,

    /// For each root, which nodes it can reach.
    ///
    /// When we remove a node from the graph, this set quickly answers:
    ///
    /// "Which nodes were reachable from the root before we removed the node?"
    ///
    /// Now for each of those nodes, if they are no longer reachable we can
    /// update their inverse index [`Self::roots_reaching_node`].
    reachable_from_root: HashMap<NodeId, HashSet<NodeId>>,

    /// For each node, which roots can reach it (inverse index for O(1) lookup).
    roots_reaching_node: HashMap<NodeId, HashSet<NodeId>>,

    /// Active roots.
    roots: HashMap<NodeId, RootState>,
}

impl Watch {
    /// Creates a new empty emit graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds the edge `parent.path -> child` to the graph.
    ///
    /// This does not propagate root reachability to the `child` and its
    /// descendants. Use [`Self::link_edge`] to propagate reachability.
    ///
    /// [`Self::add_edge`] only builds the graph. When a new edge is added to
    /// an existing graph, then any roots that previously reached `parent` now
    /// will also reach `child` and its descendants, that's where
    /// [`Self::link_edge`] should be called.
    fn add_edge(&mut self, parent: NodeId, path: Path, child: NodeId) {
        self.children
            .entry(parent)
            .or_default()
            .insert((path.clone(), child));

        self.parents
            .entry(child)
            .or_default()
            .insert((parent, path));
    }

    /// Computes all nodes reachable from a starting node using BFS.
    fn breadth_first_search_from(&self, start: NodeId) -> HashSet<NodeId> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();

        queue.push_back(start);
        visited.insert(start);

        while let Some(node) = queue.pop_front() {
            if let Some(edges) = self.children.get(&node) {
                for (_, child) in edges {
                    if visited.insert(*child) {
                        queue.push_back(*child);
                    }
                }
            }
        }

        visited
    }

    /// Registers a new emittable root at the given node.
    ///
    /// Triggers a BFS graph traversal starting at `root`.
    pub fn register_root(&mut self, root: NodeId, state: RootState) {
        self.roots.insert(root, state);

        // Compute initial reachability from this root
        let reachable = self.breadth_first_search_from(root);

        // Update inverse index: node -> roots
        for node in &reachable {
            self.roots_reaching_node
                .entry(*node)
                .or_default()
                .insert(root);
        }

        // Update forward index: root -> nodes
        self.reachable_from_root.insert(root, reachable);
    }

    /// Unregisters an emittable root (e.g., when it goes out of scope).
    ///
    /// Scans all reachable nodes from root and updates cached indexes. It does
    /// not fully traverse the graph starting at `root`.
    pub fn unregister_root(&mut self, root: NodeId) {
        // Remove from active roots
        self.roots.remove(&root);

        // Clean up reachability cache
        if let Some(reachable) = self.reachable_from_root.remove(&root) {
            for node in reachable {
                if let Some(roots) = self.roots_reaching_node.get_mut(&node) {
                    roots.remove(&root);
                    if roots.is_empty() {
                        self.roots_reaching_node.remove(&node);
                    }
                }
            }
        }
    }

    /// Links parent.path -> child in the graph.
    ///
    /// Updates reachability incrementally for all affected roots.
    pub fn link_edge(&mut self, parent: NodeId, path: Path, child: NodeId, objects: &ObjectPool) {
        // If pointing to an object, recursively build the dependency graph.
        if let NodeId::HeapObject(index) = child {
            self.track_dependencies(Value::Object(index), objects);
        }

        self.add_edge(parent, path, child);

        // For each root that reaches parent, update reachability to include
        // child and its descendants.
        for root in self.copy_roots_reaching(parent) {
            let mut queue = VecDeque::new();
            queue.push_back(child);

            // Set of nodes reachable from `root`. Also acts as a set of visited
            // nodes. If a node is already reachable, that means we already indexed
            // all its descendants, so no need to traverse that path again.
            let reachable = self.reachable_from_root.entry(root).or_default();

            while let Some(node) = queue.pop_front() {
                // Skip already reachable nodes
                if reachable.insert(node) {
                    // Add to inverse index
                    self.roots_reaching_node
                        .entry(node)
                        .or_default()
                        .insert(root);

                    // Queue children
                    if let Some(edges) = self.children.get(&node) {
                        for (_, child) in edges {
                            queue.push_back(*child);
                        }
                    }
                }
            }
        }
    }

    /// Unlinks parent.path -> child from the graph.
    ///
    /// Updates reachability incrementally for all affected roots.
    pub fn unlink_edge(&mut self, parent: NodeId, path: Path, child: NodeId) {
        if let Some(edges) = self.children.get_mut(&parent) {
            edges.remove(&(path.clone(), child));
            if edges.is_empty() {
                self.children.remove(&parent);
            }
        }

        if let Some(edges) = self.parents.get_mut(&child) {
            edges.remove(&(parent, path));
            if edges.is_empty() {
                self.parents.remove(&child);
            }
        }

        // For each root that reaches the child, recompute reachability and
        // remove unreachable nodes.
        for root in self.copy_roots_reaching(child) {
            let still_reachable = self.breadth_first_search_from(root);

            // Get the old reachable set
            let old_reachable = self
                .reachable_from_root
                .get(&root)
                .cloned()
                .unwrap_or_default();

            // Find nodes that are no longer reachable
            let no_longer_reachable: Vec<NodeId> = old_reachable
                .difference(&still_reachable)
                .copied()
                .collect();

            // Update forward index
            self.reachable_from_root.insert(root, still_reachable);

            // Update inverse index: remove root from nodes no longer reachable
            for node in no_longer_reachable {
                if let Some(roots) = self.roots_reaching_node.get_mut(&node) {
                    roots.remove(&root);
                    if roots.is_empty() {
                        self.roots_reaching_node.remove(&node);
                    }
                }
            }
        }
    }

    /// Watched root state.
    pub fn root_state(&self, node: NodeId) -> Option<&RootState> {
        self.roots.get(&node)
    }

    pub fn root_state_mut(&mut self, node: NodeId) -> Option<&mut RootState> {
        self.roots.get_mut(&node)
    }

    /// Returns true if the given node is "watched".
    ///
    /// "Watched" means that there is at least one watched root that can reach
    /// this node.
    pub fn is_watched(&self, node: NodeId) -> bool {
        // Implementation of `contains_key` only computes hash if the map is
        // not empty. If at any given moment the Baml program is not watching
        // anything, this should just be an if statement.
        self.roots_reaching_node.contains_key(&node)
    }

    /// Returns an owned copy of all the root IDs reaching `node`.
    ///
    /// Used to avoid borrow checker issues when iterating over roots.
    pub fn copy_roots_reaching(&self, node: NodeId) -> Vec<NodeId> {
        self.roots_reaching_node
            .get(&node)
            .map(|root_ids| root_ids.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Traverses an object graph and builds emit edges from parent to all
    /// children.
    ///
    /// This is used when an object is marked as @watch to establish all the
    /// dependency edges. It does not declare any root, call
    /// [`Self::register_root`] separately.
    pub fn track_dependencies(&mut self, value: Value, objects: &ObjectPool) {
        let mut stack = vec![value];
        let mut visited = HashSet::new();

        while let Some(v) = stack.pop() {
            let Value::Object(index) = v else {
                continue;
            };

            if !visited.insert(index) {
                continue;
            }

            let node = NodeId::HeapObject(index);

            // Now traverse the object's contents
            match &objects[index] {
                Object::Instance(instance) => {
                    // For each field in the instance, build edges
                    for (field_idx, field_value) in instance.fields.iter().enumerate() {
                        if let Value::Object(child_obj) = field_value {
                            self.add_edge(
                                node,
                                Path::InstanceField(field_idx),
                                NodeId::HeapObject(*child_obj),
                            );

                            stack.push(*field_value);
                        }
                    }
                }

                Object::Array(array) => {
                    // For each element in the array, build edges
                    for (idx, elem_value) in array.iter().enumerate() {
                        if let Value::Object(child_obj) = elem_value {
                            self.add_edge(
                                node,
                                Path::ArrayIndex(idx),
                                NodeId::HeapObject(*child_obj),
                            );

                            stack.push(*elem_value);
                        }
                    }
                }

                Object::Map(map) => {
                    // For each entry in the map, build edges
                    for (key, map_value) in map.iter() {
                        if let Value::Object(child_obj) = map_value {
                            self.add_edge(
                                node,
                                Path::MapKey(key.clone()),
                                NodeId::HeapObject(*child_obj),
                            );

                            stack.push(*map_value);
                        }
                    }
                }

                _ => {
                    // Other object types (strings, functions, etc.) don't have
                    // nested structure
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root_state() -> RootState {
        RootState {
            value: Value::Int(0),
            last_assigned: None,
            last_notified: None,
            channel: "Test".to_string(),
            filter: WatchFilter::Default,
        }
    }

    // TODO: fix this nonsense
    fn dummy_object_pool(n: usize) -> ObjectPool {
        ObjectPool::from_vec(vec![crate::types::Object::String(String::new()); n])
    }

    #[test]
    fn test_basic_notify_registration() {
        let mut emit = Watch::new();

        let var = NodeId::LocalVar(StackIndex::from_raw(0));
        emit.register_root(var, test_root_state());

        // Root should be registered
        assert!(emit.roots.contains_key(&var));

        // Variable should be covered by the root
        let covering: Vec<NodeId> = emit.copy_roots_reaching(var);
        assert_eq!(covering.len(), 1);
    }

    #[test]
    fn test_link_unlink_edge() {
        let mut emit = Watch::new();

        let var = NodeId::LocalVar(StackIndex::from_raw(0));
        let obj = NodeId::HeapObject(ObjectIndex::from_raw(0));

        // Register root at var
        emit.register_root(var, test_root_state());

        // Link var -> obj
        emit.link_edge(var, Path::Binding, obj, &dummy_object_pool(1));

        // Both should be covered
        assert_eq!(emit.copy_roots_reaching(var).len(), 1);
        assert_eq!(emit.copy_roots_reaching(obj).len(), 1);

        // Unlink var -> obj
        emit.unlink_edge(var, Path::Binding, obj);

        // Only var should be covered now
        assert_eq!(emit.copy_roots_reaching(var).len(), 1);
        assert_eq!(emit.copy_roots_reaching(obj).len(), 0);
    }

    #[test]
    fn test_cycle_handling() {
        let mut emit = Watch::new();

        let a = NodeId::HeapObject(ObjectIndex::from_raw(0));
        let b = NodeId::HeapObject(ObjectIndex::from_raw(1));
        let root_node = NodeId::LocalVar(StackIndex::from_raw(0));

        // Create cycle: A -> B -> A
        emit.link_edge(a, Path::InstanceField(0), b, &dummy_object_pool(2));
        emit.link_edge(b, Path::InstanceField(0), a, &dummy_object_pool(2));

        // Register root at root_node
        emit.register_root(root_node, test_root_state());

        // Link root -> A (brings cycle into reachability)
        emit.link_edge(root_node, Path::Binding, a, &dummy_object_pool(2));

        // Both A and B should be covered
        assert_eq!(emit.copy_roots_reaching(a).len(), 1);
        assert_eq!(emit.copy_roots_reaching(b).len(), 1);

        // Unlink root -> A (disconnects cycle)
        emit.unlink_edge(root_node, Path::Binding, a);

        // Neither A nor B should be covered
        assert_eq!(emit.copy_roots_reaching(a).len(), 0);
        assert_eq!(emit.copy_roots_reaching(b).len(), 0);
    }

    #[test]
    fn test_multiple_roots() {
        let mut emit = Watch::new();

        let var1 = NodeId::LocalVar(StackIndex::from_raw(0));
        let var2 = NodeId::LocalVar(StackIndex::from_raw(1));
        let obj = NodeId::HeapObject(ObjectIndex::from_raw(0));

        // Register two roots
        emit.register_root(var1, test_root_state());
        emit.register_root(var2, test_root_state());

        // Link both to the same object
        emit.link_edge(var1, Path::Binding, obj, &dummy_object_pool(1));
        emit.link_edge(var2, Path::Binding, obj, &dummy_object_pool(1));

        // Object should be covered by both roots
        assert_eq!(emit.copy_roots_reaching(obj).len(), 2);

        // Unlink one
        emit.unlink_edge(var1, Path::Binding, obj);

        // Object should still be covered by root2
        assert_eq!(emit.copy_roots_reaching(obj).len(), 1);

        // Unregister root2
        emit.unregister_root(var2);

        // Object should not be covered anymore
        assert_eq!(emit.copy_roots_reaching(obj).len(), 0);
    }

    #[test]
    fn test_deep_object_graph() {
        let mut emit = Watch::new();

        let var = NodeId::LocalVar(StackIndex::from_raw(0));
        let obj1 = NodeId::HeapObject(ObjectIndex::from_raw(0));
        let obj2 = NodeId::HeapObject(ObjectIndex::from_raw(1));
        let obj3 = NodeId::HeapObject(ObjectIndex::from_raw(2));

        // Create chain: var -> obj1 -> obj2 -> obj3
        emit.register_root(var, test_root_state());
        emit.link_edge(var, Path::Binding, obj1, &dummy_object_pool(3));
        emit.link_edge(obj1, Path::InstanceField(0), obj2, &dummy_object_pool(3));
        emit.link_edge(obj2, Path::InstanceField(0), obj3, &dummy_object_pool(3));

        // All should be covered
        assert_eq!(emit.copy_roots_reaching(obj1).len(), 1);
        assert_eq!(emit.copy_roots_reaching(obj2).len(), 1);
        assert_eq!(emit.copy_roots_reaching(obj3).len(), 1);

        // Break the chain in the middle
        emit.unlink_edge(obj1, Path::InstanceField(0), obj2);

        // obj1 still covered, obj2 and obj3 not
        assert_eq!(emit.copy_roots_reaching(obj1).len(), 1);
        assert_eq!(emit.copy_roots_reaching(obj2).len(), 0);
        assert_eq!(emit.copy_roots_reaching(obj3).len(), 0);
    }
}
