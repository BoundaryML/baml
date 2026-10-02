use baml_linker_types::{
    DeclKey, DepSlot, Digest, ExportTable, ImportEntry, LocalRef, Locator, import_operand,
};
use baml_type::typetag::TypeTag;
use bex_vm_types::{
    BodyKey, ConstValue, DeclPath, FnPath, GlobalIndex, ImplBodyKey, Instruction, InterfaceKey,
    Object, ObjectIndex, RealizedTy, TypeHead,
    bytecode::{Bytecode, SwitchDispatch, SwitchEntry, SwitchKey, SwitchTable},
    types::{
        Class, EdgeKind, Function, FunctionKind, FunctionOrigin, GenericFunction, LocalName,
        ProgramEdge, ProgramPackage,
    },
};

use super::*;

fn item(name: &str) -> LocalName {
    LocalName::new(Vec::new(), Name::new(name))
}

fn func(name: &str, instructions: Vec<Instruction>) -> Object {
    Object::Function(Box::new(Function {
        name: name.to_string(),
        source_file: "user.baml".to_string(),
        docstring: None,
        declared_name: None,
        arity: 0,
        real_local_count: 0,
        bytecode: Bytecode {
            instructions,
            ..Bytecode::default()
        },
        kind: FunctionKind::Bytecode,
        telemetry_function_id: None,
        telemetry_registration: bex_vm_types::FunctionRegistration::default(),
        telemetry_policy_id: bex_vm_types::TelemetryPolicyId::none(),
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: baml_base::Span::fake(),
        return_type: bex_vm_types::TyTemplate::Unknown,
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        display_type_params: Vec::new(),
        type_param_names: Vec::new(),
        generic_param_bounds: Vec::new(),
        display_param_types: Vec::new(),
        display_return_type: String::new(),
        throws_type: bex_vm_types::TyTemplate::Never,
        origin: FunctionOrigin::Internal,
        is_interface_body: false,
        native_key: None,
        body_meta: None,
        runtime_package: bex_vm_types::HeapPtr::null(),
    }))
}

/// A function whose constants reference local objects `refs`.
fn func_with_constants(name: &str, refs: &[usize]) -> Object {
    let mut object = func(name, vec![Instruction::LoadConst(0), Instruction::Return]);
    let Object::Function(function) = &mut object else {
        unreachable!()
    };
    function.bytecode.constants = refs
        .iter()
        .map(|&raw| ConstValue::Object(ObjectIndex::from_raw(raw)))
        .collect();
    object
}

/// A class named `name` in the local package (displays as `user.<name>`),
/// at class-bucket offset 0 (its own tag field is its operand).
fn class(name: &str) -> Object {
    Object::Class(Box::new(Class {
        name: bex_vm_types::DeclarationName::Declared(baml_type::QualifiedTypeName::local(
            Name::new(name),
        )),
        fields: Vec::new(),
        description: None,
        alias: None,
        docstring: None,
        other: indexmap::IndexMap::new(),
        stream_done: false,
        type_tag: TypeHead::unresolved_operand(ObjectIndex::from_raw(0)).tag(),
        has_cleanup: false,
        methods: indexmap::IndexMap::new(),
        generic_param_count: 0,
        owner: bex_vm_types::types::Owner::anonymous(),
    }))
}

fn generic_value(function: usize, type_args: Vec<RealizedTy>) -> Object {
    Object::GenericFunction(GenericFunction {
        function: GlobalIndex::from_raw(function),
        type_args: type_args.into_boxed_slice(),
        runtime_package: bex_vm_types::HeapPtr::null(),
    })
}

fn free_fn(name: &str) -> DeclPath {
    DeclPath::Function(FnPath::Free(item(name)))
}

fn import(dep: u32, path: DeclPath) -> ImportEntry {
    ImportEntry {
        key: DeclKey {
            dep: DepSlot(dep),
            path,
        },
    }
}

/// A declared edge of the executable's record.
fn declared(name: &str, target: u32) -> ProgramEdge {
    ProgramEdge {
        name: Name::new(name),
        target,
        kind: EdgeKind::Declared,
    }
}

/// A direct dependency entry: `edge` in the owner's own table, bound to a
/// package whose interface payload has `digest`.
fn direct(edge: &str, digest: Digest) -> Locator {
    Locator::Direct {
        edge: Name::new(edge),
        digest,
    }
}

/// A prelude entry: `edge` in the owner's own table, no payload.
fn prelude(edge: &str) -> Locator {
    Locator::Prelude {
        edge: Name::new(edge),
    }
}

/// A record carrying a valid interface artifact around `payload`: the linker
/// checks the envelope and hashes the payload, never decoding an interface,
/// so any payload stands for one.
fn interface_record(payload: &[u8]) -> PackageRecord {
    PackageRecord {
        interface_blob: baml_artifact::encode_payload(
            baml_artifact::ArtifactKind::PackageInterface,
            payload,
        )
        .expect("an interface artifact encodes"),
    }
}

/// The fingerprint a unit compiled against `record`'s interface records.
fn fingerprint_of(record: &PackageRecord) -> Digest {
    baml_artifact::payload_digest(
        baml_artifact::ArtifactKind::PackageInterface,
        &record.interface_blob,
    )
    .expect("a test record carries an interface payload")
}

fn package_named<'p>(program: &'p Program, name: &str) -> &'p ProgramPackage {
    program
        .packages
        .iter()
        .find(|package| package.name.as_str() == name)
        .unwrap_or_else(|| panic!("no package `{name}`"))
}

fn function_names(program: &Program) -> Vec<String> {
    program
        .objects
        .iter()
        .map(|object| match object {
            Object::Function(function) => function.name.clone(),
            Object::Class(class) => format!("class {}", class.name),
            Object::GenericFunction(generic) => format!("generic@{}", generic.function.raw()),
            other => format!("{other:?}"),
        })
        .collect()
}

fn function(program: &Program, index: usize) -> &Function {
    let Object::Function(function) = &program.objects[ObjectIndex::from_raw(index)] else {
        panic!("object {index} is not a function")
    };
    function
}

/// A unit exporting one free function whose display name is `display`
/// (`pkg.name`) and whose item path is its last segment.
fn unit_with_fn(display: &str, body: Vec<Instruction>) -> CompilationUnit {
    let item_name = display.rsplit('.').next().unwrap_or(display);
    CompilationUnit {
        code: vec![func(display, body)],
        exports: ExportTable {
            objects: vec![(free_fn(item_name), LocalRef::Code(0))],
            globals: vec![(free_fn(item_name), 0)],
        },
        ..CompilationUnit::default()
    }
}

/// A unit exporting one class.
fn unit_with_class(name: &str, item_name: &str) -> CompilationUnit {
    CompilationUnit {
        classes: vec![class(name)],
        exports: ExportTable {
            objects: vec![(DeclPath::Class(item(item_name)), LocalRef::Class(0))],
            globals: Vec::new(),
        },
        ..CompilationUnit::default()
    }
}

fn package<'a>(
    name: &str,
    edges: Vec<(&str, u32)>,
    unit: &'a CompilationUnit,
    record: &'a PackageRecord,
) -> LinkPackage<'a> {
    LinkPackage {
        name: Name::new(name),
        edges: edges
            .into_iter()
            .map(|(edge, id)| LinkEdge {
                name: Name::new(edge),
                target: LinkPackageId(id),
                kind: EdgeKind::Declared,
            })
            .collect(),
        unit,
        record,
        tail: None,
    }
}

fn alloc_import(ordinal: usize) -> Instruction {
    Instruction::AllocInstance {
        class_obj: ObjectIndex::from_raw(import_operand(ordinal)),
        ntypeargs: 0,
    }
}

fn load_import(ordinal: usize) -> Instruction {
    Instruction::LoadGlobal(GlobalIndex::from_raw(import_operand(ordinal)))
}

#[test]
fn import_of_one_kind_never_binds_an_export_of_another() {
    let record = interface_record(b"iface");
    let provider = unit_with_class("Foo", "Foo");
    // The consumer asks for a FUNCTION named Foo: the class must not
    // satisfy it.
    let mut consumer = unit_with_fn("user.g", vec![load_import(0), Instruction::Return]);
    consumer
        .dependencies
        .push(direct("app", fingerprint_of(&record)));
    consumer.global_imports.push(import(1, free_fn("Foo")));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &consumer, &record),
        ],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::UnresolvedImport {
            importer: Name::new("user"),
            package: Name::new("app"),
            path: Box::new(free_fn("Foo")),
        })
    );
}

#[test]
fn duplicate_export_is_refused() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    unit.code.push(func("user.f", vec![Instruction::Return]));
    unit.exports.objects.push((free_fn("f"), LocalRef::Code(1)));
    let set = LinkSet {
        root: LinkPackageId(0),
        packages: vec![package("user", vec![], &unit, &record)],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::DuplicateExport {
            package: Name::new("user"),
            path: Box::new(free_fn("f")),
        })
    );
}

#[test]
fn the_root_and_every_edge_table_are_recorded_by_ordinal() {
    let record = interface_record(b"iface");
    let unit = CompilationUnit::default();
    let packages = || {
        vec![
            package("app", vec![], &unit, &record),
            package("user", vec![("gadgets", 0)], &unit, &record),
        ]
    };
    let program = link(&LinkSet {
        packages: packages(),
        root: LinkPackageId(1),
    })
    .unwrap();
    assert_eq!(program.root, 1);
    assert_eq!(program.packages[0].edges, vec![]);
    assert_eq!(program.packages[1].edges, vec![declared("gadgets", 0)]);
    assert!(matches!(
        link(&LinkSet {
            packages: packages(),
            root: LinkPackageId(2),
        }),
        Err(LinkError::InvalidUnit(message)) if message.contains("root package 2")
    ));
}

#[test]
fn a_packages_slot_map_is_relative_to_its_own_base() {
    // Two packages, each with one function and one `let`: every package's
    // cells are ordinals from its own base, and the bases follow set order.
    let record = interface_record(b"iface");
    let mut first = unit_with_fn("a.f", vec![Instruction::Return]);
    first.exports.globals.push((DeclPath::Let(item("x")), 1));
    let mut second = unit_with_fn("user.g", vec![Instruction::Return]);
    second.exports.globals.push((DeclPath::Let(item("y")), 1));
    let program = link(&LinkSet {
        packages: vec![
            package("a", vec![], &first, &record),
            package("user", vec![("a", 0)], &second, &record),
        ],
        root: LinkPackageId(1),
    })
    .unwrap();
    let [a, user] = program.packages.as_slice() else {
        panic!("two packages")
    };
    assert_eq!(a.globals[&free_fn("f")], 0);
    assert_eq!(a.globals[&DeclPath::Let(item("x"))], 1);
    assert_eq!(user.globals[&free_fn("g")], 0);
    assert_eq!(user.globals[&DeclPath::Let(item("y"))], 1);
    assert_eq!(a.slot_base, GlobalIndex::from_raw(0));
    assert_eq!(user.slot_base, GlobalIndex::from_raw(2));
    assert_eq!(
        user.global_slot(&free_fn("g")),
        Some(GlobalIndex::from_raw(2))
    );
    assert_eq!(
        user.global_slot(&DeclPath::Let(item("y"))),
        Some(GlobalIndex::from_raw(3))
    );
    assert_eq!(program.rendered_lets()["user.y"], GlobalIndex::from_raw(3));
}

#[test]
fn two_packages_may_share_a_spelling() {
    // Names live on edges: the root reaches each `lib` under its own edge
    // name, and the spelling identifies nothing program-wide.
    let record = interface_record(b"iface");
    let unit = CompilationUnit::default();
    let program = link(&LinkSet {
        packages: vec![
            package("lib", vec![], &unit, &record),
            package("lib", vec![], &unit, &record),
            package("user", vec![("first", 0), ("second", 1)], &unit, &record),
        ],
        root: LinkPackageId(2),
    })
    .unwrap();
    assert_eq!(program.packages.len(), 3);
    assert_eq!(
        program.packages[2].edges,
        vec![declared("first", 0), declared("second", 1)]
    );
}

#[test]
fn a_package_reaching_two_packages_under_one_name_is_refused() {
    let record = interface_record(b"iface");
    let unit = CompilationUnit::default();
    let set = LinkSet {
        packages: vec![
            package("a", vec![], &unit, &record),
            package("b", vec![], &unit, &record),
            package("user", vec![("lib", 0), ("lib", 1)], &unit, &record),
        ],
        root: LinkPackageId(2),
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::DuplicateEdge {
            package: Name::new("user"),
            edge: Name::new("lib"),
        })
    );
}

#[test]
fn a_static_unit_importing_its_own_package_is_malformed() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_fn("user.f", vec![load_import(0), Instruction::Return]);
    unit.global_imports.push(import(0, free_fn("f")));
    let set = LinkSet {
        root: LinkPackageId(0),
        packages: vec![package("user", vec![], &unit, &record)],
    };
    assert!(matches!(
        link(&set),
        Err(LinkError::InvalidUnit(message)) if message.contains("imports its own")
    ));
}

#[test]
fn generic_values_are_interned_by_base_slot_across_packages_and_tails() {
    let record = interface_record(b"iface");
    // `lib` defines `f` (slot 0) and its own `f<int>` value (code 1).
    let mut lib = unit_with_fn("lib.f", vec![Instruction::Return]);
    lib.code.push(generic_value(0, vec![RealizedTy::Int]));
    // `user` imports `f`'s slot and carries its own copy of `f<int>`
    // (code 0) and a distinct `f<string>` (code 1); `g` references both.
    let mut user = CompilationUnit {
        dependencies: vec![direct("lib", fingerprint_of(&record))],
        global_imports: vec![import(1, free_fn("f"))],
        code: vec![
            generic_value(import_operand(0), vec![RealizedTy::Int]),
            generic_value(import_operand(0), vec![RealizedTy::String]),
            func_with_constants("user.g", &[0, 1]),
        ],
        ..CompilationUnit::default()
    };
    user.exports.objects.push((free_fn("g"), LocalRef::Code(2)));
    user.exports.globals.push((free_fn("g"), 0));
    // The user tail carries a third copy of `f<int>` and one of `f<string>`
    // in its init part, plus an `$init` that references them.
    let tail = InitTail {
        dependencies: vec![direct("lib", fingerprint_of(&record))],
        objects: vec![
            generic_value(import_operand(0), vec![RealizedTy::Int]),
            generic_value(import_operand(0), vec![RealizedTy::String]),
            func_with_constants("$init", &[0, 1]),
        ],
        global_imports: vec![import(1, free_fn("f"))],
        slot_objects: vec![2],
        init: Some(2),
        test_objects_start: 3,
        test_slots_start: 1,
        ..InitTail::default()
    };
    let mut set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("lib", vec![], &lib, &record),
            package("user", vec![("lib", 0)], &user, &record),
        ],
    };
    set.packages[1].tail = Some(&tail);
    let program = linked(&set);
    // Pool: lib.f, lib's f<int>, user's f<string>, user.g, $init — every
    // duplicate shadowed.
    assert_eq!(
        function_names(&program),
        vec!["lib.f", "generic@0", "generic@0", "user.g", "$init"]
    );
    let expected = [
        ConstValue::Object(ObjectIndex::from_raw(1)),
        ConstValue::Object(ObjectIndex::from_raw(2)),
    ];
    assert_eq!(function(&program, 3).bytecode.constants, expected);
    assert_eq!(function(&program, 4).bytecode.constants, expected);
    // Slots: f, g, then the tail's `$init`.
    assert_eq!(program.globals.len(), 3);
    assert_eq!(program.init_order, vec![1]);
    let user = package_named(&program, "user");
    assert_eq!(user.init, Some(ObjectIndex::from_raw(4)));
    assert_eq!(
        user.global_slot(&free_fn("g")),
        Some(GlobalIndex::from_raw(1))
    );
}

#[test]
fn dependency_slots_bind_through_edge_tables() {
    let record = interface_record(b"iface");
    let widget = unit_with_class("Widget", "Widget");
    let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
    // The consumer reaches `app` under its own edge name.
    consumer
        .dependencies
        .push(direct("gadgets", fingerprint_of(&record)));
    consumer
        .object_imports
        .push(import(1, DeclPath::Class(item("Widget"))));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &widget, &record),
            package("user", vec![("gadgets", 0)], &consumer, &record),
        ],
    };
    let program = linked(&set);
    assert_eq!(
        function(&program, 1).bytecode.instructions[0],
        Instruction::AllocInstance {
            class_obj: ObjectIndex::from_raw(0),
            ntypeargs: 0,
        }
    );
    assert_eq!(
        package_named(&program, "app").classes[&item("Widget")],
        ObjectIndex::from_raw(0)
    );

    // The same set with the edge missing: the dependency cannot bind.
    let mut edgeless = set.clone();
    edgeless.packages[1].edges.clear();
    assert_eq!(
        link(&edgeless).err(),
        Some(LinkError::UnknownDependency {
            package: Name::new("user"),
            edge: Name::new("gadgets"),
        })
    );
}

#[test]
fn transitive_dependencies_bind_through_the_parent_edge_table() {
    let record = interface_record(b"iface");
    let leaf = unit_with_class("Leaf", "Leaf");
    let middle = CompilationUnit::default();
    let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
    consumer.dependencies = vec![
        direct("b", fingerprint_of(&record)),
        Locator::Transitive {
            via: DepSlot(1),
            edge: Name::new("inner"),
        },
    ];
    consumer
        .object_imports
        .push(import(2, DeclPath::Class(item("Leaf"))));
    let set = LinkSet {
        root: LinkPackageId(2),
        packages: vec![
            package("c", vec![], &leaf, &record),
            package("b", vec![("inner", 0)], &middle, &record),
            package("user", vec![("b", 1)], &consumer, &record),
        ],
    };
    let program = linked(&set);
    assert_eq!(
        function(&program, 1).bytecode.instructions[0],
        Instruction::AllocInstance {
            class_obj: ObjectIndex::from_raw(0),
            ntypeargs: 0,
        }
    );
}

#[test]
fn layout_is_package_major_in_set_order() {
    let stdlib_record = PackageRecord {
        interface_blob: Vec::new(),
    };
    let user_record = PackageRecord {
        interface_blob: vec![1, 2, 3],
    };
    let mut std_unit = unit_with_fn("baml.x", vec![Instruction::Return]);
    std_unit.classes.push(class("S"));
    std_unit
        .exports
        .objects
        .push((DeclPath::Class(item("S")), LocalRef::Class(0)));
    let mut user_unit = unit_with_fn("user.f", vec![Instruction::Return]);
    user_unit.classes.push(class("U"));
    user_unit
        .exports
        .objects
        .push((DeclPath::Class(item("U")), LocalRef::Class(0)));
    user_unit
        .exports
        .globals
        .push((DeclPath::Let(item("x")), 1));
    let stdlib = package("baml", vec![], &std_unit, &stdlib_record);
    let set = LinkSet {
        root: LinkPackageId(0),
        // Listed user-first on purpose: layout follows the SET order, each
        // package's objects contiguous.
        packages: vec![
            package("user", vec![("baml", 1)], &user_unit, &user_record),
            stdlib,
        ],
    };
    let program = linked(&set);
    assert_eq!(
        function_names(&program),
        vec!["class user.U", "user.f", "class user.S", "baml.x"]
    );
    let callables = program.rendered_callables();
    assert_eq!(callables["user.f"].object, ObjectIndex::from_raw(1));
    assert_eq!(callables["user.f"].slot, GlobalIndex::from_raw(0));
    assert_eq!(program.rendered_lets()["user.x"], GlobalIndex::from_raw(1));
    assert_eq!(program.globals[1], ConstValue::Null);
    assert_eq!(callables["baml.x"].object, ObjectIndex::from_raw(3));
    assert_eq!(callables["baml.x"].slot, GlobalIndex::from_raw(2));
    let user = package_named(&program, "user");
    assert_eq!(callables["user.f"].object, ObjectIndex::from_raw(1));
    assert_eq!(user.interface_blob, vec![1, 2, 3]);
    assert_eq!(
        user.global_slot(&DeclPath::Let(item("x"))),
        Some(GlobalIndex::from_raw(1))
    );
    assert_eq!(
        program
            .packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>(),
        vec!["user", "baml"]
    );
    // The linker owns the tags: each declaration's is its image index.
    for (abs, expected) in [(0, "user.U"), (2, "user.S")] {
        let Object::Class(class) = &program.objects[ObjectIndex::from_raw(abs)] else {
            panic!("object {abs} is not a class")
        };
        assert_eq!(class.name.to_string(), expected);
        assert_eq!(class.type_tag, TypeTag::of_static_index(abs));
    }
}

/// An init tail whose `$init` does nothing: what a package with `let`s
/// carries, as far as the init order is concerned.
fn let_bearing_tail(name: &str) -> InitTail {
    InitTail {
        objects: vec![func(&format!("{name}.$init"), vec![Instruction::Return])],
        slot_objects: vec![0],
        init: Some(0),
        test_objects_start: 1,
        test_slots_start: 1,
        ..InitTail::default()
    }
}

/// One package of an init-order scenario: its name, its edges by the
/// target's position, and whether it has `let`s.
struct Member {
    name: &'static str,
    edges: Vec<(&'static str, u32)>,
    lets: bool,
}

fn with_lets(name: &'static str, edges: &[(&'static str, u32)]) -> Member {
    Member {
        name,
        edges: edges.to_vec(),
        lets: true,
    }
}

fn without_lets(name: &'static str, edges: &[(&'static str, u32)]) -> Member {
    Member {
        name,
        edges: edges.to_vec(),
        lets: false,
    }
}

/// The init order of a set of packages, in set order.
fn init_order_of(members: &[Member]) -> Result<Vec<u32>, LinkError> {
    let record = interface_record(b"iface");
    let empty = CompilationUnit::default();
    let tails: Vec<Option<InitTail>> = members
        .iter()
        .map(|member| member.lets.then(|| let_bearing_tail(member.name)))
        .collect();
    let set = LinkSet {
        packages: members
            .iter()
            .zip(&tails)
            .map(|(member, tail)| {
                let mut linked = package(member.name, member.edges.clone(), &empty, &record);
                linked.tail = tail.as_ref();
                linked
            })
            .collect(),
        root: LinkPackageId(0),
    };
    link(&set).map(|program| program.init_order)
}

/// A package's `let`s may read the `let`s of any package it reaches, through
/// packages that have none: `app` reaches `zeta` only through `mid`, and
/// still initializes after it, whatever the names say.
#[test]
fn init_order_follows_reach_through_packages_without_lets() {
    let order = init_order_of(&[
        with_lets("app", &[("mid", 1)]),
        without_lets("mid", &[("zeta", 2)]),
        with_lets("zeta", &[]),
    ]);
    assert_eq!(order, Ok(vec![2, 0]));
}

/// Packages without `let`s may reach each other — the language packages all
/// do — without ordering anything.
#[test]
fn packages_without_lets_may_reach_each_other() {
    let order = init_order_of(&[
        with_lets("app", &[("left", 1)]),
        without_lets("left", &[("right", 2)]),
        without_lets("right", &[("left", 1), ("zeta", 3)]),
        with_lets("zeta", &[]),
    ]);
    assert_eq!(order, Ok(vec![3, 0]));
}

/// Two packages may share a name (names live on edges); the order among
/// packages free to initialize is by name, then by position, so every link
/// of one set orders them the same way.
#[test]
fn init_order_breaks_a_tie_between_same_named_packages_by_position() {
    let set = [
        with_lets("user", &[]),
        with_lets("lib", &[]),
        with_lets("lib", &[]),
        with_lets("lib", &[]),
    ];
    for _ in 0..32 {
        assert_eq!(init_order_of(&set), Ok(vec![1, 2, 3, 0]));
    }
}

/// Two packages with `let`s that reach each other cannot both initialize
/// after the other.
#[test]
fn an_init_cycle_is_refused_naming_both_packages() {
    let order = init_order_of(&[
        without_lets("user", &[]),
        with_lets("ping", &[("pong", 2)]),
        with_lets("pong", &[("ping", 1)]),
    ]);
    assert_eq!(
        order,
        Err(LinkError::InitCycle {
            package: Name::new("ping"),
            other: Name::new("pong"),
        })
    );
}

#[test]
fn init_order_is_topological_with_alphabetical_ties() {
    // `a` depends on `c`; `b` is independent. In-degree-zero packages sort
    // by name (b, c), then `a` is released.
    let record = interface_record(b"iface");
    let empty = CompilationUnit::default();
    let tails: Vec<InitTail> = ["a", "b", "c"]
        .into_iter()
        .map(|name| InitTail {
            objects: vec![func(&format!("{name}.$init"), vec![Instruction::Return])],
            slot_objects: vec![0],
            init: Some(0),
            test_objects_start: 1,
            test_slots_start: 1,
            ..InitTail::default()
        })
        .collect();
    let mut packages = vec![
        package("a", vec![("c", 2)], &empty, &record),
        package("b", vec![], &empty, &record),
        package("c", vec![], &empty, &record),
    ];
    for (package, tail) in packages.iter_mut().zip(&tails) {
        package.tail = Some(tail);
    }
    let program = link(&LinkSet {
        packages,
        root: LinkPackageId(0),
    })
    .unwrap();
    assert_eq!(program.init_order, vec![1, 2, 0]);
    // Placement is set order; only the execution order is topological.
    assert_eq!(
        function_names(&program),
        vec!["a.$init", "b.$init", "c.$init"]
    );
}

#[test]
fn heads_and_switch_keys_relocate_to_the_assigned_tags() {
    // `app` declares `Foo`; `user` names it three ways — a signature head,
    // a declaration key of a type switch, and an `AllocInstance` operand — all by
    // the same import. After the link every one of them is `Foo`'s image
    // index, and the switch is solved over that tag.
    let record = interface_record(b"iface");
    let provider = unit_with_class("Foo", "Foo");
    let mut consumer = unit_with_fn(
        "user.f",
        vec![
            Instruction::DenseTag(0),
            alloc_import(0),
            Instruction::Return,
        ],
    );
    let Object::Function(consumer_fn) = &mut consumer.code[0] else {
        unreachable!()
    };
    let head = TypeHead::unresolved_operand(ObjectIndex::from_raw(import_operand(0)));
    consumer_fn.param_types = vec![bex_vm_types::TyTemplate::Class(head, Box::new([]))];
    consumer_fn.bytecode.switch_tables = vec![SwitchTable::of_keys(
        vec![
            SwitchKey::Kind(baml_type::typetag::INT),
            SwitchKey::Declaration(ObjectIndex::from_raw(import_operand(0))),
        ],
        vec!["int".to_string(), "Foo".to_string()],
    )];
    consumer
        .dependencies
        .push(direct("app", fingerprint_of(&record)));
    consumer
        .object_imports
        .push(import(1, DeclPath::Class(item("Foo"))));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &consumer, &record),
        ],
    };
    let program = linked(&set);
    let foo = TypeTag::of_static_index(0);
    let Object::Class(class) = &program.objects[ObjectIndex::from_raw(0)] else {
        panic!("object 0 is not the class")
    };
    assert_eq!(class.type_tag, foo);
    let f = function(&program, 1);
    let bex_vm_types::TyTemplate::Class(head, _) = &f.param_types[0] else {
        panic!("the parameter type is a class")
    };
    assert_eq!(head.tag(), foo);
    assert_eq!(
        f.bytecode.instructions[1],
        Instruction::AllocInstance {
            class_obj: ObjectIndex::from_raw(0),
            ntypeargs: 0,
        }
    );
    let table = &f.bytecode.switch_tables[0];
    assert!(table.dispatch.is_dispatchable());
    assert_eq!(table.arm_of(baml_type::typetag::INT), Some(0));
    assert_eq!(
        table.arm_of(foo.as_i64()),
        Some(1),
        "the key operand relocated like every other object operand, and the \
         table is solved over the tag its declaration was assigned"
    );
    assert_eq!(table.arm_of(baml_type::typetag::STRING), None);
}

/// `count` distinct indices below `range` with no structure to them, drawn
/// in order from a fixed sequence (`SplitMix64` from state zero).
fn scattered(count: usize, range: u64) -> Vec<usize> {
    let mut state = 0u64;
    let mut indices = Vec::with_capacity(count);
    while indices.len() < count {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^= mixed >> 31;
        let index = usize::try_from(mixed % range).expect("the range fits usize");
        if !indices.contains(&index) {
            indices.push(index);
        }
    }
    indices
}

/// A switch links wherever its declarations land. Sixty-four class arms
/// scattered over a few thousand objects have tags that no multiply-shift
/// hash separates within 128 slots, so a search that stops there finds
/// nothing; the table is solved regardless.
#[test]
fn a_switch_over_many_sparse_declarations_links() {
    const CLASSES: usize = 3000;
    const ARMS: usize = 64;
    let record = interface_record(b"iface");
    let provider = CompilationUnit {
        classes: (0..CLASSES)
            .map(|index| {
                let mut object = class(&format!("C{index}"));
                let Object::Class(declared) = &mut object else {
                    unreachable!()
                };
                // Its own tag field is its operand: its class-bucket offset.
                declared.type_tag =
                    TypeHead::unresolved_operand(ObjectIndex::from_raw(index)).tag();
                object
            })
            .collect(),
        exports: ExportTable {
            objects: (0..CLASSES)
                .map(|index| {
                    (
                        DeclPath::Class(item(&format!("C{index}"))),
                        LocalRef::Class(u32::try_from(index).expect("few classes")),
                    )
                })
                .collect(),
            globals: Vec::new(),
        },
        ..CompilationUnit::default()
    };
    // Arms over classes far apart in the image, in no order.
    let armed = scattered(ARMS, CLASSES as u64);
    let mut consumer = unit_with_fn(
        "user.f",
        vec![Instruction::DenseTag(0), Instruction::Return],
    );
    consumer
        .dependencies
        .push(direct("app", fingerprint_of(&record)));
    for &class in &armed {
        consumer
            .object_imports
            .push(import(1, DeclPath::Class(item(&format!("C{class}")))));
    }
    let Object::Function(consumer_fn) = &mut consumer.code[0] else {
        unreachable!()
    };
    consumer_fn.bytecode.switch_tables = vec![SwitchTable::of_keys(
        (0..ARMS)
            .map(|arm| SwitchKey::Declaration(ObjectIndex::from_raw(import_operand(arm))))
            .collect(),
        armed.iter().map(|class| format!("C{class}")).collect(),
    )];
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &consumer, &record),
        ],
    };
    let program = linked(&set);
    let table = &function(&program, CLASSES).bytecode.switch_tables[0];
    assert!(table.dispatch.is_dispatchable());
    for (arm, &class) in armed.iter().enumerate() {
        assert_eq!(
            table.arm_of(TypeTag::of_static_index(class).as_i64()),
            Some(u32::try_from(arm).expect("few arms")),
            "class C{class} selects arm {arm}"
        );
    }
    let unarmed = (0..CLASSES)
        .find(|class| !armed.contains(class))
        .expect("most classes have no arm");
    assert_eq!(
        table.arm_of(TypeTag::of_static_index(unarmed).as_i64()),
        None
    );
}

/// A unit states a switch by its keys. One that arrives with values was
/// solved over tags of some other layout, and is refused rather than trusted.
#[test]
fn a_unit_that_states_a_switch_by_values_is_refused() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_fn(
        "user.f",
        vec![Instruction::DenseTag(0), Instruction::Return],
    );
    let Object::Function(function) = &mut unit.code[0] else {
        unreachable!()
    };
    function.bytecode.switch_tables = vec![SwitchTable {
        dispatch: SwitchDispatch::Sorted(vec![SwitchEntry { value: 7, arm: 0 }]),
        key_names: vec!["seven".to_string()],
    }];
    let set = LinkSet {
        root: LinkPackageId(0),
        packages: vec![package("user", vec![], &unit, &record)],
    };
    let error = link(&set).expect_err("a solved table in a unit is not a unit's table");
    assert!(
        matches!(&error, LinkError::InvalidUnit(message) if message.contains("instead of keys")),
        "{error}"
    );
}

#[test]
fn impl_body_import_from_a_dependency_is_refused() {
    let record = interface_record(b"iface");
    let provider = CompilationUnit::default();
    let mut unit = unit_with_fn(
        "user.f",
        vec![
            Instruction::Call {
                callee: GlobalIndex::from_raw(import_operand(0)),
                ntypeargs: 0,
            },
            Instruction::Return,
        ],
    );
    unit.dependencies
        .push(direct("app", fingerprint_of(&record)));
    let body = DeclPath::InterfaceBody(BodyKey::ImplMethod(Box::new(ImplBodyKey {
        interface: InterfaceKey {
            package: baml_type::wire::EdgePath::own(),
            path: item("Greeter"),
        },
        coherence: bex_vm_types::ImplBodyCoherence {
            for_ty_pattern: baml_type::TyTemplate::Int,
            interface_args: Vec::new(),
            generic_param_bounds: Vec::new(),
        },
        method: Name::new("greet"),
    })));
    unit.global_imports.push(import(1, body));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &unit, &record),
        ],
    };
    assert!(matches!(
        link(&set),
        Err(LinkError::InvalidUnit(message)) if message.contains("own package")
    ));
}

#[test]
fn a_tail_reaches_its_own_packages_declarations_as_imports() {
    // A `$init` helper constructing one of its package's own classes: the
    // tail is its own table, so the class is an import at `SELF`.
    let record = interface_record(b"iface");
    let unit = unit_with_class("Widget", "Widget");
    let tail = InitTail {
        objects: vec![func("$init", vec![alloc_import(0), Instruction::Return])],
        object_imports: vec![import(0, DeclPath::Class(item("Widget")))],
        slot_objects: vec![0],
        init: Some(0),
        test_objects_start: 1,
        test_slots_start: 1,
        ..InitTail::default()
    };
    let mut user = package("user", vec![], &unit, &record);
    user.tail = Some(&tail);
    let program = link(&LinkSet {
        root: LinkPackageId(0),
        packages: vec![user],
    })
    .unwrap();
    assert_eq!(function_names(&program), vec!["class user.Widget", "$init"]);
    assert_eq!(
        function(&program, 1).bytecode.instructions[0],
        Instruction::AllocInstance {
            class_obj: ObjectIndex::from_raw(0),
            ntypeargs: 0,
        }
    );
}

/// An impl rule's provided body must be an interface body: a unit whose rule
/// aims a method at a NAMED function is refused at link, never dispatched to
/// the wrong arity and frame convention.
#[test]
fn a_rule_body_aimed_at_a_named_function_is_an_invalid_unit() {
    use baml_linker_types::{ProgramImplRuleFrag, ProgramMethodImplFrag};
    use bex_vm_types::types::InterfaceDef;

    let record = interface_record(b"iface");
    let mut unit = unit_with_fn("user.named", vec![Instruction::Return]);
    unit.interfaces
        .push(Object::Interface(Box::new(InterfaceDef {
            structural_default: None,
            name: baml_type::TypeName::local(Name::new("Greeter")),
            type_tag: TypeHead::unresolved_operand(ObjectIndex::from_raw(0)).tag(),
            args: Vec::new(),
            requires: Vec::new(),
            assoc: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            owner: bex_vm_types::HeapPtr::null(),
        })));
    unit.exports
        .objects
        .push((DeclPath::Interface(item("Greeter")), LocalRef::Interface(0)));
    unit.impl_rules.push(ProgramImplRuleFrag {
        // Unit convention: the interface bucket follows the (empty) class and
        // enum buckets, so the interface is object 0; code offset 0 is the
        // named function.
        interface_head: ObjectIndex::from_raw(0),
        for_ty_pattern: baml_type::TyTemplate::Int,
        generic_param_bounds: Vec::new(),
        interface_args: Vec::new(),
        interface_assoc: Vec::new(),
        methods: vec![(
            Name::new("greet"),
            ProgramMethodImplFrag {
                code_offset: 0,
                frame: Vec::new(),
            },
        )],
        field_links: Box::new([]),
    });
    let set = LinkSet {
        root: LinkPackageId(0),
        packages: vec![package("user", vec![], &unit, &record)],
    };
    assert!(matches!(
        link(&set),
        Err(LinkError::InvalidUnit(message)) if message.contains("not an interface body")
    ));
}

#[test]
fn a_unit_compiled_against_another_interface_is_refused() {
    // `user` recorded the digest of interface `v2`; the set binds `app` to a
    // package carrying `v1`. The edge locates, the fingerprint binds.
    let record = interface_record(b"v1");
    let other = interface_record(b"v2");
    let provider = unit_with_class("Widget", "Widget");
    let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
    consumer
        .dependencies
        .push(direct("app", fingerprint_of(&other)));
    consumer
        .object_imports
        .push(import(1, DeclPath::Class(item("Widget"))));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &consumer, &record),
        ],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::InterfaceMismatch {
            package: Name::new("user"),
            edge: Name::new("app"),
            expected: fingerprint_of(&other),
            found: fingerprint_of(&record),
        })
    );
}

#[test]
fn a_direct_dependency_without_a_fingerprint_is_malformed() {
    let record = interface_record(b"iface");
    let provider = unit_with_class("Widget", "Widget");
    let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
    // A declared edge recorded as a prelude entry: no digest to bind by.
    consumer.dependencies.push(prelude("app"));
    consumer
        .object_imports
        .push(import(1, DeclPath::Class(item("Widget"))));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &consumer, &record),
        ],
    };
    assert!(matches!(
        link(&set),
        Err(LinkError::InvalidUnit(message)) if message.contains("no interface fingerprint")
    ));
}

#[test]
fn a_prelude_dependency_carries_no_fingerprint() {
    // The prelude is bound by its fixed name and carries no blob; a unit
    // recording a fingerprint for it is malformed, one recording none links.
    let stdlib = unit_with_class("S", "S");
    let make_consumer = |fingerprint: Option<Digest>| {
        let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
        consumer.dependencies.push(match fingerprint {
            Some(digest) => direct("baml", digest),
            None => prelude("baml"),
        });
        consumer
            .object_imports
            .push(import(1, DeclPath::Class(item("S"))));
        consumer
    };
    let empty = PackageRecord {
        interface_blob: Vec::new(),
    };
    let set = |consumer| {
        let mut set = LinkSet {
            root: LinkPackageId(1),
            packages: vec![
                package("baml", vec![], &stdlib, &empty),
                package("user", vec![("baml", 0)], consumer, &empty),
            ],
        };
        set.packages[1].edges[0].kind = EdgeKind::Prelude;
        set
    };
    let unfingerprinted = make_consumer(None);
    link(&set(&unfingerprinted)).unwrap();
    let fingerprinted = make_consumer(Some([7; 32]));
    assert!(matches!(
        link(&set(&fingerprinted)),
        Err(LinkError::InvalidUnit(message)) if message.contains("prelude")
    ));
}

#[test]
fn a_declared_dependency_without_an_interface_payload_is_refused() {
    let empty = PackageRecord {
        interface_blob: Vec::new(),
    };
    let provider = unit_with_class("Widget", "Widget");
    let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
    consumer.dependencies.push(direct("app", [7; 32]));
    consumer
        .object_imports
        .push(import(1, DeclPath::Class(item("Widget"))));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &empty),
            package("user", vec![("app", 0)], &consumer, &empty),
        ],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::MissingInterface {
            package: Name::new("user"),
            edge: Name::new("app"),
        })
    );
}

/// Link `set`, and hold the result to the executable's laws: what the link
/// produces by construction is exactly what a decoded program is checked
/// against, so every linked program here is the oracle for that check.
fn linked(set: &LinkSet<'_>) -> Program {
    let program = link(set).expect("the set links");
    program
        .validate()
        .expect("a linked program satisfies the executable's laws");
    program
}

/// A one-package set around `unit`, with `tail` when given.
fn lone(unit: &CompilationUnit, tail: Option<&InitTail>, record: &PackageRecord) -> Program {
    let mut package = package("user", vec![], unit, record);
    package.tail = tail;
    linked(&LinkSet {
        root: LinkPackageId(0),
        packages: vec![package],
    })
}

fn refused(unit: &CompilationUnit, tail: Option<&InitTail>, record: &PackageRecord) -> String {
    let mut package = package("user", vec![], unit, record);
    package.tail = tail;
    match link(&LinkSet {
        root: LinkPackageId(0),
        packages: vec![package],
    }) {
        Err(LinkError::InvalidUnit(message)) => message,
        other => panic!("expected an invalid unit, got {other:?}"),
    }
}

/// A malformed unit is refused as such, never a panic, whatever it puts
/// where: each bucket holds its kind, an exported function is a function, a
/// head and a type switch's declaration key name type declarations, and a
/// head is in the unit convention. The same units link once corrected.
#[test]
fn a_unit_whose_buckets_hold_another_kind_is_refused() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_class("Foo", "Foo");
    // A function in the class bucket.
    unit.classes
        .push(func("user.stray", vec![Instruction::Return]));
    assert!(refused(&unit, None, &record).contains("of `classes`"));

    // A class in the code bucket: a declaration no export names.
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    unit.code.push(class("Stray"));
    assert!(refused(&unit, None, &record).contains("of `code`"));

    // A class in a tail.
    let unit = unit_with_fn("user.f", vec![Instruction::Return]);
    let mut tail = let_bearing_tail("user");
    tail.objects.push(class("Stray"));
    tail.test_objects_start = 2;
    assert!(refused(&unit, Some(&tail), &record).contains("of `objects`"));
}

#[test]
fn an_exported_function_must_be_a_function() {
    let record = interface_record(b"iface");
    // The code bucket holds a string where the export says a function is.
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    unit.code[0] = Object::String("not code".into());
    assert!(refused(&unit, None, &record).contains("as a String object"));

    // A function exported as an interface body, and a body as a function.
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    let Object::Function(function) = &mut unit.code[0] else {
        unreachable!()
    };
    function.is_interface_body = true;
    assert!(refused(&unit, None, &record).contains("exports function f"));
}

#[test]
fn a_head_naming_a_code_object_is_refused() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    let Object::Function(function) = &mut unit.code[0] else {
        unreachable!()
    };
    // Operand 0 is the function itself: the unit has no type buckets.
    function.param_types = vec![bex_vm_types::TyTemplate::Class(
        TypeHead::unresolved_operand(ObjectIndex::from_raw(0)),
        Box::new([]),
    )];
    assert!(refused(&unit, None, &record).contains("not a type declaration"));

    // A head importing a FUNCTION of a dependency.
    let provider = unit_with_fn("app.make", vec![Instruction::Return]);
    let mut consumer = unit_with_fn("user.f", vec![Instruction::Return]);
    let Object::Function(function) = &mut consumer.code[0] else {
        unreachable!()
    };
    function.param_types = vec![bex_vm_types::TyTemplate::Class(
        TypeHead::unresolved_operand(ObjectIndex::from_raw(import_operand(0))),
        Box::new([]),
    )];
    consumer
        .dependencies
        .push(direct("app", fingerprint_of(&record)));
    consumer.object_imports.push(import(1, free_fn("make")));
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &consumer, &record),
        ],
    };
    assert!(matches!(
        link(&set),
        Err(LinkError::InvalidUnit(message)) if message.contains("where a type declaration is required")
    ));
}

#[test]
fn a_head_outside_the_unit_convention_is_refused() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    let Object::Function(function) = &mut unit.code[0] else {
        unreachable!()
    };
    // A head decodes from its tag alone, so a decoded unit may carry any
    // tag; a negative one encodes no operand.
    let negative = <TypeHead as borsh::BorshDeserialize>::try_from_slice(
        &borsh::to_vec(&TypeTag::from_i64(-3)).expect("serialize"),
    )
    .expect("decode");
    function.param_types = vec![bex_vm_types::TyTemplate::Class(negative, Box::new([]))];
    assert!(refused(&unit, None, &record).contains("outside the unit convention"));
}

#[test]
fn a_switch_keyed_on_a_code_object_is_refused() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_fn(
        "user.f",
        vec![Instruction::DenseTag(0), Instruction::Return],
    );
    let Object::Function(function) = &mut unit.code[0] else {
        unreachable!()
    };
    function.bytecode.switch_tables = vec![SwitchTable::of_keys(
        vec![SwitchKey::Declaration(ObjectIndex::from_raw(0))],
        vec!["f".to_string()],
    )];
    assert!(refused(&unit, None, &record).contains("not a type declaration"));
}

#[test]
fn a_tail_part_nothing_names_is_refused() {
    let record = interface_record(b"iface");
    let unit = unit_with_fn("user.f", vec![Instruction::Return]);
    // A session submission's shape: helpers in the init part, no `$init`.
    // The graft runs it; the static linker would place nothing and leave
    // every operand into it dangling, so it refuses the tail.
    let mut tail = let_bearing_tail("user");
    tail.init = None;
    assert!(refused(&unit, Some(&tail), &record).contains("no `$init` to run them"));

    // Named, or empty, it links.
    let program = lone(&unit, Some(&let_bearing_tail("user")), &record);
    assert!(program.packages[0].init.is_some());
    let program = lone(&unit, Some(&InitTail::default()), &record);
    assert!(program.packages[0].init.is_none());
}

#[test]
fn a_transitive_entry_reached_via_itself_or_a_later_slot_is_refused() {
    let record = interface_record(b"iface");
    let leaf = unit_with_class("Leaf", "Leaf");
    let mut consumer = unit_with_fn("user.f", vec![Instruction::Return]);
    for via in [1, 2] {
        // Slot 1 reached via slot 1 (itself), then via slot 2 (later).
        consumer.dependencies = vec![Locator::Transitive {
            via: DepSlot(via),
            edge: Name::new("app"),
        }];
        let set = LinkSet {
            root: LinkPackageId(1),
            packages: vec![
                package("app", vec![], &leaf, &record),
                package("user", vec![("app", 0)], &consumer, &record),
            ],
        };
        assert!(matches!(
            link(&set),
            Err(LinkError::InvalidUnit(message)) if message.contains("not earlier")
        ));
    }
}

#[test]
fn an_unknown_transitive_edge_names_the_package_whose_table_lacks_it() {
    let record = interface_record(b"iface");
    let leaf = unit_with_class("Leaf", "Leaf");
    let middle = CompilationUnit::default();
    let mut consumer = unit_with_fn("user.f", vec![Instruction::Return]);
    consumer.dependencies = vec![
        direct("b", fingerprint_of(&record)),
        // `b` has no edge named `nothing`.
        Locator::Transitive {
            via: DepSlot(1),
            edge: Name::new("nothing"),
        },
    ];
    let set = LinkSet {
        root: LinkPackageId(2),
        packages: vec![
            package("c", vec![], &leaf, &record),
            package("b", vec![("inner", 0)], &middle, &record),
            package("user", vec![("b", 1)], &consumer, &record),
        ],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::UnknownDependency {
            package: Name::new("b"),
            edge: Name::new("nothing"),
        })
    );
}

#[test]
fn a_runtime_only_edge_kind_is_refused() {
    let record = interface_record(b"iface");
    let provider = unit_with_class("Foo", "Foo");
    let mut consumer = unit_with_fn("user.f", vec![Instruction::Return]);
    consumer
        .dependencies
        .push(direct("app", fingerprint_of(&record)));
    for kind in [EdgeKind::ReExported, EdgeKind::Anonymous] {
        let mut set = LinkSet {
            root: LinkPackageId(1),
            packages: vec![
                package("app", vec![], &provider, &record),
                package("user", vec![("app", 0)], &consumer, &record),
            ],
        };
        set.packages[1].edges[0].kind = kind;
        assert!(matches!(
            link(&set),
            Err(LinkError::InvalidUnit(message)) if message.contains("runtime-only")
        ));
    }
}

#[test]
fn a_duplicate_global_export_is_refused() {
    let record = interface_record(b"iface");
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    unit.code.push(func("user.f", vec![Instruction::Return]));
    // Two slots for one path: the slot table, not the object table.
    unit.exports.globals.push((free_fn("f"), 1));
    let set = LinkSet {
        root: LinkPackageId(0),
        packages: vec![package("user", vec![], &unit, &record)],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::DuplicateExport {
            package: Name::new("user"),
            path: Box::new(free_fn("f")),
        })
    );
}

#[test]
fn load_current_package_is_rewritten_to_the_packages_ordinal() {
    let record = interface_record(b"iface");
    let first = unit_with_fn(
        "app.a",
        vec![Instruction::LoadCurrentPackage(0), Instruction::Return],
    );
    let second = unit_with_fn(
        "user.b",
        vec![Instruction::LoadCurrentPackage(0), Instruction::Return],
    );
    let set = LinkSet {
        root: LinkPackageId(1),
        packages: vec![
            package("app", vec![], &first, &record),
            package("user", vec![], &second, &record),
        ],
    };
    let program = linked(&set);
    assert_eq!(
        function(&program, 0).bytecode.instructions[0],
        Instruction::LoadCurrentPackage(0)
    );
    assert_eq!(
        function(&program, 1).bytecode.instructions[0],
        Instruction::LoadCurrentPackage(1),
        "a unit says `this package`; the link says which"
    );
}
