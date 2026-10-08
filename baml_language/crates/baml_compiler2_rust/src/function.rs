//! Admission and analysis of one MIR function.
//!
//! [`analyze`] decides whether a function lies in the native subset and, if
//! so, records everything the printer needs: the kind of every local, the
//! calls it makes, and its structured control flow. Anything outside the
//! subset is [`Rejection::Unsupported`]; anything a checked program's MIR
//! must never contain is [`Rejection::Invalid`].

use baml_compiler2_hir::{
    item_data::{function_data, method_owner},
    loc::{DeclRef, FunctionLoc},
};
use baml_compiler2_mir::{
    BasicBlock, BinOp, BlockId, Constant, Local, MirFunction, MirFunctionBody, MirFunctionKind,
    Operand, OptLevel, Place, RuntimeTy, Rvalue, ShortCircuitKind, StatementKind, SwitchKey,
    Terminator, TyTemplate, TypeTest, UnaryOp, function_link_name, lower_function,
};
use baml_type::{Int63, Literal};

use crate::{
    Rejection, Scalar,
    structure::{Cfg, Flow, Stmt, structurize},
};

/// The link name of the stdlib function whose call is emitted as a panic.
const PANIC_LINK_NAME: &str = "baml.sys.panic";

/// The native representation of a MIR local.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalKind {
    /// `int`, a [`bex_lang::Int63`] in the generated code.
    Int,
    /// `bool`.
    Bool,
    /// `void` / `null`: the unit type. The return place of a void function
    /// and the destination of a call to one.
    Void,
    /// `never`: only ever the destination of a `baml.sys.panic` call, which
    /// returns before anything is assigned. Not declared in the output.
    Never,
}

impl LocalKind {
    pub(crate) fn scalar(self) -> Option<Scalar> {
        match self {
            Self::Int => Some(Scalar::Int),
            Self::Bool => Some(Scalar::Bool),
            Self::Void | Self::Never => None,
        }
    }
}

/// What a `Call` terminator does in the generated code.
#[derive(Debug, Clone)]
pub(crate) enum CallKind<'db> {
    /// A direct call of another source function, admitted separately.
    Direct {
        callee: FunctionLoc<'db>,
        args: Vec<LocalKind>,
        destination: LocalKind,
    },
    /// `baml.sys.panic("...")`: returns the panic instead of calling.
    Panic(String),
}

/// A function admitted to the subset, with everything the printer reads.
pub(crate) struct Candidate<'db> {
    pub loc: FunctionLoc<'db>,
    pub link_name: String,
    pub mir: &'db MirFunction<'db>,
    pub body: &'db MirFunctionBody<'db>,
    /// Parallel to `body.locals`.
    pub kinds: Vec<LocalKind>,
    /// BAML parameter names, parallel to `_1..=arity`.
    pub param_names: Vec<String>,
    /// What each `Call` terminator does, by block.
    pub calls: Vec<(BlockId, CallKind<'db>)>,
    pub structured: Vec<Stmt>,
}

impl Candidate<'_> {
    pub(crate) fn arity(&self) -> usize {
        self.mir.arity
    }

    pub(crate) fn param_kinds(&self) -> &[LocalKind] {
        &self.kinds[1..=self.mir.arity]
    }

    pub(crate) fn return_kind(&self) -> LocalKind {
        self.kinds[0]
    }

    pub(crate) fn call(&self, block: BlockId) -> Option<&CallKind<'_>> {
        self.calls
            .iter()
            .find(|(id, _)| *id == block)
            .map(|(_, call)| call)
    }
}

/// Decide whether `loc` is in the subset and gather what emitting it needs.
pub(crate) fn analyze<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    loc: FunctionLoc<'db>,
) -> Result<Candidate<'db>, Rejection> {
    let data = function_data(db, loc);
    if !data.generic_params.is_empty() {
        return Err(Rejection::unsupported("generic function"));
    }
    if data.params.iter().any(|param| param.has_default) {
        return Err(Rejection::unsupported(
            "default parameter (the callee prologue fills omitted arguments)",
        ));
    }
    if method_owner(db, loc).is_some() {
        return Err(Rejection::unsupported("method"));
    }
    if baml_compiler2_hir_ty::infer::trace_hooks::declaration_plan(db, loc).is_some() {
        return Err(Rejection::unsupported("declared trace hook"));
    }
    let mir = lower_function(db, loc, OptLevel::One)
        .as_ref()
        .map_err(|error| Rejection::invalid(format!("MIR lowering failed: {error}")))?;
    let link_name = mir.identity.link_name(db);
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return Err(Rejection::unsupported("builtin function"));
    };
    if !mir.lambdas.is_empty() {
        return Err(Rejection::unsupported("lambda"));
    }
    if body.locals.len() <= mir.arity {
        return Err(Rejection::invalid("fewer locals than parameters"));
    }
    if body.blocks.is_empty() {
        return Err(Rejection::invalid("no basic blocks"));
    }

    for block in &body.blocks {
        if block.unwind.is_some() || block.landing.is_some() || block.handling.is_some() {
            return Err(Rejection::unsupported("catch or defer"));
        }
        if block.shielded {
            return Err(Rejection::unsupported("defer body"));
        }
    }
    let kinds = local_kinds(db, mir, body)?;
    let param_names = data
        .params
        .iter()
        .map(|param| param.name.to_string())
        .collect::<Vec<_>>();
    if param_names.len() != mir.arity {
        return Err(Rejection::invalid(
            "parameter list disagrees with MIR arity",
        ));
    }

    let mut flows = Vec::with_capacity(body.blocks.len());
    let mut calls = Vec::new();
    let checker = Checker { db, kinds: &kinds };
    for (index, block) in body.blocks.iter().enumerate() {
        if block.id.0 != index {
            return Err(Rejection::invalid(format!(
                "{} is stored at position {index}",
                block.id
            )));
        }
        for statement in &block.statements {
            checker.statement(&statement.kind)?;
        }
        let terminator = block
            .terminator
            .as_ref()
            .ok_or_else(|| Rejection::invalid(format!("{} has no terminator", block.id)))?;
        let (flow, call) = checker.terminator(terminator)?;
        flows.push(flow);
        if let Some(call) = call {
            calls.push((block.id, call));
        }
    }

    let cfg = Cfg {
        entry: body.entry.0,
        flows,
    };
    check_initialization(body, mir.arity, &cfg)?;
    let structured = structurize(&cfg).map_err(|error| Rejection::invalid(error.to_string()))?;

    Ok(Candidate {
        loc,
        link_name,
        mir,
        body,
        kinds,
        param_names,
        calls,
        structured,
    })
}

fn local_kinds(
    db: &dyn baml_compiler2_mir::Db,
    mir: &MirFunction<'_>,
    body: &MirFunctionBody<'_>,
) -> Result<Vec<LocalKind>, Rejection> {
    let mut kinds = Vec::with_capacity(body.locals.len());
    for (index, local) in body.locals.iter().enumerate() {
        if local.is_captured {
            return Err(Rejection::unsupported("captured local"));
        }
        let is_param = (1..=mir.arity).contains(&index);
        let kind = match &local.ty {
            RuntimeTy::Int => LocalKind::Int,
            RuntimeTy::Bool => LocalKind::Bool,
            RuntimeTy::Literal(Literal::Int(_), _) if !is_param => LocalKind::Int,
            RuntimeTy::Literal(Literal::Bool(_), _) if !is_param => LocalKind::Bool,
            RuntimeTy::Void | RuntimeTy::Null if !is_param => LocalKind::Void,
            RuntimeTy::Never if !is_param && index != 0 => LocalKind::Never,
            other => {
                let spelled = other
                    .map_heads(&mut |decl| baml_compiler2_hir::package::spelling(db).wire(decl));
                let what = if is_param {
                    "parameter"
                } else if index == 0 {
                    "return"
                } else {
                    "local"
                };
                return Err(Rejection::unsupported(format!(
                    "{what} of type `{spelled}`"
                )));
            }
        };
        kinds.push(kind);
    }
    Ok(kinds)
}

/// Types the statements and terminators of one function.
struct Checker<'a, 'db> {
    db: &'db dyn baml_compiler2_mir::Db,
    kinds: &'a [LocalKind],
}

impl<'db> Checker<'_, 'db> {
    fn local(&self, place: &Place) -> Result<Local, Rejection> {
        match place {
            Place::Local(local) => {
                if local.0 >= self.kinds.len() {
                    return Err(Rejection::invalid(format!("{local} is not declared")));
                }
                Ok(*local)
            }
            other => Err(Rejection::unsupported(format!("place `{other}`"))),
        }
    }

    /// The kind of a readable local: `never` has no value to read.
    fn read(&self, place: &Place) -> Result<LocalKind, Rejection> {
        let local = self.local(place)?;
        match self.kinds[local.0] {
            LocalKind::Never => Err(Rejection::unsupported("read of a `never` local")),
            kind => Ok(kind),
        }
    }

    fn operand(&self, operand: &Operand<'db>) -> Result<LocalKind, Rejection> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.read(place),
            Operand::Constant(constant) => match constant {
                Constant::Int(value) => {
                    if Int63::new(*value).is_none() {
                        return Err(Rejection::unsupported(format!(
                            "int literal {value} is outside the int range"
                        )));
                    }
                    Ok(LocalKind::Int)
                }
                Constant::Bool(_) => Ok(LocalKind::Bool),
                Constant::Null => Ok(LocalKind::Void),
                Constant::String(_) => Err(Rejection::unsupported("string constant")),
                Constant::Float(_) => Err(Rejection::unsupported("float constant")),
                Constant::Bigint(_) => Err(Rejection::unsupported("bigint constant")),
                Constant::OmittedArg => Err(Rejection::unsupported("omitted argument")),
                Constant::Function(_) | Constant::GenericFunction { .. } => {
                    Err(Rejection::unsupported("function value"))
                }
                Constant::GlobalItem(_) => Err(Rejection::unsupported("top-level let")),
                Constant::EnumVariant { .. } => Err(Rejection::unsupported("enum value")),
            },
        }
    }

    fn operand_of(&self, operand: &Operand<'db>, expected: LocalKind) -> Result<(), Rejection> {
        let actual = self.operand(operand)?;
        if actual == expected {
            Ok(())
        } else {
            Err(Rejection::invalid(format!(
                "operand has kind {actual:?} where {expected:?} is required"
            )))
        }
    }

    fn rvalue(&self, value: &Rvalue<'db>) -> Result<LocalKind, Rejection> {
        match value {
            Rvalue::Use(operand) => self.operand(operand),
            Rvalue::BinaryOp { op, left, right } => {
                let left = self.operand(left)?;
                let right = self.operand(right)?;
                match op {
                    BinOp::Add
                    | BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr => {
                        if left == LocalKind::Int && right == LocalKind::Int {
                            Ok(LocalKind::Int)
                        } else {
                            Err(Rejection::unsupported(format!(
                                "`{op}` on {left:?} and {right:?}"
                            )))
                        }
                    }
                    BinOp::Eq | BinOp::Ne => {
                        if left == right && matches!(left, LocalKind::Int | LocalKind::Bool) {
                            Ok(LocalKind::Bool)
                        } else {
                            Err(Rejection::unsupported(format!(
                                "`{op}` on {left:?} and {right:?}"
                            )))
                        }
                    }
                    BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        if left == LocalKind::Int && right == LocalKind::Int {
                            Ok(LocalKind::Bool)
                        } else {
                            Err(Rejection::unsupported(format!(
                                "`{op}` on {left:?} and {right:?}"
                            )))
                        }
                    }
                }
            }
            Rvalue::UnaryOp { op, operand } => {
                let kind = self.operand(operand)?;
                match (op, kind) {
                    (UnaryOp::Not, LocalKind::Bool) => Ok(LocalKind::Bool),
                    (UnaryOp::Neg, LocalKind::Int) => Ok(LocalKind::Int),
                    (UnaryOp::Truthy, LocalKind::Int | LocalKind::Bool) => Ok(LocalKind::Bool),
                    _ => Err(Rejection::unsupported(format!("`{op}` on {kind:?}"))),
                }
            }
            Rvalue::IsType { operand, test } => {
                let kind = self.operand(operand)?;
                match (test, kind) {
                    (
                        TypeTest::Template(TyTemplate::Literal(Literal::Int(_), _)),
                        LocalKind::Int,
                    )
                    | (
                        TypeTest::Template(TyTemplate::Literal(Literal::Bool(_), _)),
                        LocalKind::Bool,
                    ) => Ok(LocalKind::Bool),
                    _ => Err(Rejection::unsupported("type test other than a literal")),
                }
            }
            other => Err(Rejection::unsupported(format!(
                "rvalue {}",
                rvalue_name(other)
            ))),
        }
    }

    fn statement(&self, kind: &StatementKind<'db>) -> Result<(), Rejection> {
        match kind {
            StatementKind::Assign { destination, value } => {
                let local = self.local(destination)?;
                let expected = self.kinds[local.0];
                if expected == LocalKind::Never {
                    return Err(Rejection::unsupported("assignment to a `never` local"));
                }
                let actual = self.rvalue(value)?;
                if actual != expected && !is_dead_null_write(value) {
                    return Err(Rejection::invalid(format!(
                        "{local} has kind {expected:?} but is assigned a {actual:?}"
                    )));
                }
                Ok(())
            }
            StatementKind::Drop(place) => {
                self.local(place)?;
                Ok(())
            }
            StatementKind::Nop => Ok(()),
            StatementKind::FreshCell { .. } => Err(Rejection::unsupported("captured local")),
            StatementKind::Intrinsic { .. } => Err(Rejection::unsupported("compiler intrinsic")),
            StatementKind::VirtualFieldStore { .. } => {
                Err(Rejection::unsupported("interface field store"))
            }
        }
    }

    fn terminator(
        &self,
        terminator: &Terminator<'db>,
    ) -> Result<(Flow, Option<CallKind<'db>>), Rejection> {
        let flow = match terminator {
            Terminator::Goto { target } => Flow::Goto(target.0),
            Terminator::Branch {
                condition,
                then_block,
                else_block,
            } => {
                self.operand_of(condition, LocalKind::Bool)?;
                Flow::Branch {
                    then_block: then_block.0,
                    else_block: else_block.0,
                }
            }
            Terminator::Switch {
                discriminant,
                arms,
                otherwise,
                ..
            } => {
                self.operand_of(discriminant, LocalKind::Int)?;
                let mut targets = Vec::with_capacity(arms.len());
                for (key, target) in arms {
                    let SwitchKey::Int(_) = key else {
                        return Err(Rejection::unsupported("switch on a class tag"));
                    };
                    targets.push(target.0);
                }
                Flow::Switch {
                    arms: targets,
                    otherwise: otherwise.0,
                }
            }
            Terminator::Return | Terminator::Unreachable => Flow::Exit,
            Terminator::Call { target, .. } => {
                let call = self.call(terminator)?;
                return Ok((Flow::Goto(target.0), Some(call)));
            }
            Terminator::ShortCircuit {
                operand,
                kind,
                destination,
                eval_rhs,
                join,
            } => {
                if *kind == ShortCircuitKind::Coalesce {
                    return Err(Rejection::unsupported("`??`"));
                }
                self.operand_of(operand, LocalKind::Bool)?;
                let destination = self.local(destination)?;
                if self.kinds[destination.0] != LocalKind::Bool {
                    return Err(Rejection::invalid(
                        "short-circuit destination is not a bool",
                    ));
                }
                Flow::Branch {
                    then_block: eval_rhs.0,
                    else_block: join.0,
                }
            }
            Terminator::NarrowBind { .. } => {
                return Err(Rejection::unsupported("narrowing pattern"));
            }
            Terminator::VirtualCall { .. } => {
                return Err(Rejection::unsupported("interface method call"));
            }
            Terminator::SysOp { .. } => return Err(Rejection::unsupported("sys-op call")),
            Terminator::Spawn { .. } => return Err(Rejection::unsupported("spawn")),
            Terminator::Await { .. } | Terminator::AwaitAny { .. } => {
                return Err(Rejection::unsupported("await"));
            }
            Terminator::Throw { .. } | Terminator::Rethrow { .. } => {
                return Err(Rejection::unsupported("throw"));
            }
            Terminator::ThrowIfPanic { .. } => return Err(Rejection::unsupported("catch")),
        };
        Ok((flow, None))
    }

    fn call(&self, terminator: &Terminator<'db>) -> Result<CallKind<'db>, Rejection> {
        let Terminator::Call {
            has_trace,
            argument_layout,
            callee,
            args,
            ntypeargs,
            destination,
            unwind,
            ..
        } = terminator
        else {
            return Err(Rejection::invalid("not a call"));
        };
        if *has_trace {
            return Err(Rejection::unsupported("call with a trace attachment"));
        }
        if *ntypeargs != 0 {
            return Err(Rejection::unsupported("call with type arguments"));
        }
        if unwind.is_some() {
            return Err(Rejection::unsupported("call inside a catch"));
        }
        if argument_layout.as_ref().is_some_and(|layout| {
            layout.0.len() != args.len() || layout.0.iter().any(Option::is_some)
        }) {
            return Err(Rejection::unsupported(
                "call with named or omitted arguments",
            ));
        }
        let destination = self.local(destination)?;
        let Operand::Constant(Constant::Function(callee)) = callee else {
            return Err(Rejection::unsupported("indirect call"));
        };
        if function_link_name(self.db, *callee) == PANIC_LINK_NAME {
            let [Operand::Constant(Constant::String(message))] = args.as_slice() else {
                return Err(Rejection::unsupported("panic with a computed message"));
            };
            return Ok(CallKind::Panic(message.clone()));
        }
        let DeclRef::Source(callee) = callee else {
            return Err(Rejection::unsupported("call of a function without source"));
        };
        let mut kinds = Vec::with_capacity(args.len());
        for arg in args {
            let kind = self.operand(arg)?;
            if kind.scalar().is_none() {
                return Err(Rejection::unsupported(format!("{kind:?} argument")));
            }
            kinds.push(kind);
        }
        let destination = self.kinds[destination.0];
        if destination == LocalKind::Never {
            return Err(Rejection::unsupported(
                "call of a function returning `never`",
            ));
        }
        Ok(CallKind::Direct {
            callee: *callee,
            args: kinds,
            destination,
        })
    }
}

/// Whether `value` is the `null` an exit edge the checker proved dead writes
/// to a non-void local: the fall-through of `while (true)` into an `int`
/// return place. The VM would store the null; the native code treats reaching
/// the write as unreachable, which the checker guarantees it is.
pub(crate) fn is_dead_null_write(value: &Rvalue<'_>) -> bool {
    matches!(value, Rvalue::Use(Operand::Constant(Constant::Null)))
}

fn rvalue_name(value: &Rvalue<'_>) -> &'static str {
    match value {
        Rvalue::TraceHookSettings { .. } => "trace hook settings",
        Rvalue::Use(_) => "use",
        Rvalue::BinaryOp { .. } => "binary op",
        Rvalue::UnaryOp { .. } => "unary op",
        Rvalue::Array(..) => "array literal",
        Rvalue::Uint8Array(_) => "byte array literal",
        Rvalue::Map(..) => "map literal",
        Rvalue::Aggregate { .. } => "class or array construction",
        Rvalue::Discriminant(_) => "discriminant",
        Rvalue::TypeTag(_) => "type tag",
        Rvalue::Len(_) => "length",
        Rvalue::IsType { .. } => "type test",
        Rvalue::IsTypeTag { .. } => "type tag test",
        Rvalue::MakeClosure { .. } => "closure",
        Rvalue::MakeBoundMethod { .. } => "bound method",
        Rvalue::MakeVirtualBoundMethod { .. } => "virtual bound method",
        Rvalue::MakeVirtualFunction { .. } => "virtual function",
        Rvalue::VirtualFieldAccess { .. } => "interface field read",
        Rvalue::MakeGenericFunction { .. } | Rvalue::MakeGenericFunctionFromValue { .. } => {
            "generic function value"
        }
        Rvalue::LoadType(_) => "type value",
        Rvalue::CurrentPackage(_) => "current package",
    }
}

// ── Definite initialization ─────────────────────────────────────────────────

/// Reject a read of a local that some path reaches unassigned.
///
/// MIR's own verifier runs only in debug builds. The generated code gives
/// every local zero-initialized storage, which must never stand in for a
/// value BAML never assigned, so the check runs here on every build.
fn check_initialization(
    body: &MirFunctionBody<'_>,
    arity: usize,
    cfg: &Cfg,
) -> Result<(), Rejection> {
    let on_entry = initialized_on_entry(body, arity, cfg);
    for (block, state) in body.blocks.iter().zip(&on_entry) {
        let Some(state) = state else {
            continue; // unreachable block: never emitted
        };
        let mut initialized = state.clone();
        let require = |initialized: &[bool], operand: &Operand<'_>| match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))
                if !initialized[local.0] =>
            {
                Err(Rejection::invalid(format!(
                    "{local} is read in {} before it is assigned on every path",
                    block.id
                )))
            }
            _ => Ok(()),
        };
        for statement in &block.statements {
            if let StatementKind::Assign {
                destination: Place::Local(local),
                value,
            } = &statement.kind
            {
                for operand in rvalue_operands(value) {
                    require(&initialized, operand)?;
                }
                initialized[local.0] = true;
            }
        }
        if let Some(terminator) = &block.terminator {
            for operand in terminator_operands(terminator) {
                require(&initialized, operand)?;
            }
        }
    }
    Ok(())
}

fn rvalue_operands<'a, 'db>(value: &'a Rvalue<'db>) -> Vec<&'a Operand<'db>> {
    match value {
        Rvalue::Use(operand) | Rvalue::UnaryOp { operand, .. } | Rvalue::IsType { operand, .. } => {
            vec![operand]
        }
        Rvalue::BinaryOp { left, right, .. } => vec![left, right],
        _ => Vec::new(),
    }
}

fn terminator_operands<'a, 'db>(terminator: &'a Terminator<'db>) -> Vec<&'a Operand<'db>> {
    match terminator {
        Terminator::Branch { condition, .. } => vec![condition],
        Terminator::Switch { discriminant, .. } => vec![discriminant],
        Terminator::Call { args, .. } => args.iter().collect(),
        Terminator::ShortCircuit { operand, .. } => vec![operand],
        _ => Vec::new(),
    }
}

/// The locals assigned on every path into each block (`None` for a block no
/// path reaches). Parameters are assigned on entry. A call's destination is
/// assigned on its continuation edge and a short-circuit's on its join edge:
/// both paths into the join assign it, the short one on the edge itself.
fn initialized_on_entry(
    body: &MirFunctionBody<'_>,
    arity: usize,
    cfg: &Cfg,
) -> Vec<Option<Vec<bool>>> {
    let locals = body.locals.len();
    let edges: Vec<Vec<(usize, Option<Local>)>> = body
        .blocks
        .iter()
        .map(|block| edge_writes(block, cfg))
        .collect();
    let writes: Vec<Vec<Local>> = body
        .blocks
        .iter()
        .map(|block| {
            block
                .statements
                .iter()
                .filter_map(|statement| match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        ..
                    } => Some(*local),
                    _ => None,
                })
                .collect()
        })
        .collect();
    let mut entry = vec![false; locals];
    entry[1..=arity].fill(true);
    let mut states: Vec<Option<Vec<bool>>> = vec![None; body.blocks.len()];
    states[body.entry.0] = Some(entry);
    let mut work = vec![body.entry.0];
    while let Some(index) = work.pop() {
        let mut after = states[index]
            .clone()
            .expect("only reached blocks are queued");
        for local in &writes[index] {
            after[local.0] = true;
        }
        for (target, write) in &edges[index] {
            let mut edge = after.clone();
            if let Some(local) = write {
                edge[local.0] = true;
            }
            let merged = match &states[*target] {
                None => edge,
                Some(old) => old.iter().zip(&edge).map(|(a, b)| *a && *b).collect(),
            };
            if states[*target].as_ref() != Some(&merged) {
                states[*target] = Some(merged);
                work.push(*target);
            }
        }
    }
    states
}

/// Each successor of `block` with the local assigned on the way there.
fn edge_writes(block: &BasicBlock<'_>, cfg: &Cfg) -> Vec<(usize, Option<Local>)> {
    let destination = |place: &Place| match place {
        Place::Local(local) => Some(*local),
        _ => None,
    };
    match (&block.terminator, &cfg.flows[block.id.0]) {
        (Some(Terminator::Call { destination: d, .. }), Flow::Goto(target)) => {
            vec![(*target, destination(d))]
        }
        (
            Some(Terminator::ShortCircuit { destination: d, .. }),
            Flow::Branch {
                then_block,
                else_block,
            },
        ) => vec![(*then_block, None), (*else_block, destination(d))],
        (_, flow) => match flow {
            Flow::Goto(target) => vec![(*target, None)],
            Flow::Branch {
                then_block,
                else_block,
            } => vec![(*then_block, None), (*else_block, None)],
            Flow::Switch { arms, otherwise } => arms
                .iter()
                .chain([otherwise])
                .map(|target| (*target, None))
                .collect(),
            Flow::Exit => Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use baml_compiler2_mir::{LocalDecl, Statement};

    use super::*;

    fn local_decl(ty: RuntimeTy) -> LocalDecl {
        LocalDecl {
            name: None,
            ty,
            span: None,
            scope_span: None,
            is_captured: false,
        }
    }

    fn block(
        id: usize,
        statements: Vec<Statement<'static>>,
        terminator: Terminator<'static>,
    ) -> BasicBlock<'static> {
        let mut block = BasicBlock::new(BlockId(id));
        block.statements = statements;
        block.terminator = Some(terminator);
        block
    }

    fn assign(local: usize, value: Rvalue<'static>) -> Statement<'static> {
        Statement {
            kind: StatementKind::Assign {
                destination: Place::Local(Local(local)),
                value,
            },
            span: None,
        }
    }

    fn copy(local: usize) -> Operand<'static> {
        Operand::copy_local(Local(local))
    }

    fn cfg_of(body: &MirFunctionBody<'static>) -> Cfg {
        let flows = body
            .blocks
            .iter()
            .map(
                |block| match block.terminator.as_ref().expect("terminated") {
                    Terminator::Goto { target } => Flow::Goto(target.0),
                    Terminator::Branch {
                        then_block,
                        else_block,
                        ..
                    } => Flow::Branch {
                        then_block: then_block.0,
                        else_block: else_block.0,
                    },
                    Terminator::ShortCircuit { eval_rhs, join, .. } => Flow::Branch {
                        then_block: eval_rhs.0,
                        else_block: join.0,
                    },
                    Terminator::Return => Flow::Exit,
                    other => panic!("unexpected terminator {other:?}"),
                },
            )
            .collect();
        Cfg {
            entry: body.entry.0,
            flows,
        }
    }

    #[test]
    fn read_assigned_on_one_path_only_is_invalid() {
        // _0 ret, _1 param (bool), _2 int assigned only on the then path.
        let body = MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![],
                    Terminator::Branch {
                        condition: copy(1),
                        then_block: BlockId(1),
                        else_block: BlockId(2),
                    },
                ),
                block(
                    1,
                    vec![assign(2, Rvalue::Use(Operand::Constant(Constant::Int(1))))],
                    Terminator::Goto { target: BlockId(2) },
                ),
                block(
                    2,
                    vec![assign(0, Rvalue::Use(copy(2)))],
                    Terminator::Goto { target: BlockId(3) },
                ),
                block(3, vec![], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                local_decl(RuntimeTy::Int),
                local_decl(RuntimeTy::Bool),
                local_decl(RuntimeTy::Int),
            ],
        };
        let cfg = cfg_of(&body);
        let error = check_initialization(&body, 1, &cfg).unwrap_err();
        assert!(
            matches!(&error, Rejection::Invalid(reason) if reason.contains("_2 is read in bb2")),
            "{error}"
        );
    }

    #[test]
    fn short_circuit_destination_is_assigned_on_both_paths() {
        // _0 ret bool, _1 param bool, _2 destination.
        let body = MirFunctionBody {
            blocks: vec![
                block(
                    0,
                    vec![],
                    Terminator::ShortCircuit {
                        operand: copy(1),
                        kind: ShortCircuitKind::And,
                        destination: Place::Local(Local(2)),
                        eval_rhs: BlockId(1),
                        join: BlockId(2),
                    },
                ),
                block(
                    1,
                    vec![assign(2, Rvalue::Use(copy(1)))],
                    Terminator::Goto { target: BlockId(2) },
                ),
                block(
                    2,
                    vec![assign(0, Rvalue::Use(copy(2)))],
                    Terminator::Goto { target: BlockId(3) },
                ),
                block(3, vec![], Terminator::Return),
            ],
            entry: BlockId(0),
            locals: vec![
                local_decl(RuntimeTy::Bool),
                local_decl(RuntimeTy::Bool),
                local_decl(RuntimeTy::Bool),
            ],
        };
        let cfg = cfg_of(&body);
        check_initialization(&body, 1, &cfg).expect("join sees the destination assigned");
        // Without the assignment in the rhs block the join is not covered.
        let mut broken = body.clone();
        broken.blocks[1].statements.clear();
        assert!(matches!(
            check_initialization(&broken, 1, &cfg),
            Err(Rejection::Invalid(_))
        ));
    }
}
