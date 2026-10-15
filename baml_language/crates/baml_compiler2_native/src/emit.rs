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
    TypeTest, UnaryOp, class_link_name, enum_link_name, function_link_name, lower_function,
};
use baml_type::{Literal, TyTemplate};
use bex_vm_types::{FunctionKind, Object, Program};

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
    /// The Rust symbol with the VM's native signature.
    pub symbol: String,
    /// Whether the function is observed (loops, or large): it is never
    /// inlined and calls to it from compiled code run through the VM's call
    /// path so telemetry records it.
    pub observed: bool,
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

/// Functions this large, or with any loop, are observed (see
/// [`CompiledFunction::observed`]).
const OBSERVED_STATEMENTS: usize = 64;

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
            let name = mir.identity.link_name(db);
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
                    units.insert(name, Unit { mir, analysis });
                }
                Err(rejection) => rejected.push((name, rejection)),
            }
        }
    }

    // Admission is a fixpoint: a compiled function calls another compiled
    // function directly, so every callee must itself be compiled.
    loop {
        let admitted: BTreeSet<String> = units.keys().cloned().collect();
        let dropped: Vec<(String, String)> = units
            .iter()
            .filter_map(|(name, unit)| {
                unit.analysis
                    .user_callees
                    .iter()
                    .find(|callee| !admitted.contains(*callee))
                    .map(|callee| (name.clone(), callee.clone()))
            })
            .collect();
        if dropped.is_empty() {
            break;
        }
        for (name, callee) in dropped {
            units.remove(&name);
            rejected.push((
                name,
                Rejection::Unsupported(format!("calls `{callee}`, which is not compiled")),
            ));
        }
    }

    let symbols: HashMap<String, String> = units
        .keys()
        .enumerate()
        .map(|(ordinal, name)| (name.clone(), symbol_for(name, ordinal)))
        .collect();
    let observed: HashMap<String, bool> = units
        .iter()
        .map(|(name, unit)| (name.clone(), unit.analysis.observed))
        .collect();

    let mut bodies = String::new();
    let mut functions = Vec::new();
    let mut natives: BTreeMap<String, usize> = BTreeMap::new();
    for (name, unit) in &units {
        let mut generator = Generator {
            db,
            index: &index,
            symbols: &symbols,
            observed: &observed,
            natives: &mut natives,
            body: body_of(unit.mir),
            analysis: &unit.analysis,
            out: String::new(),
            pending: Vec::new(),
            temps: 0,
        };
        match generator.function(&symbols[name]) {
            Ok(code) => {
                bodies.push_str(&code);
                functions.push(CompiledFunction {
                    name: name.clone(),
                    symbol: symbols[name].clone(),
                    observed: unit.analysis.observed,
                });
            }
            Err(rejection) => rejected.push((name.clone(), rejection)),
        }
    }
    // A function rejected during generation invalidates its callers; one more
    // pass over the fixpoint keeps the module consistent. Generation is
    // deterministic, so the pass converges.
    let compiled: BTreeSet<&str> = functions.iter().map(|f| f.name.as_str()).collect();
    let mut invalid = Vec::new();
    for function in &functions {
        let unit = &units[&function.name];
        if let Some(callee) = unit
            .analysis
            .user_callees
            .iter()
            .find(|callee| !compiled.contains(callee.as_str()))
        {
            invalid.push((function.name.clone(), callee.clone()));
        }
    }
    if !invalid.is_empty() {
        for (name, callee) in &invalid {
            rejected.push((
                name.clone(),
                Rejection::Unsupported(format!("calls `{callee}`, which is not compiled")),
            ));
        }
        // Re-render only the surviving bodies.
        bodies.clear();
        functions.retain(|f| !invalid.iter().any(|(n, _)| *n == f.name));
        natives.clear();
        for function in &functions {
            let unit = &units[&function.name];
            let mut generator = Generator {
                db,
                index: &index,
                symbols: &symbols,
                observed: &observed,
                natives: &mut natives,
                body: body_of(unit.mir),
                analysis: &unit.analysis,
                out: String::new(),
                pending: Vec::new(),
                temps: 0,
            };
            if let Ok(code) = generator.function(&function.symbol) {
                bodies.push_str(&code);
            }
        }
    }

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
}

// ─── The program's objects by name ──────────────────────────────────────────

struct FunctionInfo {
    kind: FunctionKind,
    native_key: Option<String>,
    arity: usize,
    generic: bool,
}

struct ProgramIndex {
    functions: HashMap<String, FunctionInfo>,
    /// Function name → object index.
    objects: HashMap<String, usize>,
    /// Class display name → (object index, field count).
    classes: HashMap<String, (usize, usize)>,
    /// Enum display name → object index.
    enums: HashMap<String, usize>,
}

impl ProgramIndex {
    fn new(program: &Program) -> Self {
        let mut functions = HashMap::new();
        let mut objects = HashMap::new();
        let mut classes = HashMap::new();
        let mut enums = HashMap::new();
        for (index, object) in program.objects.iter().enumerate() {
            match object {
                Object::Function(function) => {
                    objects.insert(function.name.clone(), index);
                    functions.insert(
                        function.name.clone(),
                        FunctionInfo {
                            kind: function.kind,
                            native_key: function.native_key.as_deref().map(str::to_owned),
                            arity: function.arity,
                            generic: !function.generic_param_bounds.is_empty()
                                || !function.display_type_params.is_empty(),
                        },
                    );
                }
                Object::Class(class) => {
                    classes.insert(
                        class.name.display_name().to_string(),
                        (index, class.fields.len()),
                    );
                }
                Object::Enum(enm) => {
                    enums.insert(enm.name.display_name().to_string(), index);
                }
                _ => {}
            }
        }
        Self {
            functions,
            objects,
            classes,
            enums,
        }
    }
}

/// VM-owned builtins that never yield to run a callback, by `native_key`.
/// Anything not listed is rejected: a yielding builtin cannot run inside a
/// compiled body.
fn builtin_is_callable(key: &str) -> bool {
    const ARRAY: &[&str] = &[
        "push", "pop", "first", "last", "is_empty", "reverse", "slice", "concat",
    ];
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
    observed: bool,
    /// Link names of the user functions this function calls directly.
    user_callees: BTreeSet<String>,
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
        let reprs = body
            .locals
            .iter()
            .map(|local| Repr::of(&local.ty))
            .collect();

        let mut flows = Vec::with_capacity(body.blocks.len());
        let mut unwinds = Vec::with_capacity(body.blocks.len());
        let mut user_callees = BTreeSet::new();
        for block in &body.blocks {
            let Some(terminator) = &block.terminator else {
                return Err(Rejection::Invalid(format!(
                    "{} has no terminator",
                    block.id
                )));
            };
            flows.push(flow_of(terminator)?);
            unwinds.push(block.unwind.map(|b| b.0));
            if let Terminator::Call { callee, .. } = terminator
                && let Some(name) = user_callee_name(db, index, callee)
            {
                user_callees.insert(name);
            }
        }
        let cfg = Cfg {
            entry: body.entry.0,
            flows,
            unwinds,
        };
        let structured =
            structurize(&cfg).map_err(|error| Rejection::Invalid(error.to_string()))?;
        let statements: usize = body
            .blocks
            .iter()
            .map(|block| block.statements.len() + 1)
            .sum();
        let observed = has_loop(&structured.stmts) || statements > OBSERVED_STATEMENTS;
        Ok(Self {
            arity: mir.arity,
            reprs,
            structured,
            observed,
            user_callees,
        })
    }
}

fn has_loop(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Stmt::Loop { .. } => true,
        Stmt::Block { body, .. } => has_loop(body),
        Stmt::If {
            then_branch,
            else_branch,
            ..
        } => has_loop(then_branch) || has_loop(else_branch),
        Stmt::Switch {
            arms, otherwise, ..
        } => arms.iter().any(|(_, body)| has_loop(body)) || has_loop(otherwise),
        Stmt::Leaf(_) | Stmt::Break(_) | Stmt::BreakLoop(_) | Stmt::Continue(_) | Stmt::Exit(_) => {
            false
        }
    })
}

/// The edge shape of a terminator, or why it keeps the function bytecode.
fn flow_of(terminator: &Terminator<'_>) -> Result<Flow, Rejection> {
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
            target,
            has_trace,
            ntypeargs,
            argument_layout,
            ..
        } => {
            if *has_trace {
                return Err(Rejection::Unsupported(
                    "call with a $trace attachment".into(),
                ));
            }
            if *ntypeargs > 0 {
                return Err(Rejection::Unsupported("call passing type arguments".into()));
            }
            // A layout of required positional slots is the plain call; a
            // named or omitted optional needs the VM's slot reconciliation.
            if argument_layout
                .as_ref()
                .is_some_and(|layout| layout.0.iter().any(Option::is_some))
            {
                return Err(Rejection::Unsupported(
                    "call with omitted or defaulted arguments".into(),
                ));
            }
            Flow::Goto(target.0)
        }
        Terminator::ShortCircuit { eval_rhs, join, .. } => Flow::Branch {
            then_block: join.0,
            else_block: eval_rhs.0,
        },
        Terminator::VirtualCall { .. } => {
            return Err(Rejection::Unsupported("interface method call".into()));
        }
        Terminator::SysOp { .. } => return Err(Rejection::Unsupported("sys op".into())),
        Terminator::Spawn { .. } => return Err(Rejection::Unsupported("spawn".into())),
        Terminator::Await { .. } | Terminator::AwaitAny { .. } => {
            return Err(Rejection::Unsupported("await".into()));
        }
        Terminator::ThrowIfPanic { .. } => {
            return Err(Rejection::Unsupported("defer".into()));
        }
    })
}

fn user_callee_name<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    callee: &Operand<'db>,
) -> Option<String> {
    let Operand::Constant(Constant::Function(func)) = callee else {
        return None;
    };
    let name = function_link_name(db, *func);
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
    observed: &'a HashMap<String, bool>,
    /// Builtin native keys the module binds, by key → ordinal.
    natives: &'a mut BTreeMap<String, usize>,
    body: &'a MirFunctionBody<'db>,
    analysis: &'a Analysis,
    out: String,
    /// `let` bindings to emit before the statement being generated: every
    /// sub-expression that touches `vm` is hoisted so no VM call appears
    /// inside another's argument list (which borrows `vm` twice).
    pending: Vec<String>,
    temps: usize,
}

type Gen<T> = Result<T, Rejection>;

fn unsupported<T>(what: impl Into<String>) -> Gen<T> {
    Err(Rejection::Unsupported(what.into()))
}

impl<'db> Generator<'_, 'db> {
    fn repr(&self, local: Local) -> Repr {
        self.analysis.reprs[local.0]
    }

    fn function(&mut self, symbol: &str) -> Gen<String> {
        let arity = self.body.locals.len().min(self.arity());
        let mut out = String::new();
        let inline = if self.analysis.observed {
            "#[inline(never)]"
        } else {
            "#[inline]"
        };
        let _ = writeln!(
            out,
            "{inline}\nfn body_{symbol}(vm: &mut BexVm, args: &[Value]) -> R<Value> {{"
        );
        for (i, repr) in self.analysis.reprs.iter().enumerate() {
            let ty = repr.rust_type();
            if (1..=arity).contains(&i) {
                let arg = coerce(&format!("args[{}]", i - 1), Repr::Value, *repr);
                let _ = writeln!(out, "    let mut l{i}: {ty} = {arg};");
            } else {
                let _ = writeln!(out, "    let mut l{i}: {ty} = {};", repr.default_value());
            }
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
        let stmts = self.analysis.structured.stmts.clone();
        self.out.clear();
        self.stmts(&stmts, 1)?;
        out.push_str(&self.out);
        // Every path ends in an `Exit`; this is unreachable, and keeps the
        // function well-typed if a leaf falls through after structuring.
        out.push_str("    #[allow(unreachable_code)]\n    Ok(Value::NULL)\n}\n");
        let _ = writeln!(
            out,
            "pub fn {symbol}(vm: &mut BexVm, args: &[Value]) -> NativeCallResult {{\n    \
             bex_vm::compiled_rt::into_native_result(body_{symbol}(vm, args))\n}}\n"
        );
        Ok(out)
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

    /// Bind `expr` to a temporary when it calls into the VM, so it can be an
    /// argument of another VM call.
    fn hoist(&mut self, expr: Expr) -> Expr {
        if !expr.code.contains("vm.") {
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
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign { destination, value } => {
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
            Terminator::Call {
                callee,
                args,
                destination,
                ..
            } => {
                let call = self.call(id, callee, args)?;
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
            Terminator::Goto { .. }
            | Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => {}
            Terminator::VirtualCall { .. }
            | Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. }
            | Terminator::ThrowIfPanic { .. } => unreachable!("rejected by flow_of"),
        }
        Ok(())
    }

    /// How a block leaves the function.
    fn exit(&mut self, id: BlockId, depth: usize) -> Gen<()> {
        let block = &self.body.blocks[id.0];
        match block.terminator.as_ref().expect("checked in Analysis") {
            Terminator::Return => {
                let result = coerce("l0", self.repr(Local(0)), Repr::Value);
                self.line(depth, &format!("return Ok({result});"));
            }
            Terminator::Throw { value } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, "thrown", depth)?;
                    self.statement(depth, &format!("let thrown = {value};"));
                    self.out.push_str(&jump);
                } else {
                    self.statement(
                        depth,
                        &format!("return Err(bex_vm::compiled_rt::throw({value}));"),
                    );
                }
            }
            Terminator::Rethrow { value, .. } => {
                let value = self.operand(id, value)?;
                let value = self.value_arg(&value.code, value.repr);
                if block.unwind.is_some() {
                    let jump = self.unwind_jump(id, "thrown", depth)?;
                    self.statement(depth, &format!("let thrown = {value};"));
                    self.out.push_str(&jump);
                } else {
                    self.statement(
                        depth,
                        &format!("return Err(bex_vm::compiled_rt::rethrow({value}));"),
                    );
                }
            }
            Terminator::Unreachable => {
                self.line(depth, "return Err(bex_vm::compiled_rt::unreachable());");
            }
            other => {
                return Err(Rejection::Invalid(format!(
                    "exit through a non-exiting terminator {other:?}"
                )));
            }
        }
        Ok(())
    }

    /// The statements that bind the thrown `value_var` into `block`'s handler
    /// landing and jump there.
    fn unwind_jump(&mut self, block: BlockId, value_var: &str, depth: usize) -> Gen<String> {
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
        let error = self.store(
            block,
            &Place::Local(landing.error_local),
            &Expr::new(value_var, Repr::Value),
        )?;
        let context = self.store(
            block,
            &Place::Local(landing.context_local),
            &Expr::new("Value::NULL", Repr::Value),
        )?;
        let saved = std::mem::take(&mut self.out);
        self.line(depth, &error);
        self.line(depth, &context);
        let jumps = self.analysis.structured.unwind_jumps[block.0].clone();
        self.stmts(&jumps, depth)?;
        out.push_str(&self.out);
        self.out = saved;
        Ok(out)
    }

    /// Wrap a fallible expression (`Result<_, VmError>` or
    /// `Result<_, Box<VmError>>`) for `block`: route a thrown value to the
    /// block's handler when it has one, otherwise propagate with `?`.
    fn fallible(&mut self, block: BlockId, code: &str) -> Gen<String> {
        if self.body.blocks[block.0].unwind.is_none() {
            return Ok(format!("{code}?"));
        }
        let jump = self.unwind_jump(block, "thrown", 0)?;
        Ok(format!(
            "(match {code} {{ Ok(v) => v, Err(e) => {{ let thrown = bex_vm::compiled_rt::caught(e.into())?; {jump} }} }})"
        ))
    }

    // ── Operands and places ───────────────────────────────────────────────

    fn operand(&mut self, block: BlockId, operand: &Operand<'db>) -> Gen<Expr> {
        let expr = match operand {
            Operand::Copy(place) | Operand::Move(place) => self.load(block, place)?,
            Operand::Constant(constant) => Self::constant(constant)?,
        };
        Ok(self.hoist(expr))
    }

    fn constant(constant: &Constant<'db>) -> Gen<Expr> {
        Ok(match constant {
            Constant::Int(n) => Expr::new(format!("{n}i64"), Repr::Int),
            Constant::Float(f) => Expr::new(float_literal(*f), Repr::Float),
            Constant::Bool(b) => Expr::new(format!("{b}"), Repr::Bool),
            Constant::Null => Expr::new("Value::NULL", Repr::Value),
            Constant::OmittedArg => Expr::new("Value::OMITTED_ARG", Repr::Value),
            Constant::String(s) => Expr::new(format!("vm.string_value({s:?})"), Repr::Value),
            Constant::Bigint(_) => return unsupported("bigint literal"),
            Constant::Function(_) | Constant::GenericFunction { .. } => {
                return unsupported("function used as a value");
            }
            Constant::GlobalItem(_) => return unsupported("top-level let"),
            Constant::EnumVariant { .. } => return unsupported("enum variant literal"),
        })
    }

    fn load(&mut self, block: BlockId, place: &Place) -> Gen<Expr> {
        match place {
            Place::Local(local) => Ok(Expr::new(format!("l{}", local.0), self.repr(*local))),
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
                let element_ty = primitive_realized(template).ok_or_else(|| {
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
                let name = class_link_name(self.db, *class);
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
            Rvalue::CurrentPackage(_) => unsupported("current package"),
            Rvalue::TraceHookSettings { .. } => unsupported("trace hook"),
        }
    }

    fn type_test(&mut self, value: &str, test: &TypeTest<'db>) -> Gen<String> {
        match test {
            TypeTest::Class { class, args } => {
                if !args.is_empty() {
                    return unsupported("type test against a generic class");
                }
                let name = class_link_name(self.db, *class);
                let Some(&(object, _)) = self.index.classes.get(&name) else {
                    return unsupported(format!("class `{name}` not found in the program"));
                };
                Ok(format!(
                    "vm.is_instance_of({value}, vm.idx_to_ptr(ObjectIndex::from_raw({object})))"
                ))
            }
            TypeTest::Enum(enum_ref) => {
                let name = enum_link_name(self.db, *enum_ref);
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
    ) -> Gen<String> {
        let Operand::Constant(Constant::Function(func)) = callee else {
            return unsupported("call through a function value");
        };
        let name = function_link_name(self.db, *func);
        let Some(info) = self.index.functions.get(&name) else {
            return unsupported(format!("callee `{name}` not found in the program"));
        };
        if info.arity != args.len() {
            return Err(Rejection::Invalid(format!(
                "`{name}` called with {} of {} arguments",
                args.len(),
                info.arity
            )));
        }
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            let arg = self.operand(block, arg)?;
            values.push(self.value_arg(&arg.code, arg.repr));
        }
        let args = format!("&[{}]", values.join(", "));
        match info.kind {
            FunctionKind::Bytecode => {
                let Some(symbol) = self.symbols.get(&name) else {
                    return unsupported(format!("calls `{name}`, which is not compiled"));
                };
                if self.observed.get(&name).copied().unwrap_or(false) {
                    // Through the VM's own call path, so the call is observed.
                    let object = self
                        .index
                        .objects
                        .get(&name)
                        .copied()
                        .ok_or_else(|| Rejection::Invalid(format!("`{name}` has no object")))?;
                    let callee = self.hoist(Expr::new(
                        format!("Value::object(vm.idx_to_ptr(ObjectIndex::from_raw({object})))"),
                        Repr::Value,
                    ));
                    self.fallible(
                        block,
                        &format!(
                            "vm.call_native_body({}.as_object_ptr().expect(\"function object\"), {args})",
                            callee.code
                        ),
                    )
                } else {
                    self.fallible(block, &format!("body_{symbol}(vm, {args})"))
                }
            }
            FunctionKind::NativeUnresolved | FunctionKind::Native(_) => {
                let Some(key) = info.native_key.as_deref() else {
                    return unsupported(format!("builtin `{name}` has no native key"));
                };
                if !builtin_is_callable(key) {
                    return unsupported(format!("builtin `{key}` may yield"));
                }
                let next = self.natives.len();
                let ordinal = *self.natives.entry(key.to_owned()).or_insert(next);
                self.fallible(
                    block,
                    &format!("vm.call_builtin(native_{ordinal}(), {args})"),
                )
            }
            FunctionKind::SysOp(_) => unsupported(format!("`{name}` is a sys op")),
        }
    }
}

/// Convert `code`, held as `from`, into `to`.
fn coerce(code: &str, from: Repr, to: Repr) -> String {
    use Repr::{Bool, Float, Int, Value};
    match (from, to) {
        (a, b) if a == b => code.to_string(),
        (Int, Value) => format!("Value::int({code})"),
        (Bool, Value) => format!("Value::bool({code})"),
        (Float, Value) => format!("vm.float_value({code})"),
        (Value, Int) => format!("BexVm::int_of({code})"),
        (Value, Bool) => format!("BexVm::bool_of({code})"),
        (Value, Float) => format!("BexVm::f64_of({code})"),
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

/// The `RealizedTy` constructor for a primitive element type template.
fn primitive_realized(template: &baml_compiler2_mir::TyTemplate) -> Option<&'static str> {
    match template {
        TyTemplate::Int => Some("RealizedTy::Int"),
        TyTemplate::Float => Some("RealizedTy::Float"),
        TyTemplate::Bool => Some("RealizedTy::Bool"),
        TyTemplate::String => Some("RealizedTy::String"),
        _ => None,
    }
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
         use bex_vm::{BexVm, NativeFunction, errors::VmError, package_baml::NativeCallResult};\n\
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
    out.push_str("\n/// Compiled bodies by `Function::name`.\npub static NATIVE_BODIES: &[(&str, NativeFunction)] = &[\n");
    for function in functions {
        let _ = writeln!(out, "    ({:?}, {}),", function.name, function.symbol);
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
