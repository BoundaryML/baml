//! MIR to Rust, against the VM's runtime surface.
//!
//! One Rust function per admitted BAML function, with the VM's native call
//! signature. Statically typed `int`, `float`, and `bool` locals are raw
//! `i64`, `f64`, and `bool`; every other local is a `Value`. Every operation
//! is a call into `bex_vm::compiled_rt`, the implementation the interpreter
//! runs too, so results and exceptions are identical by construction.
//!
//! Control flow is structured by [`crate::structure`] from the MIR graph, so
//! loops are Rust loops and LLVM sees them as such. A block under a `catch`
//! has an unwind edge; a fallible operation in such a block binds the thrown
//! value into the handler's error local and jumps there, exactly as the
//! interpreter's unwinder would within one frame.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
};

use baml_compiler2_hir::item_data::{file_functions, is_required_interface_method};
use baml_compiler2_mir::{
    AggregateKind, BinOp, BlockId, Constant, Local, MirFunction, MirFunctionBody, MirFunctionKind,
    Operand, OptLevel, Place, Rvalue, ShortCircuitKind, StatementKind, SwitchKey, Terminator,
    TypeTest, UnaryOp, class_display_name, enum_display_name, function_display_name,
    lower_function,
};
use baml_type::{Literal, TyTemplate};
use bex_vm_types::{FunctionKind, Object, ObjectIndex, Program};

use crate::structure::{Cfg, Flow, Stmt, Structured, structurize};

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
    /// `Function::name` of the bytecode function this body replaces.
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
    /// Root-package functions it does not compile, with the reason.
    pub rejected: Vec<(String, Rejection)>,
}

/// Compile every admitted function of `root`'s package.
///
/// `program` is the linked program the bytecode of these same functions lives
/// in; names and object indices are read from it so the generated module
/// refers to exactly this build (the generated crate embeds it as
/// `PROGRAM_ARTIFACT`, read from `artifact_path`).
pub fn compile_program<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    root: baml_base::SourceRoot,
    program: &Program,
    opt: OptLevel,
    artifact_path: &str,
) -> NativeModule {
    let index = ProgramIndex::new(program);
    let mut units: BTreeMap<String, Unit<'db>> = BTreeMap::new();
    let mut rejected: Vec<(String, Rejection)> = Vec::new();

    for &file in root.files(db) {
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
            let name = mir.identity.display_name(db);
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                continue;
            };
            let Some(info) = index.functions.get(&name) else {
                rejected.push((
                    name,
                    Rejection::Invalid("no function of this name in the program".into()),
                ));
                continue;
            };
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
                        name,
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
        let mut resumable: BTreeSet<String> = units
            .iter()
            .filter(|(_, unit)| unit.analysis.suspends)
            .map(|(name, _)| name.clone())
            .collect();
        loop {
            let grown: Vec<String> =
                units
                    .iter()
                    .filter(|(name, unit)| {
                        !resumable.contains(*name)
                            && unit.analysis.user_callees.iter().any(|callee| {
                                !units.contains_key(callee) || resumable.contains(callee)
                            })
                    })
                    .map(|(name, _)| name.clone())
                    .collect();
            if grown.is_empty() {
                break;
            }
            resumable.extend(grown);
        }
        let symbols: HashMap<String, String> = units
            .keys()
            .enumerate()
            .map(|(ordinal, name)| (name.clone(), symbol_for(name, ordinal)))
            .collect();
        let mut bodies = String::new();
        let mut functions = Vec::new();
        let mut natives: BTreeMap<String, usize> = BTreeMap::new();
        let mut failed = Vec::new();
        for (name, unit) in &units {
            let mut generator = Generator {
                db,
                index: &index,
                symbols: &symbols,
                resumable: &resumable,
                suspends: resumable.contains(name),
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
            };
            match generator.function(&symbols[name]) {
                Ok(code) => {
                    bodies.push_str(&code);
                    let info = &index.functions[name];
                    functions.push(CompiledFunction {
                        name: name.clone(),
                        symbol: symbols[name].clone(),
                        object: index.objects[name],
                        arity: info.arity,
                        resumable: resumable.contains(name),
                        slots: u32::try_from(generator.slots).unwrap_or(u32::MAX),
                        cause_slot: generator
                            .cause_slot
                            .map(|slot| u32::try_from(slot).unwrap_or(u32::MAX)),
                    });
                }
                Err(rejection) => failed.push((name.clone(), rejection)),
            }
        }
        if failed.is_empty() {
            break (bodies, functions, natives);
        }
        for (name, rejection) in failed {
            units.remove(&name);
            rejected.push((name, rejection));
        }
    };

    let source = render_module(&bodies, &functions, &natives, artifact_path);
    NativeModule {
        source,
        functions,
        rejected,
    }
}

/// `body`'s loops: the targets of its back edges (edges to a block on the
/// depth-first path from the entry, which for structured source are loop
/// headers), and whether each block lies in some loop (a header, or a block
/// that reaches a back edge's source without passing its header).
fn loops(body: &MirFunctionBody<'_>) -> (BTreeSet<usize>, Vec<bool>) {
    let successors = |block: usize| {
        body.blocks[block]
            .terminator
            .iter()
            .flat_map(Terminator::successors)
            .map(|id| id.0)
            .collect::<Vec<_>>()
    };
    let n = body.blocks.len();
    let mut preds = vec![Vec::new(); n];
    for block in 0..n {
        for succ in successors(block) {
            preds[succ].push(block);
        }
    }
    let mut headers = BTreeSet::new();
    let mut back_edges = Vec::new();
    let mut on_path = vec![false; n];
    let mut seen = vec![false; n];
    let mut stack = vec![(body.entry.0, successors(body.entry.0), 0)];
    seen[body.entry.0] = true;
    on_path[body.entry.0] = true;
    while let Some((block, succs, next)) = stack.last_mut() {
        let Some(&succ) = succs.get(*next) else {
            on_path[*block] = false;
            stack.pop();
            continue;
        };
        *next += 1;
        if on_path[succ] {
            headers.insert(succ);
            back_edges.push((*block, succ));
        } else if !seen[succ] {
            seen[succ] = true;
            on_path[succ] = true;
            stack.push((succ, successors(succ), 0));
        }
    }
    let mut in_loop = vec![false; n];
    for (source, header) in back_edges {
        let mut members = vec![false; n];
        members[header] = true;
        let mut work = vec![source];
        while let Some(block) = work.pop() {
            if !std::mem::replace(&mut members[block], true) {
                work.extend(preds[block].iter().copied());
            }
        }
        for (member, in_loop) in members.iter().zip(in_loop.iter_mut()) {
            *in_loop |= *member;
        }
    }
    (headers, in_loop)
}

fn body_of<'a, 'db>(mir: &'a MirFunction<'db>) -> &'a MirFunctionBody<'db> {
    match &mir.kind {
        MirFunctionKind::Bytecode(body) => body,
        MirFunctionKind::Builtin(_) => unreachable!("units hold bytecode bodies only"),
    }
}

fn symbol_for(name: &str, ordinal: usize) -> String {
    let mut symbol = String::with_capacity(name.len() + 8);
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            symbol.push(ch);
        } else {
            symbol.push('_');
        }
    }
    format!("{symbol}_{ordinal}")
}

struct Unit<'db> {
    mir: &'db MirFunction<'db>,
    analysis: Analysis,
    /// Line starts of the function's source file, for reported lines.
    lines: std::sync::Arc<[u32]>,
}

// ─── The program's objects by name ──────────────────────────────────────────

struct FunctionInfo {
    kind: FunctionKind,
    /// The slots a call lays its arguments over (`Function::argument_layout`).
    layout: baml_type::CallLayout,
    native_key: Option<String>,
    arity: usize,
    generic: bool,
}

struct ProgramIndex {
    functions: HashMap<String, FunctionInfo>,
    /// Function name → object index.
    objects: HashMap<String, usize>,
    /// Class link name → (object index, field count).
    classes: HashMap<String, (usize, usize)>,
    /// Enum link name → object index.
    enums: HashMap<String, usize>,
    /// String literal → the compile-time string object bytecode loads it from.
    strings: HashMap<String, usize>,
}

impl ProgramIndex {
    fn new(program: &Program) -> Self {
        let mut functions = HashMap::new();
        let mut objects = HashMap::new();
        let mut classes = HashMap::new();
        let mut enums = HashMap::new();
        let mut strings = HashMap::new();
        // Names as the loader renders them: under the label of the package
        // that placed the declaration. A pooled declaration's own `name()`
        // has no owner until load, so it would render unqualified.
        for (index, object) in program.objects.iter().enumerate() {
            match object {
                Object::Function(function) => {
                    let Some(name) = program.function_name(ObjectIndex::from_raw(index)) else {
                        continue;
                    };
                    objects.insert(name.clone(), index);
                    functions.insert(
                        name,
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
                    strings.entry(text.to_string()).or_insert(index);
                }
                _ => {}
            }
        }
        for package in &program.packages {
            for (path, &object) in &package.classes {
                if let Object::Class(class) = &program.objects[object] {
                    let name = qualified(&package.name, path);
                    classes.insert(name, (object.raw(), class.fields.len()));
                }
            }
            for (path, &object) in &package.enums {
                let name = qualified(&package.name, path);
                enums.insert(name, object.raw());
            }
        }
        Self {
            functions,
            objects,
            classes,
            enums,
            strings,
        }
    }
}

/// A declaration's fully qualified name, `package.ns.Name`: what MIR's
/// `class_display_name` and `enum_display_name` spell. `TypeName`'s display
/// name elides the local package and would miss them.
fn qualified(package: &baml_type::Name, path: &bex_vm_types::types::LocalName) -> String {
    std::iter::once(package.as_str())
        .chain(path.namespace.iter().map(baml_type::Name::as_str))
        .chain(std::iter::once(path.name.as_str()))
        .collect::<Vec<_>>()
        .join(".")
}

/// VM-owned builtins that never yield to run a callback, by `native_key`.
/// Anything not listed is rejected: a yielding builtin cannot run inside a
/// compiled body. `primitive_types` says the call has type arguments, all
/// statically known primitives or arrays of them.
fn builtin_is_callable(key: &str, primitive_types: bool) -> bool {
    const ARRAY: &[&str] = &[
        "push", "pop", "first", "last", "is_empty", "reverse", "slice", "concat",
    ];
    // Structural rendering yields only to run a user `to_string` override,
    // which a value of a primitive type cannot contain.
    if key == "baml._to_string_default" {
        return primitive_types;
    }
    if let Some(method) = key.strip_prefix("baml.Array.") {
        return ARRAY.contains(&method);
    }
    key.starts_with("baml.Int.")
        || key.starts_with("baml.Float.")
        || key.starts_with("baml.String.")
}

// ─── Per-function analysis ───────────────────────────────────────────────────

/// How a local is held in generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Repr {
    Int,
    Float,
    Bool,
    Value,
}

impl Repr {
    fn of(ty: &baml_compiler2_mir::RuntimeTy) -> Self {
        use baml_compiler2_mir::RuntimeTy as T;
        match ty {
            T::Int | T::Literal(Literal::Int(_), _) => Self::Int,
            T::Float | T::Literal(Literal::Float(_), _) => Self::Float,
            T::Bool | T::Literal(Literal::Bool(_), _) => Self::Bool,
            _ => Self::Value,
        }
    }

    fn rust_type(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Float => "f64",
            Self::Bool => "bool",
            Self::Value => "Value",
        }
    }

    fn default_value(self) -> &'static str {
        match self {
            Self::Int => "0i64",
            Self::Float => "0.0f64",
            Self::Bool => "false",
            Self::Value => "Value::NULL",
        }
    }
}

struct Analysis {
    arity: usize,
    reprs: Vec<Repr>,
    structured: Structured,
    /// Link names of the user functions this function calls directly.
    user_callees: BTreeSet<String>,
    /// Locals assigned a statically known type (`LoadType` of a template that
    /// needs no frame type arguments), with the `RealizedTy` expression that
    /// builds it. Such a local is only ever a call's type argument.
    static_types: HashMap<usize, StaticTy>,
    /// Whether the body itself suspends (await, spawn, a sys op, or a call
    /// compiled code cannot make directly); callers of suspending functions
    /// are found by the admission fixpoint.
    suspends: bool,
    /// Iterators over an array (`xs.iter()` of a local of array type), by
    /// iterator local → array local. Compiled as an index walk with the
    /// semantics `baml.iter.ArrayIterator` documents.
    array_iters: HashMap<usize, usize>,
}

impl Analysis {
    fn new<'db>(
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        mir: &MirFunction<'db>,
        body: &MirFunctionBody<'db>,
    ) -> Result<Self, Rejection> {
        if mir.arity + 1 > body.locals.len() {
            return Err(Rejection::Invalid("fewer locals than parameters".into()));
        }
        for (i, local) in body.locals.iter().enumerate() {
            if local.is_captured {
                return Err(Rejection::Unsupported(format!(
                    "local _{i} is captured by a closure"
                )));
            }
        }
        let mut reprs: Vec<Repr> = body
            .locals
            .iter()
            .map(|local| Repr::of(&local.ty))
            .collect();
        // A parameter with a default arrives as `OmittedArg` and MIR's
        // prologue tests for that sentinel in the body; such a local stays a
        // `Value` so the test sees the sentinel instead of an unboxed zero.
        let mut static_types = HashMap::new();
        for block in &body.blocks {
            for statement in &block.statements {
                let StatementKind::Assign { destination, value } = &statement.kind else {
                    continue;
                };
                match value {
                    Rvalue::BinaryOp { left, right, .. } => {
                        for (this, other) in [(left, right), (right, left)] {
                            if let (
                                Operand::Copy(Place::Local(local))
                                | Operand::Move(Place::Local(local)),
                                Operand::Constant(Constant::OmittedArg),
                            ) = (this, other)
                            {
                                reprs[local.0] = Repr::Value;
                            }
                        }
                    }
                    Rvalue::LoadType(template) => {
                        if let (Place::Local(local), Some(code)) =
                            (destination, static_realized(db, index, template))
                        {
                            let primitive = is_primitive(template);
                            static_types.insert(local.0, StaticTy { code, primitive });
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut array_iters = HashMap::new();
        for block in &body.blocks {
            if let Some(terminator) = &block.terminator
                && let Some((iter, array)) = array_iter_call(body, terminator)
            {
                array_iters.insert(iter, array);
            }
        }

        let mut flows = Vec::with_capacity(body.blocks.len());
        let mut unwinds = Vec::with_capacity(body.blocks.len());
        let mut user_callees = BTreeSet::new();
        let mut suspends = false;
        for block in &body.blocks {
            let Some(terminator) = &block.terminator else {
                return Err(Rejection::Invalid(format!(
                    "{} has no terminator",
                    block.id
                )));
            };
            let array_iteration = array_iter_call(body, terminator).is_some()
                || array_next_call(terminator, &array_iters).is_some();
            flows.push(flow_of(terminator, array_iteration)?);
            unwinds.push(block.unwind.map(|b| b.0));
            if let Terminator::Call { callee, .. } = terminator
                && let Some(name) = user_callee_name(db, index, callee)
            {
                user_callees.insert(name);
            }
            suspends |= suspends_directly(db, index, terminator, &static_types);
        }
        let cfg = Cfg {
            entry: body.entry.0,
            flows,
            unwinds,
        };
        let structured =
            structurize(&cfg).map_err(|error| Rejection::Invalid(error.to_string()))?;
        Ok(Self {
            arity: mir.arity,
            reprs,
            structured,
            user_callees,
            static_types,
            suspends,
            array_iters,
        })
    }
}

/// `xs.iter()` on a local of array type, as `(iterator local, array local)`.
fn array_iter_call(
    body: &MirFunctionBody<'_>,
    terminator: &Terminator<'_>,
) -> Option<(usize, usize)> {
    let Terminator::VirtualCall {
        has_trace: false,
        method,
        args,
        ntypeargs: 0,
        destination: Place::Local(iter),
        ..
    } = terminator
    else {
        return None;
    };
    let [Operand::Copy(Place::Local(array)) | Operand::Move(Place::Local(array))] = args.as_slice()
    else {
        return None;
    };
    (method == "iter"
        && matches!(
            body.locals[array.0].ty,
            baml_compiler2_mir::RuntimeTy::List(_)
        ))
    .then_some((iter.0, array.0))
}

/// `it.next()` on an array iterator, as the iterator local.
fn array_next_call(
    terminator: &Terminator<'_>,
    array_iters: &HashMap<usize, usize>,
) -> Option<usize> {
    let Terminator::VirtualCall {
        has_trace: false,
        method,
        args,
        ntypeargs: 0,
        destination: Place::Local(_),
        ..
    } = terminator
    else {
        return None;
    };
    let [Operand::Copy(Place::Local(iter)) | Operand::Move(Place::Local(iter))] = args.as_slice()
    else {
        return None;
    };
    (method == "next" && array_iters.contains_key(&iter.0)).then_some(iter.0)
}

/// The edge shape of a terminator, or why it keeps the function bytecode.
/// `array_iteration` marks a virtual call this backend compiles as array
/// iteration (see [`Analysis::array_iters`]).
fn flow_of(terminator: &Terminator<'_>, array_iteration: bool) -> Result<Flow, Rejection> {
    Ok(match terminator {
        Terminator::Goto { target } => Flow::Goto(target.0),
        Terminator::Branch {
            then_block,
            else_block,
            ..
        }
        | Terminator::NarrowBind {
            then_block,
            else_block,
            ..
        } => Flow::Branch {
            then_block: then_block.0,
            else_block: else_block.0,
        },
        Terminator::Switch {
            arms, otherwise, ..
        } => {
            if arms
                .iter()
                .any(|(key, _)| !matches!(key, SwitchKey::Int(_)))
            {
                return Err(Rejection::Unsupported("switch on a class tag".into()));
            }
            Flow::Switch {
                arms: arms.iter().map(|(_, target)| target.0).collect(),
                otherwise: otherwise.0,
            }
        }
        Terminator::Return | Terminator::Unreachable => Flow::Exit,
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => Flow::Exit,
        Terminator::Call {
            target, has_trace, ..
        } => {
            if *has_trace {
                return Err(Rejection::Unsupported(
                    "call with a $trace attachment".into(),
                ));
            }
            Flow::Goto(target.0)
        }
        Terminator::ShortCircuit { eval_rhs, join, .. } => Flow::Branch {
            then_block: join.0,
            else_block: eval_rhs.0,
        },
        Terminator::VirtualCall { target, method, .. } => {
            if array_iteration {
                Flow::Goto(target.0)
            } else {
                return Err(Rejection::Unsupported(format!(
                    "interface method call `{method}` on a receiver that is not an array"
                )));
            }
        }
        Terminator::SysOp { target, .. }
        | Terminator::Await { target, .. }
        | Terminator::AwaitAny { target, .. } => Flow::Goto(target.0),
        Terminator::Spawn { resume, .. } => Flow::Goto(resume.0),
        // The panic case leaves through the block's unwind edge or the
        // function; the edge here is the not-a-panic continuation.
        Terminator::ThrowIfPanic { otherwise, .. } => Flow::Goto(otherwise.0),
    })
}

/// Whether `terminator` suspends whatever it calls: the suspending
/// instructions, a call through a function value, a sys op called as a
/// function, and a builtin compiled code cannot call directly. A call to a
/// user function suspends when that function does, which the admission
/// fixpoint decides.
fn suspends_directly<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    terminator: &Terminator<'db>,
    static_types: &HashMap<usize, StaticTy>,
) -> bool {
    match terminator {
        Terminator::SysOp { .. }
        | Terminator::Spawn { .. }
        | Terminator::Await { .. }
        | Terminator::AwaitAny { .. } => true,
        Terminator::Call {
            callee,
            args,
            ntypeargs,
            ..
        } => {
            let Operand::Constant(Constant::Function(func)) = callee else {
                return true;
            };
            let Some(info) = index.functions.get(&function_display_name(db, *func)) else {
                return false;
            };
            match info.kind {
                FunctionKind::Bytecode => false,
                FunctionKind::SysOp(_) => true,
                FunctionKind::Native(_) | FunctionKind::NativeUnresolved => {
                    let types_primitive = args[..(*ntypeargs).min(args.len())].iter().all(|arg| {
                        matches!(arg, Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))
                            if static_types.get(&local.0).is_some_and(|ty| ty.primitive))
                    });
                    !info.native_key.as_deref().is_some_and(|key| {
                        builtin_is_callable(key, *ntypeargs > 0 && types_primitive)
                    })
                }
            }
        }
        _ => false,
    }
}

fn user_callee_name<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    callee: &Operand<'db>,
) -> Option<String> {
    let Operand::Constant(Constant::Function(func)) = callee else {
        return None;
    };
    let name = function_display_name(db, *func);
    let info = index.functions.get(&name)?;
    matches!(info.kind, FunctionKind::Bytecode).then_some(name)
}

// ─── Code generation ─────────────────────────────────────────────────────────

/// A generated expression and how it is held.
struct Expr {
    code: String,
    repr: Repr,
}

impl Expr {
    fn new(code: impl Into<String>, repr: Repr) -> Self {
        Self {
            code: code.into(),
            repr,
        }
    }
}

struct Generator<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    index: &'a ProgramIndex,
    symbols: &'a HashMap<String, String>,
    /// The admitted functions that can suspend.
    resumable: &'a BTreeSet<String>,
    /// Whether this function can suspend: it is emitted as a resumable state
    /// machine over its blocks rather than as structured code.
    suspends: bool,
    /// While emitting a resumable function's block loop (where a jump is
    /// `bb = N; continue`), as opposed to its resume prologue.
    in_loop: bool,
    /// Builtin native keys the module binds, by key → ordinal.
    natives: &'a mut BTreeMap<String, usize>,
    body: &'a MirFunctionBody<'db>,
    analysis: &'a Analysis,
    /// Line starts of the source file, and the function's declaration span.
    lines: &'a [u32],
    declared_at: Option<baml_base::Span>,
    /// The line the code emitted so far in the current leaf has set.
    current_line: Option<usize>,
    out: String,
    /// `let` bindings to emit before the statement being generated: every
    /// operand that is not an atom is bound first, so no call's argument list
    /// holds another call (two `vm` borrows) whatever the expression is.
    pending: Vec<String>,
    temps: usize,
    /// For a resumable function, the locals live on entry to each block:
    /// what a suspension keeps in the frame's slots.
    live_in: Vec<Vec<bool>>,
    /// The stack slots the body keeps in its frame: a resumable body's
    /// locals and iterator positions, then its point slot, then the cause
    /// slot; just the cause slot for an inline body with a `catch`.
    slots: usize,
    /// The slot holding the context of the error being handled, which the
    /// VM's cause chain reads; `None` without a `catch`.
    cause_slot: Option<usize>,
    /// A resumable body's slot for the point it suspended at.
    point_slot: usize,
    /// Each block's predecessors along normal edges.
    preds: Vec<Vec<usize>>,
    /// A resumable body's loop headers: where it checks for a yield.
    loop_headers: BTreeSet<usize>,
    /// A resumable body's write-through locals: kept across suspensions but
    /// assigned only outside loops, so each assignment also writes the
    /// local's slot and no suspension saves it.
    write_through: BTreeSet<usize>,
}

type Gen<T> = Result<T, Rejection>;

/// Loop iterations of a resumable body per check for a yield; see
/// `Generator::leaf`.
const YIELD_EVERY: u32 = 16;

/// What a suspension keeps: live locals (local `i` in slot `i`), then live
/// array iterators with the slot of their position.
type Saved = (Vec<usize>, Vec<(usize, usize)>);

fn unsupported<T>(what: impl Into<String>) -> Gen<T> {
    Err(Rejection::Unsupported(what.into()))
}

impl<'db> Generator<'_, 'db> {
    fn repr(&self, local: Local) -> Repr {
        self.analysis.reprs[local.0]
    }

    fn function(&mut self, symbol: &str) -> Gen<String> {
        self.prepare();
        if self.suspends {
            return self.resumable_function(symbol);
        }
        let arity = self.body.locals.len().min(self.arity());
        let mut out = String::new();
        // The body runs in a closure so every way out, `?` included, passes
        // the wrapper: an error leaving the function records the line it was
        // raised on in this function's frame. `depth` is the frame count with
        // that frame on top; a catch also pops whatever compiled callees left
        // above it.
        let declared = self.declared_at.map_or(0, |span| self.line_of(span));
        let _ = writeln!(
            out,
            "#[inline]\npub fn {symbol}(vm: &mut BexVm, args: &[Value]) -> R<Value> {{\n    \
             let depth = vm.frame_depth();\n    let mut line: u32 = {declared};\n    \
             let result = (|| -> R<Value> {{"
        );
        for (i, repr) in self.analysis.reprs.iter().enumerate() {
            if self.analysis.static_types.contains_key(&i) {
                continue;
            }
            let ty = repr.rust_type();
            if (1..=arity).contains(&i) {
                let arg = coerce(&format!("args[{}]", i - 1), Repr::Value, *repr);
                let _ = writeln!(out, "    let mut l{i}: {ty} = {arg};");
            } else {
                let _ = writeln!(out, "    let mut l{i}: {ty} = {};", repr.default_value());
            }
        }
        for iter in self.analysis.array_iters.keys() {
            let _ = writeln!(out, "    let mut i{iter}: i64 = 0;");
        }
        for block in &self.body.blocks {
            match &block.terminator {
                Some(
                    Terminator::Branch { .. }
                    | Terminator::NarrowBind { .. }
                    | Terminator::ShortCircuit { .. },
                ) => {
                    let _ = writeln!(out, "    let mut c{}: bool = false;", block.id.0);
                }
                Some(Terminator::Switch { .. }) => {
                    let _ = writeln!(out, "    let mut d{}: i64 = 0;", block.id.0);
                }
                _ => {}
            }
        }
        // The cause slot: the context of the error a handler is handling,
        // which the VM's cause chain reads off the frame.
        if let Some(slot) = self.cause_slot {
            let _ = writeln!(
                out,
                "    vm.reserve_slots(depth, {});\n    vm.set_slot(depth, {slot}, Value::NULL);",
                self.slots
            );
        }
        let stmts = self.analysis.structured.stmts.clone();
        self.out.clear();
        self.stmts(&stmts, 1)?;
        out.push_str(&self.out);
        // Every path ends in an `Exit`; this is unreachable, and keeps the
        // function well-typed if a leaf falls through after structuring.
        out.push_str(
            "    #[allow(unreachable_code)]\n    Ok(Value::NULL)\n    })();\n    \
             if result.is_err() {\n        vm.note_line(depth, line);\n    }\n    result\n}\n",
        );
        Ok(out)
    }

    /// The line bytecode attributes to a statement starting where `span` does.
    fn line_of(&self, span: baml_base::Span) -> usize {
        baml_compiler2_emit::statement_line(self.lines, span.range.start().into())
    }

    /// Emit `line = N;` when the line bytecode attributes to code under
    /// `span` differs from the one the current leaf last set. A terminator is
    /// a sequence point; a statement's faulting instruction follows its
    /// operands, where bytecode takes a span crossing one line break at its
    /// end ([`baml_compiler2_emit::span_line`]).
    fn set_line(&mut self, span: Option<baml_base::Span>, depth: usize, sequence_point: bool) {
        let Some(span) = span else { return };
        let line = baml_compiler2_emit::span_line(
            self.lines,
            span.range.start().into(),
            span.range.end().into(),
            sequence_point,
        );
        if self.current_line != Some(line) {
            self.current_line = Some(line);
            self.line(depth, &format!("line = {line};"));
        }
    }

    /// Whether a `Call` terminator suspends: see [`suspends_directly`]; a
    /// call to a user function suspends unless it is compiled and inline.
    fn call_suspends(&self, terminator: &Terminator<'db>) -> bool {
        if suspends_directly(self.db, self.index, terminator, &self.analysis.static_types) {
            return true;
        }
        let Terminator::Call { callee, .. } = terminator else {
            return false;
        };
        user_callee_name(self.db, self.index, callee)
            .is_some_and(|name| !self.symbols.contains_key(&name) || self.resumable.contains(&name))
    }

    /// Whether `block` ends in a suspension point.
    fn block_suspends(&self, block: usize) -> bool {
        match self.body.blocks[block].terminator.as_ref() {
            Some(
                Terminator::SysOp { .. }
                | Terminator::Spawn { .. }
                | Terminator::Await { .. }
                | Terminator::AwaitAny { .. },
            ) => true,
            Some(terminator @ Terminator::Call { .. }) => self.call_suspends(terminator),
            _ => false,
        }
    }

    /// A jump to `block` in a resumable function.
    fn jump_to(&self, block: usize) -> String {
        if self.in_loop {
            format!("bb = {block}; continue;")
        } else {
            format!("bb = {block};")
        }
    }

    /// The array iterators, in the order their positions take slots after
    /// the locals'.
    fn iters(&self) -> Vec<usize> {
        let mut iters: Vec<usize> = self.analysis.array_iters.keys().copied().collect();
        iters.sort_unstable();
        iters
    }

    /// What a suspension at the end of `block` keeps in the frame's slots:
    /// each local live once it resumes, then the position of each live array
    /// iterator, with its slot. Live once it resumes means live into the
    /// block the resume continues at, less the local the resume writes, plus
    /// live into the block's handler when a raise can land there. Every other
    /// local is dead across the suspension: it is neither saved nor restored,
    /// and keeps its default until it is next written.
    fn saved_at(&self, block: BlockId) -> Gen<Saved> {
        let (destination, target) = self.resume_edge(block)?;
        let mut live = self.live_in[target.0].clone();
        if let Place::Local(local) = destination {
            live[local.0] = false;
        }
        if let Some(handler) = self.body.blocks[block.0].unwind {
            for (live, handler) in live.iter_mut().zip(&self.live_in[handler.0]) {
                *live |= *handler;
            }
        }
        Ok(self.saved(live))
    }

    /// What a suspension keeps when the locals `live` are live once it
    /// resumes. With a cause slot, every handler's context local is kept too:
    /// a block leaving a nested handler sets the slot back from the enclosing
    /// handler's context (see [`Self::leaf`]), a read MIR's liveness does not
    /// see.
    fn saved(&self, mut live: Vec<bool>) -> Saved {
        if self.cause_slot.is_some() {
            for landing in self.body.blocks.iter().filter_map(|block| block.landing) {
                live[landing.context_local.0] = true;
            }
        }
        let locals = (0..self.analysis.reprs.len())
            .filter(|&i| live[i] && !self.analysis.static_types.contains_key(&i))
            .collect();
        let base = self.analysis.reprs.len().max(self.arity());
        let iters = self
            .iters()
            .into_iter()
            .enumerate()
            .filter(|&(_, iter)| live[iter])
            .map(|(j, iter)| (iter, base + j))
            .collect();
        (locals, iters)
    }

    /// The point a resumable body records for a yield at loop header
    /// `header`, after the points of its suspending blocks (`block + 1`).
    fn yield_point(&self, header: usize) -> usize {
        self.body.blocks.len() + 1 + header
    }

    /// Work out the body's frame layout and the control-flow facts its
    /// emission reads.
    fn prepare(&mut self) {
        let blocks = &self.body.blocks;
        self.preds = vec![Vec::new(); blocks.len()];
        for block in blocks {
            for successor in block.terminator.iter().flat_map(Terminator::successors) {
                self.preds[successor.0].push(block.id.0);
            }
        }
        let catches = blocks.iter().any(|block| block.handling.is_some());
        if self.suspends {
            let arity = self.body.locals.len().min(self.arity());
            let base = self.analysis.reprs.len().max(arity) + self.iters().len();
            self.point_slot = base;
            self.cause_slot = catches.then_some(base + 1);
            self.slots = base + 1 + usize::from(catches);
            self.live_in = baml_compiler2_mir::live_in_precise(self.body);
            let (headers, in_loop) = loops(self.body);
            self.loop_headers = headers;
            self.write_through = self.write_through_locals(&in_loop);
        } else {
            self.cause_slot = catches.then_some(0);
            self.slots = usize::from(catches);
        }
    }

    /// The locals a resumable body writes through to their slots: each one
    /// that some suspension keeps, is assigned only in blocks outside every
    /// loop (so writing its slot at each assignment runs a bounded number of
    /// times, where saving it runs at every suspension), and is a plain
    /// scalar or value (not a float, which writing would box, nor an
    /// iterator or a handler's landing local, which are assigned elsewhere).
    fn write_through_locals(&self, in_loop: &[bool]) -> BTreeSet<usize> {
        let mut kept = BTreeSet::new();
        for block in 0..self.body.blocks.len() {
            if self.block_suspends(block)
                && let Ok((locals, _)) = self.saved_at(BlockId(block))
            {
                kept.extend(locals);
            }
        }
        for &header in &self.loop_headers {
            kept.extend(self.saved(self.live_in[header].clone()).0);
        }
        let mut excluded: BTreeSet<usize> = self.analysis.array_iters.keys().copied().collect();
        for block in &self.body.blocks {
            if let Some(landing) = block.landing {
                excluded.insert(landing.error_local.0);
                excluded.insert(landing.context_local.0);
            }
        }
        for (block, &looped) in self.body.blocks.iter().zip(in_loop) {
            if !looped {
                continue;
            }
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(local),
                    ..
                } = &statement.kind
                {
                    excluded.insert(local.0);
                }
            }
            if let Some(
                Terminator::Call {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::VirtualCall {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::SysOp {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::Await {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::AwaitAny {
                    destination: Place::Local(local),
                    ..
                }
                | Terminator::Spawn {
                    future: Place::Local(local),
                    ..
                },
            ) = &block.terminator
            {
                excluded.insert(local.0);
            }
            if let Some(Terminator::NarrowBind { destination, .. }) = &block.terminator {
                excluded.insert(destination.0);
            }
        }
        kept.into_iter()
            .filter(|local| {
                !excluded.contains(local) && !matches!(self.repr(Local(*local)), Repr::Float)
            })
            .collect()
    }

    /// Whether the handler `block` runs under differs from one it can be
    /// entered from: where the cause slot is set.
    fn cause_changes_at(&self, block: usize) -> bool {
        let handling = self.body.blocks[block].handling;
        let preds = &self.preds[block];
        if preds.is_empty() {
            return handling.is_some();
        }
        preds
            .iter()
            .any(|&pred| self.body.blocks[pred].handling != handling)
    }

    /// The local a suspension at the end of `block` writes when it resumes,
    /// and the block it continues at.
    fn resume_edge(&self, block: BlockId) -> Gen<(Place, BlockId)> {
        match self.body.blocks[block.0].terminator.as_ref() {
            Some(
                Terminator::Call {
                    destination,
                    target,
                    ..
                }
                | Terminator::SysOp {
                    destination,
                    target,
                    ..
                }
                | Terminator::Await {
                    destination,
                    target,
                    ..
                }
                | Terminator::AwaitAny {
                    destination,
                    target,
                    ..
                },
            ) => Ok((destination.clone(), *target)),
            Some(Terminator::Spawn { future, resume, .. }) => Ok((future.clone(), *resume)),
            _ => Err(Rejection::Invalid(
                "resume after a non-suspending block".into(),
            )),
        }
    }

    /// A resumable function: the VM drives it through `Resume`/`Step`. Its
    /// arguments and every local live across a suspension sit in its frame's
    /// stack slots (local `i` in slot `i`, an iterator position after the
    /// locals), so the GC sees them while it is suspended. The blocks run in a
    /// `loop { match bb }`, since a resume enters the middle of the function.
    fn resumable_function(&mut self, symbol: &str) -> Gen<String> {
        let arity = self.body.locals.len().min(self.arity());
        let iters = self.iters();
        let slots = self.slots;
        let point_slot = self.point_slot;
        let declared = self.declared_at.map_or(0, |span| self.line_of(span));
        self.out.clear();
        let _ = writeln!(
            self.out,
            "pub fn {symbol}(vm: &mut BexVm, resume: Resume) -> Result<Step, Box<VmError>> {{\n    \
             let depth = vm.frame_depth();\n    let mut line: u32 = {declared};\n    \
             let result = (|| -> Result<Step, Box<VmError>> {{"
        );
        for (i, repr) in self.analysis.reprs.iter().enumerate() {
            if !self.analysis.static_types.contains_key(&i) {
                let _ = writeln!(
                    self.out,
                    "    let mut l{i}: {} = {};",
                    repr.rust_type(),
                    repr.default_value()
                );
            }
        }
        for iter in &iters {
            let _ = writeln!(self.out, "    let mut i{iter}: i64 = 0;");
        }
        if !self.loop_headers.is_empty() {
            let _ = writeln!(self.out, "    let mut iterations: u32 = 0;");
        }
        for block in &self.body.blocks {
            match &block.terminator {
                Some(
                    Terminator::Branch { .. }
                    | Terminator::NarrowBind { .. }
                    | Terminator::ShortCircuit { .. },
                ) => {
                    let _ = writeln!(self.out, "    let mut c{}: bool = false;", block.id.0);
                }
                Some(Terminator::Switch { .. }) => {
                    let _ = writeln!(self.out, "    let mut d{}: i64 = 0;", block.id.0);
                }
                _ => {}
            }
        }
        // Prologue: start, or read the point this body suspended at, restore
        // what it saved and take the result of its request.
        self.in_loop = false;
        self.line(1, "let mut bb: usize;");
        self.line(1, "match resume {");
        self.line(2, "Resume::Start => {");
        self.line(3, &format!("vm.reserve_slots(depth, {slots});"));
        if let Some(cause) = self.cause_slot {
            self.line(3, &format!("vm.set_slot(depth, {cause}, Value::NULL);"));
        }
        // A write-through local's slot holds its value from its first
        // assignment on; before that, its default, so a resume restoring it
        // where liveness overapproximates (a local a terminator assigns)
        // reads a well-typed value, as when every suspension saved it.
        for local in self.write_through.clone() {
            if local > arity {
                let default = coerce(
                    self.repr(Local(local)).default_value(),
                    self.repr(Local(local)),
                    Repr::Value,
                );
                self.line(3, &format!("vm.set_slot(depth, {local}, {default});"));
            }
        }
        for i in 1..=arity {
            let arg = coerce(
                &format!("vm.slot(depth, {})", i - 1),
                Repr::Value,
                self.repr(Local(i)),
            );
            self.line(3, &format!("l{i} = {arg};"));
            if self.write_through.contains(&i) {
                let slot = coerce(&format!("l{i}"), self.repr(Local(i)), Repr::Value);
                self.line(3, &format!("vm.set_slot(depth, {i}, {slot});"));
            }
        }
        let entry = self.body.entry.0;
        self.line(3, &format!("bb = {entry};"));
        self.line(2, "}");
        // Each resume arm restores what its suspension saved (`saved_at`).
        self.line(2, "resume => {");
        self.line(
            3,
            &format!("let point = bex_vm::compiled_rt::expect_int(vm.slot(depth, {point_slot}))?;"),
        );
        self.line(3, "match (point, resume) {");
        for block in 0..self.body.blocks.len() {
            if self.block_suspends(block) {
                self.resume_arms(BlockId(block), 4)?;
            }
        }
        for header in self.loop_headers.clone() {
            self.line(
                4,
                &format!("({}, Resume::Yielded) => {{", self.yield_point(header)),
            );
            let live = self.live_in[header].clone();
            self.restore(self.saved(live), 5);
            self.line(5, &format!("bb = {header};"));
            self.line(4, "}");
        }
        self.line(4, "(_, Resume::Thrown(e)) => return Err(e),");
        self.line(4, "_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.line(3, "}");
        self.line(2, "}");
        self.line(1, "}");
        // The blocks.
        self.in_loop = true;
        self.line(1, "loop {");
        self.line(2, "match bb {");
        for block in 0..self.body.blocks.len() {
            if self.body.blocks[block].terminator.is_none() {
                continue;
            }
            self.line(3, &format!("{block} => {{"));
            self.leaf(BlockId(block), 4)?;
            self.transfer(BlockId(block), 4)?;
            self.line(3, "}");
        }
        self.line(3, "_ => return Err(bex_vm::compiled_rt::unreachable()),");
        self.line(2, "}");
        self.line(1, "}");
        self.out.push_str(
            "    })();\n    if result.is_err() {\n        vm.note_line(depth, line);\n    }\n    result\n}\n",
        );
        Ok(std::mem::take(&mut self.out))
    }

    /// The prologue arms that resume after `block`'s suspension: bind the
    /// result and continue at its target, or land an error in the block's
    /// handler.
    fn resume_arms(&mut self, block: BlockId, depth: usize) -> Gen<()> {
        let point = block.0 + 1;
        let (destination, target) = self.resume_edge(block)?;
        if !matches!(destination, Place::Local(_)) {
            return unsupported("a suspending call's result stored into a field or element");
        }
        let store = self.store(block, &destination, &Expr::new("v", Repr::Value))?;
        self.line(depth, &format!("({point}, Resume::Value(v)) => {{"));
        self.restore_locals(block, depth + 1)?;
        self.line(depth + 1, &store);
        self.line(depth + 1, &format!("bb = {};", target.0));
        self.line(depth, "}");
        if self.body.blocks[block.0].unwind.is_some() {
            self.line(depth, &format!("({point}, Resume::Thrown(e)) => {{"));
            self.restore_locals(block, depth + 1)?;
            let jump = self.unwind_jump(block, "e", depth + 1)?;
            self.flush(depth + 1);
            self.out.push_str(&jump);
            self.line(depth, "}");
        }
        Ok(())
    }

    /// Read back what the suspension at the end of `block` saved.
    fn restore_locals(&mut self, block: BlockId, depth: usize) -> Gen<()> {
        let saved = self.saved_at(block)?;
        self.restore(saved, depth);
        Ok(())
    }

    /// Read `saved` back from the frame's slots.
    fn restore(&mut self, (locals, iters): Saved, depth: usize) {
        for i in locals {
            let value = coerce(
                &format!("vm.slot(depth, {i})"),
                Repr::Value,
                self.repr(Local(i)),
            );
            self.line(depth, &format!("l{i} = {value};"));
        }
        for (iter, slot) in iters {
            self.line(
                depth,
                &format!("i{iter} = bex_vm::compiled_rt::expect_int(vm.slot(depth, {slot}))?;"),
            );
        }
    }

    /// Save `saved` in the frame's slots: all but the write-through locals,
    /// whose slots already hold them.
    fn save(&mut self, (locals, iters): Saved, depth: usize) {
        for i in locals {
            if self.write_through.contains(&i) {
                continue;
            }
            let value = self.value_arg(&format!("l{i}"), self.repr(Local(i)));
            self.statement(depth, &format!("vm.set_slot(depth, {i}, {value});"));
        }
        for (iter, slot) in iters {
            self.statement(
                depth,
                &format!("vm.set_slot(depth, {slot}, Value::int(i{iter}));"),
            );
        }
    }

    /// How a block of a resumable function hands control on.
    fn transfer(&mut self, id: BlockId, depth: usize) -> Gen<()> {
        if self.block_suspends(id.0) {
            return self.pause(id, depth);
        }
        let terminator = self.body.blocks[id.0].terminator.as_ref().expect("checked");
        let b = id.0;
        let code = match terminator {
            Terminator::Goto { target }
            | Terminator::Call { target, .. }
            | Terminator::VirtualCall { target, .. } => self.jump_to(target.0),
            Terminator::Branch {
                then_block,
                else_block,
                ..
            }
            | Terminator::NarrowBind {
                then_block,
                else_block,
                ..
            } => format!(
                "bb = if c{b} {{ {} }} else {{ {} }}; continue;",
                then_block.0, else_block.0
            ),
            Terminator::ShortCircuit { eval_rhs, join, .. } => format!(
                "bb = if c{b} {{ {} }} else {{ {} }}; continue;",
                join.0, eval_rhs.0
            ),
            Terminator::Switch {
                arms, otherwise, ..
            } => {
                let mut arms_code = String::new();
                for (key, target) in arms {
                    let SwitchKey::Int(key) = key else {
                        unreachable!("rejected by flow_of")
                    };
                    let _ = write!(arms_code, "{key}i64 => {}, ", target.0);
                }
                format!(
                    "bb = match d{b} {{ {arms_code}_ => {} }}; continue;",
                    otherwise.0
                )
            }
            Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => return self.exit(id, depth),
            Terminator::ThrowIfPanic { otherwise, .. } => self.jump_to(otherwise.0),
            Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. } => unreachable!("suspensions are emitted by `pause`"),
        };
        self.line(depth, &code);
        Ok(())
    }

    /// Suspend at `block`'s terminator: save the locals live across it and
    /// its point in the frame's slots, and hand the VM the request. An await
    /// of a future that has already settled takes its value without
    /// suspending.
    fn pause(&mut self, block: BlockId, depth: usize) -> Gen<()> {
        let point = block.0 + 1;
        let terminator = self.body.blocks[block.0]
            .terminator
            .as_ref()
            .expect("checked");
        // The request, and the arguments a call pushes above the slots.
        let (step, args) = match terminator {
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                argument_layout,
                ..
            } => {
                let (type_args, args) = args.split_at((*ntypeargs).min(args.len()));
                let mut types = Vec::with_capacity(type_args.len());
                for arg in type_args {
                    let ty = match arg {
                        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                            self.analysis
                                .static_types
                                .get(&local.0)
                                .map(|ty| ty.code.clone())
                        }
                        _ => None,
                    };
                    let Some(ty) = ty else {
                        return unsupported("call with a type argument known only at runtime");
                    };
                    types.push(ty);
                }
                self.callee_ptr(block, callee)?;
                let args = match callee {
                    Operand::Constant(Constant::Function(func)) => {
                        let name = function_display_name(self.db, *func);
                        self.laid_out_args(block, &name, argument_layout.as_ref(), args)?
                    }
                    _ if argument_layout
                        .as_ref()
                        .is_some_and(|layout| layout.0.iter().any(Option::is_some)) =>
                    {
                        return unsupported("named arguments to a function value");
                    }
                    _ => self.arg_values(block, args)?,
                };
                let type_args = if types.is_empty() {
                    "TypeArgs::new()".to_string()
                } else {
                    format!("TypeArgs::from_iter([{}])", types.join(", "))
                };
                (
                    format!(
                        "Step::Call {{ callee, argc: {}, type_args: {type_args} }}",
                        args.len()
                    ),
                    args,
                )
            }
            Terminator::SysOp { callee, args, .. } => {
                self.callee_ptr(block, callee)?;
                let args = self.arg_values(block, args)?;
                (
                    format!(
                        "Step::Call {{ callee, argc: {}, type_args: TypeArgs::new() }}",
                        args.len()
                    ),
                    args,
                )
            }
            Terminator::Await {
                future,
                destination,
                target,
                ..
            } => {
                let future = self.load(block, future)?;
                let future = self.value_arg(&future.code, future.repr);
                self.statement(depth, &format!("let operand = {future};"));
                let store = self.store(block, destination, &Expr::new("v", Repr::Value))?;
                self.line(depth, "if let Some(v) = vm.ready(operand) {");
                self.statement(depth + 1, &store);
                let jump = self.jump_to(target.0);
                self.line(depth + 1, &jump);
                self.line(depth, "}");
                ("Step::Await(operand)".to_string(), Vec::new())
            }
            Terminator::AwaitAny { futures, .. } => {
                let futures = self.operand(block, futures)?;
                let futures = self.value_arg(&futures.code, futures.repr);
                self.statement(depth, &format!("let operand = {futures};"));
                ("Step::AwaitAny(operand)".to_string(), Vec::new())
            }
            Terminator::Spawn { plan, .. } => {
                let plan = self.operand(block, plan)?;
                let plan = self.value_arg(&plan.code, plan.repr);
                self.statement(depth, &format!("let operand = {plan};"));
                ("Step::Spawn(operand)".to_string(), Vec::new())
            }
            _ => return Err(Rejection::Invalid("pause at a non-suspending block".into())),
        };
        let saved = self.saved_at(block)?;
        self.save(saved, depth);
        self.flush(depth);
        let point_slot = self.point_slot;
        self.line(
            depth,
            &format!("vm.set_slot(depth, {point_slot}, Value::int({point}));"),
        );
        for arg in &args {
            self.line(depth, &format!("vm.push_arg({arg});"));
        }
        self.line(depth, "vm.note_line(depth, line);");
        self.line(depth, &format!("return Ok({step});"));
        Ok(())
    }

    /// Bind `callee` as `let callee: HeapPtr`: a named function's object, or
    /// a function value's.
    fn callee_ptr(&mut self, block: BlockId, callee: &Operand<'db>) -> Gen<()> {
        let code = match callee {
            Operand::Constant(Constant::Function(func)) => {
                let name = function_display_name(self.db, *func);
                let Some(object) = self.index.objects.get(&name) else {
                    return unsupported(format!("callee `{name}` not found in the program"));
                };
                format!("vm.idx_to_ptr(ObjectIndex::from_raw({object}))")
            }
            Operand::Constant(_) => {
                return unsupported("call of a constant that is not a function");
            }
            Operand::Copy(_) | Operand::Move(_) => {
                let value = self.operand(block, callee)?;
                let value = self.value_arg(&value.code, value.repr);
                format!("bex_vm::compiled_rt::callee_ptr({value})?")
            }
        };
        self.statement(0, &format!("let callee = {code};"));
        Ok(())
    }

    /// The argument `Value`s of a call to `name`, in the callee's slot order:
    /// a call with named or omitted optionals is laid over the callee's
    /// declared slots at compile time, as the VM's `remap_call_arguments` does
    /// at run time, with `OMITTED_ARG` where the callee's default applies.
    fn laid_out_args(
        &mut self,
        block: BlockId,
        name: &str,
        layout: Option<&baml_type::CallLayout>,
        args: &[Operand<'db>],
    ) -> Gen<Vec<String>> {
        let Some(info) = self.index.functions.get(name) else {
            return unsupported(format!("callee `{name}` not found in the program"));
        };
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let arg = self.operand(block, arg)?;
            values.push(self.value_arg(&arg.code, arg.repr));
        }
        let values = match layout {
            Some(layout) if layout.0.iter().any(Option::is_some) => layout
                .map_to(&info.layout)
                .map_err(|mismatch| {
                    Rejection::Invalid(format!("call to `{name}` does not fit it: {mismatch:?}"))
                })?
                .into_iter()
                .map(|slot| {
                    slot.map_or_else(|| "Value::OMITTED_ARG".to_string(), |i| values[i].clone())
                })
                .collect(),
            _ => values,
        };
        if values.len() != info.arity {
            return Err(Rejection::Invalid(format!(
                "`{name}` called with {} of {} arguments",
                values.len(),
                info.arity
            )));
        }
        Ok(values)
    }

    /// The comma-separated `Value`s of a request's arguments.
    fn arg_values(&mut self, block: BlockId, args: &[Operand<'db>]) -> Gen<Vec<String>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let arg = self.operand(block, arg)?;
            values.push(self.value_arg(&arg.code, arg.repr));
        }
        Ok(values)
    }

    fn arity(&self) -> usize {
        // The MIR body's parameters are locals 1..=arity; `Analysis::new`
        // checked the count against the locals.
        self.analysis.arity
    }

    fn stmts(&mut self, stmts: &[Stmt], depth: usize) -> Gen<()> {
        for stmt in stmts {
            self.stmt(stmt, depth)?;
        }
        Ok(())
    }

    fn line(&mut self, depth: usize, text: &str) {
        for _ in 0..depth {
            self.out.push_str("    ");
        }
        self.out.push_str(text);
        self.out.push('\n');
    }

    /// Emit a statement, preceded by the bindings its expressions hoisted.
    fn statement(&mut self, depth: usize, text: &str) {
        let pending = std::mem::take(&mut self.pending);
        for binding in pending {
            self.line(depth, &binding);
        }
        self.line(depth, text);
    }

    /// Bind `expr` to a temporary unless it is an atom (a local or a
    /// literal), so it can be an argument of a call into the VM.
    fn hoist(&mut self, expr: Expr) -> Expr {
        if is_atom(&expr.code) {
            return expr;
        }
        let name = format!("t{}", self.temps);
        self.temps += 1;
        self.pending.push(format!(
            "let {name}: {} = {};",
            expr.repr.rust_type(),
            expr.code
        ));
        Expr::new(name, expr.repr)
    }

    /// `code`, held as `repr`, coerced to a `Value` that can be a VM call
    /// argument.
    fn value_arg(&mut self, code: &str, repr: Repr) -> String {
        let coerced = coerce(code, repr, Repr::Value);
        self.hoist(Expr::new(coerced, Repr::Value)).code
    }

    fn stmt(&mut self, stmt: &Stmt, depth: usize) -> Gen<()> {
        match stmt {
            Stmt::Leaf(block) => self.leaf(BlockId(*block), depth),
            Stmt::Block { label, body } => {
                self.line(depth, &format!("'b{label}: {{"));
                self.stmts(body, depth + 1)?;
                self.line(depth, "}");
                Ok(())
            }
            Stmt::Loop { label, body } => {
                self.line(depth, &format!("'l{label}: loop {{"));
                self.stmts(body, depth + 1)?;
                self.line(depth, "}");
                Ok(())
            }
            Stmt::If {
                block,
                then_branch,
                else_branch,
            } => {
                self.line(depth, &format!("if c{block} {{"));
                self.stmts(then_branch, depth + 1)?;
                self.line(depth, "} else {");
                self.stmts(else_branch, depth + 1)?;
                self.line(depth, "}");
                Ok(())
            }
            Stmt::Switch {
                block,
                arms,
                otherwise,
            } => {
                let Some(Terminator::Switch { arms: keys, .. }) =
                    &self.body.blocks[*block].terminator
                else {
                    return Err(Rejection::Invalid(
                        "switch stmt on a non-switch block".into(),
                    ));
                };
                self.line(depth, &format!("match d{block} {{"));
                for (indices, body) in arms {
                    let pattern: Vec<String> = indices
                        .iter()
                        .map(|&i| match keys[i].0 {
                            SwitchKey::Int(key) => format!("{key}i64"),
                            SwitchKey::Class(_) => unreachable!("rejected by flow_of"),
                        })
                        .collect();
                    self.line(depth + 1, &format!("{} => {{", pattern.join(" | ")));
                    self.stmts(body, depth + 2)?;
                    self.line(depth + 1, "}");
                }
                self.line(depth + 1, "_ => {");
                self.stmts(otherwise, depth + 2)?;
                self.line(depth + 1, "}");
                self.line(depth, "}");
                Ok(())
            }
            Stmt::Break(label) => {
                self.line(depth, &format!("break 'b{label};"));
                Ok(())
            }
            Stmt::BreakLoop(label) => {
                self.line(depth, &format!("break 'l{label};"));
                Ok(())
            }
            Stmt::Continue(label) => {
                self.line(depth, &format!("continue 'l{label};"));
                Ok(())
            }
            Stmt::Exit(block) => self.exit(BlockId(*block), depth),
        }
    }

    /// The statements of a block and whatever its terminator computes before
    /// control transfers.
    fn leaf(&mut self, id: BlockId, depth: usize) -> Gen<()> {
        let block = &self.body.blocks[id.0];
        // A leaf can be entered from several places; nothing about `line` is
        // known on entry.
        self.current_line = None;
        // A resumable body lets the scheduler in from its loops. It counts
        // iterations in a register and takes the VM's control-flow check
        // every `YIELD_EVERY`th: a compiled iteration is a fraction of an
        // interpreted one, so this still polls about as often in time.
        if self.suspends && self.loop_headers.contains(&id.0) {
            let live = self.live_in[id.0].clone();
            let point = self.yield_point(id.0);
            let point_slot = self.point_slot;
            self.line(depth, "iterations = iterations.wrapping_add(1);");
            self.line(
                depth,
                &format!("if iterations % {YIELD_EVERY} == 0 && vm.should_yield() {{"),
            );
            self.save(self.saved(live), depth + 1);
            self.line(
                depth + 1,
                &format!("vm.set_slot(depth, {point_slot}, Value::int({point}));"),
            );
            self.line(depth + 1, "vm.note_line(depth, line);");
            self.line(depth + 1, "return Ok(Step::Yield);");
            self.line(depth, "}");
        }
        // Entering or leaving a handler's body: record the context of the
        // error it handles, which a raise here chains onto as its cause.
        if let Some(slot) = self.cause_slot
            && self.cause_changes_at(id.0)
        {
            let cause = match block.handling.and_then(|h| self.body.blocks[h.0].landing) {
                Some(landing) => {
                    let context = format!("l{}", landing.context_local.0);
                    self.value_arg(&context, self.repr(landing.context_local))
                }
                None => "Value::NULL".to_string(),
            };
            self.statement(depth, &format!("vm.set_slot(depth, {slot}, {cause});"));
        }
        for statement in &block.statements {
            self.set_line(statement.span, depth, false);
            match &statement.kind {
                StatementKind::Assign { destination, value } => {
                    // A static type local is consumed as a call's type
                    // argument; it has no runtime value of its own.
                    if let (Place::Local(local), Rvalue::LoadType(_)) = (destination, value)
                        && self.analysis.static_types.contains_key(&local.0)
                    {
                        continue;
                    }
                    let value = self.rvalue(id, value)?;
                    let code = self.store(id, destination, &value)?;
                    self.statement(depth, &code);
                }
                StatementKind::Drop(_) | StatementKind::Nop => {}
                StatementKind::FreshCell { .. } => {
                    return unsupported("captured local");
                }
                StatementKind::Intrinsic { .. } => return unsupported("compiler intrinsic"),
                StatementKind::VirtualFieldStore { .. } => {
                    return unsupported("interface field store");
                }
            }
        }
        self.set_line(block.terminator_span, depth, true);
        match block.terminator.as_ref().expect("checked in Analysis") {
            Terminator::Branch { condition, .. } => {
                let condition = self.operand(id, condition)?;
                let code = coerce(&condition.code, condition.repr, Repr::Bool);
                self.statement(depth, &format!("c{} = {code};", id.0));
            }
            Terminator::NarrowBind {
                source,
                test,
                destination,
                ..
            } => {
                let source = self.operand(id, source)?;
                let source_value = self.value_arg(&source.code, source.repr);
                self.statement(depth, &format!("let narrowed = {source_value};"));
                let test = self.type_test("narrowed", test)?;
                self.statement(depth, &format!("c{} = {test};", id.0));
                let store = self.store(
                    id,
                    &Place::Local(*destination),
                    &Expr::new("narrowed", Repr::Value),
                )?;
                self.statement(depth, &format!("if c{} {{ {store} }}", id.0));
            }
            Terminator::Switch { discriminant, .. } => {
                let discriminant = self.operand(id, discriminant)?;
                let code = coerce(&discriminant.code, discriminant.repr, Repr::Int);
                self.statement(depth, &format!("d{} = {code};", id.0));
            }
            terminator @ Terminator::Call { .. } if self.call_suspends(terminator) => {}
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                argument_layout,
                destination,
                ..
            } => {
                let call = self.call(id, callee, args, *ntypeargs, argument_layout.as_ref())?;
                let code = self.store(id, destination, &Expr::new(call, Repr::Value))?;
                self.statement(depth, &code);
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                ..
            } => {
                let operand = self.operand(id, operand)?;
                let value = self.value_arg(&operand.code, operand.repr);
                self.statement(depth, &format!("let lhs = {value};"));
                let short = match kind {
                    ShortCircuitKind::And => "!vm.is_truthy(lhs)",
                    ShortCircuitKind::Or => "vm.is_truthy(lhs)",
                    ShortCircuitKind::Coalesce => "!lhs.is_null()",
                };
                self.statement(depth, &format!("c{} = {short};", id.0));
                let store = self.store(id, destination, &Expr::new("lhs", Repr::Value))?;
                self.statement(depth, &format!("if c{} {{ {store} }}", id.0));
            }
            Terminator::VirtualCall { destination, .. } => {
                if let Some((iter, array)) =
                    array_iter_call(self.body, block.terminator.as_ref().expect("checked"))
                {
                    let array = coerce(&format!("l{array}"), self.repr(Local(array)), Repr::Value);
                    self.statement(depth, &format!("l{iter} = {array}; i{iter} = 0;"));
                } else if let Some(iter) = array_next_call(
                    block.terminator.as_ref().expect("checked"),
                    &self.analysis.array_iters,
                ) {
                    // `baml.iter.ArrayIterator.next`: the length is re-read on
                    // every step, so a loop that grows or shrinks the array
                    // sees it, and the end is a `Done` instance.
                    let Some(&(done, _)) = self.index.classes.get("baml.iter.Done") else {
                        return unsupported("`baml.iter.Done` not found in the program");
                    };
                    let step = format!(
                        "if i{iter} < bex_vm::compiled_rt::expect_int(vm.container_len(l{iter})?)? {{ \
                         let v = vm.load_array_element(l{iter}, Value::int(i{iter}))?; i{iter} += 1; v }} \
                         else {{ vm.new_instance(vm.idx_to_ptr(ObjectIndex::from_raw({done})), Vec::new()) }}"
                    );
                    let code = self.store(id, destination, &Expr::new(step, Repr::Value))?;
                    self.statement(depth, &code);
                } else {
                    return Err(Rejection::Invalid(
                        "virtual call admitted without a lowering".into(),
                    ));
                }
            }
            Terminator::Goto { .. }
            | Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => {}
            // Emitted by `pause`, in a resumable function.
            Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. } => {}
            // Before a wildcard catch arm: a panic is raised again, with its
            // context, rather than swallowed.
            Terminator::ThrowIfPanic { value, context, .. } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                let context = self.operand(id, context)?;
                let context = self.value_arg(&context.code, context.repr);
                let error = format!("bex_vm::compiled_rt::rethrow({value}, {context})");
                let raise = if block.unwind.is_some() {
                    self.unwind_jump(id, &error, depth + 1)?
                } else {
                    format!("return Err({error});")
                };
                self.statement(depth, &format!("if vm.is_panic({value}) {{"));
                self.out.push_str(&raise);
                self.out.push('\n');
                self.line(depth, "}");
            }
        }
        Ok(())
    }

    /// How a block leaves the function.
    fn exit(&mut self, id: BlockId, depth: usize) -> Gen<()> {
        let block = &self.body.blocks[id.0];
        match block.terminator.as_ref().expect("checked in Analysis") {
            Terminator::Return => {
                let result = coerce("l0", self.repr(Local(0)), Repr::Value);
                if self.suspends {
                    self.line(depth, &format!("return Ok(Step::Return({result}));"));
                } else {
                    self.line(depth, &format!("return Ok({result});"));
                }
            }
            Terminator::Throw { value } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                let error = format!("bex_vm::compiled_rt::throw({value})");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, &error, depth)?;
                    self.flush(depth);
                    self.out.push_str(&jump);
                } else {
                    self.statement(depth, &format!("return Err({error});"));
                }
            }
            Terminator::Rethrow { value, context } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                let context = self.operand(id, context)?;
                let context = self.value_arg(&context.code, context.repr);
                let error = format!("bex_vm::compiled_rt::rethrow({value}, {context})");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, &error, depth)?;
                    self.flush(depth);
                    self.out.push_str(&jump);
                } else {
                    self.statement(depth, &format!("return Err({error});"));
                }
            }
            // Bytecode's `Unreachable`: a catchable `baml.panics.Unreachable`.
            Terminator::Unreachable => {
                self.line(depth, "let unreachable = vm.unreachable_panic();");
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, "unreachable", depth)?;
                    self.flush(depth);
                    self.out.push_str(&jump);
                } else {
                    self.line(depth, "return Err(unreachable);");
                }
            }
            other => {
                return Err(Rejection::Invalid(format!(
                    "exit through a non-exiting terminator {other:?}"
                )));
            }
        }
        Ok(())
    }

    /// The statements that land `error` (an expression of type
    /// `Box<VmError>`) in `block`'s handler: build its context and pop the
    /// frames of compiled callees above this one (`BexVm::catch`), bind value
    /// and context into the handler's landing locals, and jump there. An
    /// internal error is not catchable and propagates.
    fn unwind_jump(&mut self, block: BlockId, error: &str, depth: usize) -> Gen<String> {
        let Some(handler) = self.body.blocks[block.0].unwind else {
            return Err(Rejection::Invalid(
                "unwind jump from a block without a handler".into(),
            ));
        };
        let Some(landing) = self.body.blocks[handler.0].landing else {
            return Err(Rejection::Invalid(format!(
                "{handler} is an unwind target but has no landing"
            )));
        };
        let mut out = String::new();
        let value = self.store(
            block,
            &Place::Local(landing.error_local),
            &Expr::new("thrown", Repr::Value),
        )?;
        let context = self.store(
            block,
            &Place::Local(landing.context_local),
            &Expr::new("context", Repr::Value),
        )?;
        let saved = std::mem::take(&mut self.out);
        self.line(depth, "vm.note_line(depth, line);");
        self.line(
            depth,
            &format!("let (thrown, context) = vm.catch({error}, depth)?;"),
        );
        self.line(depth, &value);
        self.line(depth, &context);
        if self.suspends {
            let jump = self.jump_to(handler.0);
            self.line(depth, &jump);
        } else {
            let jumps = self.analysis.structured.unwind_jumps[block.0].clone();
            self.stmts(&jumps, depth)?;
        }
        out.push_str(&self.out);
        self.out = saved;
        Ok(out)
    }

    /// Emit the bindings hoisted so far.
    fn flush(&mut self, depth: usize) {
        for binding in std::mem::take(&mut self.pending) {
            self.line(depth, &binding);
        }
    }

    /// Wrap a fallible expression (`Result<_, VmError>` or
    /// `Result<_, Box<VmError>>`) for `block`: route a thrown value to the
    /// block's handler when it has one, otherwise propagate with `?`.
    fn fallible(&mut self, block: BlockId, code: &str) -> Gen<String> {
        if self.body.blocks[block.0].unwind.is_none() {
            return Ok(format!("{code}?"));
        }
        let jump = self.unwind_jump(block, "e.into()", 0)?;
        Ok(format!(
            "(match {code} {{ Ok(v) => v, Err(e) => {{ {jump} }} }})"
        ))
    }

    // ── Operands and places ───────────────────────────────────────────────

    fn operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<Expr> {
        let expr = match operand {
            Operand::Copy(place) | Operand::Move(place) => self.load(block, place)?,
            Operand::Constant(constant) => self.constant(constant)?,
        };
        Ok(self.hoist(expr))
    }

    fn constant(&self, constant: &Constant<'db>) -> Gen<Expr> {
        Ok(match constant {
            Constant::Int(n) => Expr::new(format!("{n}i64"), Repr::Int),
            Constant::Float(f) => Expr::new(float_literal(*f), Repr::Float),
            Constant::Bool(b) => Expr::new(format!("{b}"), Repr::Bool),
            Constant::Null => Expr::new("Value::NULL", Repr::Value),
            Constant::OmittedArg => Expr::new("Value::OMITTED_ARG", Repr::Value),
            // The literal's compile-time string object, as bytecode loads it:
            // no allocation per evaluation.
            Constant::String(s) => match self.index.strings.get(s.as_str()) {
                Some(object) => Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                ),
                None => Expr::new(format!("vm.string_value({s:?})"), Repr::Value),
            },
            Constant::Bigint(_) => return unsupported("bigint literal"),
            // A named function as a value: its object, as bytecode loads it.
            Constant::Function(func) => {
                let name = function_display_name(self.db, *func);
                let Some(object) = self.index.objects.get(&name) else {
                    return unsupported(format!("function `{name}` not found in the program"));
                };
                Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                )
            }
            Constant::GenericFunction { .. } => {
                return unsupported("generic function used as a value");
            }
            Constant::GlobalItem(_) => return unsupported("top-level let"),
            Constant::EnumVariant { .. } => return unsupported("enum variant literal"),
        })
    }

    fn load(&mut self, block: BlockId, place: &Place) -> Gen<Expr> {
        match place {
            Place::Local(local) => {
                if self.analysis.static_types.contains_key(&local.0) {
                    return unsupported("type value");
                }
                if self.analysis.array_iters.contains_key(&local.0) {
                    return unsupported("array iterator used as a value");
                }
                Ok(Expr::new(format!("l{}", local.0), self.repr(*local)))
            }
            Place::Field { base, field } => {
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let code = self.fallible(block, &format!("vm.load_field({base}, {field})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let index = self.value_arg(&format!("l{}", index.0), self.repr(*index));
                let code =
                    self.fallible(block, &format!("vm.load_array_element({base}, {index})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Place::Capture(_) | Place::Deref(_) => unsupported("captured local"),
        }
    }

    /// The statement storing `value` into `place`.
    fn store(&mut self, block: BlockId, place: &Place, value: &Expr) -> Gen<String> {
        match place {
            Place::Local(local) => {
                let code = coerce(&value.code, value.repr, self.repr(*local));
                if self.suspends && self.write_through.contains(&local.0) {
                    let slot = coerce(&format!("l{}", local.0), self.repr(*local), Repr::Value);
                    return Ok(format!(
                        "l{0} = {code}; vm.set_slot(depth, {0}, {slot});",
                        local.0
                    ));
                }
                Ok(format!("l{} = {code};", local.0))
            }
            Place::Field { base, field } => {
                let value = self.value_arg(&value.code, value.repr);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let code =
                    self.fallible(block, &format!("vm.store_field({base}, {field}, {value})"))?;
                Ok(format!("{code};"))
            }
            Place::Index { base, index, kind } => {
                if !matches!(kind, baml_compiler2_mir::IndexKind::Array) {
                    return unsupported("map index");
                }
                let value = self.value_arg(&value.code, value.repr);
                let base = self.load(block, base)?;
                let base = self.value_arg(&base.code, base.repr);
                let index = self.value_arg(&format!("l{}", index.0), self.repr(*index));
                let code = self.fallible(
                    block,
                    &format!("vm.store_array_element({base}, {index}, {value})"),
                )?;
                Ok(format!("{code};"))
            }
            Place::Capture(_) | Place::Deref(_) => unsupported("captured local"),
        }
    }

    // ── Rvalues ───────────────────────────────────────────────────────────

    fn rvalue(&mut self, block: BlockId, rvalue: &Rvalue<'db>) -> Gen<Expr> {
        match rvalue {
            Rvalue::Use(operand) => self.operand(block, operand),
            Rvalue::BinaryOp { op, left, right } => {
                let left = self.operand(block, left)?;
                let right = self.operand(block, right)?;
                self.binary(block, *op, &left, &right)
            }
            Rvalue::UnaryOp { op, operand } => {
                let operand = self.operand(block, operand)?;
                self.unary(block, *op, &operand)
            }
            Rvalue::Len(place) => {
                let container = self.load(block, place)?;
                let container = self.value_arg(&container.code, container.repr);
                let code = self.fallible(block, &format!("vm.container_len({container})"))?;
                Ok(Expr::new(code, Repr::Value))
            }
            Rvalue::Discriminant(place) => {
                let value = self.load(block, place)?;
                let value = self.value_arg(&value.code, value.repr);
                let code = self.fallible(block, &format!("vm.discriminant({value})"))?;
                Ok(Expr::new(code, Repr::Int))
            }
            Rvalue::TypeTag(place) => {
                let value = self.load(block, place)?;
                let value = self.value_arg(&value.code, value.repr);
                Ok(Expr::new(
                    format!("bex_vm::compiled_rt::type_tag({value})"),
                    Repr::Int,
                ))
            }
            Rvalue::IsTypeTag { operand, tag } => {
                let value = self.operand(block, operand)?;
                let value = self.value_arg(&value.code, value.repr);
                Ok(Expr::new(
                    format!("(bex_vm::compiled_rt::type_tag({value}) == {tag}i64)"),
                    Repr::Bool,
                ))
            }
            Rvalue::IsType { operand, test } => {
                let value = self.operand(block, operand)?;
                let value = self.value_arg(&value.code, value.repr);
                let code = self.type_test(&format!("({value})"), test)?;
                Ok(Expr::new(code, Repr::Bool))
            }
            Rvalue::Array(template, elements) => {
                let element_ty =
                    static_realized(self.db, self.index, template).ok_or_else(|| {
                        Rejection::Unsupported("array of a non-primitive type".into())
                    })?;
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    let element = self.operand(block, element)?;
                    values.push(self.value_arg(&element.code, element.repr));
                }
                Ok(Expr::new(
                    format!("vm.new_array({element_ty}, vec![{}])", values.join(", ")),
                    Repr::Value,
                ))
            }
            Rvalue::Aggregate {
                kind:
                    AggregateKind::Class {
                        class,
                        type_arg_templates,
                    },
                fields,
            } => {
                if !type_arg_templates.is_empty() {
                    return unsupported("instance of a generic class");
                }
                let name = class_display_name(self.db, *class);
                let Some(&(object, field_count)) = self.index.classes.get(&name) else {
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                if fields.len() != field_count {
                    return unsupported(format!(
                        "`{name}` literal with {} of {field_count} fields",
                        fields.len()
                    ));
                }
                let mut values = Vec::with_capacity(fields.len());
                for field in fields {
                    let field = self.operand(block, field)?;
                    values.push(self.value_arg(&field.code, field.repr));
                }
                let class = self.hoist(Expr::new(
                    format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                    Repr::Value,
                ));
                Ok(Expr::new(
                    format!(
                        "vm.new_instance({}.as_object_ptr().expect(\"class object\"), vec![{}])",
                        class.code,
                        values.join(", ")
                    ),
                    Repr::Value,
                ))
            }
            Rvalue::Aggregate {
                kind: AggregateKind::Array,
                ..
            } => unsupported("untyped array aggregate"),
            Rvalue::Map(..) => unsupported("map literal"),
            Rvalue::Uint8Array(_) => unsupported("byte array literal"),
            Rvalue::MakeClosure { .. } => unsupported("closure"),
            Rvalue::MakeBoundMethod { .. }
            | Rvalue::MakeVirtualBoundMethod { .. }
            | Rvalue::MakeVirtualFunction { .. }
            | Rvalue::MakeGenericFunction { .. }
            | Rvalue::MakeGenericFunctionFromValue { .. } => {
                unsupported("function used as a value")
            }
            Rvalue::VirtualFieldAccess { .. } => unsupported("interface field access"),
            Rvalue::LoadType(_) => unsupported("type value"),
            Rvalue::CurrentPackage => unsupported("current package"),
            Rvalue::TraceHookSettings { .. } => unsupported("trace hook"),
        }
    }

    fn type_test(&mut self, value: &str, test: &TypeTest<'db>) -> Gen<String> {
        match test {
            TypeTest::Class { class, args } => {
                if !args.is_empty() {
                    return unsupported("type test against a generic class");
                }
                let name = class_display_name(self.db, *class);
                let Some(&(object, _)) = self.index.classes.get(&name) else {
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                Ok(format!(
                    "vm.is_instance_of({value}, vm.idx_to_ptr(ObjectIndex::from_raw({object})))"
                ))
            }
            TypeTest::Enum(enum_ref) => {
                let name = enum_display_name(self.db, *enum_ref);
                let Some(&object) = self.index.enums.get(&name) else {
                    return unsupported(format!("enum `{name}` not found in the program"));
                };
                Ok(format!(
                    "vm.is_variant_of({value}, vm.idx_to_ptr(ObjectIndex::from_raw({object})))"
                ))
            }
            TypeTest::Template(_) => unsupported("structural type test"),
        }
    }

    fn binary(&mut self, block: BlockId, op: BinOp, left: &Expr, right: &Expr) -> Gen<Expr> {
        use Repr::{Bool, Float, Int};
        let (l, r) = (&left.code, &right.code);
        let expr = match (left.repr, right.repr, op) {
            (Int, Int, BinOp::Add) => {
                self.fallible_expr(block, &format!("vm.add_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Sub) => {
                self.fallible_expr(block, &format!("vm.sub_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Mul) => {
                self.fallible_expr(block, &format!("vm.mul_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Div) => {
                self.fallible_expr(block, &format!("vm.div_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Mod) => {
                self.fallible_expr(block, &format!("vm.mod_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Shl) => {
                self.fallible_expr(block, &format!("vm.shl_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::Shr) => {
                self.fallible_expr(block, &format!("vm.shr_i64({l}, {r})"), Int)?
            }
            (Int, Int, BinOp::BitAnd) => Expr::new(format!("({l} & {r})"), Int),
            (Int, Int, BinOp::BitOr) => Expr::new(format!("({l} | {r})"), Int),
            (Int, Int, BinOp::BitXor) => Expr::new(format!("({l} ^ {r})"), Int),
            (Int, Int, BinOp::Eq) => Expr::new(format!("({l} == {r})"), Bool),
            (Int, Int, BinOp::Ne) => Expr::new(format!("({l} != {r})"), Bool),
            (Int, Int, BinOp::Lt) => Expr::new(format!("({l} < {r})"), Bool),
            (Int, Int, BinOp::Le) => Expr::new(format!("({l} <= {r})"), Bool),
            (Int, Int, BinOp::Gt) => Expr::new(format!("({l} > {r})"), Bool),
            (Int, Int, BinOp::Ge) => Expr::new(format!("({l} >= {r})"), Bool),
            (Float, Float, BinOp::Add) => Expr::new(format!("({l} + {r})"), Float),
            (Float, Float, BinOp::Sub) => Expr::new(format!("({l} - {r})"), Float),
            (Float, Float, BinOp::Mul) => Expr::new(format!("({l} * {r})"), Float),
            (Float, Float, BinOp::Div) => Expr::new(format!("({l} / {r})"), Float),
            (Float, Float, BinOp::Mod) => Expr::new(format!("({l} % {r})"), Float),
            (
                Float,
                Float,
                BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge,
            ) => Expr::new(
                format!(
                    "bex_vm_types::float_order::apply(CmpOp::{}, {l}, {r})",
                    cmp_name(op)
                ),
                Bool,
            ),
            (Bool, Bool, BinOp::Eq) => Expr::new(format!("({l} == {r})"), Bool),
            (Bool, Bool, BinOp::Ne) => Expr::new(format!("({l} != {r})"), Bool),
            (Bool, Bool, BinOp::BitAnd) => Expr::new(format!("({l} & {r})"), Bool),
            (Bool, Bool, BinOp::BitOr) => Expr::new(format!("({l} | {r})"), Bool),
            (Bool, Bool, BinOp::BitXor) => Expr::new(format!("({l} ^ {r})"), Bool),
            _ => {
                // The interpreter's generic operator over tagged values.
                let l = self.value_arg(l, left.repr);
                let r = self.value_arg(r, right.repr);
                match op {
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        let code = self.fallible(
                            block,
                            &format!("vm.cmpop(CmpOp::{}, {l}, {r})", cmp_name(op)),
                        )?;
                        Expr::new(code, Repr::Value)
                    }
                    _ => {
                        let code = self.fallible(
                            block,
                            &format!("vm.binop(BinOp::{}, {l}, {r})", binop_name(op)),
                        )?;
                        Expr::new(code, Repr::Value)
                    }
                }
            }
        };
        Ok(expr)
    }

    fn fallible_expr(&mut self, block: BlockId, code: &str, repr: Repr) -> Gen<Expr> {
        let code = self.fallible(block, code)?;
        Ok(Expr::new(code, repr))
    }

    fn unary(&mut self, block: BlockId, op: UnaryOp, operand: &Expr) -> Gen<Expr> {
        let code = &operand.code;
        Ok(match (op, operand.repr) {
            (UnaryOp::Not, Repr::Bool) => Expr::new(format!("(!{code})"), Repr::Bool),
            (UnaryOp::Not, repr) => {
                let value = self.value_arg(code, repr);
                Expr::new(format!("vm.not({value})"), Repr::Bool)
            }
            (UnaryOp::Neg, Repr::Int) => {
                self.fallible_expr(block, &format!("vm.neg_i64({code})"), Repr::Int)?
            }
            (UnaryOp::Neg, Repr::Float) => Expr::new(format!("(-{code})"), Repr::Float),
            (UnaryOp::Neg, repr) => {
                let value = self.value_arg(code, repr);
                self.fallible_expr(block, &format!("vm.neg({value})"), Repr::Value)?
            }
            (UnaryOp::Truthy, Repr::Bool) => Expr::new(code.clone(), Repr::Bool),
            (UnaryOp::Truthy, repr) => {
                let value = self.value_arg(code, repr);
                Expr::new(format!("vm.is_truthy({value})"), Repr::Bool)
            }
        })
    }

    // ── Calls ─────────────────────────────────────────────────────────────

    fn call(
        &mut self,
        block: BlockId,
        callee: &Operand<'db>,
        args: &[Operand<'db>],
        ntypeargs: usize,
        layout: Option<&baml_type::CallLayout>,
    ) -> Gen<String> {
        let Operand::Constant(Constant::Function(func)) = callee else {
            return unsupported("call through a function value");
        };
        let name = function_display_name(self.db, *func);
        let Some(info) = self.index.functions.get(&name) else {
            return unsupported(format!("callee `{name}` not found in the program"));
        };
        let (type_args, args) = args.split_at(ntypeargs.min(args.len()));
        // Type arguments must be known statically: a non-generic caller has
        // no frame type arguments to substitute.
        let mut types = Vec::with_capacity(type_args.len());
        let mut primitive = true;
        for arg in type_args {
            let ty = match arg {
                Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                    self.analysis.static_types.get(&local.0).map(|ty| {
                        primitive &= ty.primitive;
                        ty.code.clone()
                    })
                }
                Operand::Constant(_) | Operand::Copy(_) | Operand::Move(_) => None,
            };
            let Some(ty) = ty else {
                return unsupported(format!(
                    "call to `{name}` with a type argument known only at runtime"
                ));
            };
            types.push(ty);
        }
        let values = self.laid_out_args(block, &name, layout, args)?;
        let args = format!("&[{}]", values.join(", "));
        match info.kind {
            FunctionKind::Bytecode => {
                if !types.is_empty() {
                    return unsupported(format!("call to generic function `{name}`"));
                }
                let Some(symbol) = self
                    .symbols
                    .get(&name)
                    .filter(|_| !self.resumable.contains(&name))
                else {
                    return Err(Rejection::Invalid(format!(
                        "suspending call to `{name}` emitted inline"
                    )));
                };
                let object = self
                    .index
                    .objects
                    .get(&name)
                    .copied()
                    .ok_or_else(|| Rejection::Invalid(format!("`{name}` has no object")))?;
                self.fallible(
                    block,
                    &format!(
                        "vm.call_compiled(vm.idx_to_ptr(ObjectIndex::from_raw({object})), {symbol}, {args})"
                    ),
                )
            }
            FunctionKind::NativeUnresolved | FunctionKind::Native(_) => {
                let Some(key) = info.native_key.as_deref() else {
                    return unsupported(format!("builtin `{name}` has no native key"));
                };
                if !builtin_is_callable(key, !types.is_empty() && primitive) {
                    return unsupported(format!("builtin `{key}` may yield"));
                }
                let next = self.natives.len();
                let ordinal = *self.natives.entry(key.to_owned()).or_insert(next);
                if types.is_empty() {
                    self.fallible(
                        block,
                        &format!("vm.call_builtin(native_{ordinal}(), {args})"),
                    )
                } else {
                    self.fallible(
                        block,
                        &format!(
                            "vm.call_builtin_typed(native_{ordinal}(), &[{}], {args})",
                            types.join(", ")
                        ),
                    )
                }
            }
            FunctionKind::SysOp(_) => unsupported(format!("`{name}` is a sys op")),
        }
    }
}

/// Whether generated code `code` is a single token: a local, a literal, or a
/// path. Anything else is bound to a temporary before it is used as a call
/// argument.
fn is_atom(code: &str) -> bool {
    !code.contains(['(', ' ', '{', '[', '!', '&', '?'])
}

/// Convert `code`, held as `from`, into `to`.
fn coerce(code: &str, from: Repr, to: Repr) -> String {
    use Repr::{Bool, Float, Int, Value};
    match (from, to) {
        (a, b) if a == b => code.to_string(),
        (Int, Value) => format!("Value::int({code})"),
        (Bool, Value) => format!("Value::bool({code})"),
        (Float, Value) => format!("vm.float_value({code})"),
        // Checked: a value of the wrong kind here is a checker hole, reported
        // as the internal error the interpreter's generic paths raise, never
        // read as zero.
        (Value, Int) => format!("bex_vm::compiled_rt::expect_int({code})?"),
        (Value, Bool) => format!("bex_vm::compiled_rt::expect_bool({code})?"),
        (Value, Float) => format!("bex_vm::compiled_rt::expect_f64({code})?"),
        // Scalars of different kinds never flow into one another in checked
        // MIR; go through the tagged form so the VM's own rule decides.
        (a, b) => coerce(&coerce(code, a, Value), Value, b),
    }
}

fn float_literal(f: f64) -> String {
    if f.is_nan() {
        "f64::NAN".to_string()
    } else if f.is_infinite() {
        if f > 0.0 {
            "f64::INFINITY".to_string()
        } else {
            "f64::NEG_INFINITY".to_string()
        }
    } else {
        format!("{f:?}f64")
    }
}

fn cmp_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Eq => "Eq",
        BinOp::Ne => "NotEq",
        BinOp::Lt => "Lt",
        BinOp::Le => "LtEq",
        BinOp::Gt => "Gt",
        BinOp::Ge => "GtEq",
        _ => unreachable!("not a comparison"),
    }
}

fn binop_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "Add",
        BinOp::Sub => "Sub",
        BinOp::Mul => "Mul",
        BinOp::Div => "Div",
        BinOp::Mod => "Mod",
        BinOp::BitAnd => "BitAnd",
        BinOp::BitOr => "BitOr",
        BinOp::BitXor => "BitXor",
        BinOp::Shl => "Shl",
        BinOp::Shr => "Shr",
        _ => unreachable!("not an arithmetic operator"),
    }
}

/// A type argument known statically: the `RealizedTy` expression that builds
/// it, and whether it is a primitive (or an array of them), which can hold no
/// user-defined behavior.
#[derive(Clone, Debug)]
struct StaticTy {
    code: String,
    primitive: bool,
}

/// Whether `template` is a primitive or an array of primitives.
fn is_primitive(template: &baml_compiler2_mir::TyTemplate) -> bool {
    match template {
        TyTemplate::Int | TyTemplate::Float | TyTemplate::Bool | TyTemplate::String => true,
        TyTemplate::List(element) => is_primitive(element),
        _ => false,
    }
}

/// The `RealizedTy` expression for a type template that needs no frame type
/// arguments: primitives, non-generic classes, and arrays of them.
fn static_realized(
    db: &dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    template: &baml_compiler2_mir::TyTemplate,
) -> Option<String> {
    Some(match template {
        TyTemplate::Int => "RealizedTy::Int".to_string(),
        TyTemplate::Float => "RealizedTy::Float".to_string(),
        TyTemplate::Bool => "RealizedTy::Bool".to_string(),
        TyTemplate::String => "RealizedTy::String".to_string(),
        TyTemplate::List(element) => {
            format!(
                "RealizedTy::List(Box::new({}))",
                static_realized(db, index, element)?
            )
        }
        TyTemplate::Class(name, args) if args.is_empty() => {
            let &(object, _) = index
                .classes
                .get(&baml_compiler2_mir::decl_display_name(db, name))?;
            format!("vm.class_type(vm.idx_to_ptr(ObjectIndex::from_raw({object})))")
        }
        _ => return None,
    })
}

// ─── Module rendering ────────────────────────────────────────────────────────

fn render_module(
    bodies: &str,
    functions: &[CompiledFunction],
    natives: &BTreeMap<String, usize>,
    artifact_path: &str,
) -> String {
    let mut out = String::new();
    out.push_str(
        "//! Generated by `baml_compiler2_native`. Do not edit.\n\
         #![allow(unused_mut, unused_variables, unused_assignments, unused_labels, unused_parens, \
         unreachable_code, non_snake_case, clippy::all, clippy::pedantic)]\n\n\
         use bex_vm::{BexVm, NativeFunction, errors::VmError};\n\
         use bex_vm::compiled_rt::{CompiledEntry, CompiledFn, Resume, Step, TypeArgs};\n\
         use bex_vm_types::{BinOp, CmpOp, ObjectIndex, RealizedTy, Value};\n\n\
         type R<T> = Result<T, Box<VmError>>;\n\n",
    );
    for (key, ordinal) in natives {
        let _ = writeln!(
            out,
            "fn native_{ordinal}() -> NativeFunction {{\n    \
             static F: std::sync::OnceLock<NativeFunction> = std::sync::OnceLock::new();\n    \
             *F.get_or_init(|| bex_vm::package_baml::resolve_native({key:?}).expect(\"builtin bound at compile time\"))\n}}\n"
        );
    }
    out.push_str(bodies);
    out.push_str(
        "\n/// Compiled bodies by the object index of the bytecode function each replaces.\n\
         pub static NATIVE_BODIES: &[CompiledEntry] = &[\n",
    );
    for function in functions {
        let _ = writeln!(
            out,
            "    CompiledEntry {{ object: {}, name: {:?}, arity: {}, body: CompiledFn::{}({}), \
             slots: {}, cause_slot: {:?} }},",
            function.object,
            function.name,
            function.arity,
            if function.resumable {
                "Resumable"
            } else {
                "Inline"
            },
            function.symbol,
            function.slots,
            function.cause_slot,
        );
    }
    out.push_str("];\n\n/// The program these bodies were compiled from.\n");
    let _ = writeln!(
        out,
        "pub static PROGRAM_ARTIFACT: &[u8] = include_bytes!({artifact_path:?});"
    );
    match syn::parse_file(&out) {
        Ok(file) => prettyplease::unparse(&file),
        // Unparseable output is a backend bug; keep the raw text so the
        // failure is visible at `cargo build` of the generated crate.
        Err(_) => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbols_are_identifiers() {
        assert_eq!(symbol_for("user.Add", 3), "user_Add_3");
        assert_eq!(symbol_for("pkg.ns.Class.m", 0), "pkg_ns_Class_m_0");
    }

    #[test]
    fn float_literals_round_trip() {
        assert_eq!(float_literal(1.0), "1.0f64");
        assert_eq!(float_literal(f64::NAN), "f64::NAN");
        assert_eq!(float_literal(f64::NEG_INFINITY), "f64::NEG_INFINITY");
    }
}
