//! Admission: which functions of a program compile, and the module's
//! index of the program they run in.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque, hash_map::Entry},
    hash::Hash,
};

use baml_base::SourceRoot;
use baml_compiler2_emit::{
    DeclCoords, class_coords, class_head_coords, declared_function_coords, enum_coords,
    enum_head_coords, function_coords, function_identity, interface_head_coords,
};
use baml_compiler2_hir::{
    contributions::Definition,
    file_package::file_package,
    item_data::{file_functions, file_lets, is_required_interface_method, let_data},
    loc::{FunctionLoc, LetLoc},
    package::{edge_table, world_files},
};
use baml_compiler2_hir_ty::extern_loc::{ClassRef, EnumRef, FunctionRef};
use baml_compiler2_mir::{
    FunctionOwner, MirFunction, MirFunctionBody, MirFunctionId, MirFunctionKind, OptLevel,
    lower_function, lower_let_body,
};
use baml_type::{DeclName, typetag::TypeTag};
use bex_vm_types::{
    Bytecode, ConstValue, DeclPath, FnPath, FunctionKind, Object, ObjectIndex, Program, TypeHead,
    bytecode::BlockPcs,
    types::{FunctionIdentity, LocalName, PackageKind},
};

use crate::{
    analysis::{Analysis, Repr, capture_reprs, value_reprs},
    codegen::Generator,
    reach::{reachable, references},
    render::render_module,
};

/// Why a function stays bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// Outside the subset this backend compiles; the bytecode is authoritative.
    Unsupported(String),
    /// The MIR violated an invariant a checked program must hold: a compiler
    /// bug, reported rather than compiled around.
    Invalid(String),
    /// Compilable, but nothing the pack's entry points reach calls it
    /// (`compile_program`'s `entries`); compiling it would only cost build
    /// time and binary size.
    Unreached,
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(reason) => write!(f, "unsupported: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid MIR: {reason}"),
            Self::Unreached => write!(f, "unreached from the entry points"),
        }
    }
}

/// One admitted function in the generated module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFunction {
    /// `Function::name` of the bytecode function this body replaces, as the
    /// loader renders it (`Program::function_name`): display only, repeated
    /// in the module's table so a table from another build is refused at
    /// load, never what the body is bound by.
    pub name: String,
    /// `name` under its package's label, for reports: a closure body renders
    /// under no package label (`.<lambda(f, 0)>`), so this says whose it is
    /// (`user.<lambda(f, 0)>`); any other function's is its `name`.
    pub display: String,
    /// The Rust symbol of the body (`bex_vm::compiled_rt::CompiledBody`).
    pub symbol: String,
    /// Object index of that bytecode function in the program.
    pub object: usize,
    /// Its arity.
    pub arity: usize,
    /// Whether it can suspend (it then runs as a resumable state machine;
    /// see `bex_vm::compiled_rt::ResumableBody`).
    pub resumable: bool,
    /// The stack slots its body keeps in its frame
    /// (`bex_vm_types::types::NativeBody::slots`).
    pub slots: u32,
    /// Its cause slot, when it has a `catch`
    /// (`bex_vm_types::types::NativeBody::cause_slot`).
    pub cause_slot: Option<u32>,
}

/// The generated Rust module for one program.
#[derive(Debug, Clone)]
pub struct NativeModule {
    /// Rust source of the module.
    pub source: String,
    /// The functions it compiles, in emission order.
    pub functions: Vec<CompiledFunction>,
    /// Functions of any package it does not compile, with the reason.
    pub rejected: Vec<(String, Rejection)>,
}

/// Compile every admitted function of `root`'s program: the root's package,
/// the stdlib, and every dependency it reaches ([`world_files`]), the file
/// set bytecode emit covers.
///
/// `program` is the linked program the bytecode of these same functions lives
/// in; names and object indices are read from it so the generated module
/// refers to exactly this build (the generated crate embeds it as
/// `PROGRAM_ARTIFACT`, read from `artifact_path`).
///
/// `entries` are the object indices of the functions the pack runs (its
/// targets): only what they reach is compiled (see [`crate::reach`]), along
/// with what every top-level `let` initializer reaches (`$init` runs them
/// all at startup) and every method a non-stdlib package declares, which the
/// stdlib may dispatch to without naming it. `None` compiles every admitted
/// function, for a report of the backend's coverage.
pub fn compile_program<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    artifact_path: &str,
    entries: Option<&[usize]>,
) -> Result<NativeModule, String> {
    let index = ProgramIndex::new(db, root, program);
    // Admission works on object indices: the identity generated code bakes
    // in, unique across packages where a rendered name need not be.
    let mut units: BTreeMap<usize, Unit<'db>> = BTreeMap::new();
    let mut rejected: Vec<(String, Rejection)> = Vec::new();
    // What each lowered function refers to, admitted or not: a rejected
    // function stays bytecode and still calls what it calls.
    let mut refs: HashMap<usize, BTreeSet<usize>> = HashMap::new();
    let mut seeds: BTreeSet<usize> = entries.into_iter().flatten().copied().collect();

    for &file in world_files(db, root) {
        let package = index.package_of(file_package(db, file).root);
        for &function in file_functions(db, file) {
            if is_required_interface_method(db, function) {
                continue;
            }
            let mir = match lower_function(db, function, opt) {
                Ok(mir) => mir,
                Err(error) => {
                    rejected.push((
                        format!("{function:?}"),
                        Rejection::Invalid(format!("lowering failed: {error:?}")),
                    ));
                    continue;
                }
            };
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                continue;
            };
            // Its lambdas are units of their own, whatever becomes of it: a
            // closure over a bytecode lambda is still a closure, and a
            // compiled lambda is entered through the VM either way.
            admit_lambdas(db, &index, &mut units, &mut rejected, mir, &[], package);
            let object = match index.declared(db, function) {
                Ok(Some(object)) => object,
                Ok(None) => {
                    rejected.push((
                        mir.identity.display_name(db),
                        Rejection::Invalid("no function of this identity in the program".into()),
                    ));
                    continue;
                }
                Err(rejection) => {
                    rejected.push((mir.identity.display_name(db), rejection));
                    continue;
                }
            };
            if entries.is_some() {
                refs.insert(object, references(db, &index, body, &mir.lambdas));
            }
            admit_unit(
                db,
                &index,
                &mut units,
                &mut rejected,
                object,
                mir,
                body,
                Vec::new(),
                package,
            );
        }
        if entries.is_some() {
            for &binding in file_lets(db, file) {
                if let Ok(Some((body, lambdas))) = lower_let_body(db, binding, opt) {
                    seeds.extend(references(db, &index, &body, &lambdas));
                }
            }
        }
    }

    if entries.is_some() {
        seeds.extend(index.user_methods.iter().copied());
        let reached = reachable(seeds, &refs);
        units.retain(|object, _| {
            let keep = reached.contains(object);
            if !keep {
                rejected.push((index.name(*object).to_owned(), Rejection::Unreached));
            }
            keep
        });
    }

    // Admission is one fixpoint. A function is admitted when its body
    // generates (every construct in it is supported); generation is the only
    // judge, so nothing half-supported is emitted, and each round removes at
    // least one function, so it terminates. Whether an admitted function can
    // suspend follows from its body and its callees: it suspends when it
    // awaits, spawns, runs a sys op, or calls anything compiled code does not
    // call directly (bytecode, a function value, a builtin that runs
    // callbacks, or a function that itself suspends).
    let (bodies, functions, natives) = loop {
        let mut resumable: BTreeSet<usize> = units
            .iter()
            .filter(|(_, unit)| unit.analysis.suspends)
            .map(|(&object, _)| object)
            .collect();
        loop {
            let grown: Vec<usize> =
                units
                    .iter()
                    .filter(|(object, unit)| {
                        !resumable.contains(*object)
                            && unit.analysis.user_callees.iter().any(|callee| {
                                !units.contains_key(callee) || resumable.contains(callee)
                            })
                    })
                    .map(|(&object, _)| object)
                    .collect();
            if grown.is_empty() {
                break;
            }
            resumable.extend(grown);
        }
        // Garbage collection inside compiled code. An inline body checks for
        // a collection at the headers of its allocating loops, and on entry
        // when it is recursive and allocates, so memory stays bounded as the
        // interpreter's does. A function may collect when it checks or calls
        // one that may; its callers root their live values around such a
        // call. A resumable body yields to the engine instead.
        let recursive = recursive_units(&units);
        let checks: HashMap<usize, GcChecks> = units
            .iter()
            .filter(|(object, _)| !resumable.contains(*object))
            .map(|(&object, unit)| {
                let checks = GcChecks {
                    loops: unit.analysis.loop_allocates,
                    entry: recursive.contains(&object) && unit.analysis.allocates,
                };
                (object, checks)
            })
            .filter(|(_, checks)| checks.loops || checks.entry)
            .collect();
        let mut may_collect: BTreeSet<usize> = checks.keys().copied().collect();
        loop {
            let grown: Vec<usize> = units
                .iter()
                .filter(|(object, unit)| {
                    !may_collect.contains(*object)
                        && !resumable.contains(*object)
                        && unit
                            .analysis
                            .user_callees
                            .iter()
                            .any(|callee| may_collect.contains(callee))
                })
                .map(|(&object, _)| object)
                .collect();
            if grown.is_empty() {
                break;
            }
            may_collect.extend(grown);
        }
        let symbols: HashMap<usize, String> = units
            .keys()
            .map(|&object| (object, symbol_for(index.display(object), object)))
            .collect();
        let mut bodies = String::new();
        let mut functions = Vec::new();
        let mut natives: BTreeMap<String, usize> = BTreeMap::new();
        let mut failed = Vec::new();
        for (&object, unit) in &units {
            let mut generator = Generator {
                db,
                index: &index,
                object,
                package: unit.package,
                symbols: &symbols,
                resumable: &resumable,
                may_collect: &may_collect,
                gc_checks: checks.get(&object).copied().unwrap_or_default(),
                spill_base: 0,
                suspends: resumable.contains(&object),
                in_loop: false,
                natives: &mut natives,
                body: body_of(unit.mir),
                lambdas: &unit.mir.lambdas,
                analysis: &unit.analysis,
                positions: index.positions(object),
                current_mark: None,
                out: String::new(),
                pending: Vec::new(),
                temps: 0,
                live_in: Vec::new(),
                slots: 0,
                cause_slot: None,
                point_slot: 0,
                preds: Vec::new(),
                loop_headers: BTreeSet::new(),
                write_through: BTreeSet::new(),
                call_types: Vec::new(),
                structured: None,
                frame_templates: Vec::new(),
                hoist_types: true,
                static_value_sites: Vec::new(),
            };
            match generator.function(&symbols[&object]) {
                Ok(code) => {
                    bodies.push_str(&code);
                    functions.push(CompiledFunction {
                        name: index.name(object).to_owned(),
                        display: index.display(object).to_owned(),
                        symbol: symbols[&object].clone(),
                        object,
                        arity: index.info(object).arity,
                        resumable: resumable.contains(&object),
                        slots: u32::try_from(generator.slots).unwrap_or(u32::MAX),
                        cause_slot: generator
                            .cause_slot
                            .map(|slot| u32::try_from(slot).unwrap_or(u32::MAX)),
                    });
                }
                Err(rejection) => failed.push((object, rejection)),
            }
        }
        if failed.is_empty() {
            break (bodies, functions, natives);
        }
        for (object, rejection) in failed {
            units.remove(&object);
            rejected.push((index.display(object).to_owned(), rejection));
        }
    };

    let fingerprint = program
        .bytecode_fingerprint()
        .map_err(|error| format!("failed to fingerprint native bytecode program: {error}"))?;
    let source = render_module(&bodies, &functions, &natives, artifact_path, fingerprint)?;
    Ok(NativeModule {
        source,
        functions,
        rejected,
    })
}

/// Make `mir`, the function at `object` with the bytecode body `body`, a
/// unit when the program has it as bytecode and its analysis admits it;
/// `captures` is a lambda's (see [`Analysis::captures`]).
#[allow(clippy::too_many_arguments)]
fn admit_unit<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    units: &mut BTreeMap<usize, Unit<'db>>,
    rejected: &mut Vec<(String, Rejection)>,
    object: usize,
    mir: &'db MirFunction<'db>,
    body: &'db MirFunctionBody<'db>,
    captures: Vec<Repr>,
    package: Option<u32>,
) {
    let name = index.display(object).to_owned();
    let info = index.info(object);
    if !matches!(info.kind, FunctionKind::Bytecode) {
        return;
    }
    match Analysis::new(db, index, mir, body, captures) {
        Ok(analysis) => {
            units.insert(
                object,
                Unit {
                    mir,
                    analysis,
                    package,
                },
            );
        }
        Err(rejection) => rejected.push((name, rejection)),
    }
}

/// The lambdas lowered inside `parent` (and inside those, recursively) as
/// units of their own, each bound to its object by identity. `captures` is
/// the parent's own, when it is a lambda.
fn admit_lambdas<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    units: &mut BTreeMap<usize, Unit<'db>>,
    rejected: &mut Vec<(String, Rejection)>,
    parent: &'db MirFunction<'db>,
    captures: &[Repr],
    package: Option<u32>,
) {
    let MirFunctionKind::Bytecode(parent_body) = &parent.kind else {
        return;
    };
    let values = value_reprs(parent_body);
    for (lambda_idx, lambda) in parent.lambdas.iter().enumerate() {
        let MirFunctionKind::Bytecode(body) = &lambda.kind else {
            continue;
        };
        let captures = capture_reprs(parent_body, lambda_idx, &values, captures);
        let object = match index.lambda(db, &lambda.identity) {
            Ok(Some(object)) => object,
            Ok(None) => {
                rejected.push((
                    lambda.identity.display_name(db),
                    Rejection::Invalid("no function of this identity in the program".into()),
                ));
                continue;
            }
            Err(rejection) => {
                rejected.push((lambda.identity.display_name(db), rejection));
                continue;
            }
        };
        admit_lambdas(db, index, units, rejected, lambda, &captures, package);
        admit_unit(
            db, index, units, rejected, object, lambda, body, captures, package,
        );
    }
}

fn body_of<'a, 'db>(mir: &'a MirFunction<'db>) -> &'a MirFunctionBody<'db> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => body,
        MirFunctionKind::Builtin(_) => unreachable!("units hold bytecode bodies only"),
    }
}

/// The Rust symbol of the body of function `object`, named `name` (its
/// display name, with its package label): the name for readers, the object
/// index for uniqueness. A rendered name repeats across packages and within
/// one (two spellings can render alike); an object index is one function of
/// the program.
fn symbol_for(name: &str, object: usize) -> String {
    let mut symbol = String::with_capacity(name.len() + 8);
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            symbol.push(ch);
        } else {
            symbol.push('_');
        }
    }
    format!("{symbol}_{object}")
}

struct Unit<'db> {
    mir: &'db MirFunction<'db>,
    analysis: Analysis,
    /// The ordinal of the package the function belongs to, when the program
    /// places it.
    package: Option<u32>,
}

// ─── The program's objects by identity ──────────────────────────────────────

pub(crate) struct FunctionInfo {
    pub(crate) kind: FunctionKind,
    /// The slots a call lays its arguments over (`Function::argument_layout`).
    pub(crate) layout: baml_type::CallLayout,
    pub(crate) native_key: Option<String>,
    pub(crate) arity: usize,
    /// Whether the function holds its caller's `$trace` controls
    /// (`Function::trace_passthrough`), which the VM's call path sets up.
    pub(crate) trace_passthrough: bool,
}

/// A declaration's identity in the program: the package placing it, as its
/// ordinal in `Program::packages`, and its path there — what the package
/// tables key on. Two declarations of a linked program never share one; their
/// rendered names can (a namespace `Foo` with a function `m` and a class `Foo`
/// with a method `m` both read `pkg.Foo.m`), which is why nothing here is
/// looked up by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DeclKey {
    package: u32,
    path: DeclPath,
}

/// A closure body's identity in the program: the package placing it and its
/// [`FunctionIdentity`] there (its parent and ordinal). A lambda enters no
/// package table — only the `MakeClosure` in its parent names its object —
/// so it is found on the function objects themselves.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LambdaKey {
    package: u32,
    identity: FunctionIdentity,
}

#[derive(Default)]
pub(crate) struct ProgramIndex {
    /// Each package's ordinal by the source root it was emitted from, read
    /// off the program's own edge tables from the root.
    packages: HashMap<SourceRoot, u32>,
    /// Function object by identity.
    functions: HashMap<DeclKey, usize>,
    /// Class (object, field count) by identity.
    classes: HashMap<DeclKey, (usize, usize)>,
    /// Enum object by identity.
    enums: HashMap<DeclKey, usize>,
    /// Interface and recursive alias objects by identity: heads a type
    /// template can name, anchored like classes and enums.
    interfaces: HashMap<DeclKey, usize>,
    aliases: HashMap<DeclKey, usize>,
    /// The global slot of each top-level `let`, by identity.
    lets: HashMap<DeclKey, usize>,
    /// Lambda (closure body) object by identity.
    lambdas: HashMap<LambdaKey, usize>,
    /// Identities more than one object claimed: a malformed program. Every
    /// lookup of one is refused, so neither object is compiled against the
    /// other's index.
    ambiguous: HashSet<DeclKey>,
    ambiguous_lambdas: HashSet<LambdaKey>,
    /// Every function of the program by object index, its name as the
    /// loader renders it, and that name under its package's label
    /// ([`CompiledFunction::display`]).
    infos: HashMap<usize, FunctionInfo>,
    names: HashMap<usize, String>,
    /// Methods and interface bodies by method name: what an interface
    /// dispatch of that name can reach.
    methods: HashMap<String, Vec<usize>>,
    /// The methods and interface bodies of every non-stdlib package.
    user_methods: Vec<usize>,
    displays: HashMap<usize, String>,
    /// String literal → the compile-time string object bytecode loads it from.
    pub(crate) strings: HashMap<String, usize>,
    /// Function object → the global slot its object is loaded from: what a
    /// `GenericFunction` names its function by.
    pub(crate) globals: HashMap<usize, usize>,
    /// Class or enum object → the tag its type head carries in this
    /// program, which the heads in pooled objects compare by.
    heads: HashMap<usize, baml_type::typetag::TypeTag>,
    /// The program's pooled `foo<..>` values, as (function global slot,
    /// type arguments, object index).
    generic_functions: Vec<(usize, Box<[bex_vm_types::RealizedTy]>, usize)>,
    /// Bigint literal (as decimal text) → its pooled `Object::Bigint`.
    pub(crate) bigints: HashMap<String, usize>,
    /// Byte array literal → its pooled `Object::Uint8Array` template, which
    /// bytecode deep-copies on every evaluation.
    pub(crate) byte_arrays: HashMap<Vec<u8>, usize>,
    /// Each class object's dispatch tag (`Class::type_tag`), the value a
    /// class-tag switch matches on.
    class_tags: HashMap<usize, i64>,
    /// Each enum object's variant count.
    enum_variant_counts: HashMap<usize, usize>,
    /// The type constants (`ConstValue::Type`, `ConstValue::Literal`) of each
    /// bytecode function's constant pool, with their indices: what its
    /// `IsType` and `LoadType` instructions read, and what a compiled body
    /// reads for the same test or allocation (`BexVm::type_constant`).
    type_constants: HashMap<usize, Vec<(usize, ConstValue)>>,
    /// Where each bytecode function's MIR positions landed in its bytecode:
    /// the PCs its compiled body records.
    positions: HashMap<usize, Positions>,
}

/// Where a bytecode function's MIR positions landed (`Bytecode::block_pcs`),
/// with its line and shield tables to say what each PC reads as. A compiled
/// body records the PC of each position it passes, so its frame reads as the
/// bytecode frame would there.
pub(crate) struct Positions(Bytecode);

impl Positions {
    fn of(bytecode: &Bytecode) -> Self {
        Self(Bytecode {
            line_table: bytecode.line_table.clone(),
            shield_table: bytecode.shield_table.clone(),
            block_pcs: bytecode.block_pcs.clone(),
            ..Bytecode::default()
        })
    }

    /// The PCs of MIR block `block`.
    pub(crate) fn block(&self, block: usize) -> Option<&BlockPcs> {
        self.0.block_pcs.get(block)
    }

    /// Whether these positions were emitted from `body`: one entry per block
    /// and per statement. The bytecode and this backend lower the same MIR
    /// (`lower_function` at one optimization level); a mismatch means they
    /// did not, and no PC here can be trusted.
    pub(crate) fn fit(&self, body: &baml_compiler2_mir::MirFunctionBody<'_>) -> bool {
        self.0.block_pcs.len() == body.blocks.len()
            && self
                .0
                .block_pcs
                .iter()
                .zip(&body.blocks)
                .all(|(pcs, block)| pcs.statements.len() == block.statements.len())
    }

    /// What a frame standing at `pc` reads as: its source line and whether
    /// it is shielded. A body records a new PC only where this changes.
    pub(crate) fn reading(&self, pc: usize) -> (usize, bool) {
        (self.0.source_line_for_pc(pc), self.0.pc_is_shielded(pc))
    }
}

impl ProgramIndex {
    fn new(db: &dyn baml_compiler2_mir::Db, root: SourceRoot, program: &Program) -> Self {
        let mut index = Self {
            packages: package_ordinals(db, root, program),
            ..Self::default()
        };
        let mut collisions: Vec<String> = Vec::new();
        for (object, item) in program.objects.iter().enumerate() {
            match item {
                Object::Function(function) => {
                    let Some(name) = program.function_name(ObjectIndex::from_raw(object)) else {
                        continue;
                    };
                    // The ordinal of the package placing the object; a
                    // hand-built program may list no ranges.
                    let placing = u32::try_from(object).ok().and_then(|raw| {
                        (0u32..)
                            .zip(&program.packages)
                            .find(|(_, package)| package.objects.contains(&raw))
                    });
                    let display = match &placing {
                        Some((_, package)) if name.starts_with('.') => {
                            format!("{}{name}", package.name)
                        }
                        _ => name.clone(),
                    };
                    if let (FunctionIdentity::Synthetic { .. }, Some((package, _))) =
                        (&function.identity, &placing)
                    {
                        collisions.extend(
                            claim(
                                &mut index.lambdas,
                                &mut index.ambiguous_lambdas,
                                LambdaKey {
                                    package: *package,
                                    identity: function.identity.clone(),
                                },
                                object,
                            )
                            .map(|key| format!("{key:?}")),
                        );
                    }
                    index.names.insert(object, name);
                    let method = match &function.identity {
                        FunctionIdentity::Declared(FnPath::Method { name, .. }) => Some(name),
                        FunctionIdentity::InterfaceBody(_) => function.identity.declared_name(),
                        _ => None,
                    };
                    if let Some(method) = method {
                        index
                            .methods
                            .entry(method.as_str().to_owned())
                            .or_default()
                            .push(object);
                        let user = program
                            .package_placing(ObjectIndex::from_raw(object))
                            .is_some_and(|package| package.kind == PackageKind::Ordinary);
                        if user {
                            index.user_methods.push(object);
                        }
                    }
                    index.displays.insert(object, display);
                    index.infos.insert(
                        object,
                        FunctionInfo {
                            kind: function.kind,
                            layout: function.argument_layout(),
                            native_key: function.native_key.as_deref().map(str::to_owned),
                            arity: function.arity,
                            trace_passthrough: function.trace_passthrough,
                        },
                    );
                }
                Object::String(text) => {
                    index.strings.entry(text.to_string()).or_insert(object);
                }
                Object::GenericFunction(generic) => {
                    index.generic_functions.push((
                        generic.function.raw(),
                        generic.type_args.clone(),
                        object,
                    ));
                }
                Object::Bigint(value) => {
                    index.bigints.entry(value.to_string()).or_insert(object);
                }
                Object::Uint8Array(bytes) => {
                    index
                        .byte_arrays
                        .entry(bytes.lock().clone())
                        .or_insert(object);
                }
                Object::Class(class) => {
                    index.heads.insert(object, class.type_tag);
                    index.class_tags.insert(object, class.type_tag.as_i64());
                }
                Object::Enum(declaration) => {
                    index.heads.insert(object, declaration.type_tag);
                    index
                        .enum_variant_counts
                        .insert(object, declaration.variants.len());
                }
                _ => {}
            }
        }
        for (slot, global) in program.globals.iter().enumerate() {
            if let ConstValue::Object(object) = global
                && index.infos.contains_key(&object.raw())
            {
                index.globals.entry(object.raw()).or_insert(slot);
            }
        }
        for (object, item) in program.objects.iter().enumerate() {
            let Object::Function(function) = item else {
                continue;
            };
            if !matches!(function.kind, FunctionKind::Bytecode) {
                continue;
            }
            let constants: Vec<(usize, ConstValue)> = function
                .bytecode
                .constants
                .iter()
                .enumerate()
                .filter(|(_, constant)| {
                    matches!(constant, ConstValue::Type(_) | ConstValue::Literal(_))
                })
                .map(|(i, constant)| (i, constant.clone()))
                .collect();
            if !constants.is_empty() {
                index.type_constants.insert(object, constants);
            }
            index
                .positions
                .insert(object, Positions::of(&function.bytecode));
        }
        for (ordinal, package) in (0u32..).zip(&program.packages) {
            let key = |path: DeclPath| DeclKey {
                package: ordinal,
                path,
            };
            // A function's cell holds its object; a `let`'s holds its value.
            for (path, &cell) in &package.globals {
                let slot = package.slot_base.raw() + cell as usize;
                if matches!(path, DeclPath::Let(_)) {
                    collisions.extend(
                        claim(&mut index.lets, &mut index.ambiguous, key(path.clone()), slot)
                            .map(|key| format!("{key:?}")),
                    );
                    continue;
                }
                if !matches!(path, DeclPath::Function(_) | DeclPath::InterfaceBody(_)) {
                    continue;
                }
                let Some(ConstValue::Object(object)) = program.globals.get(slot) else {
                    continue;
                };
                if !index.infos.contains_key(&object.raw()) {
                    continue;
                }
                collisions.extend(
                    claim(
                        &mut index.functions,
                        &mut index.ambiguous,
                        key(path.clone()),
                        object.raw(),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
            for (path, &object) in &package.classes {
                let Object::Class(class) = &program.objects[object] else {
                    continue;
                };
                collisions.extend(
                    claim(
                        &mut index.classes,
                        &mut index.ambiguous,
                        key(DeclPath::Class(path.clone())),
                        (object.raw(), class.fields.len()),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
            for (path, &object) in &package.enums {
                collisions.extend(
                    claim(
                        &mut index.enums,
                        &mut index.ambiguous,
                        key(DeclPath::Enum(path.clone())),
                        object.raw(),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
            for (path, &object) in &package.interfaces {
                collisions.extend(
                    claim(
                        &mut index.interfaces,
                        &mut index.ambiguous,
                        key(DeclPath::Interface(path.clone())),
                        object.raw(),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
            for (path, &object) in &package.type_aliases {
                collisions.extend(
                    claim(
                        &mut index.aliases,
                        &mut index.ambiguous,
                        key(DeclPath::TypeAlias(path.clone())),
                        object.raw(),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
        }
        debug_assert!(
            collisions.is_empty(),
            "the program's tables place two objects at one identity: {collisions:?}"
        );
        index
    }

    /// The object of the closure body `identity` (a synthetic function), if
    /// the program has one. `Err` when its identity is ambiguous in the
    /// program.
    pub(crate) fn lambda<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        identity: &MirFunctionId<'db>,
    ) -> Result<Option<usize>, Rejection> {
        let Some(&package) = self.packages.get(&identity_root(db, identity)) else {
            return Ok(None);
        };
        let key = LambdaKey {
            package,
            identity: function_identity(db, identity),
        };
        lookup_key(&self.lambdas, &self.ambiguous_lambdas, &key)
    }

    /// The object of the declared function `function`, if the program has
    /// one. `Err` when its identity is ambiguous in the program.
    pub(crate) fn declared(
        &self,
        db: &dyn baml_compiler2_mir::Db,
        function: FunctionLoc<'_>,
    ) -> Result<Option<usize>, Rejection> {
        let Some(coords) = declared_function_coords(db, function) else {
            return Ok(None);
        };
        self.lookup(&self.functions, coords)
    }

    /// The object and info of the callable `function`, if the program has
    /// it. `Err` when its identity is ambiguous in the program.
    pub(crate) fn function<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        function: FunctionRef<'db>,
    ) -> Result<Option<(usize, &FunctionInfo)>, Rejection> {
        let Some(coords) = function_coords(db, function) else {
            return Ok(None);
        };
        Ok(self
            .lookup(&self.functions, coords)?
            .map(|object| (object, self.info(object))))
    }

    /// The (object, field count) of `class`, if the program has it.
    pub(crate) fn class<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        class: ClassRef<'db>,
    ) -> Result<Option<(usize, usize)>, Rejection> {
        self.lookup(&self.classes, class_coords(db, class))
    }

    /// [`Self::class`] for the class a type template's head names.
    pub(crate) fn class_at(
        &self,
        head: &baml_type::DeclName,
    ) -> Result<Option<(usize, usize)>, Rejection> {
        self.lookup(&self.classes, class_head_coords(head))
    }

    /// The object of the interface a dispatch template's head names, if the
    /// program has it.
    pub(crate) fn interface_at(
        &self,
        head: &baml_type::DeclName,
    ) -> Result<Option<usize>, Rejection> {
        self.lookup(&self.interfaces, interface_head_coords(head))
    }

    /// [`Self::enum_`] for the enum a type template's head names.
    pub(crate) fn enum_at(&self, head: &baml_type::DeclName) -> Result<Option<usize>, Rejection> {
        self.lookup(&self.enums, enum_head_coords(head))
    }

    /// The program's pooled `GenericFunction` over the function at `object`
    /// at `type_args` (a plain function reference at none): the object
    /// `Constant::Function` and `Constant::GenericFunction` load, so
    /// identity is the program's. `None` when the program pools no such
    /// object, or a type argument names a declaration it does not have.
    pub(crate) fn pooled_generic_function(
        &self,
        object: usize,
        type_args: &[baml_compiler2_mir::RealizedTy],
    ) -> Option<usize> {
        let global = *self.globals.get(&object)?;
        let head_of = |decl: &baml_type::DeclName| {
            let object = self
                .class_at(decl)
                .ok()
                .flatten()
                .map(|(object, _)| object)
                .or_else(|| self.enum_at(decl).ok().flatten())?;
            self.heads.get(&object).copied()
        };
        let mut known = true;
        for ty in type_args {
            ty.visit_heads(&mut |decl| known &= head_of(decl).is_some());
        }
        if !known {
            return None;
        }
        let anchored: Vec<bex_vm_types::RealizedTy> = type_args
            .iter()
            .map(|ty| {
                ty.map_heads(&mut |decl| {
                    bex_vm_types::TypeHead::unresolved(head_of(decl).expect("checked above"))
                })
            })
            .collect();
        self.generic_functions
            .iter()
            .find(|(slot, args, _)| *slot == global && args[..] == anchored[..])
            .map(|&(_, _, object)| object)
    }

    /// The object of `enum_ref`, if the program has it.
    pub(crate) fn enum_<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        enum_ref: EnumRef<'db>,
    ) -> Result<Option<usize>, Rejection> {
        self.lookup(&self.enums, enum_coords(db, enum_ref))
    }

    /// The global slot of the top-level `let` `binding`, if the program has
    /// it.
    pub(crate) fn let_slot<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        binding: LetLoc<'db>,
    ) -> Result<Option<usize>, Rejection> {
        let head = baml_compiler2_hir_ty::lower::qualify_def(
            db,
            Definition::Let(binding),
            &let_data(db, binding).name,
        );
        let root = file_package(db, binding.file(db)).root;
        self.lookup(&self.lets, (root, DeclPath::Let(item_path(&head))))
    }

    /// The ordinal of the package emitted from `root`, if the program places
    /// it.
    pub(crate) fn package_of(&self, root: SourceRoot) -> Option<u32> {
        self.packages.get(&root).copied()
    }

    /// The dispatch tag of the class at `object`, which [`Self::class`]
    /// found: what `Rvalue::TypeTag` reads off its instances.
    pub(crate) fn class_tag(&self, object: usize) -> i64 {
        self.class_tags[&object]
    }

    /// The variant count of the enum at `object`, which [`Self::enum_`]
    /// found.
    pub(crate) fn enum_variants(&self, object: usize) -> usize {
        self.enum_variant_counts[&object]
    }

    /// `template` as the program's constant pool spells it: every head
    /// replaced by the tag the linker gave its declaration, which is how a
    /// pooled `ConstValue::Type` compares. `None` when a head names no
    /// declaration of the program.
    pub(crate) fn anchor(
        &self,
        template: &baml_compiler2_mir::TyTemplate,
    ) -> Result<Option<bex_vm_types::TyTemplate>, Rejection> {
        let mut missing = None;
        let mut failed = None;
        let anchored = template.map_heads(&mut |head: &DeclName| {
            let tag = match self.head_object(head) {
                Ok(Some(object)) => TypeTag::of_static_index(object),
                Ok(None) => {
                    missing = Some(head.clone());
                    TypeTag::of_static_index(0)
                }
                Err(rejection) => {
                    failed = Some(rejection);
                    TypeTag::of_static_index(0)
                }
            };
            TypeHead::unresolved(tag)
        });
        if let Some(rejection) = failed {
            return Err(rejection);
        }
        Ok(missing.is_none().then_some(anchored))
    }

    /// The object of the declaration `head` names: a class, enum, interface
    /// or recursive alias.
    fn head_object(&self, head: &DeclName) -> Result<Option<usize>, Rejection> {
        let path = item_path(head);
        let root = head.root();
        if let Some((object, _)) =
            self.lookup(&self.classes, (root, DeclPath::Class(path.clone())))?
        {
            return Ok(Some(object));
        }
        for (table, path) in [
            (&self.enums, DeclPath::Enum(path.clone())),
            (&self.interfaces, DeclPath::Interface(path.clone())),
            (&self.aliases, DeclPath::TypeAlias(path)),
        ] {
            if let Some(object) = self.lookup(table, (root, path))? {
                return Ok(Some(object));
            }
        }
        Ok(None)
    }

    /// The index in `function`'s constant pool of a constant equal to
    /// `wanted`, if bytecode pooled one: the constant its own `IsType` or
    /// `LoadType` of the same type reads.
    pub(crate) fn pooled_constant(&self, function: usize, wanted: &ConstValue) -> Option<usize> {
        self.type_constants
            .get(&function)?
            .iter()
            .find(|(_, constant)| constant == wanted)
            .map(|(index, _)| *index)
    }

    /// Where the MIR positions of the bytecode function at `object` landed.
    pub(crate) fn positions(&self, object: usize) -> &Positions {
        &self.positions[&object]
    }

    /// Info of the function at `object`, which [`Self::declared`] or
    /// [`Self::function`] found.
    pub(crate) fn info(&self, object: usize) -> &FunctionInfo {
        &self.infos[&object]
    }

    /// The name of the function at `object` as the loader renders it.
    pub(crate) fn name(&self, object: usize) -> &str {
        &self.names[&object]
    }

    /// Every method and interface body named `method`, whichever class or
    /// interface declares it: what a dispatch of that name can reach.
    pub(crate) fn methods_named(&self, method: &str) -> impl Iterator<Item = usize> + '_ {
        self.methods.get(method).into_iter().flatten().copied()
    }

    /// [`Self::name`] under its package's label; see
    /// [`CompiledFunction::display`].
    pub(crate) fn display(&self, object: usize) -> &str {
        &self.displays[&object]
    }

    fn lookup<T: Copy>(
        &self,
        table: &HashMap<DeclKey, T>,
        (root, path): DeclCoords,
    ) -> Result<Option<T>, Rejection> {
        // A root outside the program: nothing of it was linked.
        let Some(&package) = self.packages.get(&root) else {
            return Ok(None);
        };
        lookup_key(table, &self.ambiguous, &DeclKey { package, path })
    }
}

/// `key`'s entry in `table`, unless `ambiguous` lists it.
fn lookup_key<K: Hash + Eq + std::fmt::Debug, T: Copy>(
    table: &HashMap<K, T>,
    ambiguous: &HashSet<K>,
    key: &K,
) -> Result<Option<T>, Rejection> {
    if ambiguous.contains(key) {
        return Err(Rejection::Invalid(format!(
            "ambiguous declaration {key:?}: more than one object of the program has it"
        )));
    }
    Ok(table.get(key).copied())
}

/// The path of `head` within its package, as the program's tables key it.
fn item_path(head: &DeclName) -> LocalName {
    LocalName {
        namespace: head.namespace().clone(),
        name: head.name().clone(),
    }
}

/// Record that `key` names `value`. A second, different claim on one key
/// marks the key ambiguous for every later lookup and is returned, so the
/// caller can report every collision at once.
fn claim<K: Hash + Eq + Clone, T: Copy + Eq>(
    table: &mut HashMap<K, T>,
    ambiguous: &mut HashSet<K>,
    key: K,
    value: T,
) -> Option<K> {
    match table.entry(key) {
        Entry::Vacant(vacant) => {
            vacant.insert(value);
            None
        }
        Entry::Occupied(occupied) if *occupied.get() == value => None,
        Entry::Occupied(occupied) => {
            ambiguous.insert(occupied.key().clone());
            Some(occupied.key().clone())
        }
    }
}

/// The source root a function's identity places it in: its declaration's
/// file's, or — for a closure body — the file's of the declaration (a
/// function or a top-level `let`) its chain of owners ends in.
fn identity_root(db: &dyn baml_compiler2_mir::Db, identity: &MirFunctionId<'_>) -> SourceRoot {
    match identity {
        MirFunctionId::Declared(function) => file_package(db, function.file(db)).root,
        MirFunctionId::Synthetic { parent, .. } => match parent.as_ref() {
            FunctionOwner::Function(function) => file_package(db, function.file(db)).root,
            FunctionOwner::Let(binding) => file_package(db, binding.file(db)).root,
            FunctionOwner::Synthetic(owner) => identity_root(db, owner),
        },
    }
}

/// Each package of `program` by the source root it was emitted from: the
/// root is `program.root`, and every edge of a placed package names, in the
/// program and in the database alike, the same dependency under the same
/// manifest-fixed name. A package the root does not reach (one behind a
/// dependency that emitted nothing) gets no ordinal, so nothing of it
/// resolves.
fn package_ordinals(
    db: &dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
) -> HashMap<SourceRoot, u32> {
    let mut ordinals = HashMap::from([(root, program.root)]);
    let mut queue = VecDeque::from([(program.root, root)]);
    while let Some((ordinal, source)) = queue.pop_front() {
        let Some(package) = program.packages.get(ordinal as usize) else {
            continue;
        };
        let edges = edge_table(db, source);
        for edge in &package.edges {
            let Some((_, target)) = edges.iter().find(|(name, _)| *name == edge.name) else {
                continue;
            };
            if let Entry::Vacant(vacant) = ordinals.entry(*target) {
                vacant.insert(edge.target);
                queue.push_back((edge.target, *target));
            }
        }
    }
    ordinals
}

/// Where an inline body checks for a garbage collection.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GcChecks {
    /// At the header of each loop: some loop of the body allocates.
    pub(crate) loops: bool,
    /// On entry: the function is recursive and allocates.
    pub(crate) entry: bool,
}

/// The units on a cycle of direct calls between units (a function calling
/// itself included): Tarjan's strongly connected components, iteratively.
fn recursive_units(units: &BTreeMap<usize, Unit<'_>>) -> BTreeSet<usize> {
    let edges = |object: usize| -> Vec<usize> {
        units[&object]
            .analysis
            .user_callees
            .iter()
            .copied()
            .filter(|callee| units.contains_key(callee))
            .collect()
    };
    let mut index_of: HashMap<usize, usize> = HashMap::new();
    let mut low: HashMap<usize, usize> = HashMap::new();
    let mut on_stack: BTreeSet<usize> = BTreeSet::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut recursive = BTreeSet::new();
    for &root in units.keys() {
        if index_of.contains_key(&root) {
            continue;
        }
        let mut work: Vec<(usize, Vec<usize>, usize)> = Vec::new();
        let start = index_of.len();
        index_of.insert(root, start);
        low.insert(root, start);
        stack.push(root);
        on_stack.insert(root);
        work.push((root, edges(root), 0));
        while let Some((node, successors, next)) = work.last_mut() {
            let node = *node;
            if let Some(&succ) = successors.get(*next) {
                *next += 1;
                if succ == node {
                    recursive.insert(node);
                }
                if let Some(&succ_index) = index_of.get(&succ) {
                    if on_stack.contains(&succ) {
                        let lowest = low[&node].min(succ_index);
                        low.insert(node, lowest);
                    }
                } else {
                    let i = index_of.len();
                    index_of.insert(succ, i);
                    low.insert(succ, i);
                    stack.push(succ);
                    on_stack.insert(succ);
                    work.push((succ, edges(succ), 0));
                }
                continue;
            }
            work.pop();
            if let Some((parent, _, _)) = work.last() {
                let lowest = low[parent].min(low[&node]);
                low.insert(*parent, lowest);
            }
            if low[&node] == index_of[&node] {
                let mut component = Vec::new();
                loop {
                    let member = stack.pop().expect("the component's members are stacked");
                    on_stack.remove(&member);
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                if component.len() > 1 {
                    recursive.extend(component);
                }
            }
        }
    }
    recursive
}

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use bex_vm_types::{FnPath, types::LocalName};

    use super::*;

    #[test]
    fn symbols_are_identifiers() {
        assert_eq!(symbol_for("user.Add", 3), "user_Add_3");
        assert_eq!(
            symbol_for("user.<lambda(f, 0)>", 7),
            "user__lambda_f__0___7"
        );
        assert_eq!(symbol_for("pkg.ns.Class.m", 0), "pkg_ns_Class_m_0");
        assert_eq!(
            symbol_for("user.<(user.A as baml.Show)>.show", 12),
            "user___user_A_as_baml_Show___show_12"
        );
    }

    /// Two objects at one identity is a malformed program; the index then
    /// compiles neither against the other's object: both resolve to an
    /// `Invalid` rejection, while an identity claimed once still resolves.
    #[test]
    fn a_doubly_claimed_identity_is_rejected_on_lookup() {
        let key = |name: &str| DeclKey {
            package: 0,
            path: DeclPath::Function(FnPath::Free(LocalName {
                namespace: Vec::new(),
                name: Name::new(name),
            })),
        };
        let mut index = ProgramIndex::default();
        assert_eq!(
            claim(&mut index.functions, &mut index.ambiguous, key("f"), 3),
            None
        );
        assert_eq!(
            claim(&mut index.functions, &mut index.ambiguous, key("f"), 3),
            None,
            "the same object again is not a collision"
        );
        assert_eq!(
            claim(&mut index.functions, &mut index.ambiguous, key("f"), 4),
            Some(key("f"))
        );
        assert_eq!(
            claim(&mut index.functions, &mut index.ambiguous, key("g"), 5),
            None
        );
        assert!(matches!(
            lookup_key(&index.functions, &index.ambiguous, &key("f")),
            Err(Rejection::Invalid(reason)) if reason.starts_with("ambiguous declaration")
        ));
        assert_eq!(
            lookup_key(&index.functions, &index.ambiguous, &key("g")),
            Ok(Some(5))
        );
        assert_eq!(
            lookup_key(&index.functions, &index.ambiguous, &key("h")),
            Ok(None)
        );
    }
}
