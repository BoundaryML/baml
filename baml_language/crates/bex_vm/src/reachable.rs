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
/// Decided by tag range: a compiled declaration's tag is its object index
/// above `CLASS_BASE`, and every runtime-created one — a typebuilder declaration,
/// *and* a runtime-compiled package member, which is reminted at graft —
/// comes from the counter range above `DYNAMIC_BASE`. So this is an
/// integer compare that touches neither the heap nor a pointer, which is what
/// makes it stable: tags never change, addresses move under the collector, and
/// an unresolved head still answers correctly.
///
/// Only the type's own heads are inspected, not the declarations behind them.
/// That is exact rather than approximate: the static image never references a
/// runtime declaration (the collector relies on the same invariant to skip the
/// compile-time region), so a compile-time head cannot reach a runtime one.
/// A runtime declaration reached *through type arguments* — `Box<RuntimeFoo>`
/// instantiating a compiled generic — is a head of `ty` itself and is seen.
#[must_use]
pub fn is_statically_declared(ty: &bex_vm_types::RealizedTy) -> bool {
    let mut all_static = true;
    ty.visit_heads(&mut |head| {
        all_static &= !head.tag().is_dynamic();
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
/// holding a [`BexVm`].
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

/// Every declaration `ty` reaches when each reached declaration leads on to the
/// heads `follow` reports for it, in the order a depth-first walk meets them:
/// the heads of `ty`, then each declaration's followed heads.
#[must_use]
pub fn declarations_reached(
    vm: &BexVm,
    ty: &bex_vm_types::RealizedTy,
    follow: impl Fn(&Object, &mut dyn FnMut(&bex_vm_types::TypeHead)),
) -> Vec<HeapPtr> {
    let mut found = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut pending = Vec::new();
    ty.visit_heads(&mut |head| {
        if head.is_resolved() {
            pending.push(head.ptr());
        }
    });
    pending.reverse();
    while let Some(ptr) = pending.pop() {
        if !seen.insert(ptr) {
            continue;
        }
        found.push(ptr);
        let mut next = Vec::new();
        follow(vm.get_object(ptr), &mut |head| {
            if head.is_resolved() {
                next.push(head.ptr());
            }
        });
        next.reverse();
        pending.extend(next);
    }
    found
}

/// Every declaration `ty` reaches (classes, enums, interfaces, type aliases),
/// including compile-time ones, through every head a declaration holds (field
/// types, templates, alias bodies, ...).
#[must_use]
pub fn all_declarations(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> Vec<HeapPtr> {
    declarations_reached(vm, ty, |object, visit| {
        bex_vm_types::head_walk::visit_object_heads(object, &mut |head| visit(head));
    })
}

/// Every class and enum `ty` reaches, including compile-time declarations.
///
/// Most runtime consumers intentionally stop at the immutable program image,
/// but source rendering must emit a standalone graph: a runtime class field
/// that names a static class needs that static declaration in the output too.
#[must_use]
pub fn all_nominals(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> (Vec<HeapPtr>, Vec<HeapPtr>) {
    let mut classes = Vec::new();
    let mut enums = Vec::new();
    for ptr in all_declarations(vm, ty) {
        match vm.get_object(ptr) {
            Object::Class(_) => classes.push(ptr),
            Object::Enum(_) => enums.push(ptr),
            _ => {}
        }
    }
    (classes, enums)
}

/// Where a declaration a type reaches is defined: in a package, or nowhere —
/// an anonymous declaration, standing for itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reached {
    Package(HeapPtr),
    Anonymous(HeapPtr),
}

/// Where the declaration at `declaration` (whose object is `object`) is
/// defined, if it is one.
#[must_use]
pub fn declared_in(object: &Object, declaration: HeapPtr) -> Option<Reached> {
    let in_package = |package: HeapPtr| {
        debug_assert!(
            !package.is_null(),
            "a loaded declaration's package is assigned"
        );
        Reached::Package(package)
    };
    match object {
        Object::Class(class) => Some(match class.owner.package() {
            Some(package) => in_package(package),
            None => Reached::Anonymous(declaration),
        }),
        Object::Enum(enm) => Some(match enm.owner.package() {
            Some(package) => in_package(package),
            None => Reached::Anonymous(declaration),
        }),
        Object::Interface(interface) => Some(in_package(interface.owner)),
        Object::TypeAlias(alias) => Some(in_package(alias.owner)),
        _ => None,
    }
}

/// Where the declarations at `declarations` are defined, deduplicated in
/// first-visit order. A resolved head points at a declaration; anything else
/// is an internal error.
#[must_use]
pub fn defined_in(vm: &BexVm, declarations: impl IntoIterator<Item = HeapPtr>) -> Vec<Reached> {
    let mut reached = Vec::new();
    for declaration in declarations {
        let where_ = declared_in(vm.get_object(declaration), declaration)
            .unwrap_or_else(|| unreachable!("a resolved head points at a declaration"));
        if !reached.contains(&where_) {
            reached.push(where_);
        }
    }
    reached
}

/// Where the declarations `ty` names are defined: its own heads', and every
/// head each runtime declaration it reaches names in turn — a compiled
/// declaration's references are its own package's business, recorded in that
/// package's interface. First-visit order, deduplicated.
#[must_use]
pub fn reached(vm: &BexVm, ty: &bex_vm_types::RealizedTy) -> Vec<Reached> {
    let mut heads = Vec::new();
    ty.visit_heads(&mut |head| {
        if head.is_resolved() {
            heads.push(head.ptr());
        }
    });
    for declaration in runtime_definitions(vm, ty) {
        bex_vm_types::head_walk::visit_object_heads(vm.get_object(declaration), &mut |head| {
            if head.is_resolved() {
                heads.push(head.ptr());
            }
        });
    }
    defined_in(vm, heads)
}
