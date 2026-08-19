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
        if vm.heap.is_compile_time_ptr(ptr) || found.contains(&ptr) {
            continue;
        }
        found.push(ptr);
        let mut next = Vec::new();
        bex_vm_types::head_walk::visit_object_heads(vm.get_object(ptr), &mut |head| {
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
pub fn runtime_nominals(
    vm: &BexVm,
    ty: &bex_vm_types::RealizedTy,
) -> (Vec<HeapPtr>, Vec<HeapPtr>) {
    let mut classes = Vec::new();
    let mut enums = Vec::new();
    for ptr in runtime_definitions(vm, ty) {
        match vm.get_object(ptr) {
            Object::Class(_) => classes.push(ptr),
            Object::Enum(_) => enums.push(ptr),
            _ => {}
        }
    }
    (classes, enums)
}
