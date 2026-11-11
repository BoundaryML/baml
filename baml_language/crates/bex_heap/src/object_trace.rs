//! The VM `Object` enum as a [`Trace::Rust`](crate::Trace::Rust) cell: how its
//! pointers are found, how it is dropped and how its outside storage is
//! measured. This is the enum-specific tracing the copying collector used,
//! unchanged, behind the descriptor hooks.

use bex_vm_types::{
    HeapPtr, Meter, Object, Value,
    types::{Future, Objects, Slots},
};

/// A `HeapPtr` for an address found in the heap. The epoch is zero, as for
/// pointers read back out of a `Value`.
#[inline]
pub(crate) fn heap_ptr_at(address: usize) -> HeapPtr {
    let ptr = address as *mut Object;
    // SAFETY: only an address; nothing is dereferenced here.
    #[cfg(feature = "heap_debug")]
    unsafe {
        HeapPtr::from_ptr(ptr, 0)
    }
    // SAFETY: only an address; nothing is dereferenced here.
    #[cfg(not(feature = "heap_debug"))]
    unsafe {
        HeapPtr::from_ptr(ptr)
    }
}

/// Extract the inner `HeapPtr` held by a settled future (Ready / Error), if
/// the held value is an object reference. Pending / Cancelled / InternalError
/// futures hold no reachable outgoing references; this returns `None` for
/// those, and for settled futures whose payload is a non-object `Value`.
fn future_object_ref(fut: &Future) -> Option<HeapPtr> {
    use bex_vm_types::FutureRead;
    match fut.read() {
        FutureRead::Ready(v) | FutureRead::Error(v) => v.as_object_ptr(),
        FutureRead::Pending(_) | FutureRead::Cancelled | FutureRead::InternalError(_) => None,
    }
}

/// [`RustHooks::trace`](crate::RustHooks::trace) for `Object`.
///
/// # Safety
/// `payload` points to a live `Object`, and no mutator runs.
pub(crate) unsafe fn trace_object_hook(payload: *const u8, worklist: &mut Vec<HeapPtr>) {
    // SAFETY: forwarded to the caller.
    let object = unsafe { &*payload.cast::<Object>() };
    debug_assert!(
        !matches!(object, Object::Tombstone),
        "the collector reached a tombstone at {payload:?}: a stale pointer, or a \
         reservation dropped or held unpublished across a collection"
    );
    trace_object(object, worklist);
}

/// [`RustHooks::drop`](crate::RustHooks::drop) for `Object`.
///
/// # Safety
/// `payload` points to a dead `Object` nothing will read again.
pub(crate) unsafe fn drop_object_hook(payload: *mut u8) {
    // SAFETY: forwarded to the caller.
    unsafe { std::ptr::drop_in_place(payload.cast::<Object>()) };
}

/// [`RustHooks::measure`](crate::RustHooks::measure) for `Object`.
///
/// # Safety
/// `payload` points to a live `Object` no mutator can reach.
pub(crate) unsafe fn measure_object_hook(payload: *mut u8, meter: &mut Meter) {
    // SAFETY: forwarded to the caller.
    unsafe { &mut *payload.cast::<Object>() }.measure(meter);
}

/// Add object references to the worklist for tracing. Null pointers may be
/// pushed; marking skips them.
pub(crate) fn trace_object(obj: &Object, worklist: &mut Vec<HeapPtr>) {
    trace_variant(obj, worklist);
    // Heads are ordinary references: a type naming a declaration keeps that
    // declaration alive. An unresolved head has no pointer to trace — emit
    // mints those, and the loader binds them before the VM exists.
    bex_vm_types::head_walk::visit_object_heads(obj, &mut |head| {
        if head.is_resolved() {
            worklist.push(head.ptr());
        }
    });
}

/// Push the heap pointer of every object-valued `Value`.
fn push_values(worklist: &mut Vec<HeapPtr>, values: impl IntoIterator<Item = Value>) {
    worklist.extend(values.into_iter().filter_map(|value| value.as_object_ptr()));
}

/// The edges of one object, its type heads aside.
fn trace_variant(obj: &Object, worklist: &mut Vec<HeapPtr>) {
    match obj {
        Object::Closure(closure) => {
            worklist.push(closure.function);
            push_values(worklist, closure.captures.iter().copied());
        }
        Object::BoundMethod(bm) => {
            worklist.push(bm.function);
            push_values(worklist, [bm.receiver]);
        }
        Object::Cell(cell) => push_values(worklist, [cell.load()]),
        Object::Variant(var) => worklist.push(var.enm),
        Object::Future(fut) => worklist.extend(future_object_ref(fut)),
        Object::Package(package) => trace_package(package, worklist),
        Object::Function(function) => trace_function(function, worklist),
        Object::GenericFunction(function) => worklist.push(function.runtime_package),
        // A declaration's own type edges are its heads, traced by
        // `trace_object` for every object kind; what these arms add is the
        // back-edge to the package that owns them, so reaching a member
        // keeps its package — and so its globals and dependencies — alive.
        // A class's inherent method bodies are direct pointers, like an
        // interface's default bodies.
        Object::Class(class) => {
            worklist.extend(class.owner.edges().iter().copied());
            worklist.extend(class.methods.values().map(|method| method.function_ptr));
        }
        Object::Enum(enm) => worklist.extend(enm.owner.edges().iter().copied()),
        // An impl rule points at its interface and at each method body. For
        // the static image those are compile-time pointers, but a rule a
        // runtime package or an anonymous-class witness allocated points at
        // collectible objects, so the edges are traced like any other.
        Object::ImplRule(rule) => {
            worklist.push(rule.interface_head);
            worklist.extend(rule.methods.values().map(|method| method.fqn));
        }
        // Runtime-declared interfaces/aliases back-reference their owning
        // package (null for static declarations), and an interface's
        // default-method bodies are direct pointers.
        Object::Interface(interface) => {
            worklist.push(interface.owner);
            worklist.extend(
                interface
                    .structural_default
                    .as_ref()
                    .map(|d| d.function_ptr),
            );
            worklist.extend(interface.methods.iter().map(|method| method.default_fn));
        }
        Object::TypeAlias(alias) => worklist.push(alias.owner),
        // `HostClosure` carries only an `Arc<HostValueArc>` (Rust-side stub,
        // not a heap object) and a `Box<RuntimeTy>` (no `HeapPtr`s). A `type`
        // value is exactly the type it denotes: every edge it has is a head.
        Object::Tombstone
        | Object::Shared(_)
        | Object::HostClosure(_)
        | Object::String(_)
        | Object::Bigint(_)
        | Object::Uint8Array(_)
        | Object::RustData(_)
        | Object::Type(_)
        | Object::Float(_) => {}
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => {}
    }
}

fn trace_package(package: &bex_vm_types::types::Package, worklist: &mut Vec<HeapPtr>) {
    worklist.extend(package.classes.values().copied());
    worklist.extend(package.enums.values().copied());
    worklist.extend(package.interfaces.values().copied());
    worklist.extend(package.type_aliases.values().copied());
    worklist.extend(package.edges.values().map(|edge| edge.target));
    worklist.extend(package.init);
    worklist.extend(package.test_init);
    for (interface, rules) in &package.impl_rules {
        worklist.push(*interface);
        worklist.extend(rules.iter().copied());
    }
    if let Slots::Own { cells, .. } = &package.slots {
        push_values(worklist, cells.iter().map(|slot| slot.load()));
    }
    if let Objects::Own(objects) = &package.objects {
        worklist.extend(objects.iter().copied());
    }
}

/// A runtime function's owner and constants. A compile-time function (null
/// owner) holds only compile-time constants.
fn trace_function(function: &bex_vm_types::Function, worklist: &mut Vec<HeapPtr>) {
    if function.owner.as_ptr().is_null() {
        return;
    }
    worklist.push(function.owner);
    push_values(
        worklist,
        function.bytecode.resolved_constants.iter().copied(),
    );
}
