//! The per-package emitter: one package's sources compiled into its
//! [`EmittedPackage`] (unit, record, and tail), against nothing but the
//! interfaces of the packages it reaches.
//!
//! A package's emit reads its own declarations and, for every package in its
//! dependency closure, the declaration facts `hir_ty` answers by identity —
//! whichever lane serves that package. It never sees another package's
//! emit: a dependency's class layout is a fact of the dependency's
//! declaration, not of its bytecode. That is what makes each unit a pure
//! function of its package and its dependencies' interfaces, and what lets
//! a driver compile the packages of a program in any order, in parallel, or
//! not at all when a cached unit is current. Linking is someone else's job.
//!
//! # Import totality
//!
//! A unit's import table is total over the foreign declarations it references
//! in any way, by construction: a type head is a declaration operand like any
//! other, minted by the same resolver that mints index operands, so a
//! declaration referenced only through a type is imported exactly as one
//! referenced by an instruction. Nothing the binder must relocate is implied
//! by an operand it cannot see.

use std::collections::HashSet;

use baml_base::{SourceFile, SourceRoot};
use baml_compiler2_hir::{
    contributions::Definition,
    item_data::{
        class_data, enum_data, file_classes, file_enums, file_functions, file_interfaces,
        file_lets, file_type_aliases, interface_data, let_data, type_alias_data,
    },
    loc::{FunctionLoc, LetLoc},
    package::{spelling, visible_packages},
};
use baml_compiler2_hir_ty::{layout, lower::qualify_def};
use baml_compiler2_mir::RuntimeLowering;
use baml_linker_types::{EmittedPackage, ExportTable, LocalRef};
use bex_vm_types::{DeclPath, Object};

use crate::{
    ClassFieldSnapshot, LoweringError, OptLevel,
    items::{
        build_alias_object, build_class_object, build_enum_object, build_interface_def,
        owns_no_slot,
    },
    refs::{LocalTables, Own, PackageRefs, function_path, item_path, unit_tag},
};

mod bodies;
mod finish;

use bodies::RefTables;

/// Compile one package: its sources against the interfaces of the packages
/// it reaches, into the [`EmittedPackage`] a linker binds.
///
/// # Errors
///
/// A body that fails to lower, a dependency cycle among its `let`s, or a
/// spelling table that is not injective over its reach (two packages spelled
/// alike).
pub fn emit_package(
    db: &dyn crate::Db,
    root: SourceRoot,
    opt: OptLevel,
) -> Result<EmittedPackage, LoweringError> {
    let reach = visible_packages(db, root);
    // Every wire name a lowered type carries is resolved back to its
    // declaration through this table, so it must be injective over the
    // package's reach: a shared spelling would make two packages'
    // declarations one name.
    if let Some(collision) = spelling(db).within(reach).collisions().first() {
        return Err(LoweringError::Internal(format!(
            "cannot emit a package against these packages: {collision}"
        )));
    }
    let class_fields = class_field_snapshot(db, reach);
    let (types, refs) = type_pass(db, root)?;
    let bodies = bodies::emit_bodies(db, &class_fields, &types, refs, opt)?;
    finish::finish(db, &class_fields, types, bodies, opt)
}

/// The field layout of every class in `roots`, by declaration: what
/// instruction selection and operand display read for a field access. Each
/// class is read from its declaration through its own package's lowering,
/// whichever lane serves that package, at the compiler's head — a layout is
/// a fact of a declaration, never of an anchored head.
pub(crate) fn class_field_snapshot<'db>(
    db: &'db dyn crate::Db,
    roots: &[SourceRoot],
) -> ClassFieldSnapshot<'db> {
    let mut snapshot = ClassFieldSnapshot::new();
    for &root in roots {
        let lowering = RuntimeLowering::of(db, root);
        for class in layout::package_classes(db, root) {
            let fields = layout::class_fields(db, class)
                .iter()
                .map(|(name, ty)| (name.to_string(), lowering.convert(ty)))
                .collect();
            snapshot.insert(class, fields);
        }
    }
    snapshot
}

// ── Type passes ──────────────────────────────────────────────────────────────

/// One package after its type passes: every declaration slotted, every type
/// pooled, nothing compiled yet.
pub(crate) struct TypePass<'db> {
    pub(crate) root: SourceRoot,
    pub(crate) files: Vec<SourceFile>,
    pub(crate) tables: LocalTables<'db>,
    pub(crate) classes: Vec<Object>,
    pub(crate) enums: Vec<Object>,
    pub(crate) interfaces: Vec<Object>,
    pub(crate) aliases: Vec<Object>,
    pub(crate) exports: ExportTable,
    /// Every slot-owning function, in slot order, with its export path.
    pub(crate) slotted: Vec<(FunctionLoc<'db>, DeclPath)>,
    /// Every top-level `let`, in slot order.
    pub(crate) lets: Vec<LetLoc<'db>>,
}

/// The type passes: number every declaration first — a declaration's operand
/// is what its own object and every reference to it carry, so the tables
/// must be complete before any object is built — then build each object
/// through a resolver over the completed tables. The resolver's interned
/// imports (the foreign heads the declarations' types name) continue into
/// the code pass.
fn type_pass(
    db: &dyn crate::Db,
    root: SourceRoot,
) -> Result<(TypePass<'_>, RefTables), LoweringError> {
    let files: Vec<SourceFile> = root.files(db).clone();
    let lowering = RuntimeLowering::of(db, root);
    let mut tables = LocalTables::default();
    let mut exports = ExportTable::default();

    // Pass 1: slots. Functions and interface bodies first, then `let`s, each
    // in file order — the partition the export table records.
    let mut slotted = Vec::new();
    for &file in &files {
        for &function in file_functions(db, file) {
            if owns_no_slot(db, function) {
                continue;
            }
            let slot = u32::try_from(slotted.len()).expect("slots fit u32");
            let path = function_path(db, function);
            tables.function_slots.insert(function, slot);
            exports.globals.push((path.clone(), slot));
            slotted.push((function, path));
        }
    }
    let mut lets = Vec::new();
    for &file in &files {
        for &binding in file_lets(db, file) {
            let slot = u32::try_from(slotted.len() + lets.len()).expect("slots fit u32");
            let head = qualify_def(db, Definition::Let(binding), &let_data(db, binding).name);
            tables.let_slots.insert(binding, slot);
            exports
                .globals
                .push((DeclPath::Let(item_path(&head)), slot));
            lets.push(binding);
        }
    }

    // Passes 2, 3, 3b, 3c — numbering: one bucket per kind, each declaration
    // exported under its path.
    let mut class_locs = Vec::new();
    for &file in &files {
        for &class in file_classes(db, file) {
            let head = qualify_def(db, Definition::Class(class), &class_data(db, class).name);
            let k = u32::try_from(class_locs.len()).expect("bucket fits u32");
            tables.classes.insert(class, k);
            class_locs.push(class);
            exports
                .objects
                .push((DeclPath::Class(item_path(&head)), LocalRef::Class(k)));
        }
    }
    let mut enum_locs = Vec::new();
    for &file in &files {
        for &enum_loc in file_enums(db, file) {
            let head = qualify_def(
                db,
                Definition::Enum(enum_loc),
                &enum_data(db, enum_loc).name,
            );
            let k = u32::try_from(enum_locs.len()).expect("bucket fits u32");
            tables.enums.insert(enum_loc, k);
            enum_locs.push(enum_loc);
            exports
                .objects
                .push((DeclPath::Enum(item_path(&head)), LocalRef::Enum(k)));
        }
    }
    let mut interface_locs = Vec::new();
    for &file in &files {
        for &interface in file_interfaces(db, file) {
            let head = qualify_def(
                db,
                Definition::Interface(interface),
                &interface_data(db, interface).name,
            );
            let k = u32::try_from(interface_locs.len()).expect("bucket fits u32");
            tables.interfaces.insert(interface, k);
            interface_locs.push((interface, head.clone()));
            exports.objects.push((
                DeclPath::Interface(item_path(&head)),
                LocalRef::Interface(k),
            ));
        }
    }
    let mut alias_locs = Vec::new();
    let mut emitted_aliases = HashSet::new();
    for &file in &files {
        for &alias in file_type_aliases(db, file) {
            let head = qualify_def(
                db,
                Definition::TypeAlias(alias),
                &type_alias_data(db, alias).name,
            );
            // Only a recursive alias survives lowering as a head; each is
            // pooled once, in the file that declares it.
            if !lowering.aliases.recursive.contains(&head) || !emitted_aliases.insert(head.clone())
            {
                continue;
            }
            let k = u32::try_from(alias_locs.len()).expect("bucket fits u32");
            tables.aliases.insert(alias, k);
            alias_locs.push(alias);
            exports.objects.push((
                DeclPath::TypeAlias(item_path(&head)),
                LocalRef::TypeAlias(k),
            ));
        }
    }

    // Building: every type the declarations carry resolves through the
    // completed tables. A declaration's own tag field is its operand.
    let bucket_index = |k: usize| u32::try_from(k).expect("bucket fits u32");
    let mut refs = PackageRefs::new(db, root, Own::Unit(&tables));
    let classes = class_locs
        .iter()
        .enumerate()
        .map(|(k, &class)| {
            let tag = unit_tag(LocalTables::class_object(bucket_index(k)));
            Object::Class(Box::new(build_class_object(
                db, class, &lowering, &mut refs, tag,
            )))
        })
        .collect();
    let enums = enum_locs
        .iter()
        .enumerate()
        .map(|(k, &enum_loc)| {
            let tag = unit_tag(tables.enum_object(bucket_index(k)));
            Object::Enum(Box::new(build_enum_object(db, enum_loc, &lowering, tag)))
        })
        .collect();
    let interfaces = interface_locs
        .iter()
        .enumerate()
        .map(|(k, (interface, head))| {
            let tag = unit_tag(tables.interface_object(bucket_index(k)));
            Object::Interface(Box::new(build_interface_def(
                db, *interface, head, tag, &lowering, &mut refs,
            )))
        })
        .collect();
    let aliases = alias_locs
        .iter()
        .enumerate()
        .map(|(k, &alias)| {
            let tag = unit_tag(tables.alias_object(bucket_index(k)));
            Ok(Object::TypeAlias(Box::new(build_alias_object(
                db, alias, &lowering, &mut refs, tag,
            )?)))
        })
        .collect::<Result<Vec<_>, LoweringError>>()?;
    let refs = RefTables::of(refs);

    Ok((
        TypePass {
            root,
            files,
            tables,
            classes,
            enums,
            interfaces,
            aliases,
            exports,
            slotted,
            lets,
        },
        refs,
    ))
}
