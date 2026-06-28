//! Loading the per-package program structure onto the heap.
//!
//! The compiled `Program` carries packages as a global-index-keyed
//! [`ProgramPackage`] map (it must be `HeapPtr`-free — there is no heap at emit
//! time, and a `HeapPtr` can't serialize). At load we allocate the heap
//! `Object::Package` / `Object::ImplRule` objects from it and build the
//! `vm.packages` index (package name → its `Object::Package` pointer).
//!
//! These are *cross-referencing compile-time objects* — their `HeapPtr` fields
//! point at interfaces / functions / classes / other packages — and a
//! compile-time `HeapPtr` only exists once the compile-time `Vec` is laid out.
//! So we use the two-phase heap construction ([`BexHeap::build_unsealed`]):
//! reserve a placeholder slot per impl rule and per package, build the heap
//! unsealed, fill each slot with its resolved pointers, then seal.

use std::sync::Arc;

use baml_type::Name;
use bex_heap::BexHeap;
use bex_vm_types::{
    HeapPtr, Object, ObjectIndex,
    types::{LocalName, MethodImpl, Package, ProgramImplRule, ProgramPackage, RuntimeImplRule},
};
use indexmap::IndexMap;

/// The compile-time object slots a package and its impl rules were reserved at,
/// so the fill pass can resolve them to `HeapPtr`s.
struct PackageSlots {
    package_slot: usize,
    /// Parallel to [`ProgramPackage::impl_rules`]: per implemented-interface
    /// index, the slot of each of its rules (same order as the rule vec).
    impl_rule_slots: IndexMap<ObjectIndex, Vec<usize>>,
}

/// A throwaway placeholder for a reserved slot; overwritten before the heap is
/// sealed, so its contents are never observed.
fn placeholder() -> Object {
    Object::ImplRule(Box::new(RuntimeImplRule {
        interface_head: HeapPtr::null(),
        for_ty_pattern: baml_type::TyTemplate::TypeArgRef(0),
        generic_param_bounds: Vec::new(),
        interface_args: Vec::new(),
        interface_assoc: Vec::new(),
        methods: IndexMap::new(),
    }))
}

/// Reserve one placeholder slot per impl rule and one per package, appended to
/// `compile_time_objects`. Returns where each was placed. Appending never shifts
/// the existing emit object indices, so the `ObjectIndex`es baked into
/// `ProgramPackage` stay valid.
fn reserve_package_slots(
    compile_time_objects: &mut Vec<Object>,
    packages: &IndexMap<Name, ProgramPackage>,
) -> IndexMap<Name, PackageSlots> {
    let mut layout = IndexMap::new();
    for (name, pkg) in packages {
        let mut impl_rule_slots = IndexMap::new();
        for (iface_idx, rules) in &pkg.impl_rules {
            let slots: Vec<usize> = rules
                .iter()
                .map(|_| {
                    let slot = compile_time_objects.len();
                    compile_time_objects.push(placeholder());
                    slot
                })
                .collect();
            impl_rule_slots.insert(*iface_idx, slots);
        }
        let package_slot = compile_time_objects.len();
        compile_time_objects.push(placeholder());
        layout.insert(
            name.clone(),
            PackageSlots {
                package_slot,
                impl_rule_slots,
            },
        );
    }
    layout
}

/// Resolve a `LocalName → ObjectIndex` member map to `LocalName → HeapPtr`.
fn resolve_members(
    heap: &BexHeap,
    members: &IndexMap<LocalName, ObjectIndex>,
) -> IndexMap<LocalName, HeapPtr> {
    members
        .iter()
        .map(|(ln, idx)| (ln.clone(), heap.compile_time_ptr(idx.into_raw())))
        .collect()
}

/// Build the resolved [`RuntimeImplRule`] for a single [`ProgramImplRule`],
/// turning its `interface_head` / method `fqn` indices into compile-time
/// pointers.
fn resolve_impl_rule(heap: &BexHeap, rule: &ProgramImplRule) -> RuntimeImplRule {
    RuntimeImplRule {
        interface_head: heap.compile_time_ptr(rule.interface_head.into_raw()),
        for_ty_pattern: rule.for_ty_pattern.clone(),
        generic_param_bounds: rule.generic_param_bounds.clone(),
        interface_args: rule.interface_args.clone(),
        interface_assoc: rule.interface_assoc.clone(),
        methods: rule
            .methods
            .iter()
            .map(|(n, m)| {
                (
                    n.clone(),
                    MethodImpl {
                        fqn: heap.compile_time_ptr(m.fqn.into_raw()),
                        frame: m.frame.clone(),
                    },
                )
            })
            .collect(),
    }
}

/// Fill every reserved slot with its resolved object, returning the `vm.packages`
/// index (package name → `Object::Package` pointer).
fn fill_package_slots(
    heap: &mut BexHeap,
    packages: &IndexMap<Name, ProgramPackage>,
    layout: &IndexMap<Name, PackageSlots>,
) -> IndexMap<Name, HeapPtr> {
    let mut vm_packages = IndexMap::new();
    for (name, pkg) in packages {
        let slots = &layout[name];
        // Impl rules first: a package's `impl_rules` map points at their slots.
        let mut impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>> = IndexMap::new();
        for (iface_idx, rules) in &pkg.impl_rules {
            let iface_ptr = heap.compile_time_ptr(iface_idx.into_raw());
            let rule_slots = &slots.impl_rule_slots[iface_idx];
            let mut rule_ptrs = Vec::with_capacity(rules.len());
            for (rule, &slot) in rules.iter().zip(rule_slots) {
                let resolved = resolve_impl_rule(heap, rule);
                heap.set_compile_time_object(slot, Object::ImplRule(Box::new(resolved)));
                rule_ptrs.push(heap.compile_time_ptr(slot));
            }
            impl_rules.insert(iface_ptr, rule_ptrs);
        }
        let package = Package {
            // Dependency pointers are not populated yet (no consumer needs them).
            dependencies: Vec::new(),
            classes: resolve_members(heap, &pkg.classes),
            enums: resolve_members(heap, &pkg.enums),
            functions: resolve_members(heap, &pkg.functions),
            interfaces: resolve_members(heap, &pkg.interfaces),
            impl_rules,
            recursive_type_aliases: pkg.recursive_type_aliases.clone(),
        };
        heap.set_compile_time_object(slots.package_slot, Object::Package(Box::new(package)));
        vm_packages.insert(name.clone(), heap.compile_time_ptr(slots.package_slot));
    }
    vm_packages
}

/// Flatten every package's recursive type aliases into one `TypeName → RuntimeTy`
/// map (the shape `SysOpContext::type_alias_definitions` wants for output-format
/// rendering), reconstructing each qualified name from its package + `LocalName`.
pub fn all_recursive_type_aliases(
    packages: &IndexMap<Name, HeapPtr>,
) -> IndexMap<baml_type::TypeName, baml_type::RuntimeTy> {
    let mut out = IndexMap::new();
    for (pkg_name, &pkg_ptr) in packages {
        // SAFETY: `packages` only ever holds compile-time `Object::Package`
        // pointers (built by `fill_package_slots`), valid for the heap's lifetime.
        #[expect(unsafe_code, reason = "deref a compile-time package pointer")]
        let object = unsafe { pkg_ptr.get() };
        let Some(package) = object.as_package() else {
            continue;
        };
        for (local, ty) in &package.recursive_type_aliases {
            let qtn = baml_type::TypeName::new(
                pkg_name.clone(),
                local.namespace.clone(),
                local.name.clone(),
            );
            out.insert(qtn, ty.clone());
        }
    }
    out
}

/// Build the unified heap from `compile_time_objects`, additionally allocating
/// the per-package `Object::Package` / `Object::ImplRule` objects and returning
/// the `vm.packages` index. The heap is sealed on return.
pub fn build_heap_with_packages(
    mut compile_time_objects: Vec<Object>,
    packages: &IndexMap<Name, ProgramPackage>,
) -> (Arc<BexHeap>, IndexMap<Name, HeapPtr>) {
    let layout = reserve_package_slots(&mut compile_time_objects, packages);
    let mut heap = BexHeap::build_unsealed_default(compile_time_objects);
    let vm_packages = fill_package_slots(&mut heap, packages, &layout);
    (heap.seal(), vm_packages)
}
