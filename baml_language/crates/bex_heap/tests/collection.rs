//! Tests the copying collector's tracing and forwarding of every object kind,
//! its stats, and multi-cycle behavior.
//!
//! These tests drive `collect_garbage` directly and are placed in the `tests/`
//! directory so they can exercise cross-module public APIs.

use std::sync::Arc;

use baml_type::Name;
use bex_heap::{BexHeap, GcStats, Generation, Tlab};
use bex_vm_types::{
    Class, ClassField, ForwardRoots, GenericFunction, GlobalIndex, HeapPtr, Object, RealizedTy,
    RootVisitor, RustInstance, RustPayload, TestRustData, Value, Weak,
    types::{
        Edge, EdgeKind, ExportSurface, InterfaceDef, LocalName, MethodImpl, Objects, Package,
        PackageKind, RuntimeImplRule, Slots, TypeAliasDef, TypeValue,
    },
};
use indexmap::IndexMap;

// ============================================================================
// Survival and Reclamation Tests
// ============================================================================

/// A runtime package owns its objects — here a type value naming its class,
/// a generic value, and the class, which carries a back-edge to the package.
/// The moving collector must preserve and fix up that cycle when rooted, and
/// reclaim the whole cycle when dropped.
#[test]
fn runtime_package_object_cycle_survives_when_rooted_and_collects_when_dropped() {
    fn alloc_cycle(heap: &Arc<BexHeap>) -> (Tlab, bex_vm_types::HeapPtr, bex_vm_types::HeapPtr) {
        let mut tlab = Tlab::new(Arc::clone(heap));
        let package = Package {
            name: Name::default(),
            kind: PackageKind::Ordinary,
            edges: IndexMap::new(),
            classes: IndexMap::new(),
            enums: IndexMap::new(),
            interfaces: IndexMap::new(),
            impl_rules: IndexMap::new(),
            type_aliases: IndexMap::new(),
            globals: indexmap::IndexMap::new(),
            slots: Slots::Own {
                cells: Box::new([]),
                initialized: true,
            },
            objects: Objects::Own(Box::new([])),
            surface: ExportSurface::Projected,
            init: None,
            test_init: None,
            diagnostics: Vec::new(),
            session: None,
        };
        let package_ptr = tlab.alloc(Object::Package(Box::new(package)));
        let class_name = bex_vm_types::types::LocalName::new(Vec::new(), Name::new("RuntimeClass"));
        let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
            path: class_name.clone(),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            stream_done: false,
            declared_crossing: baml_type::ClassCrossing::Fields,
            type_tag,
            cleanup: None,
            methods: IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::Package(package_ptr),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
        })));
        let ty = RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, type_tag),
            Box::new([]),
        );
        let type_ptr = tlab.alloc_type(TypeValue::new(ty));
        let function_ptr = tlab.alloc(Object::GenericFunction(GenericFunction {
            function: GlobalIndex::from_raw(0),
            type_args: Box::new([]),
            runtime_package: package_ptr,
        }));
        // SAFETY: this TLAB exclusively owns both fresh Gen0 objects and no GC
        // can run concurrently in this single-threaded test.
        let Object::Package(package) = (unsafe { package_ptr.get_mut() }) else {
            unreachable!()
        };
        package.classes.insert(
            LocalName {
                namespace: Vec::new(),
                name: Name::new("RuntimeClass"),
            },
            class_ptr,
        );
        package.objects = Objects::Own(vec![type_ptr, function_ptr, class_ptr].into_boxed_slice());
        (tlab, package_ptr, function_ptr)
    }

    let rooted_heap = BexHeap::new(vec![]);
    let (mut rooted_tlab, package_ptr, function_ptr) = alloc_cycle(&rooted_heap);
    let (stats, roots, forwarding) = unsafe { rooted_heap.collect_garbage(&[function_ptr]) };
    rooted_tlab.invalidate();
    assert_eq!(stats.live_count, 4);
    let moved_function = roots[0];
    let Object::GenericFunction(function) = (unsafe { moved_function.get() }) else {
        panic!("root ceased to be a generic function")
    };
    let moved_package = function.runtime_package;
    let Object::Package(package) = (unsafe { moved_package.get() }) else {
        panic!("root ceased to be a package")
    };
    let moved_class = package.classes.values().next().copied().unwrap();
    let moved_type = package.objects.own().unwrap()[0];
    let Object::Type(type_value) = (unsafe { moved_type.get() }) else {
        panic!("package type value ceased to be a type")
    };
    let RealizedTy::Class(head, _) = &type_value.ty else {
        panic!("package type value ceased to wrap a class type")
    };
    assert_eq!(
        head.ptr(),
        moved_class,
        "the type's head must follow its declaration"
    );
    let Object::Class(class) = (unsafe { moved_class.get() }) else {
        panic!("package class ceased to be a class")
    };
    assert_eq!(class.owner.package(), Some(moved_package));
    assert_eq!(forwarding.get(&package_ptr), Some(&moved_package));

    let dropped_heap = BexHeap::new(vec![]);
    let (mut dropped_tlab, dropped_package, _) = alloc_cycle(&dropped_heap);
    let (stats, roots, forwarding) = unsafe { dropped_heap.collect_garbage(&[]) };
    dropped_tlab.invalidate();
    assert!(roots.is_empty());
    assert_eq!(stats.live_count, 0);
    assert!(stats.collected_count >= 4);
    assert!(!forwarding.contains_key(&dropped_package));
}

#[test]
fn test_gc_back_to_back_no_allocations_between() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let a = tlab.alloc_string("survivor".to_string());
    let (_, roots1, _) = unsafe { heap.collect_garbage(&[a]) };
    tlab.invalidate();

    // Second GC with no new allocations
    let (stats, roots2, _) = unsafe { heap.collect_garbage(&[roots1[0]]) };
    tlab.invalidate();

    assert_eq!(stats.live_count, 1);
    assert_eq!(stats.collected_count, 0);
    // Data still correct
    let Object::String(s) = (unsafe { roots2[0].get() }) else {
        panic!("not string")
    };
    assert_eq!(s, "survivor");
}

#[test]
fn test_all_garbage_is_reclaimed() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // Allocate objects, don't root any
    for i in 0..100 {
        tlab.alloc_string(format!("garbage_{i}"));
    }

    let (stats, new_roots, _) = unsafe { heap.collect_garbage(&[]) };
    tlab.invalidate();

    assert_eq!(stats.live_count, 0);
    // TLAB pre-allocates a full chunk (1024 slots), so collected_count >= 100
    assert!(stats.collected_count >= 100);
    assert!(new_roots.is_empty());

    // Verify can allocate again from clean state
    let fresh = tlab.alloc_string("fresh".to_string());
    assert_eq!(heap.generation_of(fresh), Generation::Gen0);
}

// ============================================================================
// GcStats Pin Tests
// ============================================================================

#[test]
fn test_stats_compile_time_not_counted() {
    let compile_time = vec![Object::String("builtin".into())];
    let heap = BexHeap::new(compile_time);
    let ct_ptr = heap.compile_time_ptr(0);

    let mut tlab = Tlab::new(Arc::clone(&heap));
    let _garbage = tlab.alloc_string("garbage".to_string());

    // Root only compile-time ptr
    let (stats, _, _) = unsafe { heap.collect_garbage(&[ct_ptr]) };

    // Compile-time object not counted in live_count or collected_count
    // (only runtime objects are tracked in those stats)
    assert_eq!(stats.live_count, 0);
    // At least 1 garbage runtime object collected (plus TLAB chunk padding)
    assert!(stats.collected_count >= 1);
}

// ============================================================================
// Multi-Cycle Accumulation Tests
// ============================================================================

#[test]
fn test_forwarding_map_correctness_across_10_cycles() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    // One persistent string
    let initial = tlab.alloc_string("persistent_data".to_string());
    let mut root = initial;

    for _ in 0..10 {
        // Allocate garbage each cycle
        for j in 0..50 {
            tlab.alloc_string(format!("garbage_{j}"));
        }

        let (_, new_roots, _) = unsafe { heap.collect_garbage(&[root]) };
        tlab.invalidate();
        root = new_roots[0];

        // Verify data integrity after every cycle
        let Object::String(s) = (unsafe { root.get() }) else {
            panic!("not string")
        };
        assert_eq!(s, "persistent_data");
    }
}

// ============================================================================
// Survivor Edges and Weak References
// ============================================================================

/// A survivor written after its collection to reference a fresh nursery
/// object keeps that object alive through the next collection, which repoints
/// the edge to the object's new copy.
#[test]
fn survivor_edge_to_a_fresh_object_is_traced_and_repointed() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let container = tlab.alloc_array(RealizedTy::unknown(), vec![Value::NULL]);
    let (_, roots, _) = unsafe { heap.collect_garbage(&[container]) };
    tlab.invalidate();
    let survivor = roots[0];
    assert_eq!(heap.generation_of(survivor), Generation::Gen2);

    let fresh = tlab.alloc_string("fresh".to_string());
    let Object::Array(array) = (unsafe { survivor.get() }) else {
        panic!("expected the container array")
    };
    array.lock_mut(tlab.alloc_debt())[0] = Value::object(fresh);

    // Only the container is a root: `fresh` is reachable through it alone.
    let (_, roots, forwarding) = unsafe { heap.collect_garbage(&[survivor]) };
    tlab.invalidate();
    let Object::Array(array) = (unsafe { roots[0].get() }) else {
        panic!("expected the container array")
    };
    let edge = array
        .get(0)
        .and_then(|value| value.as_object_ptr())
        .expect("slot 0 holds an object");
    assert_eq!(edge, forwarding[&fresh]);
    let Object::String(text) = (unsafe { edge.get() }) else {
        panic!("expected the fresh string")
    };
    assert_eq!(text, "fresh");
}

/// The same through a chain of fresh objects: survivor → fresh array → fresh
/// string, every link repointed.
#[test]
fn survivor_edge_to_a_fresh_chain_is_traced_and_repointed() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let container = tlab.alloc_array(RealizedTy::unknown(), vec![Value::NULL]);
    let (_, roots, _) = unsafe { heap.collect_garbage(&[container]) };
    tlab.invalidate();
    let survivor = roots[0];

    let leaf = tlab.alloc_string("leaf".to_string());
    let middle = tlab.alloc_array(RealizedTy::unknown(), vec![Value::object(leaf)]);
    let Object::Array(array) = (unsafe { survivor.get() }) else {
        panic!("expected the container array")
    };
    array.lock_mut(tlab.alloc_debt())[0] = Value::object(middle);

    let (_, roots, forwarding) = unsafe { heap.collect_garbage(&[survivor]) };
    tlab.invalidate();
    let link = |ptr: bex_vm_types::HeapPtr| {
        let Object::Array(array) = (unsafe { ptr.get() }) else {
            panic!("expected an array link")
        };
        array
            .get(0)
            .and_then(|value| value.as_object_ptr())
            .expect("slot 0 holds an object")
    };
    let middle_after = link(roots[0]);
    assert_eq!(middle_after, forwarding[&middle]);
    let leaf_after = link(middle_after);
    assert_eq!(leaf_after, forwarding[&leaf]);
    let Object::String(text) = (unsafe { leaf_after.get() }) else {
        panic!("expected the leaf string")
    };
    assert_eq!(text, "leaf");
}

/// A weak reference follows its object while it lives and reports it dead
/// once it is collected; a compile-time object is always live; and visiting
/// the same reference twice in one pass gives the same verdict.
#[test]
fn weak_verdicts_follow_the_collection() {
    let heap = BexHeap::new(vec![Object::String("builtin".into())]);
    let builtin = heap.compile_time_ptr(0);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let live = tlab.alloc_string("live".to_string());
    let dead = tlab.alloc_string("dead".to_string());
    let (_, _, forwarding) = unsafe { heap.collect_garbage(&[live]) };
    tlab.invalidate();

    let mut forward = ForwardRoots::new(&forwarding, &*heap);
    let mut live_ref = Weak::new(live);
    assert!(live_ref.visit(&mut forward));
    assert_eq!(live_ref.ptr(), forwarding[&live]);
    assert!(
        live_ref.visit(&mut forward),
        "a second visit in the same pass agrees with the first"
    );
    assert!(!Weak::new(dead).visit(&mut forward));
    let mut builtin_ref = Weak::new(builtin);
    assert!(builtin_ref.visit(&mut forward));
    assert_eq!(builtin_ref.ptr(), builtin);
}

/// A strong root the collection never reached is a root the collector was
/// not told about: forwarding catches it rather than leaving it dangling.
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "the collection did not reach")]
fn forwarding_a_root_the_collection_never_saw_panics() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let unreported = tlab.alloc_string("unreported".to_string());
    let (_, _, forwarding) = unsafe { heap.collect_garbage(&[]) };
    let mut root = unreported;
    ForwardRoots::new(&forwarding, &*heap).visit(&mut root);
}

/// A helper: an otherwise-empty static-shaped package.
fn empty_package() -> Package {
    Package {
        name: Name::default(),
        kind: PackageKind::Ordinary,
        edges: IndexMap::new(),
        classes: IndexMap::new(),
        enums: IndexMap::new(),
        interfaces: IndexMap::new(),
        impl_rules: IndexMap::new(),
        type_aliases: IndexMap::new(),
        globals: indexmap::IndexMap::new(),
        slots: Slots::Own {
            cells: Box::new([]),
            initialized: true,
        },
        objects: Objects::Own(Box::new([])),
        surface: ExportSurface::Projected,
        init: None,
        test_init: None,
        diagnostics: Vec::new(),
        session: None,
    }
}

/// Collect three times with only `root` rooted, following `tracked` through
/// each collection's forwarding map. Returns the last collection's stats, the
/// root's final address, and where each tracked object ended up.
///
/// Where each object ended up is the only valid expectation for an edge. Its
/// starting address is not: a later collection can reuse a chunk an earlier
/// one freed, so an object can come back to the address it started at.
fn collect_three_times<const N: usize>(
    heap: &BexHeap,
    root: HeapPtr,
    mut tracked: [HeapPtr; N],
) -> (GcStats, HeapPtr, [HeapPtr; N]) {
    let mut collect = |root: HeapPtr| {
        // SAFETY: single-threaded test; no VM is executing.
        let (stats, roots, forwarding) = unsafe { heap.collect_garbage(&[root]) };
        for ptr in &mut tracked {
            *ptr = forwarding[ptr];
        }
        (stats, roots[0])
    };
    let (_, root) = collect(root);
    let (_, root) = collect(root);
    let (stats, root) = collect(root);
    (stats, root, tracked)
}

/// A session graft inserts runtime `Object::TypeAlias` pointers into
/// `Package.type_aliases`; the collector must keep them alive and repoint the
/// map entry when the alias moves.
#[test]
fn package_type_aliases_are_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let alias_name = bex_vm_types::types::LocalName::new(Vec::new(), Name::new("SessionAlias"));
    let alias_ptr = tlab.alloc(Object::TypeAlias(Box::new(TypeAliasDef {
        path: alias_name.clone(),
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        definition: RealizedTy::int(),
        owner: bex_vm_types::HeapPtr::null(),
    })));
    let mut package = empty_package();
    package.type_aliases.insert(
        LocalName {
            namespace: Vec::new(),
            name: Name::new("SessionAlias"),
        },
        alias_ptr,
    );
    let package_ptr = tlab.alloc(Object::Package(Box::new(package)));

    // Root only the package: the alias must survive through the map alone, and
    // the entry must follow the alias through every move.
    let (stats, package_ptr, [moved_alias]) = collect_three_times(&heap, package_ptr, [alias_ptr]);

    assert_eq!(stats.live_count, 2, "package and alias should both survive");
    let Object::Package(package) = (unsafe { package_ptr.get() }) else {
        panic!("root was not the package")
    };
    let forwarded = *package
        .type_aliases
        .values()
        .next()
        .expect("alias entry was lost");
    assert_eq!(
        forwarded, moved_alias,
        "the entry must be repointed to where the alias moved"
    );
    let Object::TypeAlias(alias) = (unsafe { forwarded.get() }) else {
        panic!("type_aliases entry does not point at a TypeAlias")
    };
    assert_eq!(alias.path, alias_name);
}

/// A package's edge table points at the packages it reaches; for a runtime
/// package those move. The collector must keep them alive through the table
/// alone and repoint each entry when its target moves.
#[test]
fn package_edges_are_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let mut dependency = empty_package();
    dependency.name = Name::new("lib");
    let dependency_ptr = tlab.alloc(Object::Package(Box::new(dependency)));
    let mut package = empty_package();
    package.edges.insert(
        Name::new("gadgets"),
        Edge {
            target: dependency_ptr,
            kind: EdgeKind::Dependency,
        },
    );
    let package_ptr = tlab.alloc(Object::Package(Box::new(package)));

    let (stats, package_ptr, [moved_dependency]) =
        collect_three_times(&heap, package_ptr, [dependency_ptr]);

    assert_eq!(
        stats.live_count, 2,
        "package and dependency should both survive"
    );
    let Object::Package(package) = (unsafe { package_ptr.get() }) else {
        panic!("root was not the package")
    };
    let forwarded = package
        .edges
        .get(&Name::new("gadgets"))
        .expect("edge entry was lost")
        .target;
    assert_eq!(
        forwarded, moved_dependency,
        "the edge must be repointed to where the dependency moved"
    );
    let Object::Package(dependency) = (unsafe { forwarded.get() }) else {
        panic!("edge does not point at a package")
    };
    assert_eq!(dependency.name, Name::new("lib"));
}

/// A package's own cells hold values, and its `$init` is an object of its
/// image; both live on the package now, so the collector must keep them
/// alive through the package alone and repoint each when it moves.
#[test]
fn package_cells_and_init_are_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let held = tlab.alloc(Object::String("held by a cell".into()));
    let init = tlab.alloc(Object::String("stands in for $init".into()));
    let mut package = empty_package();
    package.slots = Slots::Own {
        cells: Box::new([bex_vm_types::AtomicValueSlot::new(
            bex_vm_types::Value::object(held),
        )]),
        initialized: true,
    };
    package.init = Some(init);
    let package_ptr = tlab.alloc(Object::Package(Box::new(package)));

    let (stats, package_ptr, [moved_held, moved_init]) =
        collect_three_times(&heap, package_ptr, [held, init]);

    assert_eq!(
        stats.live_count, 3,
        "package, cell value, and init should survive"
    );
    let Object::Package(package) = (unsafe { package_ptr.get() }) else {
        panic!("root was not the package")
    };
    let cell = package.slots.own().expect("own cells")[0]
        .load()
        .as_object_ptr()
        .expect("the cell still holds an object");
    assert_eq!(
        cell, moved_held,
        "the cell must be repointed to where its value moved"
    );
    let forwarded_init = package.init.expect("init entry was lost");
    assert_eq!(
        forwarded_init, moved_init,
        "the entry must be repointed to where init moved"
    );
    assert!(matches!(unsafe { cell.get() }, Object::String(s) if s.as_str() == "held by a cell"));
    assert!(
        matches!(unsafe { forwarded_init.get() }, Object::String(s) if s.as_str() == "stands in for $init")
    );
}

/// A field's exact-operand `TypeValue` reaches its declaration through a head,
/// and that declaration's `owner` is what keeps the package alive. The
/// collector must walk the whole chain — field type, head, owner — and repoint
/// each link.
#[test]
fn a_field_type_value_keeps_its_declaration_and_package_alive() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let package_ptr = tlab.alloc(Object::Package(Box::new(empty_package())));
    let enum_tag = baml_type::typetag::TypeTag::fresh_dynamic();
    let enum_ptr = tlab.alloc(Object::Enum(Box::new(bex_vm_types::Enum {
        path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("FieldEnum")),
        variants: Vec::new(),
        description: None,
        alias: None,
        docstring: None,
        other: IndexMap::new(),
        type_tag: enum_tag,
        owner: bex_vm_types::types::Owner::Package(package_ptr),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })));
    let field_ty = RealizedTy::Enum(bex_vm_types::TypeHead::new(enum_ptr, enum_tag));
    let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
        path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("FieldOwner")),
        fields: vec![ClassField {
            name: "value".to_string(),
            field_type: bex_vm_types::RuntimeTy::from(field_ty.clone()),
            field_template: bex_vm_types::TyTemplate::from(field_ty.clone()),
            description: None,
            alias: None,
            docstring: None,
            other: IndexMap::new(),
            skip: false,
            stream_done: false,
            must_exist: false,
            runtime_type: Some(TypeValue::new(field_ty)),
        }],
        description: None,
        alias: None,
        docstring: None,
        other: IndexMap::new(),
        stream_done: false,
        declared_crossing: baml_type::ClassCrossing::Fields,
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        cleanup: None,
        methods: IndexMap::new(),
        generic_param_count: 0,
        owner: bex_vm_types::types::Owner::anonymous(),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })));

    // Root only the outer class: the enum survives through the field type's
    // head, and the package survives through the enum's owner — across moves
    // and a compaction.
    let (_, roots, _) = unsafe { heap.collect_garbage(&[class_ptr]) };
    let (_, roots, _) = unsafe { heap.collect_garbage(&roots) };
    let (stats, roots, _) = unsafe { heap.collect_garbage(&roots) };

    assert_eq!(
        stats.live_count, 3,
        "class, field enum, and owner package should all survive"
    );
    let Object::Class(class) = (unsafe { roots[0].get() }) else {
        panic!("root was not the class")
    };
    let exact = class.fields[0]
        .runtime_type
        .as_ref()
        .expect("field runtime type was lost");
    let RealizedTy::Enum(head) = &exact.ty else {
        panic!("field runtime type ceased to wrap an enum")
    };
    assert_eq!(head.tag(), enum_tag, "a move must not change identity");
    // The property is that the head points at the live enum and the enum at
    // the live package: an address comparison against the vacated slot would
    // be flaky, since a collection may place the moved object exactly where
    // it was.
    let Object::Enum(enm) = (unsafe { head.ptr().get() }) else {
        panic!("the field type's head does not point at an enum")
    };
    let owner = enm
        .owner
        .package()
        .expect("the enum belongs to its package");
    assert!(
        matches!(unsafe { owner.get() }, Object::Package(_)),
        "the declaration's owner does not point at the package"
    );
}

/// A class with no fields, as a `@@rust_type` class is, allocated at run time.
fn alloc_tagged_class(tlab: &mut Tlab, name: &str) -> HeapPtr {
    tlab.alloc(Object::Class(Box::new(Class {
        path: LocalName::new(Vec::new(), Name::new(name)),
        fields: Vec::new(),
        description: None,
        alias: None,
        docstring: None,
        other: IndexMap::new(),
        stream_done: false,
        declared_crossing: baml_type::ClassCrossing::Opaque,
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        cleanup: None,
        methods: IndexMap::new(),
        generic_param_count: 1,
        owner: bex_vm_types::types::Owner::anonymous(),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })))
}

/// A `@@rust_type` instance holds its class by pointer: a collection keeps the
/// class alive through the instance alone and repoints the instance when the
/// class moves.
#[test]
fn rust_instance_keeps_and_follows_its_class() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let class = alloc_tagged_class(&mut tlab, "Tagged");
    // Reached by nothing: the control for "the class survives through the
    // instance", not through anything else.
    let orphan = alloc_tagged_class(&mut tlab, "Orphan");
    let instance = tlab.alloc(Object::RustInstance(RustInstance::new(
        class,
        Box::new([RealizedTy::int()]),
        RustPayload::Foreign(Arc::new(TestRustData(7))),
    )));

    let (_, roots, forwarding) = unsafe { heap.collect_garbage(&[instance]) };
    tlab.invalidate();
    assert!(
        !forwarding.contains_key(&orphan),
        "an unreached class is collected"
    );
    let Object::RustInstance(moved) = (unsafe { roots[0].get() }) else {
        panic!("expected a rust instance")
    };
    assert_eq!(
        moved.class, forwarding[&class],
        "the class edge was repointed"
    );
    assert_eq!(heap.generation_of(moved.class), Generation::Gen2);
    let Object::Class(class_object) = (unsafe { moved.class.get() }) else {
        panic!("the class edge must reach a class")
    };
    assert_eq!(class_object.name().to_string(), "Tagged");
    assert_eq!(&*moved.class_type_args, &[RealizedTy::int()]);
    let RustPayload::Foreign(payload) = &moved.payload;
    assert_eq!(
        payload.downcast_ref::<TestRustData>(),
        Some(&TestRustData(7))
    );
}

/// A runtime-allocated impl rule (a session's `implements` block or an
/// anonymous class's witness) points at a moving interface and moving method
/// bodies. The collector must keep both alive through the rule and repoint
/// them when they move — dispatch reads `methods[].fqn` straight off the rule.
#[test]
fn impl_rule_edges_are_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let iface_name = bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Runtime"));
    let iface_ptr = tlab.alloc(Object::Interface(Box::new(InterfaceDef {
        path: iface_name.clone(),
        structural_default: None,
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        args: Vec::new(),
        requires: Vec::new(),
        assoc: Vec::new(),
        fields: Vec::new(),
        methods: Vec::new(),
        owner: bex_vm_types::HeapPtr::null(),
    })));
    // Any heap object serves as the method body's stand-in target.
    let method_ptr = tlab.alloc_string("method body".to_string());
    let rule_ptr = tlab.alloc(Object::ImplRule(Box::new(RuntimeImplRule {
        interface_head: iface_ptr,
        for_ty_pattern: baml_type::TyTemplate::from(RealizedTy::int()),
        generic_param_bounds: Vec::new(),
        interface_args: Vec::new(),
        interface_assoc: Vec::new(),
        methods: IndexMap::from([(
            Name::new("run"),
            MethodImpl {
                fqn: method_ptr,
                frame: Vec::new(),
            },
        )]),
        field_links: Box::default(),
    })));

    // Root only the rule. Movement is checked through the forwarding map: a
    // later allocation may reuse a freed address, so inequality with the old
    // pointer is not a valid movement assertion.
    let (stats, roots, forwarding) = unsafe { heap.collect_garbage(&[rule_ptr]) };

    assert_eq!(
        stats.live_count, 3,
        "rule, interface and method body must all survive"
    );
    let Object::ImplRule(rule) = (unsafe { roots[0].get() }) else {
        panic!("root was not the impl rule")
    };
    assert_eq!(
        rule.interface_head, forwarding[&iface_ptr],
        "interface moved; head must be repointed"
    );
    let Object::Interface(iface) = (unsafe { rule.interface_head.get() }) else {
        panic!("interface_head does not point at an interface")
    };
    assert_eq!(iface.path, iface_name);
    let fqn = rule.methods["run"].fqn;
    assert_eq!(
        fqn, forwarding[&method_ptr],
        "method body moved; fqn must be repointed"
    );
    assert!(matches!(unsafe { fqn.get() }, Object::String(_)));
}

/// A runtime-declared interface back-references its owning package and points
/// at its default-method bodies; the collector must keep both alive through
/// the interface alone and repoint them as they move.
#[test]
fn interface_owner_and_default_bodies_are_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let package_ptr = tlab.alloc(Object::Package(Box::new(empty_package())));
    let body_ptr = tlab.alloc_string("default body".to_string());
    let structural_ptr = tlab.alloc_string("structural body".to_string());
    let iface_ptr = tlab.alloc(Object::Interface(Box::new(InterfaceDef {
        path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("SessionIface")),
        structural_default: Some(bex_vm_types::types::StructuralDefault {
            kind: baml_type::StructuralInterface::ToString,
            function: bex_vm_types::ObjectIndex::from_raw(0),
            function_ptr: structural_ptr,
        }),
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        args: Vec::new(),
        requires: Vec::new(),
        assoc: Vec::new(),
        fields: Vec::new(),
        methods: vec![bex_vm_types::types::InterfaceMethodDef {
            name: Name::new("greet"),
            args: Vec::new(),
            kwargs: Vec::new(),
            returns: baml_type::RuntimeTy::Null,
            errors: baml_type::RuntimeTy::Never,
            default: Some(bex_vm_types::ObjectIndex::from_raw(0)),
            default_fn: body_ptr,
        }],
        owner: package_ptr,
    })));

    // Check each forwarding step. A later allocation may reuse the original
    // address after its source chunk was freed; comparing against that stale
    // address after three collections is not a valid movement assertion.
    let mut roots = vec![iface_ptr];
    let mut expected_body = body_ptr;
    let mut expected_structural = structural_ptr;
    let mut expected_owner = package_ptr;
    for _ in 0..3 {
        let (stats, next_roots, forwarding) = unsafe { heap.collect_garbage(&roots) };
        assert_eq!(stats.live_count, 4);
        let next_body = forwarding[&expected_body];
        let next_structural = forwarding[&expected_structural];
        let next_owner = forwarding[&expected_owner];
        assert_ne!(next_body, expected_body);
        assert_ne!(next_structural, expected_structural);
        assert_ne!(next_owner, expected_owner);
        expected_body = next_body;
        expected_structural = next_structural;
        expected_owner = next_owner;
        roots = next_roots;
    }
    let Object::Interface(iface) = (unsafe { roots[0].get() }) else {
        panic!("root was not the interface")
    };
    assert_eq!(iface.owner, expected_owner);
    assert!(matches!(unsafe { iface.owner.get() }, Object::Package(_)));
    assert_eq!(iface.methods[0].default_fn, expected_body);
    assert_eq!(
        iface.structural_default.as_ref().unwrap().function_ptr,
        expected_structural
    );
    let Object::String(structural) = (unsafe { expected_structural.get() }) else {
        panic!("structural body was not forwarded")
    };
    assert_eq!(structural.as_str(), "structural body");
    let Object::String(body) = (unsafe { expected_body.get() }) else {
        panic!("body was not forwarded")
    };
    assert_eq!(body.as_str(), "default body");
}

/// A class's inherent method bodies are direct pointers, exactly like an
/// interface's default bodies: traced, forwarded across collections, and never
/// reachable through a name.
#[test]
fn class_method_bodies_are_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let package_ptr = tlab.alloc(Object::Package(Box::new(empty_package())));
    let body_ptr = tlab.alloc_string("method body".to_string());
    let class_name = bex_vm_types::types::LocalName::new(Vec::new(), Name::new("SessionClass"));
    let mut methods = IndexMap::new();
    methods.insert(
        Name::new("greet"),
        bex_vm_types::types::ClassMethodDef {
            function: bex_vm_types::ObjectIndex::from_raw(0),
            function_ptr: body_ptr,
        },
    );
    let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
        path: class_name.clone(),
        fields: Vec::new(),
        description: None,
        alias: None,
        docstring: None,
        other: IndexMap::new(),
        stream_done: false,
        declared_crossing: baml_type::ClassCrossing::Fields,
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        cleanup: None,
        methods,
        generic_param_count: 0,
        owner: bex_vm_types::types::Owner::Package(package_ptr),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })));

    let mut roots = vec![class_ptr];
    let mut expected_body = body_ptr;
    let mut expected_owner = package_ptr;
    for _ in 0..3 {
        let (stats, next_roots, forwarding) = unsafe { heap.collect_garbage(&roots) };
        assert_eq!(stats.live_count, 3);
        let next_body = forwarding[&expected_body];
        let next_owner = forwarding[&expected_owner];
        assert_ne!(next_body, expected_body);
        assert_ne!(next_owner, expected_owner);
        expected_body = next_body;
        expected_owner = next_owner;
        roots = next_roots;
    }
    let Object::Class(class) = (unsafe { roots[0].get() }) else {
        panic!("root was not the class")
    };
    assert_eq!(class.owner.package(), Some(expected_owner));
    assert_eq!(
        class.methods[&Name::new("greet")].function_ptr,
        expected_body
    );
    let Object::String(body) = (unsafe { expected_body.get() }) else {
        panic!("body was not forwarded")
    };
    assert_eq!(body.as_str(), "method body");
}

/// A runtime-declared alias back-references its owning package; the collector
/// must keep the package alive through the alias alone.
#[test]
fn type_alias_owner_is_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));
    let package_ptr = tlab.alloc(Object::Package(Box::new(empty_package())));
    let alias_ptr = tlab.alloc(Object::TypeAlias(Box::new(TypeAliasDef {
        path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("OwnedAlias")),
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        definition: RealizedTy::int(),
        owner: package_ptr,
    })));

    let (stats, alias_ptr, [moved_package]) = collect_three_times(&heap, alias_ptr, [package_ptr]);

    assert_eq!(
        stats.live_count, 2,
        "alias and owner package must both survive"
    );
    let Object::TypeAlias(alias) = (unsafe { alias_ptr.get() }) else {
        panic!("root was not the alias")
    };
    assert_eq!(
        alias.owner, moved_package,
        "the owner must be repointed to where the package moved"
    );
    assert!(matches!(unsafe { alias.owner.get() }, Object::Package(_)));
}

/// A live future's `Future<T, E>` output-type heads are real GC edges: the
/// declarations they name stay live and the heads are repointed when those
/// declarations move. Regression for the walk gap where `Object::Future` was
/// excluded from `visit_object_heads(_mut)` — its root walk repoints only the
/// future object itself, so an output type naming a runtime-created class
/// dangled (and the class could be collected) after a major collection.
#[test]
fn future_output_type_heads_are_traced_and_forwarded() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let class_name = bex_vm_types::types::LocalName::new(Vec::new(), Name::new("SpawnOut"));
    let type_tag = baml_type::typetag::TypeTag::fresh_dynamic();
    let class_ptr = tlab.alloc(Object::Class(Box::new(Class {
        path: class_name,
        fields: Vec::new(),
        description: None,
        alias: None,
        docstring: None,
        other: IndexMap::new(),
        stream_done: false,
        declared_crossing: baml_type::ClassCrossing::Fields,
        type_tag,
        cleanup: None,
        methods: IndexMap::new(),
        generic_param_count: 0,
        owner: bex_vm_types::types::Owner::anonymous(),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    })));
    let returns = RealizedTy::Class(
        bex_vm_types::TypeHead::new(class_ptr, type_tag),
        Box::new([]),
    );
    let future = bex_vm_types::types::Future::pending(
        bex_vm_types::types::FutureId::from_usize(1),
        returns,
        RealizedTy::never(),
        bex_vm_types::types::CancellationToken::new(),
    );
    let future_ptr = tlab.alloc(Object::Future(future));

    // Root ONLY the future: the class must survive through the output-type
    // head alone.
    let (stats, roots, _forwarding) = unsafe { heap.collect_garbage(&[future_ptr]) };
    tlab.invalidate();
    assert_eq!(stats.live_count, 2, "the class is live through the head");

    let moved_future = roots[0];
    let Object::Future(future) = (unsafe { moved_future.get() }) else {
        panic!("root ceased to be a future")
    };
    let RealizedTy::Class(head, _) = future.returns() else {
        panic!("future returns ceased to be a class")
    };
    assert!(head.is_resolved());
    assert_ne!(
        head.ptr(),
        class_ptr,
        "a major collection moves Gen0 objects, so the head must be repointed"
    );
    let Object::Class(class) = (unsafe { head.ptr().get() }) else {
        panic!("forwarded head does not point at the class")
    };
    assert_eq!(class.type_tag, type_tag, "identity survives the move");
}

// ============================================================================
// Evacuation Moves Survivors
// ============================================================================

/// The addresses and capacities of everything `roots` owns outside its slots,
/// for an array, a byte array and a map, in that order.
fn backing_storage(roots: &[bex_vm_types::HeapPtr]) -> [(usize, usize); 3] {
    // SAFETY: single-threaded tests; the roots are live and nothing mutates them.
    unsafe {
        let Object::Array(array) = roots[0].get() else {
            panic!("roots[0] is not an array")
        };
        let Object::Uint8Array(bytes) = roots[1].get() else {
            panic!("roots[1] is not a byte array")
        };
        let Object::Map(map) = roots[2].get() else {
            panic!("roots[2] is not a map")
        };
        let array = array.data_unchecked();
        let bytes = bytes.data_unchecked();
        let map = map.data_unchecked();
        [
            (array.as_ptr().addr(), array.capacity()),
            (bytes.as_ptr().addr(), bytes.capacity()),
            (std::ptr::from_ref(map).addr(), map.len()),
        ]
    }
}

/// A collection moves a survivor rather than copying it: whatever the object
/// owns outside its slot is the same allocation, with the same spare capacity,
/// however many collections it passes through.
#[test]
fn test_survivor_backing_storage_is_moved_not_copied() {
    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let mut elements = Vec::with_capacity(100);
    elements.extend([1, 2, 3].map(bex_vm_types::Value::int));
    let array = tlab.alloc_array(RealizedTy::int(), elements);

    let mut bytes = Vec::with_capacity(4096);
    bytes.extend_from_slice(b"abc");
    let bytes = tlab.alloc_uint8array(bytes);

    let map = tlab.alloc_map(
        RealizedTy::string(),
        RealizedTy::int(),
        IndexMap::from([(bex_str::BexStr::from("key"), bex_vm_types::Value::int(1))]),
    );

    let mut roots = vec![array, bytes, map];
    let before = backing_storage(&roots);
    assert_eq!(before[0].1, 100, "the array starts with spare capacity");

    for _ in 0..3 {
        let (_, new_roots, _) = unsafe { heap.collect_garbage(&roots) };
        tlab.invalidate();
        assert_ne!(new_roots, roots, "a collection relocates slots");
        roots = new_roots;
        assert_eq!(
            backing_storage(&roots),
            before,
            "a collection must not reallocate what a survivor owns"
        );
    }
    assert_eq!(heap.generation_of(roots[0]), Generation::Gen2);
}

/// Counts how many times it has been dropped.
struct CountsDrops(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for CountsDrops {
    fn drop(&mut self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

impl bex_vm_types::BexRustData for CountsDrops {
    fn measure(&self, _: &mut bex_vm_types::Meter) {}
}

/// Moving a survivor neither duplicates nor leaks what it owns: a payload is
/// untouched for as long as its object lives, and dropped once when it dies.
#[test]
fn test_survivor_payload_is_dropped_exactly_once() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let heap = BexHeap::new(vec![]);
    let mut tlab = Tlab::new(Arc::clone(&heap));

    let drops = Arc::new(AtomicUsize::new(0));
    let payload = Arc::new(CountsDrops(Arc::clone(&drops)));
    let mut roots = vec![tlab.alloc_rust_data(payload.clone())];

    for _ in 0..4 {
        let (_, new_roots, _) = unsafe { heap.collect_garbage(&roots) };
        tlab.invalidate();
        roots = new_roots;
        assert_eq!(
            Arc::strong_count(&payload),
            2,
            "a collection must leave exactly the heap's reference"
        );
        assert_eq!(drops.load(Ordering::SeqCst), 0);
    }

    // Unrooted, the object dies and releases its reference.
    let _ = unsafe { heap.collect_garbage(&[]) };
    assert_eq!(Arc::strong_count(&payload), 1);
    drop(payload);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}

/// A class whose `cleanup` method is the function at `cleanup`.
fn class_with_cleanup(name: &str, cleanup: bex_vm_types::HeapPtr) -> Class {
    let mut methods = IndexMap::new();
    methods.insert(
        Name::new(baml_base::CLEANUP_METHOD),
        bex_vm_types::types::ClassMethodDef {
            function: bex_vm_types::ObjectIndex::from_raw(0),
            function_ptr: cleanup,
        },
    );
    Class {
        path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new(name)),
        fields: Vec::new(),
        description: None,
        alias: None,
        docstring: None,
        other: IndexMap::new(),
        stream_done: false,
        declared_crossing: baml_type::ClassCrossing::Fields,
        type_tag: baml_type::typetag::TypeTag::fresh_dynamic(),
        cleanup: Some(0),
        methods,
        generic_param_count: 0,
        owner: bex_vm_types::types::Owner::anonymous(),
        telemetry_definition: bex_vm_types::DefinitionCell::default(),
    }
}

/// A dead instance is still queued for `cleanup` when its class is a runtime
/// class that the same collection has already moved out of the slot the
/// instance points at.
#[test]
fn test_finalizer_is_queued_when_its_runtime_class_was_evacuated() {
    {
        // A compile-time class with `cleanup` is what turns the finalizer scan
        // on; its method body sits in the pool beside it.
        let heap = BexHeap::new(vec![
            Object::String("static cleanup".into()),
            Object::Class(Box::new(class_with_cleanup(
                "Static",
                bex_vm_types::HeapPtr::null(),
            ))),
        ]);
        let mut tlab = Tlab::new(Arc::clone(&heap));

        // The runtime class's `cleanup` body is a runtime object too: a method
        // body stand-in that the collection moves.
        let cleanup = tlab.alloc_string("runtime cleanup".to_string());
        let class = tlab.alloc(Object::Class(Box::new(class_with_cleanup(
            "Runtime", cleanup,
        ))));
        let dead_instance = tlab.alloc_instance(class, Vec::new());

        // Root the class and its method body: both are evacuated by the
        // trace, before the scan that discovers the unreachable instance.
        let (_, roots, _) = unsafe { heap.collect_garbage(&[class, cleanup]) };
        tlab.invalidate();
        assert_ne!(roots[0], class, "a collection moves the class");

        let pending = heap.take_pending_finalizers();
        assert_eq!(
            pending.len(),
            1,
            "a collection must queue the dead instance"
        );
        let (kept_alive, queued_cleanup) = &pending[0];
        assert_ne!(*kept_alive, dead_instance);
        assert_eq!(
            *queued_cleanup, roots[1],
            "the queued finalizer is the class's relocated `cleanup` body"
        );
        let Object::Instance(instance) = (unsafe { kept_alive.get() }) else {
            panic!("the queued pointer is not an instance")
        };
        assert_eq!(
            instance.class, roots[0],
            "the kept-alive instance points at its relocated class"
        );
    }
}
