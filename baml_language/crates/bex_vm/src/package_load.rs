//! Loading the per-package program structure onto the heap.
//!
//! The compiled `Program` carries packages as a global-index-keyed
//! [`ProgramPackage`] map (it must be `HeapPtr`-free — there is no heap at emit
//! time, and a `HeapPtr` can't serialize). At load we allocate the heap
//! `Object::Package` / `Object::ImplRule` objects from it and build the
//! [`PackageIndex`] (package name → its `Object::Package` pointer, plus the
//! program-wide interface → impl-rules index).
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
        field_links: Box::default(),
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

/// The loaded program's package tables: every package by name, plus the
/// program-wide impl-rule index derived from them.
///
/// The two are built together and kept together because the second is a *view*
/// over the first — pairing an index with any other package set would answer
/// membership for the wrong program. Both fields are private and only the load
/// pass ([`build_heap_with_packages`]) constructs a populated value, so they
/// cannot diverge.
///
/// # Why the impl-rule index is program-wide
///
/// An `implement I for T` is baked into the package whose *source* declares it,
/// which by the orphan rule (RFC-2451 covered; `orphan_check` in the compiler's
/// `interfaces::impl_rules`) need be neither `T`'s package nor `I`'s: a package
/// may anchor the impl on any local type among `[T, I's args…]`, so
/// `implement baml.ops.Add<Meters> for int` legally lives in `Meters`' package.
/// Deriving a narrower search set at query time means re-deriving the orphan
/// rule at runtime and drifting from it — the compiler proves membership the
/// checker accepts, and the VM then fails to find the rule. Indexing every
/// package's rules once, at load, makes that whole failure class
/// unrepresentable: resolution asks one question of one complete table.
#[derive(Default)]
pub struct PackageIndex {
    /// Package name → its `Object::Package` pointer.
    by_name: IndexMap<Name, HeapPtr>,
    /// Canonical `Object::Interface` pointer → every `Object::ImplRule` of that
    /// interface in the program, in package-load order.
    impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>>,
}

impl PackageIndex {
    /// The `Object::Package` pointer for `name`, if the package is loaded.
    pub fn package_ptr(&self, name: &Name) -> Option<HeapPtr> {
        self.by_name.get(name).copied()
    }

    /// Every loaded package, by name.
    pub fn iter(&self) -> impl Iterator<Item = (&Name, HeapPtr)> + '_ {
        self.by_name.iter().map(|(name, &ptr)| (name, ptr))
    }

    /// Every impl rule of the interface at `iface_ptr`, across all packages.
    /// Empty for an interface nothing implements.
    pub fn impl_rules_of(&self, iface_ptr: HeapPtr) -> &[HeapPtr] {
        self.impl_rules
            .get(&iface_ptr)
            .map_or(&[], |rules| rules.as_slice())
    }
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
        field_links: rule.field_links.clone(),
    }
}

/// Fill every reserved slot with its resolved object, returning the
/// [`PackageIndex`].
fn fill_package_slots(
    heap: &mut BexHeap,
    packages: &IndexMap<Name, ProgramPackage>,
    layout: &IndexMap<Name, PackageSlots>,
) -> PackageIndex {
    let mut index = PackageIndex::default();
    for (name, pkg) in packages {
        let slots = &layout[name];
        // Impl rules first: a package's `impl_rules` map points at their slots.
        let mut impl_rules: IndexMap<HeapPtr, Vec<HeapPtr>> = IndexMap::new();
        for (iface_idx, rules) in &pkg.impl_rules {
            let iface_ptr = heap.compile_time_ptr(iface_idx.into_raw());
            let rule_slots = &slots.impl_rule_slots[iface_idx];
            // The reserve pass allocated one slot per rule from this same map, so
            // the counts must match; a mismatch would leave a reserved slot as a
            // never-overwritten `HeapPtr::null()` placeholder (UB on first deref).
            debug_assert_eq!(
                rules.len(),
                rule_slots.len(),
                "impl-rule reserve/fill count mismatch for an interface",
            );
            let mut rule_ptrs = Vec::with_capacity(rules.len());
            for (rule, &slot) in rules.iter().zip(rule_slots) {
                let resolved = resolve_impl_rule(heap, rule);
                heap.set_compile_time_object(slot, Object::ImplRule(Box::new(resolved)));
                rule_ptrs.push(heap.compile_time_ptr(slot));
            }
            // Mirror the package's rules into the program-wide index as they are
            // resolved. Interfaces are keyed by their canonical pointer, so rules
            // for one interface written across several packages accumulate under
            // the same entry (see [`PackageIndex`] for why the runtime needs the
            // union rather than a per-package view).
            index
                .impl_rules
                .entry(iface_ptr)
                .or_default()
                .extend(rule_ptrs.iter().copied());
            impl_rules.insert(iface_ptr, rule_ptrs);
        }
        let package = Package {
            classes: resolve_members(heap, &pkg.classes),
            enums: resolve_members(heap, &pkg.enums),
            interfaces: resolve_members(heap, &pkg.interfaces),
            impl_rules,
            recursive_type_aliases: pkg.recursive_type_aliases.clone(),
        };
        heap.set_compile_time_object(slots.package_slot, Object::Package(Box::new(package)));
        index
            .by_name
            .insert(name.clone(), heap.compile_time_ptr(slots.package_slot));
    }
    index
}

/// Look up a class or enum object pointer by its fully-qualified dotted name —
/// package as the leading segment — through the package index. The free-function
/// form of [`crate::vm::BexVm::lookup_type_by_fqn`], usable before a `BexVm`
/// exists (e.g. to pre-resolve builtin error/panic classes in `BexVm::new`).
pub fn lookup_type_by_fqn(packages: &PackageIndex, fqn: &str) -> Option<HeapPtr> {
    let mut parts: Vec<Name> = fqn.split('.').map(Name::new).collect();
    let name = parts.pop()?;
    if parts.is_empty() {
        return None;
    }
    let pkg = parts.remove(0);
    let pkg_ptr = packages.package_ptr(&pkg)?;
    // SAFETY: `packages` only ever holds compile-time `Object::Package` pointers.
    #[expect(unsafe_code, reason = "deref a compile-time package pointer")]
    let package = (unsafe { pkg_ptr.get() }).as_package()?;
    let local = LocalName {
        namespace: parts,
        name,
    };
    package
        .classes
        .get(&local)
        .or_else(|| package.enums.get(&local))
        .copied()
}

/// Flatten every package's recursive type aliases into one `TypeName → RuntimeTy`
/// map (the shape `SysOpContext::type_alias_definitions` wants for output-format
/// rendering), reconstructing each qualified name from its package + `LocalName`.
pub fn all_recursive_type_aliases(
    packages: &PackageIndex,
) -> IndexMap<baml_type::TypeName, baml_type::RuntimeTy> {
    let mut out = IndexMap::new();
    for (pkg_name, pkg_ptr) in packages.iter() {
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
/// the [`PackageIndex`]. The heap is sealed on return.
pub fn build_heap_with_packages(
    mut compile_time_objects: Vec<Object>,
    packages: &IndexMap<Name, ProgramPackage>,
) -> (Arc<BexHeap>, PackageIndex) {
    let layout = reserve_package_slots(&mut compile_time_objects, packages);
    let mut heap = BexHeap::build_unsealed_default(compile_time_objects);
    let index = fill_package_slots(&mut heap, packages, &layout);
    (heap.seal(), index)
}
