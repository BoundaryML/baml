//! Admission: which functions of a program compile, and the module's
//! index of the program they run in.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque, hash_map::Entry};

use baml_base::SourceRoot;
use baml_compiler2_emit::{
    DeclCoords, class_coords, class_head_coords, declared_function_coords, enum_coords,
    function_coords,
};
use baml_compiler2_hir::{
    item_data::{file_functions, is_required_interface_method},
    loc::FunctionLoc,
    package::{edge_table, world_files},
};
use baml_compiler2_hir_ty::extern_loc::{ClassRef, EnumRef, FunctionRef};
use baml_compiler2_mir::{MirFunction, MirFunctionBody, MirFunctionKind, OptLevel, lower_function};
use bex_vm_types::{ConstValue, DeclPath, FunctionKind, Object, ObjectIndex, Program};

use crate::{analysis::Analysis, codegen::Generator, render::render_module};

/// Why a function stays bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {
    /// Outside the subset this backend compiles; the bytecode is authoritative.
    Unsupported(String),
    /// The MIR violated an invariant a checked program must hold: a compiler
    /// bug, reported rather than compiled around.
    Invalid(String),
}

impl std::fmt::Display for Rejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(reason) => write!(f, "unsupported: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid MIR: {reason}"),
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
pub fn compile_program<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    root: SourceRoot,
    program: &Program,
    opt: OptLevel,
    artifact_path: &str,
) -> NativeModule {
    let index = ProgramIndex::new(db, root, program);
    // Admission works on object indices: the identity generated code bakes
    // in, unique across packages where a rendered name need not be.
    let mut units: BTreeMap<usize, Unit<'db>> = BTreeMap::new();
    let mut rejected: Vec<(String, Rejection)> = Vec::new();

    for &file in world_files(db, root) {
        let lines: std::sync::Arc<[u32]> =
            baml_compiler2_emit::build_line_starts(file.text(db)).into();
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
            let name = index.name(object).to_owned();
            let info = index.info(object);
            if info.generic {
                rejected.push((name, Rejection::Unsupported("generic function".into())));
                continue;
            }
            if !matches!(info.kind, FunctionKind::Bytecode) {
                continue;
            }
            match Analysis::new(db, &index, mir, body) {
                Ok(analysis) => {
                    units.insert(
                        object,
                        Unit {
                            mir,
                            analysis,
                            lines: lines.clone(),
                        },
                    );
                }
                Err(rejection) => rejected.push((name, rejection)),
            }
        }
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
        let symbols: HashMap<usize, String> = units
            .keys()
            .map(|&object| (object, symbol_for(index.name(object), object)))
            .collect();
        let mut bodies = String::new();
        let mut functions = Vec::new();
        let mut natives: BTreeMap<String, usize> = BTreeMap::new();
        let mut failed = Vec::new();
        for (&object, unit) in &units {
            let mut generator = Generator {
                db,
                index: &index,
                symbols: &symbols,
                resumable: &resumable,
                suspends: resumable.contains(&object),
                in_loop: false,
                natives: &mut natives,
                body: body_of(unit.mir),
                analysis: &unit.analysis,
                lines: &unit.lines,
                declared_at: unit.mir.span,
                current_line: None,
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
            };
            match generator.function(&symbols[&object]) {
                Ok(code) => {
                    bodies.push_str(&code);
                    functions.push(CompiledFunction {
                        name: index.name(object).to_owned(),
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
            rejected.push((index.name(object).to_owned(), rejection));
        }
    };

    let source = render_module(&bodies, &functions, &natives, artifact_path);
    NativeModule {
        source,
        functions,
        rejected,
    }
}

fn body_of<'a, 'db>(mir: &'a MirFunction<'db>) -> &'a MirFunctionBody<'db> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => body,
        MirFunctionKind::Builtin(_) => unreachable!("units hold bytecode bodies only"),
    }
}

/// The Rust symbol of the body of function `object`, named `name`: the name
/// for readers, the object index for uniqueness. A rendered name repeats
/// across packages and within one (two spellings can render alike, and a
/// closure body renders under no package label); an object index is one
/// function of the program.
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
    /// Line starts of the function's source file, for reported lines.
    lines: std::sync::Arc<[u32]>,
}

// ─── The program's objects by identity ──────────────────────────────────────

pub(crate) struct FunctionInfo {
    pub(crate) kind: FunctionKind,
    /// The slots a call lays its arguments over (`Function::argument_layout`).
    pub(crate) layout: baml_type::CallLayout,
    pub(crate) native_key: Option<String>,
    pub(crate) arity: usize,
    pub(crate) generic: bool,
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
    /// Identities more than one object claimed: a malformed program. Every
    /// lookup of one is refused, so neither object is compiled against the
    /// other's index.
    ambiguous: HashSet<DeclKey>,
    /// Every function of the program by object index, and its name as the
    /// loader renders it.
    infos: HashMap<usize, FunctionInfo>,
    names: HashMap<usize, String>,
    /// String literal → the compile-time string object bytecode loads it from.
    pub(crate) strings: HashMap<String, usize>,
}

impl ProgramIndex {
    fn new(db: &dyn baml_compiler2_mir::Db, root: SourceRoot, program: &Program) -> Self {
        let mut index = Self {
            packages: package_ordinals(db, root, program),
            ..Self::default()
        };
        for (object, item) in program.objects.iter().enumerate() {
            match item {
                Object::Function(function) => {
                    let Some(name) = program.function_name(ObjectIndex::from_raw(object)) else {
                        continue;
                    };
                    index.names.insert(object, name);
                    index.infos.insert(
                        object,
                        FunctionInfo {
                            kind: function.kind,
                            layout: function.argument_layout(),
                            native_key: function.native_key.as_deref().map(str::to_owned),
                            arity: function.arity,
                            generic: !function.generic_param_bounds.is_empty()
                                || !function.display_type_params.is_empty(),
                        },
                    );
                }
                Object::String(text) => {
                    index.strings.entry(text.to_string()).or_insert(object);
                }
                _ => {}
            }
        }
        let mut collisions = Vec::new();
        for (ordinal, package) in (0u32..).zip(&program.packages) {
            let key = |path: DeclPath| DeclKey {
                package: ordinal,
                path,
            };
            // A function's cell holds its object; a `let`'s holds its value.
            for (path, &cell) in &package.globals {
                if !matches!(path, DeclPath::Function(_) | DeclPath::InterfaceBody(_)) {
                    continue;
                }
                let slot = package.slot_base.raw() + cell as usize;
                let Some(ConstValue::Object(object)) = program.globals.get(slot) else {
                    continue;
                };
                if !index.infos.contains_key(&object.raw()) {
                    continue;
                }
                collisions.extend(claim(
                    &mut index.functions,
                    &mut index.ambiguous,
                    key(path.clone()),
                    object.raw(),
                ));
            }
            for (path, &object) in &package.classes {
                let Object::Class(class) = &program.objects[object] else {
                    continue;
                };
                collisions.extend(claim(
                    &mut index.classes,
                    &mut index.ambiguous,
                    key(DeclPath::Class(path.clone())),
                    (object.raw(), class.fields.len()),
                ));
            }
            for (path, &object) in &package.enums {
                collisions.extend(claim(
                    &mut index.enums,
                    &mut index.ambiguous,
                    key(DeclPath::Enum(path.clone())),
                    object.raw(),
                ));
            }
        }
        debug_assert!(
            collisions.is_empty(),
            "the program's tables place two objects at one identity: {collisions:?}"
        );
        index
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

    /// The object of `enum_ref`, if the program has it.
    pub(crate) fn enum_<'db>(
        &self,
        db: &'db dyn baml_compiler2_mir::Db,
        enum_ref: EnumRef<'db>,
    ) -> Result<Option<usize>, Rejection> {
        self.lookup(&self.enums, enum_coords(db, enum_ref))
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

    fn lookup<T: Copy>(
        &self,
        table: &HashMap<DeclKey, T>,
        (root, path): DeclCoords,
    ) -> Result<Option<T>, Rejection> {
        // A root outside the program: nothing of it was linked.
        let Some(&package) = self.packages.get(&root) else {
            return Ok(None);
        };
        self.lookup_key(table, &DeclKey { package, path })
    }

    fn lookup_key<T: Copy>(
        &self,
        table: &HashMap<DeclKey, T>,
        key: &DeclKey,
    ) -> Result<Option<T>, Rejection> {
        if self.ambiguous.contains(key) {
            return Err(Rejection::Invalid(format!(
                "ambiguous declaration {key:?}: more than one object of the program has it"
            )));
        }
        Ok(table.get(key).copied())
    }
}

/// Record that `key` names `value`. A second, different claim on one key
/// marks the key ambiguous for every later lookup and is returned, so the
/// caller can report every collision at once.
fn claim<T: Copy + Eq>(
    table: &mut HashMap<DeclKey, T>,
    ambiguous: &mut HashSet<DeclKey>,
    key: DeclKey,
    value: T,
) -> Option<DeclKey> {
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

#[cfg(test)]
mod tests {
    use baml_base::Name;
    use bex_vm_types::{FnPath, types::LocalName};

    use super::*;

    #[test]
    fn symbols_are_identifiers() {
        assert_eq!(symbol_for("user.Add", 3), "user_Add_3");
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
            index.lookup_key(&index.functions, &key("f")),
            Err(Rejection::Invalid(reason)) if reason.starts_with("ambiguous declaration")
        ));
        assert_eq!(index.lookup_key(&index.functions, &key("g")), Ok(Some(5)));
        assert_eq!(index.lookup_key(&index.functions, &key("h")), Ok(None));
    }
}
