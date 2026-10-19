//! Admission, reachability, and program metadata for Rust emission.

use std::{
    borrow::Borrow,
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
    codegen::{Generator, ProgramContext},
    reach::{reachable, references},
    render::render_module,
};

/// Why a function stays bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// Outside the supported subset; execution stays in bytecode.
    Unsupported(String),
    /// A compiler invariant failed.
    Invalid(String),
    /// Unreachable from the requested entry points.
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
    /// Loader name, checked when binding the body to its program.
    pub name: String,
    /// Package-qualified name for reports, including closure bodies.
    pub display: String,
    /// The Rust symbol of the body (`bex_vm::compiled_rt::InlineBody`).
    pub symbol: String,
    /// Object index of that bytecode function in the program.
    pub object: usize,
    /// Its arity.
    pub arity: usize,
    /// Whether the body needs a resumable frame.
    pub resumable: bool,
    /// Number of frame slots.
    pub slots: u32,
    /// Frame slot holding the active catch cause.
    pub cause_slot: Option<u32>,
}

/// The generated Rust module for one program.
#[derive(Debug, Clone)]
pub struct RustModule {
    /// Rust source of the module.
    pub source: String,
    /// The functions it compiles, in emission order.
    pub functions: Vec<CompiledFunction>,
    /// Functions of any package it does not compile, with the reason.
    pub rejected: Vec<(String, Rejection)>,
}

/// Compile the reachable supported functions in the linked program.
///
/// `artifact_path` embeds that program, and `fingerprint` is its authenticated
/// digest from `baml_artifact::encode_with_digest`. Names and object indices
/// must come from the same build.
///
/// `entries` selects roots for reachability, along with top-level initializers
/// and non-stdlib methods that may be dispatched dynamically. `None` attempts
/// every function.
pub fn compile_program(
    db: &dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    artifact_path: &str,
    fingerprint: [u8; 32],
    entries: Option<&[usize]>,
) -> Result<RustModule, String> {
    crate::analysis::validate_builtin_metadata()?;
    let index = ProgramIndex::new(db, root, program);
    let mut lowered = lower_program(db, root, &index, opt, entries);
    if entries.is_some() {
        retain_reachable(&index, &mut lowered);
    }
    let (bodies, functions, builtins) =
        admission_fixpoint(db, &index, &mut lowered.units, &mut lowered.rejected);

    let source = render_module(&bodies, &functions, &builtins, artifact_path, fingerprint)?;
    Ok(RustModule {
        source,
        functions,
        rejected: lowered.rejected,
    })
}

struct LoweredProgram<'db> {
    units: BTreeMap<usize, Unit<'db>>,
    rejected: Vec<(String, Rejection)>,
    refs: HashMap<usize, BTreeSet<usize>>,
    seeds: BTreeSet<usize>,
}

fn lower_program<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    index: &ProgramIndex,
    opt: OptLevel,
    entries: Option<&[usize]>,
) -> LoweredProgram<'db> {
    // Rendered names can collide across and within packages.
    let mut units: BTreeMap<usize, Unit<'db>> = BTreeMap::new();
    let mut rejected: Vec<(String, Rejection)> = Vec::new();
    // Rejected bodies still contribute edges to reachability.
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
            // Lambdas are admitted independently of their parent.
            admit_lambdas(db, index, &mut units, &mut rejected, mir, &[], package);
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
                refs.insert(object, references(db, index, body, &mir.lambdas));
            }
            admit_unit(
                db,
                index,
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
                    seeds.extend(references(db, index, &body, &lambdas));
                }
            }
        }
    }

    LoweredProgram {
        units,
        rejected,
        refs,
        seeds,
    }
}

fn retain_reachable(index: &ProgramIndex, lowered: &mut LoweredProgram<'_>) {
    let LoweredProgram {
        units,
        rejected,
        refs,
        seeds,
    } = lowered;
    seeds.extend(index.user_methods.iter().copied());
    let reached = reachable(std::mem::take(seeds), refs);
    units.retain(|object, _| {
        let keep = reached.contains(object);
        if !keep {
            rejected.push((index.name(*object).to_owned(), Rejection::Unreached));
        }
        keep
    });
}

fn admission_fixpoint(
    db: &dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    units: &mut BTreeMap<usize, Unit<'_>>,
    rejected: &mut Vec<(String, Rejection)>,
) -> (String, Vec<CompiledFunction>, BTreeMap<String, usize>) {
    // Every failed round removes at least one unit.
    loop {
        let mut resumable: BTreeSet<usize> = units
            .iter()
            .filter(|(_, unit)| unit.analysis.suspends)
            .map(|(&object, _)| object)
            .collect();
        propagate(units, &mut resumable, |_, callee, resumable| {
            !units.contains_key(&callee) || resumable.contains(&callee)
        });
        // Inline callers must root live values around any call that may collect.
        let recursive = recursive_units(units);
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
        propagate(units, &mut may_collect, |caller, callee, may_collect| {
            !resumable.contains(&caller) && may_collect.contains(&callee)
        });
        let symbols: HashMap<usize, String> = units
            .keys()
            .map(|&object| (object, symbol_for(index.display(object), object)))
            .collect();
        let mut bodies = String::new();
        let mut functions = Vec::new();
        let mut builtins: BTreeMap<String, usize> = BTreeMap::new();
        let mut failed = Vec::new();
        for (&object, unit) in units.iter() {
            let program = ProgramContext {
                db,
                index,
                symbols: &symbols,
                resumable: &resumable,
                may_collect: &may_collect,
            };
            let mut generator = Generator::new(
                &program,
                &mut builtins,
                object,
                unit,
                checks.get(&object).copied().unwrap_or_default(),
            );
            match generator.function(&symbols[&object]) {
                Ok(code) => {
                    let (slots, cause_slot) =
                        match checked_frame_slots(generator.slots, generator.cause_slot) {
                            Ok(layout) => layout,
                            Err(rejection) => {
                                failed.push((object, rejection));
                                continue;
                            }
                        };
                    bodies.push_str(&code);
                    functions.push(CompiledFunction {
                        name: index.name(object).to_owned(),
                        display: index.display(object).to_owned(),
                        symbol: symbols[&object].clone(),
                        object,
                        arity: index.info(object).arity,
                        resumable: resumable.contains(&object),
                        slots,
                        cause_slot,
                    });
                }
                Err(rejection) => failed.push((object, rejection)),
            }
        }
        if failed.is_empty() {
            break (bodies, functions, builtins);
        }
        for (object, rejection) in failed {
            units.remove(&object);
            rejected.push((index.display(object).to_owned(), rejection));
        }
    }
}

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

/// Admit nested lambdas with their enclosing capture representations.
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

pub(crate) fn body_of<'a, 'db>(mir: &'a MirFunction<'db>) -> &'a MirFunctionBody<'db> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => body,
        MirFunctionKind::Builtin(_) => unreachable!("units hold bytecode bodies only"),
    }
}

/// Object indices disambiguate functions whose display names collide.
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

pub(crate) struct Unit<'db> {
    pub(crate) mir: &'db MirFunction<'db>,
    pub(crate) analysis: Analysis,
    /// Ordinal of the package that owns the function.
    pub(crate) package: Option<u32>,
}

pub(crate) struct FunctionInfo {
    pub(crate) kind: FunctionKind,
    /// The slots a call lays its arguments over (`Function::argument_layout`).
    pub(crate) layout: baml_type::CallLayout,
    pub(crate) native_key: Option<String>,
    pub(crate) arity: usize,
    /// Whether calls inherit the caller's `$trace` controls.
    pub(crate) trace_passthrough: bool,
}

/// Package and declaration path uniquely identify an object; rendered names do not.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DeclKey {
    package: u32,
    path: DeclPath,
}

/// Lambdas have no package-table entry, so lookup uses their full identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct LambdaKey {
    package: u32,
    identity: FunctionIdentity,
}

/// Typed object key with raw-index lookups for codegen.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ObjectKey(usize);

impl Borrow<usize> for ObjectKey {
    fn borrow(&self) -> &usize {
        &self.0
    }
}

#[derive(Default)]
struct ObjectInfo {
    function: Option<FunctionInfo>,
    name: String,
    display: String,
    head: Option<TypeTag>,
    class_tag: Option<i64>,
    enum_variants: Option<usize>,
    type_constants: Vec<(usize, ConstValue)>,
    positions: Option<Positions>,
}

#[derive(Default)]
pub(crate) struct ProgramIndex {
    /// Package ordinals derived from the linked dependency edges.
    packages: HashMap<SourceRoot, u32>,
    /// Function object by identity.
    functions: HashMap<DeclKey, usize>,
    /// Class (object, field count) by identity.
    classes: HashMap<DeclKey, (usize, usize)>,
    /// Enum object by identity.
    enums: HashMap<DeclKey, usize>,
    /// Type heads anchored by declaration identity.
    interfaces: HashMap<DeclKey, usize>,
    aliases: HashMap<DeclKey, usize>,
    /// The global slot of each top-level `let`, by identity.
    lets: HashMap<DeclKey, usize>,
    /// Lambda (closure body) object by identity.
    lambdas: HashMap<LambdaKey, usize>,
    /// Duplicate claims reject all lookups of the affected identity.
    ambiguous: HashSet<DeclKey>,
    ambiguous_lambdas: HashSet<LambdaKey>,
    objects: Vec<ObjectInfo>,
    /// Method names conservatively resolve all possible dispatch targets.
    methods: HashMap<String, Vec<usize>>,
    /// The methods and interface bodies of every non-stdlib package.
    user_methods: Vec<usize>,
    /// String literal → the compile-time string object bytecode loads it from.
    pub(crate) strings: HashMap<String, usize>,
    /// Global slot used by pooled generic function values.
    pub(crate) globals: HashMap<ObjectKey, usize>,
    /// Find a pooled generic function with the program's anchored type arguments.
    generic_functions: Vec<(usize, Box<[bex_vm_types::RealizedTy]>, usize)>,
    /// Bigint literal (as decimal text) → its pooled `Object::Bigint`.
    pub(crate) bigints: HashMap<String, usize>,
    /// Byte array literal → its pooled `Object::Uint8Array` template, which
    /// bytecode deep-copies on every evaluation.
    pub(crate) byte_arrays: HashMap<Vec<u8>, usize>,
}

/// Source and shield metadata for MIR positions in a bytecode function.
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

    /// Both backends must lower the same blocks and statements.
    pub(crate) fn fit(&self, body: &baml_compiler2_mir::MirFunctionBody<'_>) -> bool {
        self.0.block_pcs.len() == body.blocks.len()
            && self
                .0
                .block_pcs
                .iter()
                .zip(&body.blocks)
                .all(|(pcs, block)| pcs.statements.len() == block.statements.len())
    }

    /// Source line and shield state at `pc`.
    pub(crate) fn reading(&self, pc: usize) -> (usize, bool) {
        (self.0.source_line_for_pc(pc), self.0.pc_is_shielded(pc))
    }
}

impl ProgramIndex {
    fn new(db: &dyn baml_compiler2_mir::Db, root: SourceRoot, program: &Program) -> Self {
        let mut index = Self {
            packages: package_ordinals(db, root, program),
            objects: (0..program.objects.len())
                .map(|_| ObjectInfo::default())
                .collect(),
            ..Self::default()
        };
        let mut collisions = Vec::new();
        index.index_objects(program, &mut collisions);
        index.index_globals(program);
        index.index_bytecode(program);
        index.index_declarations(program, &mut collisions);
        debug_assert!(
            collisions.is_empty(),
            "the program's tables place two objects at one identity: {collisions:?}"
        );
        index
    }

    fn index_objects(&mut self, program: &Program, collisions: &mut Vec<String>) {
        for (object, item) in program.objects.iter().enumerate() {
            match item {
                Object::Function(function) => {
                    let Some(name) = program.function_name(ObjectIndex::from_raw(object)) else {
                        continue;
                    };
                    // Hand-built programs may omit package ranges.
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
                                &mut self.lambdas,
                                &mut self.ambiguous_lambdas,
                                LambdaKey {
                                    package: *package,
                                    identity: function.identity.clone(),
                                },
                                object,
                            )
                            .map(|key| format!("{key:?}")),
                        );
                    }
                    self.objects[object].name = name;
                    let method = match &function.identity {
                        FunctionIdentity::Declared(FnPath::Method { name, .. }) => Some(name),
                        FunctionIdentity::InterfaceBody(_) => function.identity.declared_name(),
                        _ => None,
                    };
                    if let Some(method) = method {
                        self.methods
                            .entry(method.as_str().to_owned())
                            .or_default()
                            .push(object);
                        let user = program
                            .package_placing(ObjectIndex::from_raw(object))
                            .is_some_and(|package| package.kind == PackageKind::Ordinary);
                        if user {
                            self.user_methods.push(object);
                        }
                    }
                    self.objects[object].display = display;
                    self.objects[object].function = Some(FunctionInfo {
                        kind: function.kind,
                        layout: function.argument_layout(),
                        native_key: function.native_key.as_deref().map(str::to_owned),
                        arity: function.arity,
                        trace_passthrough: function.trace_passthrough,
                    });
                }
                Object::String(text) => {
                    self.strings.entry(text.to_string()).or_insert(object);
                }
                Object::GenericFunction(generic) => {
                    self.generic_functions.push((
                        generic.function.raw(),
                        generic.type_args.clone(),
                        object,
                    ));
                }
                Object::Bigint(value) => {
                    self.bigints.entry(value.to_string()).or_insert(object);
                }
                Object::Uint8Array(bytes) => {
                    self.byte_arrays
                        .entry(bytes.lock().clone())
                        .or_insert(object);
                }
                Object::Class(class) => {
                    self.objects[object].head = Some(class.type_tag);
                    self.objects[object].class_tag = Some(class.type_tag.as_i64());
                }
                Object::Enum(declaration) => {
                    self.objects[object].head = Some(declaration.type_tag);
                    self.objects[object].enum_variants = Some(declaration.variants.len());
                }
                _ => {}
            }
        }
    }

    fn index_globals(&mut self, program: &Program) {
        for (slot, global) in program.globals.iter().enumerate() {
            if let ConstValue::Object(object) = global
                && self
                    .objects
                    .get(object.raw())
                    .is_some_and(|info| info.function.is_some())
            {
                self.globals.entry(ObjectKey(object.raw())).or_insert(slot);
            }
        }
    }

    fn index_bytecode(&mut self, program: &Program) {
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
                self.objects[object].type_constants = constants;
            }
            self.objects[object].positions = Some(Positions::of(&function.bytecode));
        }
    }

    fn index_declarations(&mut self, program: &Program, collisions: &mut Vec<String>) {
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
                        claim(&mut self.lets, &mut self.ambiguous, key(path.clone()), slot)
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
                if self
                    .objects
                    .get(object.raw())
                    .is_none_or(|info| info.function.is_none())
                {
                    continue;
                }
                collisions.extend(
                    claim(
                        &mut self.functions,
                        &mut self.ambiguous,
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
                        &mut self.classes,
                        &mut self.ambiguous,
                        key(DeclPath::Class(path.clone())),
                        (object.raw(), class.fields.len()),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
            for (path, &object) in &package.enums {
                collisions.extend(
                    claim(
                        &mut self.enums,
                        &mut self.ambiguous,
                        key(DeclPath::Enum(path.clone())),
                        object.raw(),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
            for (path, &object) in &package.interfaces {
                collisions.extend(
                    claim(
                        &mut self.interfaces,
                        &mut self.ambiguous,
                        key(DeclPath::Interface(path.clone())),
                        object.raw(),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
            for (path, &object) in &package.type_aliases {
                collisions.extend(
                    claim(
                        &mut self.aliases,
                        &mut self.ambiguous,
                        key(DeclPath::TypeAlias(path.clone())),
                        object.raw(),
                    )
                    .map(|key| format!("{key:?}")),
                );
            }
        }
    }

    /// Look up a lambda, rejecting ambiguous identities.
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

    /// Look up a declaration, rejecting ambiguous identities.
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

    /// Look up a callable, rejecting ambiguous identities.
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

    /// Find a pooled generic function with the program's anchored type arguments.
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
            self.objects.get(object)?.head
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

    /// Ordinal of the package that owns the function.
    pub(crate) fn package_of(&self, root: SourceRoot) -> Option<u32> {
        self.packages.get(&root).copied()
    }

    pub(crate) fn class_tag(&self, object: usize) -> i64 {
        self.objects[object].class_tag.expect("class metadata")
    }

    pub(crate) fn enum_variants(&self, object: usize) -> usize {
        self.objects[object].enum_variants.expect("enum metadata")
    }

    /// Replace declaration heads with their linked tags for constant-pool lookup.
    /// Return `None` if any declaration is absent.
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

    /// Find a matching constant in the bytecode function's pool.
    pub(crate) fn pooled_constant(&self, function: usize, wanted: &ConstValue) -> Option<usize> {
        self.objects
            .get(function)?
            .type_constants
            .iter()
            .find(|(_, constant)| constant == wanted)
            .map(|(index, _)| *index)
    }

    /// Where the MIR positions of the bytecode function at `object` landed.
    pub(crate) fn positions(&self, object: usize) -> &Positions {
        self.objects[object]
            .positions
            .as_ref()
            .expect("bytecode positions")
    }

    pub(crate) fn info(&self, object: usize) -> &FunctionInfo {
        self.objects[object]
            .function
            .as_ref()
            .expect("function metadata")
    }

    /// The name of the function at `object` as the loader renders it.
    pub(crate) fn name(&self, object: usize) -> &str {
        &self.objects[object].name
    }

    /// Every method and interface body named `method`, whichever class or
    /// interface declares it: what a dispatch of that name can reach.
    pub(crate) fn methods_named(&self, method: &str) -> impl Iterator<Item = usize> + '_ {
        self.methods.get(method).into_iter().flatten().copied()
    }

    /// [`Self::name`] under its package's label; see
    /// [`CompiledFunction::display`].
    pub(crate) fn display(&self, object: usize) -> &str {
        &self.objects[object].display
    }

    fn lookup<T: Copy>(
        &self,
        table: &HashMap<DeclKey, T>,
        (root, path): DeclCoords,
    ) -> Result<Option<T>, Rejection> {
        let Some(&package) = self.packages.get(&root) else {
            return Ok(None);
        };
        lookup_key(table, &self.ambiguous, &DeclKey { package, path })
    }
}

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

fn item_path(head: &DeclName) -> LocalName {
    LocalName {
        namespace: head.namespace().clone(),
        name: head.name().clone(),
    }
}

/// Conflicting claims make every later lookup of the identity fail.
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

/// Follow synthetic owners to their enclosing declaration's source root.
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

/// Match linked dependency edges to source roots. Unplaced packages stay unresolved.
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

/// Propagate a call effect until the set stops growing.
fn propagate(
    units: &BTreeMap<usize, Unit<'_>>,
    set: &mut BTreeSet<usize>,
    joins: impl Fn(usize, usize, &BTreeSet<usize>) -> bool,
) {
    loop {
        let grown: Vec<usize> = units
            .iter()
            .filter(|(object, unit)| {
                !set.contains(*object)
                    && unit
                        .analysis
                        .user_callees
                        .iter()
                        .any(|&callee| joins(**object, callee, set))
            })
            .map(|(&object, _)| object)
            .collect();
        if grown.is_empty() {
            return;
        }
        set.extend(grown);
    }
}

/// Find recursive call components with iterative Tarjan traversal.
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

fn checked_frame_slots(
    slots: usize,
    cause_slot: Option<usize>,
) -> Result<(u32, Option<u32>), Rejection> {
    let overflow = |_| Rejection::Unsupported("compiled frame slot count exceeds u32".into());
    Ok((
        u32::try_from(slots).map_err(overflow)?,
        cause_slot
            .map(u32::try_from)
            .transpose()
            .map_err(overflow)?,
    ))
}

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use bex_vm_types::{FnPath, types::LocalName};

    use super::*;

    #[test]
    fn frame_slots_preserve_representable_counts() {
        assert_eq!(checked_frame_slots(0, None), Ok((0, None)));
        assert_eq!(
            checked_frame_slots(u32::MAX as usize, Some(u32::MAX as usize - 1)),
            Ok((u32::MAX, Some(u32::MAX - 1)))
        );
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn oversized_frame_slots_are_rejected() {
        let oversized = u32::MAX as usize + 1;
        assert!(matches!(
            checked_frame_slots(oversized, None),
            Err(Rejection::Unsupported(_))
        ));
        assert!(matches!(
            checked_frame_slots(0, Some(oversized)),
            Err(Rejection::Unsupported(_))
        ));
    }

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
