//! Per-function analysis: how each local is held, the edge shape of each
//! block, and what the function calls or suspends on.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::OnceLock,
};

use baml_compiler2_mir::{
    CellId, Constant, IntrinsicOp, Local, MirFunctionBody, Operand, Place, RuntimeTy, Rvalue,
    StatementKind, Terminator, UnaryOp,
    memory::{Resources, place_reads, walk_operand_places, walk_rvalue_places},
};
use baml_type::{Literal, TyTemplate};
use bex_vm_types::FunctionKind;

use crate::{
    admit::Rejection,
    codegen::types::realized_expr,
    index::ProgramIndex,
    structure::{Cfg, Flow},
};

/// How a local is held in generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Repr {
    Int,
    Float,
    Bool,
    Value,
}

impl Repr {
    pub(crate) fn of(ty: &baml_compiler2_mir::RuntimeTy) -> Self {
        use baml_compiler2_mir::RuntimeTy as T;
        match ty {
            T::Int | T::Literal(Literal::Int(_), _) => Self::Int,
            T::Float | T::Literal(Literal::Float(_), _) => Self::Float,
            T::Bool | T::Literal(Literal::Bool(_), _) => Self::Bool,
            _ => Self::Value,
        }
    }

    pub(crate) fn rust_type(self) -> &'static str {
        match self {
            Self::Int => "i64",
            Self::Float => "f64",
            Self::Bool => "bool",
            Self::Value => "Value",
        }
    }

    pub(crate) fn default_value(self) -> &'static str {
        match self {
            Self::Int => "0i64",
            Self::Float => "0.0f64",
            Self::Bool => "false",
            Self::Value => "Value::NULL",
        }
    }
}

/// The typed signature of a body's direct entry: its parameters and result
/// held as the body holds them, so a compiled caller passes and receives
/// scalars unboxed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Signature {
    pub(crate) params: Vec<Repr>,
    pub(crate) ret: Repr,
}

pub(crate) struct Analysis {
    pub(crate) arity: usize,
    /// Captured slots hold cells; other slots hold their values.
    pub(crate) reprs: Vec<Repr>,
    /// Value representations, including the contents of captured cells.
    pub(crate) values: Vec<Repr>,
    /// Representations of the values behind this closure's captures.
    pub(crate) captures: Vec<Repr>,
    pub(crate) cfg: Cfg,
    pub(crate) user_callees: BTreeSet<usize>,
    /// Type locals that need no frame type arguments.
    pub(crate) static_types: HashMap<usize, StaticTy>,
    /// Type locals built from the frame where they are used. In a body that
    /// binds frame types, only those whose every use follows their load in
    /// its block with no binding in between.
    pub(crate) deferred_types: HashMap<usize, baml_compiler2_mir::TyTemplate>,
    /// Direct suspension; admission propagates it through user calls.
    pub(crate) suspends: bool,
    /// Iterator locals for iteration compiled as an index walk.
    pub(crate) array_iters: BTreeSet<usize>,
    /// Loop headers, which poll for a yield at their back edge.
    pub(crate) loop_headers: BTreeSet<usize>,
    /// The body runs a declaration trace hook prologue.
    pub(crate) hooked: bool,
    /// The body binds type values into its frame's type arguments.
    pub(crate) binds_types: bool,
    /// Locals holding a declaration hook's result (`ApplyTraceHook` operands).
    pub(crate) hook_results: BTreeSet<usize>,
}

impl Analysis {
    /// The body's [`Signature`]. A captured parameter is boxed into its cell
    /// from a `Value`, and a type parameter has no value binding.
    pub(crate) fn signature(&self, body: &MirFunctionBody<'_>) -> Signature {
        let params = (1..=self.arity)
            .map(|i| {
                if i >= body.locals.len() || self.is_type_local(i) || body.locals[i].is_captured {
                    Repr::Value
                } else {
                    self.reprs[i]
                }
            })
            .collect();
        let ret = if self.is_type_local(0) {
            Repr::Value
        } else {
            self.reprs[0]
        };
        Signature { params, ret }
    }

    pub(crate) fn new<'db>(
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        arity: usize,
        body: &MirFunctionBody<'db>,
        captures: Vec<Repr>,
    ) -> Result<Self, Rejection> {
        if arity + 1 > body.locals.len() {
            return Err(Rejection::Invalid("fewer locals than parameters".into()));
        }
        if body.blocks.len() > 512 {
            return Err(Rejection::Unsupported("more than 512 MIR blocks".into()));
        }
        let values = value_reprs(body);
        let reprs = slot_reprs(body, &values);
        let intrinsics = Intrinsics::of(body);
        let (static_types, deferred_types) = type_locals(index, body, intrinsics.binds_types);
        let array_iters = array_iterators(body, arity);
        let BodyFlow {
            cfg,
            user_callees,
            suspends,
        } = body_flow(
            db,
            index,
            body,
            arity,
            &static_types,
            &array_iters,
            &intrinsics.hook_results,
        )?;
        let loop_headers = cfg.loop_headers();
        if loop_headers.len() > 128 {
            return Err(Rejection::Unsupported("more than 128 MIR loops".into()));
        }
        Ok(Self {
            arity,
            reprs,
            values,
            captures,
            cfg,
            user_callees,
            static_types,
            deferred_types,
            suspends,
            array_iters,
            loop_headers,
            hooked: intrinsics.hooked,
            binds_types: intrinsics.binds_types,
            hook_results: intrinsics.hook_results,
        })
    }

    /// Whether the body may suspend on its own, before its callees are known.
    pub(crate) fn pauses(&self) -> bool {
        self.suspends || !self.loop_headers.is_empty()
    }

    /// Type locals have no runtime `Value` binding.
    pub(crate) fn is_type_local(&self, i: usize) -> bool {
        self.static_types.contains_key(&i) || self.deferred_types.contains_key(&i)
    }
}

/// What the body's intrinsics ask of its frame, from one scan.
struct Intrinsics {
    binds_types: bool,
    hooked: bool,
    hook_results: BTreeSet<usize>,
}

impl Intrinsics {
    fn of(body: &MirFunctionBody<'_>) -> Self {
        let mut out = Self {
            binds_types: false,
            hooked: false,
            hook_results: BTreeSet::new(),
        };
        for statement in body.blocks.iter().flat_map(|block| &block.statements) {
            let StatementKind::Intrinsic { op, args } = &statement.kind else {
                continue;
            };
            match op {
                IntrinsicOp::BindType(_) => out.binds_types = true,
                IntrinsicOp::BuiltinTraceHook(_) => out.hooked = true,
                IntrinsicOp::ApplyTraceHook => {
                    out.hooked = true;
                    out.hook_results
                        .extend(args.iter().filter_map(|arg| match arg {
                            Operand::Copy(Place::Local(local))
                            | Operand::Move(Place::Local(local)) => Some(local.0),
                            _ => None,
                        }));
                }
                IntrinsicOp::Log(_) => {}
            }
        }
        out
    }
}

fn slot_reprs(body: &MirFunctionBody<'_>, values: &[Repr]) -> Vec<Repr> {
    values
        .iter()
        .zip(&body.locals)
        .map(|(&value, local)| {
            if local.is_captured {
                Repr::Value
            } else {
                value
            }
        })
        .collect()
}

/// The static and deferred type locals of `body`.
fn type_locals(
    index: &ProgramIndex,
    body: &MirFunctionBody<'_>,
    binds_types: bool,
) -> (
    HashMap<usize, StaticTy>,
    HashMap<usize, baml_compiler2_mir::TyTemplate>,
) {
    let mut static_types = HashMap::new();
    let mut deferred_types = HashMap::new();
    // Where each frame-derived type is loaded, as (block, statement).
    let mut loads: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
    // Reused type temporaries need ordinary value bindings.
    let mut written = HashSet::new();
    for (b, block) in body.blocks.iter().enumerate() {
        for (s, statement) in block.statements.iter().enumerate() {
            let StatementKind::Assign {
                destination: Place::Local(local),
                value,
            } = &statement.kind
            else {
                continue;
            };
            let Rvalue::LoadType(template) = value else {
                written.insert(local.0);
                continue;
            };
            if let Some(code) = static_realized(index, template) {
                let primitive = is_primitive(template);
                static_types.insert(local.0, StaticTy { code, primitive });
            } else {
                deferred_types.insert(local.0, template.clone());
                loads.entry(local.0).or_default().push((b, s));
            }
        }
        if let Some(Place::Local(local)) = block.terminator.as_ref().and_then(destination) {
            written.insert(local.0);
        }
    }
    static_types.retain(|local, _| !written.contains(local));
    deferred_types.retain(|local, _| !written.contains(local));
    // Frame types can change between a LoadType and its use. A type the
    // body binds is read from the frame at the use instead, which is the
    // load's own reading only while no binding comes between the two.
    if binds_types {
        deferred_types.retain(|&local, _| {
            matches!(loads[&local].as_slice(), [(block, statement)]
                if read_only_after_load(body, local, *block, *statement))
        });
    }
    (static_types, deferred_types)
}

/// The place a terminator writes, if any.
fn destination<'a>(terminator: &'a Terminator<'_>) -> Option<&'a Place> {
    if let Terminator::ShortCircuit { destination, .. } = terminator {
        Some(destination)
    } else {
        continuation(terminator).map(|(place, _)| place)
    }
}

pub(crate) fn continuation<'a>(
    terminator: &'a Terminator<'_>,
) -> Option<(&'a Place, baml_compiler2_mir::BlockId)> {
    match terminator {
        Terminator::Call {
            destination,
            target,
            ..
        }
        | Terminator::VirtualCall {
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
        } => Some((destination, *target)),
        Terminator::Spawn { future, resume, .. } => Some((future, *resume)),
        _ => None,
    }
}

/// Every place a statement reads.
fn walk_statement_places<'a>(statement: &'a StatementKind<'_>, f: &mut impl FnMut(&'a Place)) {
    match statement {
        StatementKind::Assign { value, .. } => walk_rvalue_places(value, f),
        StatementKind::Intrinsic { args, .. } => {
            for arg in args {
                walk_operand_places(arg, f);
            }
        }
        StatementKind::VirtualFieldStore {
            receiver, value, ..
        } => {
            walk_operand_places(receiver, f);
            walk_operand_places(value, f);
        }
        StatementKind::Drop(_) | StatementKind::FreshCell { .. } | StatementKind::Nop => {}
    }
}

/// Whether `place` names `local`, as its root, a cell's local, or an index.
fn mentions(place: &Place, local: usize) -> bool {
    let mut reads = Resources::default();
    place_reads(place, &mut reads);
    reads.locals.contains(&Local(local))
}

fn statement_reads(statement: &StatementKind<'_>, local: usize) -> bool {
    let mut reads = false;
    walk_statement_places(statement, &mut |place| reads |= mentions(place, local));
    reads
}

/// Whether `terminator` reads or writes `local`.
fn terminator_mentions(terminator: &Terminator<'_>, local: usize) -> bool {
    let mut found = destination(terminator).is_some_and(|place| mentions(place, local))
        || matches!(terminator, Terminator::NarrowBind { destination, .. } if destination.0 == local);
    walk_terminator_places(terminator, &mut |place| found |= mentions(place, local));
    found
}

/// Whether every read of `local` follows its load at (`block`, `statement`)
/// in that block, with no type binding between the load and the read.
fn read_only_after_load(
    body: &MirFunctionBody<'_>,
    local: usize,
    block: usize,
    statement: usize,
) -> bool {
    for (b, candidate) in body.blocks.iter().enumerate() {
        let mut bound = false;
        for (s, kind) in candidate.statements.iter().enumerate() {
            let after_load = b == block && s > statement;
            match &kind.kind {
                StatementKind::Intrinsic {
                    op: IntrinsicOp::BindType(_),
                    ..
                } if after_load => bound = true,
                StatementKind::FreshCell { local: cell, .. } if cell.0 == local => return false,
                _ => {}
            }
            if statement_reads(&kind.kind, local) && (!after_load || bound) {
                return false;
            }
        }
        if candidate
            .terminator
            .as_ref()
            .is_some_and(|terminator| terminator_mentions(terminator, local))
            && (b != block || bound)
        {
            return false;
        }
    }
    true
}

/// Array iterator locals a loop consumes in place. The index walk stands in
/// for the `ArrayIterator` only while the
/// iterator local is nothing but the loop's `next` receiver: one that
/// escapes (passed on, adapted, copied, captured, stored, reassigned) is the
/// real object the dispatch builds, and its `iter` and `next` calls go
/// through the VM like any other virtual call.
fn array_iterators(body: &MirFunctionBody<'_>, arity: usize) -> BTreeSet<usize> {
    let mut array_iters = BTreeSet::new();
    for block in &body.blocks {
        if let Some(terminator) = &block.terminator
            && let Some((iter, _)) = array_iter_call(body, arity, terminator)
        {
            array_iters.insert(iter);
        }
    }
    array_iters.retain(|&iter| !body.locals[iter].is_captured && !iter_escapes(body, arity, iter));
    array_iters
}

/// Whether `iter` is mentioned anywhere but as the destination of its own
/// `xs.iter()` call or the receiver of a `next` call.
fn iter_escapes(body: &MirFunctionBody<'_>, arity: usize, iter: usize) -> bool {
    body.blocks.iter().any(|block| {
        block.statements.iter().any(|statement| {
            statement_reads(&statement.kind, iter)
                || match &statement.kind {
                    StatementKind::Assign { destination, .. } => mentions(destination, iter),
                    StatementKind::FreshCell { local, .. } => local.0 == iter,
                    _ => false,
                }
        }) || block.terminator.as_ref().is_some_and(|terminator| {
            array_iter_call(body, arity, terminator).is_none_or(|(dest, _)| dest != iter)
                && array_next_shape(terminator) != Some(iter)
                && terminator_mentions(terminator, iter)
        })
    })
}

struct BodyFlow {
    cfg: Cfg,
    user_callees: BTreeSet<usize>,
    suspends: bool,
}

fn body_flow<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    body: &MirFunctionBody<'db>,
    arity: usize,
    static_types: &HashMap<usize, StaticTy>,
    array_iters: &BTreeSet<usize>,
    hook_results: &BTreeSet<usize>,
) -> Result<BodyFlow, Rejection> {
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
        let array_iteration = array_iter_walk(body, arity, terminator, array_iters).is_some()
            || array_next_call(terminator, array_iters).is_some();
        flows.push(flow_of(terminator));
        unwinds.push(block.unwind.map(|b| b.0));
        if let Terminator::Call { callee, .. } = terminator
            && let Some(object) = user_callee(db, index, callee)
        {
            user_callees.insert(object);
        }
        // Virtual calls need VM dispatch except for array iteration.
        suspends |= suspends_directly(db, index, body, terminator, static_types, hook_results)
            || (matches!(terminator, Terminator::VirtualCall { .. }) && !array_iteration);
    }
    let cfg = Cfg {
        entry: body.entry.0,
        flows,
        unwinds,
    };
    Ok(BodyFlow {
        cfg,
        user_callees,
        suspends,
    })
}

/// Defaulted parameters must preserve `OmittedArg`; in-place bool coercions
/// must preserve the uncoerced value. Both require `Value` slots.
pub(crate) fn value_reprs(body: &MirFunctionBody<'_>) -> Vec<Repr> {
    let mut values: Vec<Repr> = body
        .locals
        .iter()
        .map(|local| Repr::of(&local.ty))
        .collect();
    for block in &body.blocks {
        for statement in &block.statements {
            if let StatementKind::Assign {
                destination: Place::Local(destination),
                value:
                    Rvalue::UnaryOp {
                        op: UnaryOp::Truthy,
                        operand:
                            Operand::Copy(Place::Local(source)) | Operand::Move(Place::Local(source)),
                    },
            } = &statement.kind
                && destination == source
                && values[destination.0] == Repr::Bool
            {
                values[destination.0] = Repr::Value;
            }
            let StatementKind::Assign {
                value: Rvalue::BinaryOp { left, right, .. },
                ..
            } = &statement.kind
            else {
                continue;
            };
            for (this, other) in [(left, right), (right, left)] {
                if let (
                    Operand::Copy(place) | Operand::Move(place),
                    Operand::Constant(Constant::OmittedArg),
                ) = (this, other)
                    && let Place::Local(local) | Place::Deref(CellId::Local(local)) = place
                {
                    values[local.0] = Repr::Value;
                }
            }
        }
    }
    values
}

/// Captures inherit their value representation from a parent local or capture.
pub(crate) fn capture_reprs(
    parent: &MirFunctionBody<'_>,
    lambda_idx: usize,
    values: &[Repr],
    captures: &[Repr],
) -> Vec<Repr> {
    let sites = parent.blocks.iter().flat_map(|block| &block.statements);
    for statement in sites {
        let StatementKind::Assign {
            value:
                Rvalue::MakeClosure {
                    lambda_idx: made,
                    captures: cells,
                    ..
                },
            ..
        } = &statement.kind
        else {
            continue;
        };
        if *made != lambda_idx {
            continue;
        }
        return cells
            .iter()
            .map(|cell| match cell {
                Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                    values.get(local.0).copied().unwrap_or(Repr::Value)
                }
                Operand::Copy(Place::Capture(index)) | Operand::Move(Place::Capture(index)) => {
                    captures.get(*index).copied().unwrap_or(Repr::Value)
                }
                _ => Repr::Value,
            })
            .collect();
    }
    Vec::new()
}

/// `xs.iter()` on a local of array type, as `(iterator local, array local)`,
/// in a function of `arity` parameters. An iterator bound to the return
/// place or a parameter outlives the body (`implements Iterable` returns
/// one), so it must be the real `ArrayIterator` the dispatch builds.
pub(crate) fn array_iter_call(
    body: &MirFunctionBody<'_>,
    arity: usize,
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
        && iter.0 > arity
        && matches!(
            body.locals[array.0].ty,
            baml_compiler2_mir::RuntimeTy::List(_)
        ))
    .then_some((iter.0, array.0))
}

/// An `xs.iter()` call compiled as an index walk: [`array_iter_call`] on an
/// iterator in `array_iters`.
pub(crate) fn array_iter_walk(
    body: &MirFunctionBody<'_>,
    arity: usize,
    terminator: &Terminator<'_>,
    array_iters: &BTreeSet<usize>,
) -> Option<(usize, usize)> {
    array_iter_call(body, arity, terminator).filter(|(iter, _)| array_iters.contains(iter))
}

/// `it.next()` on a local, as that local.
fn array_next_shape(terminator: &Terminator<'_>) -> Option<usize> {
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
    (method == "next").then_some(iter.0)
}

/// `it.next()` on an array iterator compiled as an index walk, as the
/// iterator local.
pub(crate) fn array_next_call(
    terminator: &Terminator<'_>,
    array_iters: &BTreeSet<usize>,
) -> Option<usize> {
    array_next_shape(terminator).filter(|iter| array_iters.contains(iter))
}

fn flow_of(terminator: &Terminator<'_>) -> Flow {
    match terminator {
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
        } => Flow::Switch {
            arms: arms.iter().map(|(_, target)| target.0).collect(),
            otherwise: otherwise.0,
        },
        Terminator::Return | Terminator::Unreachable => Flow::Exit,
        Terminator::Throw { .. } | Terminator::Rethrow { .. } => Flow::Exit,
        Terminator::Call { target, .. } => Flow::Goto(target.0),
        Terminator::ShortCircuit { eval_rhs, join, .. } => Flow::Branch {
            then_block: join.0,
            else_block: eval_rhs.0,
        },
        Terminator::VirtualCall { target, .. } => Flow::Goto(target.0),
        Terminator::SysOp { target, .. }
        | Terminator::Await { target, .. }
        | Terminator::AwaitAny { target, .. } => Flow::Goto(target.0),
        Terminator::Spawn { resume, .. } => Flow::Goto(resume.0),
    }
}

/// Direct suspension excludes user callees, handled by the admission fixpoint.
pub(crate) fn suspends_directly<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    body: &MirFunctionBody<'db>,
    terminator: &Terminator<'db>,
    static_types: &HashMap<usize, StaticTy>,
    hook_results: &BTreeSet<usize>,
) -> bool {
    match terminator {
        Terminator::SysOp { .. }
        | Terminator::Spawn { .. }
        | Terminator::Await { .. }
        | Terminator::AwaitAny { .. } => true,
        // Trace attachments require the VM's call path.
        Terminator::Call {
            has_trace: true, ..
        } => true,
        // The VM's call path runs a declaration hook: it suppresses hooks
        // under it and attributes it to the target's caller.
        Terminator::Call {
            destination: Place::Local(result),
            ..
        } if hook_results.contains(&result.0) => true,
        Terminator::Call {
            callee,
            args,
            ntypeargs,
            ..
        } => {
            let Operand::Constant(Constant::Function(func)) = callee else {
                return true;
            };
            // Generation rejects missing callees.
            let Some((_, info)) = index.function(db, *func) else {
                return false;
            };
            match info.kind {
                // Pass-through tracing needs VM-managed frame spans.
                FunctionKind::Bytecode => info.trace_passthrough,
                FunctionKind::SysOp(_) => true,
                FunctionKind::Native(_) | FunctionKind::NativeUnresolved => {
                    let shape = call_shape(body, args, *ntypeargs, static_types);
                    !info
                        .native_key
                        .as_deref()
                        .is_some_and(|key| builtin_is_callable(key, shape))
                }
            }
        }
        _ => false,
    }
}

pub(crate) fn user_callee<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    callee: &Operand<'db>,
) -> Option<usize> {
    let Operand::Constant(Constant::Function(func)) = callee else {
        return None;
    };
    let (object, info) = index.function(db, *func)?;
    matches!(info.kind, FunctionKind::Bytecode).then_some(object)
}

/// Static constraints that can rule out builtin callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CallShape {
    /// Nonempty type arguments containing only primitives or arrays of them.
    pub(crate) type_args_primitive: bool,
    /// Map keys hash and compare without user code.
    pub(crate) map_key_primitive: bool,
    /// Some value argument holds nothing a user `Equals.eq` could compare.
    pub(crate) equality_primitive: bool,
}

pub(crate) fn call_shape(
    body: &MirFunctionBody<'_>,
    args: &[Operand<'_>],
    ntypeargs: usize,
    static_types: &HashMap<usize, StaticTy>,
) -> CallShape {
    let ntypeargs = ntypeargs.min(args.len());
    let local_of = |arg: &Operand<'_>| match arg {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => Some(local.0),
        _ => None,
    };
    let type_args_primitive = ntypeargs > 0
        && args[..ntypeargs].iter().all(|arg| {
            local_of(arg)
                .is_some_and(|local| static_types.get(&local).is_some_and(|ty| ty.primitive))
        });
    let map_key_primitive =
        args.get(ntypeargs)
            .and_then(local_of)
            .is_some_and(|receiver| match body.locals[receiver].ty.strip_null() {
                RuntimeTy::Map { key, .. } => is_primitive_key(&key),
                _ => false,
            });
    let equality_primitive = args[ntypeargs..]
        .iter()
        .any(|arg| operand_ty(body, arg).is_some_and(|ty| equality_is_primitive(&ty)));
    CallShape {
        type_args_primitive,
        map_key_primitive,
        equality_primitive,
    }
}

/// The static type of a local or scalar constant operand.
pub(crate) fn operand_ty(body: &MirFunctionBody<'_>, operand: &Operand<'_>) -> Option<RuntimeTy> {
    match operand {
        Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
            Some(body.locals[local.0].ty.clone())
        }
        Operand::Constant(Constant::Int(_)) => Some(RuntimeTy::Int),
        Operand::Constant(Constant::Bigint(_)) => Some(RuntimeTy::Bigint),
        Operand::Constant(Constant::Float(_)) => Some(RuntimeTy::Float),
        Operand::Constant(Constant::String(_)) => Some(RuntimeTy::String),
        Operand::Constant(Constant::Bool(_)) => Some(RuntimeTy::Bool),
        Operand::Constant(Constant::Null) => Some(RuntimeTy::Null),
        _ => None,
    }
}

/// Whether `==` against a value of `ty` can never reach a user `Equals.eq`.
///
/// The `==` driver calls user code only for a pair of distinct instances of
/// one class, or variants of one enum, with an explicit `eq`. It compares
/// pairs drawn one from each operand at the same position (array elements,
/// map keys and values, class fields), so an operand whose type admits no
/// class or enum value rules out every such pair.
pub(crate) fn equality_is_primitive(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Int
        | RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::String
        | RuntimeTy::Bool
        | RuntimeTy::Null
        | RuntimeTy::Uint8Array
        | RuntimeTy::Literal(..)
        | RuntimeTy::Never => true,
        RuntimeTy::List(element) => equality_is_primitive(element),
        RuntimeTy::Map { key, value } => equality_is_primitive(key) && equality_is_primitive(value),
        RuntimeTy::Union(members) => members.iter().all(equality_is_primitive),
        _ => false,
    }
}

/// Non-string keys, including ints, may invoke BAML hash and equality code.
fn is_primitive_key(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::String | RuntimeTy::Literal(Literal::String(_), _) => true,
        RuntimeTy::Union(members) => members.iter().all(is_primitive_key),
        _ => false,
    }
}

/// Keep aligned with `bex_vm::package_baml::VM_NATIVE_PACKAGES`.
const VM_PACKAGES: &[&str] = &["baml", "testing", "ai", "reflect", "trace"];

fn builtin_metadata() -> Result<&'static HashMap<String, bool>, &'static str> {
    static TABLE: OnceLock<Result<HashMap<String, bool>, String>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            let mut table = HashMap::new();
            for package in VM_PACKAGES {
                let (builtins, _, _) = baml_builtins2_codegen::extract_native_builtins_for(package)
                    .map_err(|error| {
                        format!("the stdlib's `{package}` builtins do not extract: {error}")
                    })?;
                for builtin in builtins {
                    table.insert(builtin.path, builtin.may_yield);
                }
            }
            Ok(table)
        })
        .as_ref()
        .map_err(String::as_str)
}

pub(crate) fn validate_builtin_metadata() -> Result<(), String> {
    builtin_metadata().map(|_| ()).map_err(str::to_owned)
}

fn builtin_may_yield(key: &str) -> Option<bool> {
    builtin_metadata().ok()?.get(key).copied()
}

// Primitive values cannot override `to_string`.
const PRIMITIVE_TYPE_BUILTINS: &[&str] = &["baml._to_string_default"];

/// `==`, which yields only to a user `Equals.eq`.
pub(crate) const EQUALS_EQUALS: &str = "baml.ops.equals_equals";

// These map methods yield only to hash or compare keys.
const MAP_KEY_METHODS: &[&str] = &["has", "set", "get", "index", "delete", "get_or_insert"];

/// Yielding builtins need static types that exclude their callbacks.
pub(crate) fn builtin_is_callable(key: &str, shape: CallShape) -> bool {
    match builtin_may_yield(key) {
        None => false,
        Some(false) => true,
        Some(true) if PRIMITIVE_TYPE_BUILTINS.contains(&key) => shape.type_args_primitive,
        Some(true) if key == EQUALS_EQUALS => shape.equality_primitive,
        Some(true) => key
            .strip_prefix("baml.Map.")
            .is_some_and(|method| MAP_KEY_METHODS.contains(&method) && shape.map_key_primitive),
    }
}

/// A static type expression and whether its values exclude user behavior.
#[derive(Clone, Debug)]
pub(crate) struct StaticTy {
    pub(crate) code: String,
    pub(crate) primitive: bool,
}

/// Primitives and arrays of primitives cannot carry user callbacks.
fn is_primitive(template: &baml_compiler2_mir::TyTemplate) -> bool {
    match template {
        TyTemplate::Int
        | TyTemplate::Float
        | TyTemplate::Bool
        | TyTemplate::String
        | TyTemplate::Null
        | TyTemplate::Never => true,
        TyTemplate::List(element) => is_primitive(element),
        _ => false,
    }
}

pub(crate) fn static_realized(
    index: &ProgramIndex,
    template: &baml_compiler2_mir::TyTemplate,
) -> Option<String> {
    realized_expr(index, template, false)
}

pub(crate) fn reads_frame(template: &baml_compiler2_mir::TyTemplate) -> bool {
    let mut reads = false;
    template.for_each_type_arg_ref(&mut |_| reads = true);
    reads
}

/// Walk every place a terminator reads, calling `f` for each: its operands
/// and the future an `Await` reads. The places it writes are not walked.
fn walk_terminator_places<'a>(terminator: &'a Terminator<'_>, f: &mut impl FnMut(&'a Place)) {
    match terminator {
        Terminator::Branch { condition: x, .. }
        | Terminator::NarrowBind { source: x, .. }
        | Terminator::Switch {
            discriminant: x, ..
        }
        | Terminator::Spawn { plan: x, .. }
        | Terminator::AwaitAny { futures: x, .. }
        | Terminator::Throw { value: x }
        | Terminator::ShortCircuit { operand: x, .. } => walk_operand_places(x, f),
        Terminator::Rethrow { value, context } => {
            walk_operand_places(value, f);
            walk_operand_places(context, f);
        }
        Terminator::Call { callee, args, .. } | Terminator::SysOp { callee, args, .. } => {
            walk_operand_places(callee, f);
            for arg in args {
                walk_operand_places(arg, f);
            }
        }
        Terminator::VirtualCall { args, .. } => {
            for arg in args {
                walk_operand_places(arg, f);
            }
        }
        Terminator::Await { future, .. } => f(future),
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECT: CallShape = CallShape {
        type_args_primitive: false,
        map_key_primitive: false,
        equality_primitive: false,
    };

    #[test]
    fn callable_builtins_follow_the_may_yield_directive() {
        for key in [
            "baml.Array.push",
            "baml.Array.slice",
            "baml.Int.abs",
            "baml.String.byte_length",
            "baml.Float.floor",
            "baml.deep_copy",
            "baml.Map.length",
            "baml.Map.keys",
        ] {
            assert_eq!(builtin_may_yield(key), Some(false), "{key}");
            assert!(builtin_is_callable(key, DIRECT), "{key}");
        }
        for key in [
            "baml.Array.filter",
            "baml.Array.sort_by",
            "baml._equals_structural_default",
        ] {
            assert_eq!(builtin_may_yield(key), Some(true), "{key}");
            assert!(!builtin_is_callable(key, DIRECT), "{key}");
            assert!(
                !builtin_is_callable(
                    key,
                    CallShape {
                        type_args_primitive: true,
                        map_key_primitive: true,
                        equality_primitive: true,
                    }
                ),
                "{key} runs a callback whatever the types"
            );
        }
        assert_eq!(
            builtin_may_yield("baml.sys.sleep"),
            None,
            "a sys op is no VM builtin"
        );
        assert_eq!(builtin_may_yield("baml.no_such_builtin"), None);
        assert!(!builtin_is_callable("baml.no_such_builtin", DIRECT));
    }

    #[test]
    fn callback_free_yielders_are_callable_when_the_types_rule_callbacks_out() {
        let primitive_types = CallShape {
            type_args_primitive: true,
            map_key_primitive: false,
            equality_primitive: false,
        };
        assert!(builtin_is_callable(
            "baml._to_string_default",
            primitive_types
        ));
        assert!(!builtin_is_callable("baml._to_string_default", DIRECT));
        let primitive_keys = CallShape {
            type_args_primitive: false,
            map_key_primitive: true,
            equality_primitive: false,
        };
        for method in MAP_KEY_METHODS {
            let key = format!("baml.Map.{method}");
            assert_eq!(builtin_may_yield(&key), Some(true), "{key}");
            assert!(builtin_is_callable(&key, primitive_keys), "{key}");
            assert!(!builtin_is_callable(&key, DIRECT), "{key}");
        }
    }

    #[test]
    fn equality_is_callable_when_an_operand_admits_no_class_or_enum() {
        assert_eq!(builtin_may_yield(EQUALS_EQUALS), Some(true));
        assert!(!builtin_is_callable(EQUALS_EQUALS, DIRECT));
        assert!(builtin_is_callable(
            EQUALS_EQUALS,
            CallShape {
                equality_primitive: true,
                ..DIRECT
            }
        ));
        // `unknown` stands for any type that admits an instance.
        let class = RuntimeTy::Unknown;
        for ty in [
            RuntimeTy::Null,
            RuntimeTy::Int,
            RuntimeTy::Union(Box::new([RuntimeTy::String, RuntimeTy::Null])),
            RuntimeTy::List(Box::new(RuntimeTy::Float)),
            RuntimeTy::Map {
                key: Box::new(RuntimeTy::String),
                value: Box::new(RuntimeTy::List(Box::new(RuntimeTy::Bool))),
            },
        ] {
            assert!(equality_is_primitive(&ty), "{ty:?}");
        }
        for ty in [
            class.clone(),
            RuntimeTy::Union(Box::new([class.clone(), RuntimeTy::Null])),
            RuntimeTy::List(Box::new(class.clone())),
            RuntimeTy::Map {
                key: Box::new(RuntimeTy::String),
                value: Box::new(class),
            },
            RuntimeTy::Type,
        ] {
            assert!(!equality_is_primitive(&ty), "{ty:?}");
        }
    }

    #[test]
    fn only_string_keys_skip_the_map_hash_dispatch() {
        let literal =
            RuntimeTy::Literal(Literal::String("a".into()), baml_type::Freshness::Regular);
        assert!(is_primitive_key(&RuntimeTy::String));
        assert!(is_primitive_key(&literal));
        assert!(is_primitive_key(&RuntimeTy::Union(Box::new([
            RuntimeTy::String,
            literal
        ]))));
        assert!(!is_primitive_key(&RuntimeTy::Int));
        assert!(!is_primitive_key(&RuntimeTy::Union(Box::new([
            RuntimeTy::Int,
            RuntimeTy::String
        ]))));
        assert!(!is_primitive_key(&RuntimeTy::Unknown));
    }
}
