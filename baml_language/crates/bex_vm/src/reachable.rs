//! The declarations a type reaches.
//!
//! A type's heads point at the declarations that give it meaning, and each of
//! those declarations carries types with heads of its own. So "which
//! declarations does this type depend on" is a question the type already
//! answers, by construction — which is why the `defs` table that used to ride
//! alongside every `type` value is gone. A table is a second answer to the
//! same question, and two answers can disagree; the graph cannot.

use bex_vm_types::{HeapPtr, Object};

use crate::BexVm;

/// Whether every declaration `ty` names was compiled into the program.
///
/// Only the type's own heads are inspected, not the declarations behind them.
/// That is exact rather than approximate: the static image never references a
/// runtime declaration (the collector relies on the same invariant to skip the
/// compile-time region), so a compile-time head cannot reach a runtime one.
/// A runtime declaration reached *through type arguments* — `Box<RuntimeFoo>`
/// instantiating a compiled generic — is a head of `ty` itself and is seen.
#[must_use]
pub fn is_statically_declared(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> bool {
    let mut all_static = true;
    ty.visit_heads(&mut |head| {
        all_static &= head.is_resolved() && vm.heap.is_compile_time_ptr(head.ptr());
    });
    all_static
}

/// Every runtime declaration `ty` reaches, transitively, in first-visit order.
///
/// Compile-time declarations are skipped: they are findable from the program
/// index and never move, so no consumer needs them enumerated — and stopping
/// there is what keeps the walk proportional to the runtime graph rather than
/// to the whole program. The order is deterministic so callers that render or
/// export it produce stable output.
#[must_use]
pub fn runtime_definitions(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> Vec<HeapPtr> {
    runtime_definitions_under_permit(&vm.heap, ty, vm.proof())
}

/// [`runtime_definitions`] for a caller that holds the heap permit without
/// holding a [`BexVm`] — the engine's host-conversion layer.
///
/// This is the primitive: the walk is a heap operation, and `&BexVm` is just a
/// place the permit is already implied.
#[must_use]
pub fn runtime_definitions_under_permit(
    heap: &bex_heap::BexHeap,
    ty: &bex_vm_types::RealizedTy,
    _permit: bex_heap::PermitProof<'_>,
) -> Vec<HeapPtr> {
    let mut found = Vec::new();
    let mut pending = Vec::new();
    ty.visit_heads(&mut |head| {
        if head.is_resolved() {
            pending.push(head.ptr());
        }
    });
    // `pending` is a stack, so reverse it to keep first-visit order.
    pending.reverse();
    while let Some(ptr) = pending.pop() {
        if heap.is_compile_time_ptr(ptr) || found.contains(&ptr) {
            continue;
        }
        found.push(ptr);
        let mut next = Vec::new();
        // SAFETY: the permit is held for the whole walk, so no collection can
        // move or free a declaration between reaching it and reading it.
        #[expect(unsafe_code, reason = "reading a declaration under a held permit")]
        let object = unsafe { ptr.get() };
        bex_vm_types::head_walk::visit_object_heads(object, &mut |head| {
            if head.is_resolved() {
                next.push(head.ptr());
            }
        });
        next.reverse();
        pending.extend(next);
    }
    found
}

/// The runtime classes and enums `ty` reaches, split by kind.
///
/// The two collections consumers actually want out of [`runtime_definitions`]:
/// reflection renders both, and the sys-op schema overlay describes both.
/// Interfaces, aliases and functions are reachable too but no consumer
/// enumerates them, so they are simply not projected here.
#[must_use]
pub fn runtime_nominals(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> (Vec<HeapPtr>, Vec<HeapPtr>) {
    runtime_nominals_under_permit(&vm.heap, ty, vm.proof())
}

/// [`runtime_nominals`] for a permit-holding caller without a [`BexVm`].
#[must_use]
pub fn runtime_nominals_under_permit(
    heap: &bex_heap::BexHeap,
    ty: &bex_vm_types::RealizedTy,
    permit: bex_heap::PermitProof<'_>,
) -> (Vec<HeapPtr>, Vec<HeapPtr>) {
    let mut classes = Vec::new();
    let mut enums = Vec::new();
    for ptr in runtime_definitions_under_permit(heap, ty, permit) {
        // SAFETY: as in `runtime_definitions_under_permit`.
        #[expect(unsafe_code, reason = "reading a declaration under a held permit")]
        match unsafe { ptr.get() } {
            Object::Class(_) => classes.push(ptr),
            Object::Enum(_) => enums.push(ptr),
            _ => {}
        }
    }
    (classes, enums)
}
