//! Every heap edge an [`Object`] holds.
//!
//! An edge is a pointer from one heap object to another: a [`Value`] holding
//! an object, a pointer field such as an instance's class, or a resolved
//! [`TypeHead`](crate::TypeHead) (see [`head_walk`](crate::head_walk)). The
//! collector traces edges to find what is live and rewrites them when what
//! they point at moves; both must see *every* edge, so both forms are expanded
//! from one macro body rather than written twice. A variant that gains an edge
//! adds it here once, and the exhaustive match makes a new variant decide.
//!
//! A null pointer field is "no edge" where the field documents it may be null
//! (a static declaration's owner, an unset method body), and an invariant
//! violation everywhere else.

#![expect(
    unsafe_code,
    reason = "the collector walks objects while every mutator is parked, \
              reading containers past their locks"
)]

use crate::{
    HeapPtr, Object, RustPayload, Value,
    types::{AtomicValueSlot, FutureRead, Objects, Slots},
};

/// Choose the expression for the walk being expanded.
macro_rules! by_mode {
    (visit, visit: $visit:expr, map: $map:expr $(,)?) => {
        $visit
    };
    (map, visit: $visit:expr, map: $map:expr $(,)?) => {
        $map
    };
}

/// Report one edge, given a reference to where it is stored.
///
/// - `ptr`: a pointer field that is never null.
/// - `nullable`: a pointer field where null means no edge.
/// - `value`: a [`Value`], an edge only when it holds an object.
/// - `slot`: an [`AtomicValueSlot`], likewise.
macro_rules! edge {
    (visit, ptr, $f:ident, $place:expr) => {{
        let ptr: HeapPtr = *$place;
        debug_assert!(!ptr.is_null(), "a non-nullable heap edge is null");
        $f(ptr);
    }};
    (map, ptr, $f:ident, $place:expr) => {{
        let place: &mut HeapPtr = $place;
        debug_assert!(!place.is_null(), "a non-nullable heap edge is null");
        *place = $f(*place);
    }};
    (visit, nullable, $f:ident, $place:expr) => {{
        let ptr: HeapPtr = *$place;
        if !ptr.is_null() {
            $f(ptr);
        }
    }};
    (map, nullable, $f:ident, $place:expr) => {{
        let place: &mut HeapPtr = $place;
        if !place.is_null() {
            *place = $f(*place);
        }
    }};
    (visit, value, $f:ident, $place:expr) => {{
        let value: &Value = $place;
        if let Some(ptr) = value.as_object_ptr() {
            $f(ptr);
        }
    }};
    (map, value, $f:ident, $place:expr) => {{
        let place: &mut Value = $place;
        if let Some(ptr) = place.as_object_ptr() {
            *place = Value::object($f(ptr));
        }
    }};
    (visit, slot, $f:ident, $place:expr) => {{
        let slot: &AtomicValueSlot = $place;
        if let Some(ptr) = slot.load().as_object_ptr() {
            $f(ptr);
        }
    }};
    (map, slot, $f:ident, $place:expr) => {{
        let slot: &AtomicValueSlot = &*$place;
        if let Some(ptr) = slot.load().as_object_ptr() {
            slot.store(Value::object($f(ptr)));
        }
    }};
}

/// A declaration's [`Owner`](crate::types::Owner): its package, or the witness rules an anonymous
/// declaration holds.
macro_rules! walk_owner {
    ($mode:ident, $f:ident, $owner:expr) => {{
        let owner = $owner;
        for edge in by_mode!($mode, visit: owner.edges(), map: owner.edges_mut()) {
            edge!($mode, ptr, $f, edge);
        }
    }};
}

macro_rules! walk_object_edges {
    ($mode:ident, $object:ident, $f:ident $(, $mut:tt)?) => {
        match $object {
            Object::Array(array) => {
                let values = by_mode!($mode,
                    // SAFETY: the caller guarantees no mutator is running.
                    visit: unsafe { array.data_unchecked() }.iter(),
                    map: array.data.get_mut().iter_mut(),
                );
                for value in values {
                    edge!($mode, value, $f, value);
                }
            }
            Object::Map(map) => {
                by_mode!($mode,
                    visit: {
                        // SAFETY: the caller guarantees no mutator is running.
                        let data = unsafe { map.data_unchecked() };
                        for value in data.keys().chain(data.values()) {
                            edge!($mode, value, $f, value);
                        }
                    },
                    // Moving a key keeps its hash: the VM hashes by content.
                    map: for value in map.data.get_mut().trace_values_mut() {
                        edge!($mode, value, $f, value);
                    },
                );
            }
            Object::Instance(instance) => {
                edge!($mode, ptr, $f, &$($mut)? instance.class);
                for slot in &instance.fields {
                    edge!($mode, slot, $f, slot);
                }
            }
            Object::RustInstance(instance) => {
                edge!($mode, ptr, $f, &$($mut)? instance.class);
                match &$($mut)? instance.payload {
                    // None of these kinds holds a heap edge. A kind whose
                    // state does (an `OrderedSet`'s members) walks it here.
                    RustPayload::Foreign(_)
                    | RustPayload::DefaultHasher(_)
                    | RustPayload::Sha256(_)
                    | RustPayload::Aes256GcmSiv(_)
                    | RustPayload::Aes128GcmSiv(_)
                    | RustPayload::ChaCha20Poly1305(_)
                    | RustPayload::XChaCha20Poly1305(_)
                    | RustPayload::Regex(_)
                    | RustPayload::CsvRecord(_)
                    | RustPayload::Limit(_) => {}
                }
            }
            Object::Closure(closure) => {
                edge!($mode, ptr, $f, &$($mut)? closure.function);
                for value in &$($mut)? closure.captures[..] {
                    edge!($mode, value, $f, value);
                }
            }
            Object::BoundMethod(method) => {
                edge!($mode, ptr, $f, &$($mut)? method.function);
                edge!($mode, value, $f, &$($mut)? method.receiver);
            }
            Object::Cell(cell) => edge!($mode, slot, $f, &cell.value),
            Object::Variant(variant) => edge!($mode, ptr, $f, &$($mut)? variant.enm),
            // Only a settled future holds a value.
            Object::Future(future) => {
                by_mode!($mode,
                    visit: if let FutureRead::Ready(value) | FutureRead::Error(value) =
                        future.read()
                    {
                        edge!($mode, value, $f, &value);
                    },
                    // SAFETY: the caller guarantees exclusive access to the heap.
                    map: if let Some(value) = unsafe { future.value_mut_for_fixup() } {
                        edge!($mode, value, $f, value);
                    },
                );
            }
            Object::Package(package) => {
                for (_, declaration) in (&$($mut)? package.classes)
                    .into_iter()
                    .chain(&$($mut)? package.enums)
                    .chain(&$($mut)? package.interfaces)
                    .chain(&$($mut)? package.type_aliases)
                {
                    edge!($mode, ptr, $f, declaration);
                }
                for (_, dependency) in &$($mut)? package.edges {
                    edge!($mode, ptr, $f, &$($mut)? dependency.target);
                }
                for init in (&$($mut)? package.init).into_iter().chain(&$($mut)? package.test_init) {
                    edge!($mode, ptr, $f, init);
                }
                by_mode!($mode,
                    visit: for (interface, rules) in &package.impl_rules {
                        edge!($mode, ptr, $f, interface);
                        for rule in rules {
                            edge!($mode, ptr, $f, rule);
                        }
                    },
                    // The interfaces are keys, so moving one rebuilds the map.
                    map: package.impl_rules = std::mem::take(&mut package.impl_rules)
                        .into_iter()
                        .map(|(mut interface, mut rules)| {
                            edge!($mode, ptr, $f, &mut interface);
                            for rule in &mut rules {
                                edge!($mode, ptr, $f, rule);
                            }
                            (interface, rules)
                        })
                        .collect(),
                );
                match &$($mut)? package.slots {
                    Slots::Own { cells, .. } => {
                        for slot in cells.iter() {
                            edge!($mode, slot, $f, slot);
                        }
                    }
                    Slots::Program { .. } => {}
                }
                match &$($mut)? package.objects {
                    Objects::Own(objects) => {
                        for object in &$($mut)? objects[..] {
                            edge!($mode, ptr, $f, object);
                        }
                    }
                    Objects::Program => {}
                }
            }
            // A function with no owner is a static one: its constants are
            // compile-time objects.
            Object::Function(function) => {
                if !function.owner.is_null() {
                    edge!($mode, ptr, $f, &$($mut)? function.owner);
                    for value in &$($mut)? function.bytecode.resolved_constants {
                        edge!($mode, value, $f, value);
                    }
                }
            }
            Object::GenericFunction(function) => {
                edge!($mode, nullable, $f, &$($mut)? function.runtime_package);
            }
            // A declaration's owner is a back-edge to the package that owns
            // it, so reaching a member keeps its package — and so its globals
            // and dependencies — alive. Method bodies are direct pointers.
            Object::Class(class) => {
                walk_owner!($mode, $f, &$($mut)? class.owner);
                for (_, method) in &$($mut)? class.methods {
                    edge!($mode, nullable, $f, &$($mut)? method.function_ptr);
                }
            }
            Object::Enum(enm) => walk_owner!($mode, $f, &$($mut)? enm.owner),
            Object::Interface(interface) => {
                edge!($mode, nullable, $f, &$($mut)? interface.owner);
                if let Some(default) = &$($mut)? interface.structural_default {
                    edge!($mode, nullable, $f, &$($mut)? default.function_ptr);
                }
                for method in &$($mut)? interface.methods {
                    edge!($mode, nullable, $f, &$($mut)? method.default_fn);
                }
            }
            Object::TypeAlias(alias) => edge!($mode, nullable, $f, &$($mut)? alias.owner),
            // An impl rule points at its interface and at each method body.
            // For the static image those are compile-time pointers, but a rule
            // a runtime package or an anonymous-class witness allocated points
            // at moving objects.
            Object::ImplRule(rule) => {
                edge!($mode, ptr, $f, &$($mut)? rule.interface_head);
                for (_, method) in &$($mut)? rule.methods {
                    edge!($mode, ptr, $f, &$($mut)? method.fqn);
                }
            }
            // A `type` value's edges are all heads, walked below. A host
            // closure holds an `Arc` into the host and types, nothing on the
            // heap.
            Object::Type(_)
            | Object::HostClosure(_)
            | Object::String(_)
            | Object::Bigint(_)
            | Object::Uint8Array(_)
            | Object::RustData(_)
            | Object::Float(_)
            | Object::Tombstone => {}
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => {}
        }

        // Heads are ordinary references: a type naming a declaration keeps
        // that declaration alive. An unresolved head has no pointer yet — emit
        // mints those, and the loader binds them before the VM exists.
        by_mode!($mode,
            visit: crate::head_walk::visit_object_heads($object, &mut |head| {
                if head.is_resolved() {
                    $f(head.ptr());
                }
            }),
            // Repointing changes only the access path, never the identity, so
            // nothing rehashes or reorders.
            map: crate::head_walk::visit_object_heads_mut($object, &mut |head| {
                if head.is_resolved() {
                    let moved = $f(head.ptr());
                    if moved != head.ptr() {
                        head.forward_to(moved);
                    }
                }
            }),
        );
    };
}

/// Call `f` on every heap edge `object` holds, its resolved type heads
/// included.
///
/// # Safety
///
/// No mutator may run while this does: containers are read without taking
/// their lock. The collector's stop-the-world phase is such a time.
pub unsafe fn visit_object_edges(object: &Object, f: &mut impl FnMut(HeapPtr)) {
    walk_object_edges!(visit, object, f);
}

/// Replace every heap edge `object` holds, its resolved type heads included,
/// with `f` of it. `f` sees exactly the edges [`visit_object_edges`] reports.
///
/// # Safety
///
/// The caller must have exclusive access to the heap, as the collector does
/// while every VM is parked: a settled future's value is rewritten without
/// synchronization.
pub unsafe fn map_object_edges(object: &mut Object, f: &mut impl FnMut(HeapPtr) -> HeapPtr) {
    walk_object_edges!(map, object, f, mut);
}
