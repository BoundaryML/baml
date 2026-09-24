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
//! in any way. Index operands intern their entries as codegen resolves them;
//! a foreign declaration referenced only by a type head — a `LoadType`
//! template, a signature, a field type, a rule pattern — gets its entry from
//! one walk over every emitted object's heads at the end, so nothing the
//! binder must relocate is implied by an operand it cannot see.

use std::collections::{HashMap, HashSet};

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
use baml_type::typetag::TypeTag;
use bex_vm_types::{DeclPath, Object};

use crate::{
    ClassFieldSnapshot, LoweringError, OptLevel,
    items::{
        build_alias_object, build_class_object, build_enum_object, build_interface_def,
        owns_no_slot,
    },
    refs::{
        LocalTables, class_head, enum_head, function_path, head_tag, interface_head, item_path,
    },
};

mod bodies;
mod finish;

/// Compile one package: its sources against the interfaces of the packages
/// it reaches, into the [`EmittedPackage`] a linker binds.
///
/// # Errors
///
/// A body that fails to lower, two declarations in the package's reach under
/// one type tag, a dependency cycle among its `let`s, or a spelling table
/// that is not injective over its reach (two packages spelled alike).
pub fn emit_package(
    db: &dyn crate::Db,
    root: SourceRoot,
    opt: OptLevel,
) -> Result<EmittedPackage, LoweringError> {
    let reach = visible_packages(db, root);
    // Every wire name in the unit is spelled by this table, so it must be
    // injective over the package's reach: a shared spelling would fuse two
    // packages' declarations under one wire name.
    if let Some(collision) = spelling(db).within(reach).collisions().first() {
        return Err(LoweringError::Internal(format!(
            "cannot emit a package against these packages: {collision}"
        )));
    }
    let heads = HeadIndex::new(db, reach)?;
    let class_fields = class_field_snapshot(db, reach);
    let types = type_pass(db, root)?;
    let bodies = bodies::emit_bodies(db, &class_fields, &types, opt)?;
    finish::finish(db, &heads, &class_fields, types, bodies, opt)
}

// ── What the package reaches ─────────────────────────────────────────────────

/// Every type declaration in a package's reach, by the tag its references
/// bake: what a head baked into bytecode refers to. The emit-time twin of
/// the loader's transient tag index, and the one collision detector over the
/// reach.
pub(crate) struct HeadIndex {
    tags: HashMap<TypeTag, (SourceRoot, DeclPath)>,
}

impl HeadIndex {
    fn new(db: &dyn crate::Db, reach: &[SourceRoot]) -> Result<Self, LoweringError> {
        let mut index = Self {
            tags: HashMap::new(),
        };
        for &root in reach {
            for class in layout::package_classes(db, root) {
                let head = class_head(db, class);
                index.claim(db, DeclPath::Class(item_path(&head)), &head)?;
            }
            for enum_ref in layout::package_enums(db, root) {
                let head = enum_head(db, enum_ref);
                index.claim(db, DeclPath::Enum(item_path(&head)), &head)?;
            }
            for interface in layout::package_interfaces(db, root) {
                let head = interface_head(db, interface);
                index.claim(db, DeclPath::Interface(item_path(&head)), &head)?;
            }
            for alias in layout::package_aliases(db, root) {
                let head = layout::alias_head(db, alias);
                index.claim(db, DeclPath::TypeAlias(item_path(&head)), &head)?;
            }
        }
        Ok(index)
    }

    fn claim(
        &mut self,
        db: &dyn crate::Db,
        path: DeclPath,
        head: &baml_type::DeclName,
    ) -> Result<(), LoweringError> {
        let tag = head_tag(db, head);
        if let Some((_, previous)) = self.tags.insert(tag, (head.root(), path)) {
            return Err(LoweringError::Internal(format!(
                "the fully-qualified type names `{previous}` and `{}` hash to the same 47-bit \
                 type tag. This is an extremely rare hash collision between two names, not a \
                 compiler bug; renaming either declaration (or moving one to a different \
                 namespace/package) resolves it. This is a known limitation of \
                 content-addressed type tags.",
                spelling(db).wire(head).render_dotted(false)
            )));
        }
        Ok(())
    }

    /// The declaration a baked tag refers to.
    pub(crate) fn declaration(&self, tag: TypeTag) -> Option<(SourceRoot, &DeclPath)> {
        self.tags.get(&tag).map(|(root, path)| (*root, path))
    }
}

/// The field layout of every class in a package's reach, by tag: what
/// instruction selection reads for a field access. Each class is read from
/// its declaration through its own package's lowering, whichever lane serves
/// that package.
fn class_field_snapshot(db: &dyn crate::Db, reach: &[SourceRoot]) -> ClassFieldSnapshot {
    let mut snapshot = ClassFieldSnapshot::new();
    for &root in reach {
        let lowering = RuntimeLowering::of(db, root);
        for class in layout::package_classes(db, root) {
            let fields = layout::class_fields(db, class)
                .iter()
                .map(|(name, ty)| {
                    (
                        name.to_string(),
                        bex_vm_types::anchor_runtime_ty(&lowering.convert(ty)),
                    )
                })
                .collect();
            snapshot.insert(head_tag(db, &class_head(db, class)), fields);
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

fn type_pass(db: &dyn crate::Db, root: SourceRoot) -> Result<TypePass<'_>, LoweringError> {
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

    // Passes 2, 3, 3b, 3c: one bucket per kind, each declaration exported
    // under its path.
    let mut classes = Vec::new();
    for &file in &files {
        for &class in file_classes(db, file) {
            let head = qualify_def(db, Definition::Class(class), &class_data(db, class).name);
            let k = u32::try_from(classes.len()).expect("bucket fits u32");
            tables.classes.insert(class, k);
            classes.push(Object::Class(Box::new(build_class_object(
                db,
                class,
                &lowering,
                head_tag(db, &head),
            ))));
            exports
                .objects
                .push((DeclPath::Class(item_path(&head)), LocalRef::Class(k)));
        }
    }
    let mut enums = Vec::new();
    for &file in &files {
        for &enum_loc in file_enums(db, file) {
            let head = qualify_def(
                db,
                Definition::Enum(enum_loc),
                &enum_data(db, enum_loc).name,
            );
            let k = u32::try_from(enums.len()).expect("bucket fits u32");
            tables.enums.insert(enum_loc, k);
            enums.push(Object::Enum(Box::new(build_enum_object(
                db,
                enum_loc,
                &lowering,
                head_tag(db, &head),
            ))));
            exports
                .objects
                .push((DeclPath::Enum(item_path(&head)), LocalRef::Enum(k)));
        }
    }
    let mut interfaces = Vec::new();
    for &file in &files {
        for &interface in file_interfaces(db, file) {
            let head = qualify_def(
                db,
                Definition::Interface(interface),
                &interface_data(db, interface).name,
            );
            let k = u32::try_from(interfaces.len()).expect("bucket fits u32");
            tables.interfaces.insert(interface, k);
            interfaces.push(Object::Interface(Box::new(build_interface_def(
                db,
                interface,
                &head,
                head_tag(db, &head),
                &lowering,
            ))));
            exports.objects.push((
                DeclPath::Interface(item_path(&head)),
                LocalRef::Interface(k),
            ));
        }
    }
    let mut aliases = Vec::new();
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
            let k = u32::try_from(aliases.len()).expect("bucket fits u32");
            tables.aliases.insert(alias, k);
            aliases.push(Object::TypeAlias(Box::new(build_alias_object(
                db,
                alias,
                &lowering,
                head_tag(db, &head),
            )?)));
            exports.objects.push((
                DeclPath::TypeAlias(item_path(&head)),
                LocalRef::TypeAlias(k),
            ));
        }
    }

    Ok(TypePass {
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
    })
}
