//! The code pass: every body of a package compiled into its code bucket
//! against the package's resolver — serially, or across workers with each
//! item's tables remapped into the unit's at the merge.

use std::collections::{HashMap, HashSet};

use baml_base::SourceFile;
use baml_compiler2_hir::{
    item_data::{file_functions, is_required_interface_method},
    loc::FunctionLoc,
};
use baml_compiler2_mir::{
    MirFunction, MirFunctionBody, MirFunctionKind, RuntimeLowering, lower_function, native_key_for,
};
use baml_linker_types::{DeclKey, DepSlot, import_operand, import_ordinal};
use bex_vm_types::{
    Function, GlobalIndex, Object, ObjectIndex, ObjectPool, TypeHead,
    head_walk::{visit_function_heads_mut, visit_object_heads_mut},
    relink::{IndexOperand, visit_index_operands, visit_object_operands},
};

use super::TypePass;
use crate::{
    ClassFieldSnapshot, FnSeed, GenericFunctionInterner, LoweringError, MirCodegenContext,
    OptLevel, attach_function_metadata, build_line_starts, builtin_emit_function, compile_lambdas,
    emit::compile_mir_function,
    lower_seed_mirs, lowered,
    refs::{ImportTables, LocalTables, Own, PackageRefs},
    relative_source_path,
};

/// One package after its code pass: every body compiled into the code
/// bucket, and the resolver that grew while compiling them.
pub(super) struct BodyPass<'db> {
    pub(super) code: ObjectPool,
    /// The code-bucket offset of every compiled function.
    pub(super) offsets: HashMap<FunctionLoc<'db>, u32>,
    pub(super) refs: RefTables,
}

/// The interned tables a resolver accumulated, detached from its borrows.
pub(super) struct RefTables {
    pub(super) deps: baml_compiler2_hir::package::DependencyTable,
    pub(super) imports: ImportTables,
}

impl RefTables {
    pub(super) fn of(refs: PackageRefs<'_, '_>) -> Self {
        Self {
            deps: refs.deps,
            imports: refs.imports,
        }
    }

    pub(super) fn attach<'w, 'db>(
        self,
        db: &'db dyn crate::Db,
        root: baml_base::SourceRoot,
        own: Own<'w, 'db>,
    ) -> PackageRefs<'w, 'db> {
        PackageRefs::with_tables(db, root, own, self.deps, self.imports)
    }
}

/// Compile one bytecode body — its lambda children first — into `objects`
/// based at `objects_base`, resolving every reference through `refs`.
#[expect(clippy::too_many_arguments)]
fn compile_body<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    refs: &mut PackageRefs<'_, 'db>,
    class_fields: &ClassFieldSnapshot<'db>,
    objects: &mut ObjectPool,
    objects_base: usize,
    mir: &MirFunction<'db>,
    body: &MirFunctionBody<'db>,
    line_starts: &[u32],
    source_file: &str,
    opt: OptLevel,
) -> Function {
    let empty_capture_types = Vec::new();
    let empty_spawn_capture_indices = HashSet::new();
    let lambda_info = compile_lambdas(
        db,
        &mir.lambdas,
        Some(body),
        &empty_capture_types,
        &empty_spawn_capture_indices,
        line_starts,
        source_file,
        &mut *refs,
        class_fields,
        objects,
        objects_base,
        opt,
    );
    let lambda_object_indices: Vec<usize> = lambda_info.iter().map(|(idx, _)| *idx).collect();
    let lambda_names: Vec<String> = lambda_info.iter().map(|(_, name)| name.clone()).collect();
    let ctx = MirCodegenContext {
        db,
        refs,
        class_fields,
        objects,
        objects_base,
        lambda_object_indices: &lambda_object_indices,
        lambda_names: &lambda_names,
        capture_types: &empty_capture_types,
        spawn_capture_indices: &empty_spawn_capture_indices,
    };
    let mut function = compile_mir_function(body, mir.arity, mir.span, line_starts, ctx, opt);
    function.name = mir.identity.link_name(db);
    function.source_file = source_file.to_string();
    function
}

/// Whether a file's functions are stdlib builtins for origin purposes: the
/// path prefix is the wire-contract spelling of "stdlib stub".
fn is_builtin_file(db: &dyn crate::Db, file: SourceFile) -> bool {
    file.path(db).to_string_lossy().starts_with("<builtin>/")
}

/// `refs` continues the type pass's resolver: the imports its declarations
/// interned come first in the unit's tables, then everything the bodies add.
pub(super) fn emit_bodies<'db>(
    db: &'db dyn crate::Db,
    class_fields: &ClassFieldSnapshot<'db>,
    types: &TypePass<'db>,
    refs: RefTables,
    opt: OptLevel,
) -> Result<BodyPass<'db>, LoweringError> {
    if rayon::current_num_threads() > 1 && db.parallel_db_handle().is_some() {
        emit_bodies_parallel(db, class_fields, types, refs, opt)
    } else {
        emit_bodies_serial(db, class_fields, types, refs, opt)
    }
}

/// The code pass, serial: the reference implementation.
fn emit_bodies_serial<'db>(
    db: &'db dyn crate::Db,
    class_fields: &ClassFieldSnapshot<'db>,
    types: &TypePass<'db>,
    refs: RefTables,
    opt: OptLevel,
) -> Result<BodyPass<'db>, LoweringError> {
    let base = types.tables.code_base() as usize;
    let mut refs = refs.attach(db, types.root, types.scope.own(&types.tables));
    let mut code = ObjectPool::default();
    let mut offsets = HashMap::new();
    let lowering = RuntimeLowering::of(db, types.root);
    for &file in &types.files {
        let line_starts = build_line_starts(file.text(db));
        let source_file = relative_source_path(db, file);
        let builtin = is_builtin_file(db, file);
        for &function in file_functions(db, file) {
            // Required interface methods are signature-only items.
            if is_required_interface_method(db, function) {
                continue;
            }
            let mir = lowered(db, function, lower_function(db, function, opt))?;
            let mut compiled = match &mir.kind {
                MirFunctionKind::Bytecode(body) => compile_body(
                    db,
                    &mut refs,
                    class_fields,
                    &mut code,
                    base,
                    mir,
                    body,
                    &line_starts,
                    &source_file,
                    opt,
                ),
                MirFunctionKind::Builtin(kind) => {
                    let fq_name = mir.identity.link_name(db);
                    match builtin_emit_function(
                        *kind,
                        &fq_name,
                        &native_key_for(db, function),
                        mir.arity,
                    ) {
                        Some(function) => function,
                        // Intrinsics and `__await_any` have no callable body.
                        None => continue,
                    }
                }
            };
            let fq_name = compiled.name.clone();
            attach_function_metadata(
                db,
                function,
                &lowering,
                builtin,
                &fq_name,
                &mut refs,
                &mut compiled,
            );
            place_function(&types.tables, &mut code, &mut offsets, function, compiled);
        }
    }
    Ok(BodyPass {
        code,
        offsets,
        refs: RefTables::of(refs),
    })
}

/// Pool a compiled function at the next code offset, recording the offset
/// by declaration: the export tables carry it, so lambdas and literals may
/// interleave with the slot-owning functions in the code bucket.
fn place_function<'db>(
    tables: &LocalTables<'db>,
    code: &mut ObjectPool,
    offsets: &mut HashMap<FunctionLoc<'db>, u32>,
    function: FunctionLoc<'db>,
    compiled: Function,
) {
    let offset = u32::try_from(code.len()).expect("code bucket fits u32");
    code.push(Object::Function(Box::new(compiled)));
    debug_assert_eq!(
        tables.function_slots.get(&function).copied(),
        Some(u32::try_from(offsets.len()).expect("slots fit u32")),
        "the code pass visits slot-owning functions in slot order"
    );
    offsets.insert(function, offset);
}

/// One worker's output for one body: the compiled function, the fragment
/// pool it minted into, and the import tables it interned — item-local
/// ordinals, remapped into the unit's tables at the merge.
struct WorkerItem {
    function: Function,
    fragment: ObjectPool,
    refs: RefTables,
}

/// The code pass, parallel: byte-identical to the serial pass.
///
/// Stage A lowers every body to MIR across worker-owned database handles;
/// Stage B compiles each body on a worker into a fresh fragment pool based
/// at the code bucket's start, with a fresh resolver whose dependency slots
/// and import ordinals are numbered by first use WITHIN THE ITEM; Stage C
/// merges the fragments in the serial pass's order, replaying each item's
/// dependency table and remapping its ordinals into the unit-wide tables —
/// first use per item, items in order, is exactly the order the serial pass
/// interns in — and replaying the generic-value interning the serial pass
/// performs across bodies.
fn emit_bodies_parallel<'db>(
    db: &'db dyn crate::Db,
    class_fields: &ClassFieldSnapshot<'db>,
    types: &TypePass<'db>,
    refs: RefTables,
    opt: OptLevel,
) -> Result<BodyPass<'db>, LoweringError> {
    use rayon::prelude::*;

    let mut seeds: Vec<FnSeed> = Vec::new();
    for &file in &types.files {
        let line_starts: std::sync::Arc<[u32]> = build_line_starts(file.text(db)).into();
        let source_file = relative_source_path(db, file);
        let builtin = is_builtin_file(db, file);
        for &function in file_functions(db, file) {
            if is_required_interface_method(db, function) {
                continue;
            }
            seeds.push(FnSeed {
                file,
                local_id: function.id(db),
                source_file: source_file.clone(),
                line_starts: line_starts.clone(),
                is_builtin_file: builtin,
            });
        }
    }
    let mirs = lower_seed_mirs(db, &seeds, opt)?;
    let base = types.tables.code_base() as usize;
    let root = types.root;
    let scope = types.scope;
    let tables = &types.tables;

    let handle_seed = std::sync::Mutex::new(db.parallel_db_handle().unwrap_or_else(|| {
        unreachable!("parallel emit runs only where the database hands out handles")
    }));
    let compiled: Vec<Option<WorkerItem>> = seeds
        .par_iter()
        .zip(mirs.par_iter())
        .map_init(
            || {
                handle_seed
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .parallel_db_handle()
                    .unwrap_or_else(|| unreachable!("a database handle clones itself"))
            },
            |handle, (seed, mir)| {
                let db: &dyn crate::Db = &**handle;
                let MirFunctionKind::Bytecode(body) = &mir.kind else {
                    // Builtins mint nothing; Stage C constructs them serially.
                    return None;
                };
                let mut refs = PackageRefs::new(db, root, scope.own(tables));
                let mut fragment = ObjectPool::default();
                let function = compile_body(
                    db,
                    &mut refs,
                    class_fields,
                    &mut fragment,
                    base,
                    mir,
                    body,
                    &seed.line_starts,
                    &seed.source_file,
                    opt,
                );
                Some(WorkerItem {
                    function,
                    fragment,
                    refs: RefTables::of(refs),
                })
            },
        )
        .collect();

    let mut refs = refs.attach(db, root, scope.own(tables));
    let mut code = ObjectPool::default();
    let mut offsets = HashMap::new();
    let mut intern = GenericFunctionInterner::default();
    let lowering = RuntimeLowering::of(db, root);
    for ((seed, mir), item) in seeds.iter().zip(mirs).zip(compiled) {
        let function = FunctionLoc::new(db, seed.file, seed.local_id);
        let mut compiled = match item {
            Some(item) => merge_item(&mut code, base, item, &mut refs, &mut intern),
            None => {
                let MirFunctionKind::Builtin(kind) = &mir.kind else {
                    unreachable!("Stage B compiles every bytecode function")
                };
                let fq_name = mir.identity.link_name(db);
                match builtin_emit_function(
                    *kind,
                    &fq_name,
                    &native_key_for(db, function),
                    mir.arity,
                ) {
                    Some(function) => function,
                    None => continue,
                }
            }
        };
        let fq_name = compiled.name.clone();
        attach_function_metadata(
            db,
            function,
            &lowering,
            seed.is_builtin_file,
            &fq_name,
            &mut refs,
            &mut compiled,
        );
        place_function(tables, &mut code, &mut offsets, function, compiled);
    }
    Ok(BodyPass {
        code,
        offsets,
        refs: RefTables::of(refs),
    })
}

/// Splice one worker's fragment into the unit's code bucket, remapping the
/// item's dependency slots and import ordinals into the unit's tables and
/// every fragment-relative object operand to its merged index, with the
/// generic-value interning the serial pass performs replayed in order.
///
/// The item's dependency table is replayed first, in its own slot order. The
/// item met its dependencies in the order the serial pass meets them while
/// compiling the same body, and a slot interns the hops of its path before
/// the slot itself, so replaying that order interns in the unit exactly the
/// packages the serial pass would, in the order it would. The dependency
/// table is shared by both import spaces: remapping the spaces one after the
/// other instead would intern a body's object imports' packages ahead of its
/// global imports' packages.
fn merge_item(
    code: &mut ObjectPool,
    base: usize,
    item: WorkerItem,
    refs: &mut PackageRefs<'_, '_>,
    intern: &mut GenericFunctionInterner,
) -> Function {
    let WorkerItem {
        mut function,
        fragment,
        refs: item_refs,
    } = item;
    let db = refs.db;
    let PackageRefs { deps, imports, .. } = refs;
    // The unit's slot for each of the item's, by the item's slot number.
    let slots: Vec<DepSlot> = std::iter::once(DepSlot::SELF)
        .chain(item_refs.deps.roots().map(|root| deps.slot(db, root)))
        .collect();
    let remap = |space: &crate::refs::ImportSpace, target: &mut crate::refs::ImportSpace| {
        space
            .entries()
            .iter()
            .map(|entry| {
                let key = DeclKey {
                    dep: slots[entry.key.dep.0 as usize],
                    path: entry.key.path.clone(),
                };
                target.intern(key)
            })
            .collect::<Vec<usize>>()
    };
    let object_remap = remap(&item_refs.imports.objects, &mut imports.objects);
    let global_remap = remap(&item_refs.imports.globals, &mut imports.globals);
    let remap_global = |slot: &mut GlobalIndex| {
        if let Some(ordinal) = import_ordinal(slot.raw()) {
            *slot = GlobalIndex::from_raw(import_operand(global_remap[ordinal]));
        }
    };
    // A head is a declaration operand: an import ordinal remaps to the
    // unit's; a local names a type bucket, final before the code pass.
    let mut remap_head = |head: &mut TypeHead| {
        if let Some(ordinal) = import_ordinal(head.operand().raw()) {
            *head = TypeHead::unresolved_operand(ObjectIndex::from_raw(import_operand(
                object_remap[ordinal],
            )));
        }
    };

    // Pass 1: append in mint order, interning generic values by their
    // unit-wide identity — so global operands (a value's base slot may be an
    // import) and heads (its type arguments) are remapped first. Completed
    // before any object rewrite so references to later mints resolve too.
    let mut index_map: Vec<usize> = Vec::with_capacity(fragment.len());
    let mut appended: Vec<usize> = Vec::new();
    for mut object in fragment {
        visit_object_operands(&mut object, |operand| {
            if let IndexOperand::Global(slot) = operand {
                remap_global(slot);
            }
        });
        visit_object_heads_mut(&mut object, &mut remap_head);
        if let Object::GenericFunction(generic) = &object
            && let Some(existing) = intern.get(generic)
        {
            index_map.push(existing);
            continue;
        }
        let index = base + code.len();
        if let Object::GenericFunction(generic) = &object {
            intern.insert_if_absent(generic, index);
        }
        appended.push(code.len());
        code.push(object);
        index_map.push(index);
    }

    // Pass 2: object operands — an import to its unit ordinal, a
    // fragment-relative mint to its merged index; type objects below the
    // code base are already final.
    let remap_object = |index: &mut ObjectIndex| {
        let raw = index.raw();
        if let Some(ordinal) = import_ordinal(raw) {
            *index = ObjectIndex::from_raw(import_operand(object_remap[ordinal]));
        } else if raw >= base {
            *index = ObjectIndex::from_raw(index_map[raw - base]);
        }
    };
    for &local in &appended {
        visit_object_operands(&mut code[ObjectIndex::from_raw(local)], |operand| {
            if let IndexOperand::Object(index) = operand {
                remap_object(index);
            }
        });
    }
    visit_index_operands(&mut function, |operand| match operand {
        IndexOperand::Object(index) => remap_object(index),
        IndexOperand::Global(slot) => remap_global(slot),
    });
    visit_function_heads_mut(&mut function, &mut remap_head);
    function
}
