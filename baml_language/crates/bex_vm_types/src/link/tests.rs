use baml_type::typetag::TypeTag;

use super::*;
use crate::{
    ConstValue, GlobalIndex, Instruction, Object, ObjectIndex, RealizedTy,
    bytecode::Bytecode,
    types::{
        Class, Function, FunctionCaptureProps, FunctionKind, FunctionOrigin, GenericFunction,
        LocalName, ProgramPackage,
    },
    unit::{
        BodyKey, DeclKey, DeclPath, DepSlot, DependencyEntry, Digest, ExportTable, FnPath,
        ImplBodyKey, ImportEntry, InterfaceKey, LocalRef, import_operand,
    },
};

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
        local_names: Vec::new(),
        debug_locals: Vec::new(),
        span: baml_base::Span::fake(),
        return_type: crate::TyTemplate::Unknown,
        param_names: Vec::new(),
        param_types: Vec::new(),
        param_has_default: Vec::new(),
        display_type_params: Vec::new(),
        generic_param_bounds: Vec::new(),
        display_param_types: Vec::new(),
        display_return_type: String::new(),
        throws_type: crate::TyTemplate::Never,
        origin: FunctionOrigin::Internal,
        is_interface_body: false,
        native_key: None,
        body_meta: None,
        capture: FunctionCaptureProps::disabled(),
        function_id: 0,
        runtime_package: crate::HeapPtr::null(),
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

/// A class named `name` in the local package (displays as `user.<name>`).
fn class(name: &str) -> Object {
    Object::Class(Box::new(Class {
        name: crate::DeclarationName::Declared(baml_type::QualifiedTypeName::local(Name::new(
            name,
        ))),
        fields: Vec::new(),
        description: None,
        alias: None,
        docstring: None,
        other: indexmap::IndexMap::new(),
        stream_done: false,
        type_tag: TypeTag::of_head(name),
        has_cleanup: false,
        methods: indexmap::IndexMap::new(),
        generic_param_count: 0,
        owner: crate::HeapPtr::null(),
    }))
}

fn generic_value(function: usize, type_args: Vec<RealizedTy>) -> Object {
    Object::GenericFunction(GenericFunction {
        function: GlobalIndex::from_raw(function),
        type_args: type_args.into_boxed_slice(),
        runtime_package: crate::HeapPtr::null(),
    })
}

fn free_fn(name: &str) -> DeclPath {
    DeclPath::Function(FnPath::Free(item(name)))
}

fn import(dep: u32, path: DeclPath) -> ImportEntry {
    let baked_tag = path.is_type().then(|| TypeTag::of_head("baked"));
    ImportEntry {
        key: DeclKey {
            dep: DepSlot(dep),
            path,
        },
        baked_tag,
    }
}

fn direct(edge: &str, fingerprint: Option<Digest>) -> DependencyEntry {
    DependencyEntry {
        edge: Name::new(edge),
        via: DepSlot::SELF,
        fingerprint,
    }
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
        group: LinkGroup::User,
        edges: edges
            .into_iter()
            .map(|(edge, id)| (Name::new(edge), LinkPackageId(id)))
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
    let record = PackageRecord::default();
    let provider = unit_with_class("Foo", "Foo");
    // The consumer asks for a FUNCTION named Foo: the class must not
    // satisfy it.
    let mut consumer = unit_with_fn("user.g", vec![load_import(0), Instruction::Return]);
    consumer.dependencies.push(direct("app", None));
    consumer.global_imports.push(import(1, free_fn("Foo")));
    let set = LinkSet {
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &consumer, &record),
        ],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::UnresolvedImport {
            package: Name::new("app"),
            path: free_fn("Foo"),
        })
    );
}

#[test]
fn duplicate_export_is_refused() {
    let record = PackageRecord::default();
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    unit.code.push(func("user.f", vec![Instruction::Return]));
    unit.exports.objects.push((free_fn("f"), LocalRef::Code(1)));
    let set = LinkSet {
        packages: vec![package("user", vec![], &unit, &record)],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::DuplicateExport {
            package: Name::new("user"),
            path: free_fn("f"),
        })
    );
}

#[test]
fn two_packages_with_one_spelling_are_refused() {
    let record = PackageRecord::default();
    let unit = CompilationUnit::default();
    let set = LinkSet {
        packages: vec![
            package("user", vec![], &unit, &record),
            package("user", vec![], &unit, &record),
        ],
    };
    assert_eq!(
        link(&set).err(),
        Some(LinkError::DuplicateLinkName("user".to_string()))
    );
}

#[test]
fn a_static_unit_importing_its_own_package_is_malformed() {
    let record = PackageRecord::default();
    let mut unit = unit_with_fn("user.f", vec![load_import(0), Instruction::Return]);
    unit.global_imports.push(import(0, free_fn("f")));
    let set = LinkSet {
        packages: vec![package("user", vec![], &unit, &record)],
    };
    assert!(matches!(
        link(&set),
        Err(LinkError::InvalidUnit(message)) if message.contains("imports its own")
    ));
}

#[test]
fn generic_values_are_interned_by_base_slot_across_packages_and_tails() {
    let record = PackageRecord::default();
    // `lib` defines `f` (slot 0) and its own `f<int>` value (code 1).
    let mut lib = unit_with_fn("lib.f", vec![Instruction::Return]);
    lib.code.push(generic_value(0, vec![RealizedTy::Int]));
    // `user` imports `f`'s slot and carries its own copy of `f<int>`
    // (code 0) and a distinct `f<string>` (code 1); `g` references both.
    let mut user = CompilationUnit {
        dependencies: vec![direct("lib", None)],
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
        dependencies: vec![direct("lib", None)],
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
        packages: vec![
            package("lib", vec![], &lib, &record),
            package("user", vec![("lib", 0)], &user, &record),
        ],
    };
    set.packages[1].tail = Some(&tail);
    let program = link(&set).unwrap();
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
    assert_eq!(program.package_init_order, vec!["$init".to_string()]);
    let user = package_named(&program, "user");
    assert_eq!(user.init, Some(ObjectIndex::from_raw(4)));
    assert_eq!(user.globals[&free_fn("g")], GlobalIndex::from_raw(1));
}

#[test]
fn dependency_slots_bind_through_edge_tables() {
    let record = PackageRecord::default();
    let widget = unit_with_class("Widget", "Widget");
    let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
    // The consumer reaches `app` under its own edge name.
    consumer.dependencies.push(direct("gadgets", None));
    consumer
        .object_imports
        .push(import(1, DeclPath::Class(item("Widget"))));
    let set = LinkSet {
        packages: vec![
            package("app", vec![], &widget, &record),
            package("user", vec![("gadgets", 0)], &consumer, &record),
        ],
    };
    let program = link(&set).unwrap();
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
    let record = PackageRecord::default();
    let leaf = unit_with_class("Leaf", "Leaf");
    let middle = CompilationUnit::default();
    let mut consumer = unit_with_fn("user.make", vec![alloc_import(0), Instruction::Return]);
    consumer.dependencies = vec![
        direct("b", None),
        DependencyEntry {
            edge: Name::new("inner"),
            via: DepSlot(1),
            fingerprint: None,
        },
    ];
    consumer
        .object_imports
        .push(import(2, DeclPath::Class(item("Leaf"))));
    let set = LinkSet {
        packages: vec![
            package("c", vec![], &leaf, &record),
            package("b", vec![("inner", 0)], &middle, &record),
            package("user", vec![("b", 1)], &consumer, &record),
        ],
    };
    let program = link(&set).unwrap();
    assert_eq!(
        function(&program, 1).bytecode.instructions[0],
        Instruction::AllocInstance {
            class_obj: ObjectIndex::from_raw(0),
            ntypeargs: 0,
        }
    );
}

#[test]
fn layout_is_group_major_then_pass_major_with_rendered_views() {
    let stdlib_record = PackageRecord::default();
    let user_record = PackageRecord {
        exported_names: vec![item("f")],
        functions: vec![(item("f"), FnPath::Free(item("f")))],
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
    let mut stdlib = package("baml", vec![], &std_unit, &stdlib_record);
    stdlib.group = LinkGroup::Stdlib;
    let set = LinkSet {
        // Listed user-first on purpose: layout follows the GROUP, not the
        // set order.
        packages: vec![
            package("user", vec![("baml", 1)], &user_unit, &user_record),
            stdlib,
        ],
    };
    let program = link(&set).unwrap();
    assert_eq!(
        function_names(&program),
        vec!["class user.S", "baml.x", "class user.U", "user.f"]
    );
    assert_eq!(program.function_indices["baml.x"], 1);
    assert_eq!(program.function_global_indices["baml.x"], 0);
    assert_eq!(program.function_indices["user.f"], 3);
    assert_eq!(program.function_global_indices["user.f"], 1);
    assert_eq!(program.let_global_indices["user.x"], 2);
    assert_eq!(program.globals[2], ConstValue::Null);
    let user = package_named(&program, "user");
    assert_eq!(user.exported_names, vec![item("f")]);
    assert_eq!(user.functions[&item("f")], ObjectIndex::from_raw(3));
    assert_eq!(user.interface_blob, vec![1, 2, 3]);
    assert_eq!(
        user.globals[&DeclPath::Let(item("x"))],
        GlobalIndex::from_raw(2)
    );
    assert_eq!(
        program
            .packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>(),
        vec!["baml", "user"]
    );
}

#[test]
fn package_init_order_is_topological_with_alphabetical_ties() {
    // `a` depends on `c`; `b` is independent. In-degree-zero packages sort
    // by name (b, c), then `a` is released.
    let record = PackageRecord::default();
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
    let program = link(&LinkSet { packages }).unwrap();
    assert_eq!(
        program.package_init_order,
        vec![
            "b.$init".to_string(),
            "c.$init".to_string(),
            "a.$init".to_string()
        ]
    );
    assert_eq!(
        function_names(&program),
        vec!["b.$init", "c.$init", "a.$init"]
    );
}

#[test]
fn two_declarations_under_one_type_tag_are_refused() {
    let record = PackageRecord::default();
    let a = unit_with_class("Same", "A");
    let b = unit_with_class("Same", "B");
    let set = LinkSet {
        packages: vec![
            package("a", vec![], &a, &record),
            package("b", vec![], &b, &record),
        ],
    };
    assert!(matches!(link(&set), Err(LinkError::TagCollision(_))));
}

#[test]
fn impl_body_import_from_a_dependency_is_refused() {
    let record = PackageRecord::default();
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
    unit.dependencies.push(direct("app", None));
    let body = DeclPath::InterfaceBody(BodyKey::ImplMethod(Box::new(ImplBodyKey {
        interface: InterfaceKey {
            package: baml_type::Package::Local,
            path: item("Greeter"),
        },
        coherence: crate::types::ImplCoherenceKey {
            for_ty_pattern: crate::TyTemplate::Int,
            interface_args: Vec::new(),
            generic_param_bounds: Vec::new(),
        },
        method: Name::new("greet"),
    })));
    unit.global_imports.push(import(1, body));
    let set = LinkSet {
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
fn a_type_import_without_its_baked_tag_is_malformed() {
    let record = PackageRecord::default();
    let provider = unit_with_class("Foo", "Foo");
    let mut unit = unit_with_fn("user.f", vec![Instruction::Return]);
    unit.dependencies.push(direct("app", None));
    unit.object_imports.push(ImportEntry {
        key: DeclKey {
            dep: DepSlot(1),
            path: DeclPath::Class(item("Foo")),
        },
        baked_tag: None,
    });
    let set = LinkSet {
        packages: vec![
            package("app", vec![], &provider, &record),
            package("user", vec![("app", 0)], &unit, &record),
        ],
    };
    assert!(matches!(
        link(&set),
        Err(LinkError::InvalidUnit(message)) if message.contains("lacks a baked type tag")
    ));
}

#[test]
fn a_tail_reaches_its_own_packages_declarations_as_imports() {
    // A `$init` helper constructing one of its package's own classes: the
    // tail is its own table, so the class is an import at `SELF`.
    let record = PackageRecord::default();
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
