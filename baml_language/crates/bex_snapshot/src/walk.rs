//! The object graph walk.
//!
//! Starting from a list of roots, the walk assigns a dense id to every runtime
//! object it reaches, in breadth-first order, and stops at compile-time
//! objects. [`object_edges`] lists the outgoing pointers of one object. It
//! mirrors the collector's `fixup_object_references` (`bex_heap/src/gc.rs`)
//! variant by variant, plus the type heads from
//! `bex_vm_types::head_walk::visit_object_heads`.
//!
//! In strict mode (the snapshot writer) the walk fails at the first object
//! that cannot be restored in another process and reports the path from a
//! root to it. In tolerant mode (the state dump) such objects are recorded as
//! leaves.
#![allow(unsafe_code)]

use std::collections::HashMap;

use bex_heap::BexHeap;
use bex_vm_types::{HeapPtr, Object, TypeHead, Value, head_walk::visit_object_heads};

/// A labelled starting point of the walk.
pub(crate) struct Root {
    pub value: Value,
    /// Human-readable path to the root, outermost element first.
    pub path: Vec<String>,
}

/// Which slot of the parent object an edge leaves from. Rendered to text only
/// when a blocked path is reported.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Edge {
    Field(u32),
    Index(u32),
    MapEntry(u32),
    Capture(u32),
    CellValue,
    FutureValue,
    Class,
    Enum,
    Function,
    Receiver,
    TypeHead,
    RuntimePackage,
}

#[derive(Clone, Copy)]
enum Parent {
    Root(u32),
    Object(u32, Edge),
}

/// The walk could not continue past an object.
pub(crate) struct Blocked {
    pub reason: String,
    pub path: Vec<String>,
}

pub(crate) struct Graph {
    /// Runtime objects in id order.
    pub objects: Vec<HeapPtr>,
    /// Object address to id.
    pub ids: HashMap<HeapPtr, u32>,
    /// Ids of objects that cannot be serialized (tolerant mode only).
    pub opaque: Vec<u32>,
    /// How the walk reached each object, in id order.
    parents: Vec<Parent>,
}

impl Graph {
    /// The path from a root to object `id`, outermost element first, ending
    /// with a description of the object.
    pub(crate) fn path_to(&self, heap: &BexHeap, roots: &[Root], id: u32) -> Vec<String> {
        let mut path = path_to(heap, roots, &self.objects, &self.parents, id);
        // SAFETY: the object was reached by the walk under the walk's contract.
        path.push(describe(unsafe {
            heap.get_object(self.objects[id as usize])
        }));
        path
    }
}

fn path_to(
    heap: &BexHeap,
    roots: &[Root],
    objects: &[HeapPtr],
    parents: &[Parent],
    mut id: u32,
) -> Vec<String> {
    let mut reversed: Vec<String> = Vec::new();
    loop {
        match parents[id as usize] {
            Parent::Root(root) => {
                let mut path = roots[root as usize].path.clone();
                path.extend(reversed.into_iter().rev());
                return path;
            }
            Parent::Object(parent, edge) => {
                reversed.push(render_edge(heap, objects[parent as usize], edge));
                id = parent;
            }
        }
    }
}

/// Short, stable name of an object's kind, used in state dumps.
pub(crate) fn kind_name(object: &Object) -> &'static str {
    match object {
        Object::Package(_) => "package",
        Object::Function(_) => "function",
        Object::Interface(_) => "interface",
        Object::ImplRule(_) => "impl_rule",
        Object::Class(_) => "class",
        Object::Instance(_) => "instance",
        Object::Enum(_) => "enum",
        Object::TypeAlias(_) => "type_alias",
        Object::Variant(_) => "variant",
        Object::Closure(_) => "closure",
        Object::BoundMethod(_) => "bound_method",
        Object::GenericFunction(_) => "generic_function",
        Object::HostClosure(_) => "host_closure",
        Object::Cell(_) => "cell",
        Object::String(_) => "string",
        Object::Bigint(_) => "bigint",
        Object::Uint8Array(_) => "uint8array",
        Object::Array(_) => "array",
        Object::Map(_) => "map",
        Object::Float(_) => "float",
        Object::Future(_) => "future",
        Object::UnscheduledFuture(_) => "unscheduled_future",
        Object::RustData(_) => "rust_data",
        Object::Type(_) => "type",
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => "sentinel",
    }
}

/// A `RustData` payload that a snapshot can rebuild in another process.
#[derive(Clone)]
pub(crate) enum Resource {
    CancelToken(std::sync::Arc<bex_vm_types::CancelTokenData>),
    TaskGroup(std::sync::Arc<bex_vm_types::TaskGroupInner>),
}

impl Resource {
    /// The resource behind a `RustData` payload, or `None` for a payload that
    /// cannot move to another process (a file, a socket, media, ...).
    pub(crate) fn of(data: &bex_vm_types::snapshot_ctx::RustDataRef) -> Option<Self> {
        let data = std::sync::Arc::clone(data);
        match data.downcast::<bex_vm_types::CancelTokenData>() {
            Ok(token) => Some(Self::CancelToken(token)),
            Err(data) => data
                .downcast::<bex_vm_types::TaskGroupInner>()
                .ok()
                .map(Self::TaskGroup),
        }
    }

    pub(crate) fn key(&self) -> usize {
        match self {
            Self::CancelToken(token) => bex_vm_types::snapshot_ctx::resource_key_of(token),
            Self::TaskGroup(group) => bex_vm_types::snapshot_ctx::resource_key_of(group),
        }
    }

    pub(crate) fn into_rust_data(self) -> bex_vm_types::snapshot_ctx::RustDataRef {
        match self {
            Self::CancelToken(token) => token,
            Self::TaskGroup(group) => group,
        }
    }
}

/// Why a runtime object of this kind cannot be written to a snapshot, or
/// `None` when it can.
///
/// The serializable kinds are exactly the ones the loader accepts
/// (`loader::validate_object`).
pub(crate) fn unserializable_reason(object: &Object) -> Option<&'static str> {
    match object {
        Object::Instance(_)
        | Object::Variant(_)
        | Object::Closure(_)
        | Object::BoundMethod(_)
        | Object::Cell(_)
        | Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::Array(_)
        | Object::Map(_)
        | Object::Float(_)
        | Object::Type(_) => None,
        Object::GenericFunction(generic) => (!generic.runtime_package.is_null())
            .then_some("a generic function value of a runtime-compiled package"),
        Object::RustData(data) => Resource::of(data).is_none().then_some(
            "a host resource or opaque Rust value (file, socket, HTTP response, stream, media, ...)",
        ),
        Object::HostClosure(_) => Some("a callback into the host language process"),
        Object::Future(future) => future.to_snapshot().err(),
        Object::UnscheduledFuture(_) => Some("a spawn request that the engine has not scheduled"),
        Object::Function(_) => Some("a function object created at runtime"),
        Object::Package(_) => Some("a package created at runtime"),
        Object::Class(_) | Object::Enum(_) | Object::Interface(_) | Object::TypeAlias(_) => {
            Some("a type declaration created at runtime")
        }
        Object::ImplRule(_) => Some("an interface implementation created at runtime"),
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => Some("a heap debugger sentinel"),
    }
}

/// List the outgoing pointers of a serializable runtime object.
///
/// Null pointers and non-object values are skipped. Unresolved type heads are
/// passed to `on_unresolved_head` instead, because they carry no pointer.
pub(crate) fn object_edges(
    object: &Object,
    out: &mut Vec<(Edge, HeapPtr)>,
    on_unresolved_head: &mut impl FnMut(&TypeHead),
) {
    fn value(out: &mut Vec<(Edge, HeapPtr)>, edge: Edge, value: Value) {
        if let Some(ptr) = value.as_object_ptr() {
            out.push((edge, ptr));
        }
    }
    fn ptr(out: &mut Vec<(Edge, HeapPtr)>, edge: Edge, ptr: HeapPtr) {
        if !ptr.is_null() {
            out.push((edge, ptr));
        }
    }
    fn index(i: usize) -> u32 {
        u32::try_from(i).unwrap_or(u32::MAX)
    }

    match object {
        Object::Array(array) => {
            for (i, item) in array.data.lock().iter().enumerate() {
                value(out, Edge::Index(index(i)), *item);
            }
        }
        Object::Map(map) => {
            for (i, item) in map.data.lock().values().enumerate() {
                value(out, Edge::MapEntry(index(i)), *item);
            }
        }
        Object::Instance(instance) => {
            ptr(out, Edge::Class, instance.class);
            for (i, slot) in instance.fields.iter().enumerate() {
                value(out, Edge::Field(index(i)), slot.load());
            }
        }
        Object::Closure(closure) => {
            ptr(out, Edge::Function, closure.function);
            for (i, capture) in closure.captures.iter().enumerate() {
                value(out, Edge::Capture(index(i)), *capture);
            }
        }
        Object::BoundMethod(bound) => {
            ptr(out, Edge::Function, bound.function);
            value(out, Edge::Receiver, bound.receiver);
        }
        Object::Cell(cell) => value(out, Edge::CellValue, cell.load()),
        Object::Future(future) => {
            use bex_vm_types::types::FutureRead;
            match future.read() {
                FutureRead::Ready(settled) | FutureRead::Error(settled) => {
                    value(out, Edge::FutureValue, settled);
                }
                FutureRead::Pending(_) | FutureRead::Cancelled => {}
                // Not serializable: the walk never descends into it.
                FutureRead::InternalError(_) => return,
            }
        }
        Object::Variant(variant) => ptr(out, Edge::Enum, variant.enm),
        Object::GenericFunction(generic) => {
            ptr(out, Edge::RuntimePackage, generic.runtime_package);
        }
        // No pointer fields. Heads are handled below for every kind.
        Object::Type(_)
        | Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::Float(_) => {}
        // A resource or an opaque value. Neither holds heap pointers.
        Object::RustData(_) => return,
        // Not serializable: the walk never descends into these.
        Object::Package(_)
        | Object::Function(_)
        | Object::Interface(_)
        | Object::ImplRule(_)
        | Object::Class(_)
        | Object::Enum(_)
        | Object::TypeAlias(_)
        | Object::HostClosure(_)
        | Object::UnscheduledFuture(_) => return,
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => return,
    }

    visit_object_heads(object, &mut |head| {
        if head.is_resolved() {
            out.push((Edge::TypeHead, head.ptr()));
        } else {
            on_unresolved_head(head);
        }
    });
}

fn render_edge(heap: &BexHeap, parent: HeapPtr, edge: Edge) -> String {
    // SAFETY: `parent` was reached by the walk under the caller's guarantee
    // that no thread runs and the collector cannot move objects.
    let object = unsafe { heap.get_object(parent) };
    match edge {
        Edge::Field(i) => {
            let name = match object {
                Object::Instance(instance) if !instance.class.is_null() => {
                    // SAFETY: as above; the class is reachable from a live instance.
                    match unsafe { heap.get_object(instance.class) } {
                        Object::Class(class) => {
                            class.fields.get(i as usize).map(|field| field.name.clone())
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            match name {
                Some(name) => format!("field `{name}`"),
                None => format!("field {i}"),
            }
        }
        Edge::Index(i) => format!("[{i}]"),
        Edge::MapEntry(i) => match object {
            Object::Map(map) => map
                .data
                .lock()
                .get_index(i as usize)
                .map_or_else(|| format!("map entry {i}"), |(key, _)| format!("[{key:?}]")),
            _ => format!("map entry {i}"),
        },
        Edge::Capture(i) => format!("captured variable {i}"),
        Edge::CellValue => "value".to_string(),
        Edge::FutureValue => "settled value".to_string(),
        Edge::Class => "class".to_string(),
        Edge::Enum => "enum".to_string(),
        Edge::Function => "function".to_string(),
        Edge::Receiver => "receiver".to_string(),
        Edge::TypeHead => "type".to_string(),
        Edge::RuntimePackage => "runtime package".to_string(),
    }
}

fn describe(object: &Object) -> String {
    let rendered = match object {
        Object::Instance(_) => "<instance>".to_string(),
        // Not the `Display` form: a settled future prints its value, which is
        // a heap address that differs in every process.
        Object::Future(future) => {
            use bex_vm_types::types::FutureRead;
            let state = match future.read() {
                FutureRead::Pending(_) => "pending",
                FutureRead::Ready(_) => "ready",
                FutureRead::Error(_) => "failed",
                FutureRead::Cancelled => "cancelled",
                FutureRead::InternalError(_) => "internal error",
            };
            format!("#{} ({state})", future.id())
        }
        other => other.to_string(),
    };
    let mut rendered: String = rendered.chars().take(60).collect();
    if rendered.is_empty() {
        rendered = "<object>".to_string();
    }
    format!("{} {rendered}", kind_name(object))
}

/// Walk the runtime object graph from `roots`.
///
/// `declared` answers whether an unresolved type head names a compile-time
/// declaration (so that the loader will be able to bind it).
///
/// # Safety contract (not `unsafe`, but required)
///
/// No BAML thread that can reach these objects is executing and the collector
/// cannot run for the duration of the call.
pub(crate) fn walk(
    heap: &BexHeap,
    roots: &[Root],
    tolerant: bool,
    declared: &mut impl FnMut(&TypeHead) -> bool,
) -> Result<Graph, Blocked> {
    let mut graph = Graph {
        objects: Vec::new(),
        ids: HashMap::new(),
        opaque: Vec::new(),
        parents: Vec::new(),
    };
    let mut parents: Vec<Parent> = Vec::new();

    let enqueue = |graph: &mut Graph, parents: &mut Vec<Parent>, ptr: HeapPtr, parent: Parent| {
        if ptr.is_null() || heap.is_compile_time_ptr(ptr) || graph.ids.contains_key(&ptr) {
            return;
        }
        let id = u32::try_from(graph.objects.len()).unwrap_or(u32::MAX);
        graph.ids.insert(ptr, id);
        graph.objects.push(ptr);
        parents.push(parent);
    };

    for (index, root) in roots.iter().enumerate() {
        if let Some(ptr) = root.value.as_object_ptr() {
            let index = u32::try_from(index).unwrap_or(u32::MAX);
            enqueue(&mut graph, &mut parents, ptr, Parent::Root(index));
        }
    }

    let mut edges = Vec::new();
    let mut cursor = 0usize;
    while cursor < graph.objects.len() {
        if graph.objects.len() >= u32::MAX as usize {
            return Err(Blocked {
                reason: "the run reaches more objects than a snapshot can index".to_string(),
                path: Vec::new(),
            });
        }
        let id = u32::try_from(cursor).unwrap_or(u32::MAX);
        let ptr = graph.objects[cursor];
        cursor += 1;
        // SAFETY: see the function docs.
        let object = unsafe { heap.get_object(ptr) };

        if let Some(reason) = unserializable_reason(object) {
            if tolerant {
                graph.opaque.push(id);
                continue;
            }
            let mut path = path_to(heap, roots, &graph.objects, &parents, id);
            path.push(describe(object));
            return Err(Blocked {
                reason: format!("the run holds {reason}"),
                path,
            });
        }

        edges.clear();
        let mut undeclared_head = None;
        object_edges(object, &mut edges, &mut |head| {
            if !declared(head) {
                undeclared_head = Some(head.tag().as_i64());
            }
        });
        if let Some(tag) = undeclared_head
            && !tolerant
        {
            let mut path = path_to(heap, roots, &graph.objects, &parents, id);
            path.push(describe(object));
            return Err(Blocked {
                reason: format!(
                    "a value names a type (tag {tag}) that the program does not declare"
                ),
                path,
            });
        }
        for (edge, child) in edges.drain(..) {
            enqueue(&mut graph, &mut parents, child, Parent::Object(id, edge));
        }
    }
    graph.parents = parents;
    Ok(graph)
}
