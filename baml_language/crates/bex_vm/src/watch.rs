#![allow(unsafe_code)]

//! Implementation of the infamous @watch syntax in Baml.
//!
//! This module implements a reachability algorithm that tracks which nodes need
//! to emit when values change.
//!
//! # Unsafe Code
//!
//! This module uses unsafe code for heap traversal when building watch dependency graphs:
//! - `heap.get_object(idx)`: Reading objects to discover their references
//!
//! Safety is ensured by:
//! - Read-only access: Only reads objects, never writes
//! - Single-threaded context: Called from VM execution which is single-threaded
//! - Valid indices: All indices come from the VM's own stack/heap traversal
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

use bex_vm_types::{HeapPtr, Object, StackIndex, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum WatchFilter {
    Default,
    Manual,
    Paused,
    Function(HeapPtr),
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
    HeapObject(HeapPtr),
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
/// - [`track_watch_dependencies`] (free function — tracks an object's dependencies)
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
    ///
    /// Currently unused for any algorithmic purpose — all traversal uses
    /// `children` (forward) and `roots_reaching_node` (inverse index).
    /// Maintained for future use: reconstructing the full path from a
    /// modified node back to its watch root, e.g. for debug logging like
    /// "watch triggered from `obj` through `obj.inner.more_inner[5].x`".
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

    /// Marks `node` as reachable from `root`. Returns `true` if newly reachable.
    fn mark_reachable(&mut self, node: NodeId, root: NodeId) -> bool {
        let reachable = self.reachable_from_root.entry(root).or_default();

        if !reachable.insert(node) {
            return false;
        }

        self.roots_reaching_node
            .entry(node)
            .or_default()
            .insert(root);

        true
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
        let roots_reaching = self.copy_roots_reaching(child);

        for root in roots_reaching {
            let still_reachable = self.breadth_first_search_from(root);

            // Swap in the new reachable set, taking ownership of the old one
            // without cloning.
            let old_reachable = self
                .reachable_from_root
                .insert(root, still_reachable)
                .unwrap_or_default();

            let still_reachable = &self.reachable_from_root[&root];

            // Update inverse index: remove root from nodes no longer reachable.
            // If a node becomes unreachable from ALL roots, prune its graph
            // edges to avoid stale entries (and dangling HeapPtrs after GC).
            for node in old_reachable.difference(still_reachable) {
                if let Some(roots) = self.roots_reaching_node.get_mut(node) {
                    roots.remove(&root);
                    if roots.is_empty() {
                        self.roots_reaching_node.remove(node);
                        self.children.remove(node);
                        self.parents.remove(node);
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
}

/// Tracks an object's watch dependencies by walking the heap and propagating
/// root reachability in a single traversal.
///
/// Adds the edge `parent --path--> child_ptr`, then BFS-walks the object
/// graph from `child_ptr`, building edges and marking all discovered nodes
/// as reachable from the same roots that reach `parent`.
///
/// Free function to avoid borrow checker issues when calling from `BexVm`.
pub fn track_watch_dependencies(watch: &mut Watch, parent: NodeId, path: Path, child_ptr: HeapPtr) {
    let child = NodeId::HeapObject(child_ptr);

    // Add the top-level edge (e.g. root --Binding--> object).
    watch.add_edge(parent, path, child);

    // Collect roots reaching parent so we can propagate reachability.
    let roots = watch.copy_roots_reaching(parent);

    // Walk the heap from child, building edges and propagating reachability.
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    queue.push_back(child_ptr);
    visited.insert(child_ptr);

    while let Some(ptr) = queue.pop_front() {
        let node = NodeId::HeapObject(ptr);

        // Propagate reachability for this node.
        for &root in &roots {
            watch.mark_reachable(node, root);
        }

        // SAFETY: Read-only heap access during single-threaded VM execution.
        let obj = unsafe { ptr.get() };

        // Discover child edges from this object.
        let edges: Vec<(Path, HeapPtr)> = match obj {
            Object::Instance(instance) => instance
                .fields
                .iter()
                .enumerate()
                .filter_map(|(idx, v)| match v {
                    Value::Object(p) => Some((Path::InstanceField(idx), *p)),
                    _ => None,
                })
                .collect(),

            Object::Array(array) => array
                .iter()
                .enumerate()
                .filter_map(|(idx, v)| match v {
                    Value::Object(p) => Some((Path::ArrayIndex(idx), *p)),
                    _ => None,
                })
                .collect(),

            Object::Map(map) => map
                .iter()
                .filter_map(|(key, v)| match v {
                    Value::Object(p) => Some((Path::MapKey(key.clone()), *p)),
                    _ => None,
                })
                .collect(),

            _ => vec![],
        };

        for (edge_path, edge_child_ptr) in edges {
            let edge_child = NodeId::HeapObject(edge_child_ptr);

            // Build graph structure.
            watch.add_edge(node, edge_path, edge_child);

            // Continue traversal into unvisited nodes.
            if visited.insert(edge_child_ptr) {
                queue.push_back(edge_child_ptr);
            }
        }
    }
}

// --- Garbage Collection ---

/// Forward a `Value::Object` pointer if present in the forwarding map.
fn forward_value(value: &mut Value, forwarding: &HashMap<HeapPtr, HeapPtr>) {
    if let Value::Object(ptr) = value {
        if let Some(&new_ptr) = forwarding.get(ptr) {
            *ptr = new_ptr;
        }
    }
}

/// Remap a `NodeId` using the GC forwarding map.
///
/// Not all pointers are in the forwarding map — only objects that were actually
/// relocated by the copying GC have entries. Compile-time objects (permanent
/// space) and objects that weren't moved keep their original pointer.
fn remap_node(node: NodeId, forwarding: &HashMap<HeapPtr, HeapPtr>) -> NodeId {
    match node {
        NodeId::HeapObject(ptr) => NodeId::HeapObject(*forwarding.get(&ptr).unwrap_or(&ptr)),
        NodeId::LocalVar(_) => node,
    }
}

/// Remap all keys and values in a `NodeId -> HashSet<NodeId>` map.
fn remap_node_set(
    map: &mut HashMap<NodeId, HashSet<NodeId>>,
    forwarding: &HashMap<HeapPtr, HeapPtr>,
) {
    let old = std::mem::take(map);
    for (key, values) in old {
        let new_values: HashSet<_> = values
            .into_iter()
            .map(|v| remap_node(v, forwarding))
            .collect();
        map.insert(remap_node(key, forwarding), new_values);
    }
}

impl Watch {
    /// Collects GC roots from Watch state.
    ///
    /// Only `last_assigned` and `last_notified` need to be roots — `value`
    /// is always a copy of the stack slot (already a root), and graph `NodeId`s
    /// point to objects transitively reachable from stack values.
    pub fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        for state in self.roots.values() {
            if let Some(Value::Object(ptr)) = state.last_assigned {
                roots.push(ptr);
            }
            if let Some(Value::Object(ptr)) = state.last_notified {
                roots.push(ptr);
            }
        }
    }

    /// Applies GC forwarding pointers to all `HeapPtr`s in Watch state.
    ///
    /// After a copying GC, all heap objects may have moved. This updates
    /// `RootState` values and rebuilds the graph maps with new pointers.
    pub fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if forwarding.is_empty() || self.roots.is_empty() {
            return;
        }

        // Patch RootState values.
        for state in self.roots.values_mut() {
            forward_value(&mut state.value, forwarding);
            if let Some(ref mut val) = state.last_assigned {
                forward_value(val, forwarding);
            }
            if let Some(ref mut val) = state.last_notified {
                forward_value(val, forwarding);
            }
            if let WatchFilter::Function(ref mut ptr) = state.filter {
                if let Some(&new_ptr) = forwarding.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }

        // Remap all HeapObject NodeIds in the graph maps.

        // children: parent -> {(path, child)}
        let old_children = std::mem::take(&mut self.children);
        for (parent, edges) in old_children {
            let new_edges: HashSet<_> = edges
                .into_iter()
                .map(|(path, child)| (path, remap_node(child, forwarding)))
                .collect();
            self.children
                .insert(remap_node(parent, forwarding), new_edges);
        }

        // parents: child -> {(parent, path)}
        let old_parents = std::mem::take(&mut self.parents);
        for (child, edges) in old_parents {
            let new_edges: HashSet<_> = edges
                .into_iter()
                .map(|(parent, path)| (remap_node(parent, forwarding), path))
                .collect();
            self.parents
                .insert(remap_node(child, forwarding), new_edges);
        }

        remap_node_set(&mut self.reachable_from_root, forwarding);
        remap_node_set(&mut self.roots_reaching_node, forwarding);

        // roots: NodeId -> RootState
        let old_roots = std::mem::take(&mut self.roots);
        for (key, value) in old_roots {
            self.roots.insert(remap_node(key, forwarding), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::types::Instance;

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

    /// Allocates an `Object` on the heap via `Box` and returns a `HeapPtr`.
    /// Leaked intentionally — tests are short-lived.
    fn heap_alloc(obj: Object) -> HeapPtr {
        let ptr = Box::into_raw(Box::new(obj));
        #[cfg(feature = "heap_debug")]
        unsafe {
            HeapPtr::from_ptr(ptr, 0)
        }
        #[cfg(not(feature = "heap_debug"))]
        unsafe {
            HeapPtr::from_ptr(ptr)
        }
    }

    /// Allocates a leaf object (string) on the heap.
    fn leaf() -> HeapPtr {
        heap_alloc(Object::String(String::from("test leaf object")))
    }

    /// Allocates an instance whose fields point to the given objects.
    fn instance(class: HeapPtr, fields: Vec<Value>) -> HeapPtr {
        heap_alloc(Object::Instance(Instance { class, fields }))
    }

    #[test]
    fn test_basic_notify_registration() {
        let mut watch = Watch::new();

        let var = NodeId::LocalVar(StackIndex::from_raw(0));
        watch.register_root(var, test_root_state());

        assert!(watch.roots.contains_key(&var));
        assert_eq!(watch.copy_roots_reaching(var).len(), 1);
    }

    #[test]
    fn test_track_and_unlink() {
        let mut watch = Watch::new();

        let var = NodeId::LocalVar(StackIndex::from_raw(0));
        let obj_ptr = leaf();

        watch.register_root(var, test_root_state());
        track_watch_dependencies(&mut watch, var, Path::Binding, obj_ptr);

        // Both should be covered
        assert_eq!(watch.copy_roots_reaching(var).len(), 1);
        assert_eq!(
            watch.copy_roots_reaching(NodeId::HeapObject(obj_ptr)).len(),
            1
        );

        // Unlink var -> obj
        watch.unlink_edge(var, Path::Binding, NodeId::HeapObject(obj_ptr));

        assert_eq!(watch.copy_roots_reaching(var).len(), 1);
        assert_eq!(
            watch.copy_roots_reaching(NodeId::HeapObject(obj_ptr)).len(),
            0
        );
    }

    #[test]
    fn test_cycle_handling() {
        let mut watch = Watch::new();
        let root_var = NodeId::LocalVar(StackIndex::from_raw(0));
        let class_ptr = leaf();

        // Build cycle: A -> B -> A
        let a = instance(class_ptr, vec![Value::Null]); // placeholder
        let b = instance(class_ptr, vec![Value::Object(a)]);
        // Close the cycle: mutate A's field to point to B.
        unsafe {
            if let Object::Instance(inst) = a.get_mut() {
                inst.fields[0] = Value::Object(b);
            }
        }

        watch.register_root(root_var, test_root_state());
        track_watch_dependencies(&mut watch, root_var, Path::Binding, a);

        // Both A and B should be covered
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(a)).len(), 1);
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(b)).len(), 1);

        // Disconnect root -> A
        watch.unlink_edge(root_var, Path::Binding, NodeId::HeapObject(a));

        // Neither should be covered
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(a)).len(), 0);
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(b)).len(), 0);
    }

    #[test]
    fn test_deep_object_graph() {
        let mut watch = Watch::new();
        let var = NodeId::LocalVar(StackIndex::from_raw(0));

        // Build chain: obj3 (leaf), obj2 -> obj3, obj1 -> obj2
        // Class ptr is a dummy leaf — Instance only needs it for display.
        let class_ptr = leaf();
        let obj3 = leaf();
        let obj2 = instance(class_ptr, vec![Value::Object(obj3)]);
        let obj1 = instance(class_ptr, vec![Value::Object(obj2)]);

        watch.register_root(var, test_root_state());
        track_watch_dependencies(&mut watch, var, Path::Binding, obj1);

        // All should be covered
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(obj1)).len(), 1);
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(obj2)).len(), 1);
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(obj3)).len(), 1);

        // Break the chain in the middle
        watch.unlink_edge(
            NodeId::HeapObject(obj1),
            Path::InstanceField(0),
            NodeId::HeapObject(obj2),
        );

        // obj1 still covered, obj2 and obj3 not
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(obj1)).len(), 1);
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(obj2)).len(), 0);
        assert_eq!(watch.copy_roots_reaching(NodeId::HeapObject(obj3)).len(), 0);
    }

    #[test]
    fn test_multiple_roots() {
        let mut watch = Watch::new();

        let var1 = NodeId::LocalVar(StackIndex::from_raw(0));
        let var2 = NodeId::LocalVar(StackIndex::from_raw(1));
        let obj_ptr = leaf();

        watch.register_root(var1, test_root_state());
        watch.register_root(var2, test_root_state());

        track_watch_dependencies(&mut watch, var1, Path::Binding, obj_ptr);
        track_watch_dependencies(&mut watch, var2, Path::Binding, obj_ptr);

        // Object should be covered by both roots
        assert_eq!(
            watch.copy_roots_reaching(NodeId::HeapObject(obj_ptr)).len(),
            2
        );

        // Unlink one
        watch.unlink_edge(var1, Path::Binding, NodeId::HeapObject(obj_ptr));
        assert_eq!(
            watch.copy_roots_reaching(NodeId::HeapObject(obj_ptr)).len(),
            1
        );

        // Unregister the other
        watch.unregister_root(var2);
        assert_eq!(
            watch.copy_roots_reaching(NodeId::HeapObject(obj_ptr)).len(),
            0
        );
    }
}
