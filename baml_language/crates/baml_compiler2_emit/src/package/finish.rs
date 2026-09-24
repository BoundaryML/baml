//! Everything after a package's code pass: the backfills that needed the
//! bodies pooled, the rules, the record, the tail, and the head walk that
//! makes the import tables total.

use std::collections::{HashMap, HashSet};

use baml_base::{SourceFile, SourceRoot, SourceRootKind};
use baml_compiler2_hir::{
    contributions::Definition,
    item_data::{
        class_data, file_classes, file_functions, file_impls, file_interfaces, function_data,
        interface_data,
    },
    loc::{DeclRef, FunctionLoc, LetLoc},
    package::edge_table,
};
use baml_compiler2_hir_ty::{
    callable::ExternalCallTarget,
    package_interface::{export_interface, package_interface},
};
use baml_compiler2_mir::{RuntimeLowering, definition_link_name};
use baml_linker_types::{
    CompilationUnit, ExportTable, InitTail, LocalRef, PackageRecord, ProgramImplRuleFrag,
    ProgramMethodImplFrag,
};
use bex_vm_types::{
    ClassMethodDef, DeclPath, FnPath, Function, GlobalIndex, Object, ObjectIndex, ObjectPool,
    TypeHead, head_walk::visit_object_heads, types::LocalName,
};

use super::{
    EmittedPackage, HeadIndex, TypePass,
    bodies::{BodyPass, RefTables},
};
use crate::{
    ClassFieldSnapshot, LoweringError, OptLevel, UnitReferences,
    emit::CodegenRefs,
    items::{
        InitStep, bake_impl_rule, build_init_function, build_init_test_chainer, compile_let_helper,
        impl_rule_target,
    },
    refs::{LocalTables, Own, PackageRefs, item_path},
    topological_sort_lets,
};

/// Everything after the code pass: the backfills that needed the bodies
/// pooled, the rules, the record, the tail, the head walk, and the tables.
pub(super) fn finish<'db>(
    db: &'db dyn crate::Db,
    heads: &HeadIndex,
    class_fields: &ClassFieldSnapshot,
    types: TypePass<'db>,
    bodies: BodyPass<'db>,
    opt: OptLevel,
) -> Result<EmittedPackage, LoweringError> {
    let TypePass {
        root,
        files,
        tables,
        mut classes,
        enums,
        mut interfaces,
        aliases,
        mut exports,
        slotted,
        lets,
    } = types;
    let BodyPass {
        code,
        offsets,
        refs,
    } = bodies;
    let placed = CodeObjects {
        base: tables.code_base(),
        offsets: &offsets,
    };

    export_functions(&mut exports, &slotted, &offsets);
    fill_interface_defaults(db, &files, &tables, &mut interfaces, &placed);
    fill_class_methods(db, &files, &tables, &mut classes, &placed);
    let mut refs = refs.attach(db, root, Own::Unit(&tables));
    let impl_rules = bake_rules(db, root, &files, &offsets, &mut refs);
    let record = package_record(db, root, &exports);
    let tail = emit_tail(db, class_fields, root, &files, &lets, opt)?;

    // Import totality: every foreign head baked anywhere in the unit gets
    // its entry, whether or not an operand names the declaration.
    let code: Vec<Object> = code.into_iter().collect();
    for object in classes
        .iter()
        .chain(&enums)
        .chain(&interfaces)
        .chain(&aliases)
        .chain(&code)
    {
        import_heads(&mut refs, heads, object)?;
    }
    for rule in &impl_rules {
        import_rule_heads(&mut refs, heads, rule)?;
    }
    let tail = tail
        .map(|(tail, tail_refs)| seal_tail(db, heads, root, tail, tail_refs))
        .transpose()?;

    Ok(EmittedPackage {
        root,
        edges: edge_table(db, root),
        unit: CompilationUnit {
            dependencies: refs.deps.into_entries(),
            classes,
            enums,
            interfaces,
            type_alias_objects: aliases,
            code,
            object_imports: refs.imports.objects.into_entries(),
            global_imports: refs.imports.globals.into_entries(),
            exports,
            impl_rules,
        },
        record,
        tail,
    })
}

/// Where each compiled function sits in the unit's object space.
struct CodeObjects<'a, 'db> {
    base: u32,
    offsets: &'a HashMap<FunctionLoc<'db>, u32>,
}

impl<'db> CodeObjects<'_, 'db> {
    /// The unit-convention object index of `function`'s body, if this unit
    /// compiled one.
    fn get(&self, function: FunctionLoc<'db>) -> Option<ObjectIndex> {
        self.offsets
            .get(&function)
            .map(|&offset| ObjectIndex::from_raw((self.base + offset) as usize))
    }
}

/// Export every compiled function under its path.
fn export_functions<'db>(
    exports: &mut ExportTable,
    slotted: &[(FunctionLoc<'db>, DeclPath)],
    offsets: &HashMap<FunctionLoc<'db>, u32>,
) {
    for (function, path) in slotted {
        let offset = offsets
            .get(function)
            .copied()
            .unwrap_or_else(|| unreachable!("every slot-owning function was compiled"));
        exports.objects.push((path.clone(), LocalRef::Code(offset)));
    }
}

/// The interface objects were pooled before their default bodies were; fill
/// each default's operand now.
fn fill_interface_defaults<'db>(
    db: &'db dyn crate::Db,
    files: &[SourceFile],
    tables: &LocalTables<'db>,
    interfaces: &mut [Object],
    placed: &CodeObjects<'_, 'db>,
) {
    for &file in files {
        for &interface in file_interfaces(db, file) {
            let k = tables.interfaces[&interface] as usize;
            let Object::Interface(def) = &mut interfaces[k] else {
                unreachable!("the interface bucket holds interfaces")
            };
            for &body in &interface_data(db, interface).default_methods {
                let Some(object) = placed.get(body) else {
                    continue;
                };
                let name = &function_data(db, body).name;
                let method = def
                    .methods
                    .iter_mut()
                    .find(|method| method.name == *name)
                    .unwrap_or_else(|| {
                        unreachable!("interface `{}` declares no method `{name}`", def.name)
                    });
                method.default = Some(object);
            }
        }
    }
}

/// Each class's inherent method table, over the bodies this unit compiled.
fn fill_class_methods<'db>(
    db: &'db dyn crate::Db,
    files: &[SourceFile],
    tables: &LocalTables<'db>,
    classes: &mut [Object],
    placed: &CodeObjects<'_, 'db>,
) {
    for &file in files {
        for &class in file_classes(db, file) {
            let k = tables.classes[&class] as usize;
            let Object::Class(object) = &mut classes[k] else {
                unreachable!("the class bucket holds classes")
            };
            for &method in &class_data(db, class).methods {
                let Some(function) = placed.get(method) else {
                    continue;
                };
                object.methods.insert(
                    function_data(db, method).name.clone(),
                    ClassMethodDef {
                        function,
                        function_ptr: bex_vm_types::HeapPtr::null(),
                    },
                );
            }
        }
    }
}

/// The `implements` rules, each against its interface's object — local or
/// imported — and its provided bodies' code offsets.
fn bake_rules<'db>(
    db: &'db dyn crate::Db,
    root: SourceRoot,
    files: &[SourceFile],
    offsets: &HashMap<FunctionLoc<'db>, u32>,
    refs: &mut PackageRefs<'_, 'db>,
) -> Vec<ProgramImplRuleFrag> {
    let lowering = RuntimeLowering::of(db, root);
    let mut rules = Vec::new();
    for &file in files {
        for &block in file_impls(db, file) {
            let Some(target) = impl_rule_target(db, block, &lowering) else {
                continue;
            };
            let interface_head = refs.interface(target.interface);
            let Some(rule) = bake_impl_rule(db, block, target, &lowering) else {
                continue;
            };
            let methods = rule
                .methods
                .iter()
                .map(|(name, body)| {
                    let code_offset = offsets.get(body).copied().unwrap_or_else(|| {
                        unreachable!("impl method `{name}` has no pooled function object")
                    });
                    (
                        name.clone(),
                        ProgramMethodImplFrag {
                            code_offset,
                            frame: rule.frame.clone(),
                        },
                    )
                })
                .collect();
            rules.push(ProgramImplRuleFrag {
                interface_head,
                for_ty_pattern: rule.for_ty_pattern,
                generic_param_bounds: rule.generic_param_bounds,
                interface_args: rule.interface_args,
                interface_assoc: rule.interface_assoc,
                methods,
                field_links: rule.field_links,
            });
        }
    }
    rules
}

/// Give a tail its tables: the imports its head walk adds, then the
/// resolver's dependency and import tables.
fn seal_tail(
    db: &dyn crate::Db,
    heads: &HeadIndex,
    root: SourceRoot,
    mut tail: InitTail,
    tail_refs: RefTables,
) -> Result<InitTail, LoweringError> {
    let mut refs = tail_refs.attach(db, root, Own::Tail);
    for object in &tail.objects {
        import_heads(&mut refs, heads, object)?;
    }
    tail.dependencies = refs.deps.into_entries();
    tail.object_imports = refs.imports.objects.into_entries();
    tail.global_imports = refs.imports.globals.into_entries();
    Ok(tail)
}

/// Intern an import for every foreign head an object bakes.
fn import_heads(
    refs: &mut PackageRefs<'_, '_>,
    heads: &HeadIndex,
    object: &Object,
) -> Result<(), LoweringError> {
    let mut failure = None;
    visit_object_heads(object, &mut |head: &TypeHead| {
        if failure.is_none() {
            failure = refs.import_head(heads, head).err();
        }
    });
    failure.map_or(Ok(()), Err)
}

/// Intern an import for every foreign head a rule bakes: its pattern, its
/// bounds, its interface arguments and bindings, and its bodies' frames.
fn import_rule_heads(
    refs: &mut PackageRefs<'_, '_>,
    heads: &HeadIndex,
    rule: &ProgramImplRuleFrag,
) -> Result<(), LoweringError> {
    let mut failure: Option<LoweringError> = None;
    let mut visit = |head: &TypeHead| {
        if failure.is_none() {
            failure = refs.import_head(heads, head).err();
        }
    };
    rule.for_ty_pattern.visit_heads(&mut visit);
    for bounds in &rule.generic_param_bounds {
        for bound in bounds {
            visit(&bound.interface);
            for arg in &bound.args {
                arg.visit_heads(&mut visit);
            }
            for (_, assoc) in &bound.assoc {
                assoc.visit_heads(&mut visit);
            }
        }
    }
    for arg in &rule.interface_args {
        arg.visit_heads(&mut visit);
    }
    for (_, assoc) in &rule.interface_assoc {
        assoc.visit_heads(&mut visit);
    }
    for (_, method) in &rule.methods {
        for frame in &method.frame {
            frame.visit_heads(&mut visit);
        }
    }
    failure.map_or(Ok(()), Err)
}

/// The whole-package products beside the unit: what the package exports by
/// name, and the interface a dependent compiles against.
fn package_record(db: &dyn crate::Db, root: SourceRoot, exports: &ExportTable) -> PackageRecord {
    let interface = package_interface(db, root);
    // Runtime compilers already own the exact stdlib sources, so only
    // mountable packages need to carry a serialized compiler surface.
    let interface_blob = if root.kind(db) == SourceRootKind::Stdlib {
        Vec::new()
    } else {
        baml_artifact::encode(
            baml_artifact::ArtifactKind::PackageInterface,
            &export_interface(db, root),
        )
        .expect("PackageInterface artifact serialization into Vec is infallible")
    };
    let slotted: HashSet<&DeclPath> = exports.globals.iter().map(|(path, _)| path).collect();
    let mut exported_names: indexmap::IndexSet<LocalName> = interface
        .types
        .iter()
        .flat_map(|(namespace, types)| {
            types.keys().map(|name| LocalName {
                namespace: namespace.clone(),
                name: name.clone(),
            })
        })
        .collect();
    let mut functions = Vec::new();
    for (namespace, rows) in &interface.functions {
        for (name, row) in rows {
            let local = LocalName {
                namespace: namespace.clone(),
                name: name.clone(),
            };
            exported_names.insert(local.clone());
            // An interface method dispatches through its interface, and a
            // callable that owns no object (an intrinsic) resolves to
            // nothing: neither is a function of the package's table.
            let path = match &row.target {
                ExternalCallTarget::Free { function } => FnPath::Free(item_path(function)),
                ExternalCallTarget::Method { class, name } => FnPath::Method {
                    class: item_path(class),
                    name: name.clone(),
                },
                ExternalCallTarget::Interface { .. } => continue,
            };
            if slotted.contains(&DeclPath::Function(path.clone())) {
                functions.push((local, path));
            }
        }
    }
    PackageRecord {
        exported_names: exported_names.into_iter().collect(),
        functions,
        interface_blob,
    }
}

/// The package's `$init` / `$init_test` tail, compiled against a resolver of
/// its own in which the package's declarations are imports at `SELF`.
/// `None` when the package has neither `let`s nor tests.
fn emit_tail<'db>(
    db: &'db dyn crate::Db,
    class_fields: &ClassFieldSnapshot,
    root: SourceRoot,
    files: &[SourceFile],
    lets: &[LetLoc<'db>],
    opt: OptLevel,
) -> Result<Option<(InitTail, RefTables)>, LoweringError> {
    let mut refs = PackageRefs::new(db, root, Own::Tail);
    let mut objects = ObjectPool::default();
    let mut slot_objects: Vec<u32> = Vec::new();
    let pool = |objects: &mut ObjectPool, slot_objects: &mut Vec<u32>, function: Function| {
        let index = u32::try_from(objects.len()).expect("tail fits u32");
        objects.push(Object::Function(Box::new(function)));
        let slot = u32::try_from(slot_objects.len()).expect("tail slots fit u32");
        slot_objects.push(index);
        (index, slot)
    };

    // The init part: one helper per `let` in dependency order, then `$init`.
    let bindings: Vec<(String, LetLoc<'db>, SourceFile)> = lets
        .iter()
        .map(|&binding| {
            (
                definition_link_name(db, Definition::Let(binding)),
                binding,
                binding.file(db),
            )
        })
        .collect();
    let sorted = topological_sort_lets(db, &bindings)?;
    let mut init = None;
    if !sorted.is_empty() {
        let mut steps = Vec::with_capacity(sorted.len());
        for (ordinal, (fq_name, binding, _)) in sorted.iter().enumerate() {
            let mut references = UnitReferences::default();
            let helper = compile_let_helper(
                db,
                *binding,
                ordinal,
                &mut refs,
                class_fields,
                &mut objects,
                0,
                &mut references,
                opt,
            )?;
            let (_, helper_slot) = pool(&mut objects, &mut slot_objects, helper);
            let target = refs.let_global(*binding);
            steps.push(InitStep {
                helper: GlobalIndex::from_raw(helper_slot as usize),
                target,
                target_name: fq_name.clone(),
            });
        }
        let (index, _) = pool(&mut objects, &mut slot_objects, build_init_function(&steps));
        init = Some(index);
    }

    // The test part: the chainer over the package's per-file test
    // initializers, in file order.
    let test_objects_start = u32::try_from(objects.len()).expect("tail fits u32");
    let test_slots_start = u32::try_from(slot_objects.len()).expect("tail slots fit u32");
    let mut parts = Vec::new();
    for &file in files {
        for &function in file_functions(db, file) {
            if function_data(db, function).metadata.origin
                != baml_compiler2_ast::FunctionOrigin::TestInitializer
            {
                continue;
            }
            let slot = refs
                .function(DeclRef::Source(function))
                .unwrap_or_else(|| unreachable!("a test initializer owns a slot"));
            parts.push(slot);
        }
    }
    let mut init_test = None;
    if !parts.is_empty() {
        let (index, _) = pool(
            &mut objects,
            &mut slot_objects,
            build_init_test_chainer(&parts),
        );
        init_test = Some(index);
    }

    if init.is_none() && init_test.is_none() {
        return Ok(None);
    }
    Ok(Some((
        InitTail {
            dependencies: Vec::new(),
            objects: objects.into_iter().collect(),
            object_imports: Vec::new(),
            global_imports: Vec::new(),
            slot_objects,
            init,
            init_test,
            test_objects_start,
            test_slots_start,
        },
        RefTables::of(refs),
    )))
}
