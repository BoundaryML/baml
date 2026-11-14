//! Function inlining.
//!
//! Replaces a direct call of a small function with a copy of the callee's
//! blocks, so the call costs no frame and the cleanup passes see the callee's
//! code in its caller's context.
//!
//! The copy is taken from the callee's own lowering
//! ([`crate::lower::lower_function_local`]), never from its inlined form, so
//! reading a callee cannot depend on the caller's result however the two
//! call each other. Calls the copy exposes are inlined in turn from the same
//! pass, bounded by [`MAX_DEPTH`] and [`MAX_GROWTH`], and never a function
//! already being inlined at that point.
//!
//! Each copied block records the call it stands in for
//! ([`crate::InlineScope`]), which is how an error raised in it still
//! reports the callee's frame.

use std::sync::Arc;

use baml_compiler2_hir::{
    item_data::{MethodOwner, function_data, function_trace_hook, method_owner},
    loc::{DeclRef, FunctionLoc},
};

use crate::{
    AggregateKind, BasicBlock, BinOp, BlockId, Constant, InlineScope, Local, LocalDecl,
    MirFunction, MirFunctionBody, MirFunctionKind, Operand, OptLevel, Place, Rvalue, Statement,
    StatementKind, Terminator, memory,
};

/// The largest callee copied into a caller, in statements plus terminators.
pub(crate) const MAX_CALLEE_SIZE: usize = 24;

/// How many calls deep a copy may sit: a callee's callee's callee, and so on.
pub(crate) const MAX_DEPTH: usize = 4;

/// The most one body may grow by, in statements plus terminators, across all
/// the calls inlined into it.
pub(crate) const MAX_GROWTH: usize = 400;

/// Inline eligible calls in `function` and in the lambdas lowered inside it.
/// `root` is the declared function all of them belong to. Returns whether
/// any body changed.
pub(crate) fn inline_function<'db>(
    db: &'db dyn crate::Db,
    root: FunctionLoc<'db>,
    function: &mut MirFunction<'db>,
    opt: OptLevel,
) -> bool {
    // A traced function's entry calls its hook and waits on the result as a
    // call, and a pass-through function hands its trace to the call it
    // makes: their calls stay calls.
    if traced(db, root) {
        return false;
    }
    let mut changed = false;
    if let MirFunctionKind::Bytecode(body) = &mut function.kind {
        changed |= inline_body(db, root, body, opt);
    }
    for lambda in &mut function.lambdas {
        changed |= inline_function(db, root, lambda, opt);
    }
    changed
}

/// Inline eligible calls in one body. Copied blocks are appended, so the scan
/// reaches the calls they contain.
fn inline_body<'db>(
    db: &'db dyn crate::Db,
    root: FunctionLoc<'db>,
    body: &mut MirFunctionBody<'db>,
    opt: OptLevel,
) -> bool {
    let mut growth = 0;
    let mut index = 0;
    while index < body.blocks.len() {
        if let Some((callee, cost)) = eligible_call(db, root, &body.blocks[index], opt)
            && growth + cost <= MAX_GROWTH
            && let Ok(mir) = crate::lower::lower_function_local(db, callee, opt)
            && let MirFunctionKind::Bytecode(callee_body) = &mir.kind
            && let Some(binding) = bind_arguments(db, callee, mir.arity, &body.blocks[index])
        {
            splice(body, BlockId(index), callee, callee_body, &binding);
            growth += cost;
        }
        index += 1;
    }
    growth > 0
}

/// The callee of `block`'s call and its size, when the call may be inlined.
fn eligible_call<'db>(
    db: &'db dyn crate::Db,
    root: FunctionLoc<'db>,
    block: &BasicBlock<'db>,
    opt: OptLevel,
) -> Option<(FunctionLoc<'db>, usize)> {
    let Some(Terminator::Call {
        has_trace: false,
        callee: Operand::Constant(Constant::Function(DeclRef::Source(callee))),
        ntypeargs: 0,
        ..
    }) = &block.terminator
    else {
        return None;
    };
    let callee = *callee;
    // A function calling itself, directly or through the calls already
    // inlined here, would be copied without end.
    let chain = || block.inline.iter().flat_map(|scope| scope.chain());
    if callee == root || chain().any(|scope| scope.callee == callee) {
        return None;
    }
    if chain().count() >= MAX_DEPTH {
        return None;
    }
    // A package's code must not depend on the bodies of another package:
    // its cached output is keyed by its own sources and its dependencies'
    // interfaces. The standard library is part of the compiler build, which
    // is in every key.
    let package = root.file(db).source_root(db);
    let callee_package = callee.file(db).source_root(db);
    if callee_package != package && callee_package.kind(db) != baml_base::SourceRootKind::Stdlib {
        return None;
    }
    // Code loaded at run time calls functions a later load may replace: an
    // eval session's submission redefines what an earlier one declared.
    if package.kind(db) == baml_base::SourceRootKind::Dynamic {
        return None;
    }
    let cost = (*inline_cost(db, callee, opt))?;
    (cost <= MAX_CALLEE_SIZE).then_some((callee, cost))
}

/// The size of `function`'s body in statements plus terminators, when a call
/// of it may be replaced by a copy of that body: `None` for a function that
/// must keep a frame of its own.
#[salsa::tracked(returns(ref))]
pub(crate) fn inline_cost<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
    opt: OptLevel,
) -> Option<usize> {
    if traced(db, function) {
        return None;
    }
    // A body that reads its frame's type arguments needs the frame. An
    // interface's default body is entered with `Self` in its frame.
    if matches!(method_owner(db, function), Some(MethodOwner::Interface(_)))
        || !baml_compiler2_hir_ty::lower::function_generic_frame(db, function).is_empty()
    {
        return None;
    }
    if !baml_compiler2_hir::item_data::function_has_body(db, function) {
        return None;
    }
    let mir = crate::lower::lower_function_local(db, function, opt)
        .as_ref()
        .ok()?;
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return None;
    };
    // A closure names its enclosing function's lambdas by ordinal and its
    // locals by cell.
    if !mir.lambdas.is_empty() || body.locals.iter().any(|local| local.is_captured) {
        return None;
    }
    body_cost(db, function, body)
}

/// Whether the call path does something for `function` beyond running it:
/// its declared hook, a trace passed through it, or a model call's usage.
/// Such a function keeps its own frame and its own calls.
fn traced<'db>(db: &'db dyn crate::Db, function: FunctionLoc<'db>) -> bool {
    function_trace_hook(db, function).is_some()
        || function_data(db, function).metadata.trace_passthrough
        || baml_compiler2_hir::item_data::function_llm_meta(db, function).is_some()
        || crate::lower::is_model_invoke(db, function)
}

/// See [`inline_cost`]: the part decided by the body alone.
fn body_cost<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
    body: &MirFunctionBody<'db>,
) -> Option<usize> {
    let package = function.file(db).source_root(db);
    let mut reads_frame_types = false;
    let mut cost = 0;
    for block in &body.blocks {
        // Handlers, `defer` pads and shielded code are tables keyed by the
        // callee's own blocks.
        if block.unwind.is_some()
            || block.handling.is_some()
            || block.landing.is_some()
            || block.shielded
        {
            return None;
        }
        for statement in &block.statements {
            match &statement.kind {
                // Logging, trace hooks and `BindType` act on the running frame.
                StatementKind::Intrinsic { .. } | StatementKind::FreshCell { .. } => return None,
                StatementKind::Assign { value, .. } => {
                    if matches!(
                        value,
                        Rvalue::TraceHookSettings { .. }
                            | Rvalue::CurrentPackage
                            | Rvalue::MakeClosure { .. }
                    ) {
                        return None;
                    }
                    // An instance with a finalizer is observed when it dies.
                    // Behind a call it is always built; in the caller, the
                    // cleanup passes drop a construction nothing reads.
                    if let Rvalue::Aggregate {
                        kind: AggregateKind::Class { class, .. },
                        ..
                    } = value
                        && crate::escape::may_finalize(db, package, class)
                    {
                        return None;
                    }
                }
                StatementKind::Drop(_)
                | StatementKind::VirtualFieldStore { .. }
                | StatementKind::Nop => {}
            }
            memory::walk_statement_type_slots(&statement.kind, &mut |_| reads_frame_types = true);
        }
        let terminator = block.terminator.as_ref()?;
        match terminator {
            // A suspension is resumed, cancelled and attributed by frame.
            Terminator::SysOp { .. }
            | Terminator::Spawn { .. }
            | Terminator::Await { .. }
            | Terminator::AwaitAny { .. }
            | Terminator::Rethrow { .. } => return None,
            Terminator::Call {
                has_trace: true, ..
            }
            | Terminator::VirtualCall {
                has_trace: true, ..
            } => return None,
            // A function that calls itself gains nothing from one more copy.
            Terminator::Call {
                callee:
                    Operand::Constant(
                        Constant::Function(DeclRef::Source(callee))
                        | Constant::GenericFunction {
                            func: DeclRef::Source(callee),
                            ..
                        },
                    ),
                ..
            } if *callee == function => return None,
            _ => {}
        }
        memory::walk_terminator_type_slots(terminator, &mut |_| reads_frame_types = true);
        cost += block.statements.len() + 1;
    }
    (!reads_frame_types).then_some(cost)
}

/// Where each of the callee's parameters gets its value from.
struct Binding<'db> {
    parameters: Vec<Argument<'db>>,
}

struct Argument<'db> {
    /// The operand of the call that fills the parameter, or `None` for an
    /// optional the call left out.
    operand: Option<Operand<'db>>,
    /// Whether the parameter has a default, which the callee's entry applies
    /// by testing the parameter for the omission sentinel.
    optional: bool,
}

/// Lay the call's value slots over the callee's parameters, as dispatch does
/// at run time. `None` when they do not line up.
fn bind_arguments<'db>(
    db: &'db dyn crate::Db,
    callee: FunctionLoc<'db>,
    arity: usize,
    block: &BasicBlock<'db>,
) -> Option<Binding<'db>> {
    let Some(Terminator::Call {
        argument_layout,
        args,
        ..
    }) = &block.terminator
    else {
        return None;
    };
    let declared = baml_type::CallLayout::from_modes(
        baml_compiler2_hir_ty::callable::callable_signature(db, DeclRef::Source(callee))
            .params
            .iter()
            .map(|param| (param.name.clone(), param.mode)),
    );
    if declared.len() != arity {
        return None;
    }
    let slots: Vec<Option<usize>> = match argument_layout {
        // A compiler-synthesized call is already in the callee's layout.
        None => (0..arity).map(Some).collect(),
        Some(layout) => layout.map_to(&declared).ok()?,
    };
    if argument_layout
        .as_ref()
        .map_or(arity, baml_type::CallLayout::len)
        != args.len()
    {
        return None;
    }
    let parameters = slots
        .into_iter()
        .zip(&declared.0)
        .map(|(slot, declared)| Argument {
            operand: slot.map(|slot| args[slot].clone()),
            optional: declared.is_some(),
        })
        .collect();
    Some(Binding { parameters })
}

/// Replace the call ending `site` with a copy of `callee_body`.
fn splice<'db>(
    body: &mut MirFunctionBody<'db>,
    site: BlockId,
    callee: FunctionLoc<'db>,
    callee_body: &MirFunctionBody<'db>,
    binding: &Binding<'db>,
) {
    let call_block = &body.blocks[site.0];
    let Some(Terminator::Call {
        destination,
        target,
        ..
    }) = call_block.terminator.clone()
    else {
        unreachable!("an inlined site ends in a call")
    };
    let call_span = call_block.terminator_span;
    let (unwind, handling, shielded) =
        (call_block.unwind, call_block.handling, call_block.shielded);
    let scope = Arc::new(InlineScope {
        callee,
        call_span,
        parent: call_block.inline.clone(),
    });

    // The callee's locals become fresh locals of the caller. A required
    // parameter the callee only reads, filled from a caller local of the same
    // type, is that local: nothing in the copy can write it.
    let callee_defs = super::count_local_defs(callee_body);
    let mut local_map: Vec<Option<Local>> = Vec::with_capacity(callee_body.locals.len());
    let mut binds: Vec<Statement<'db>> = Vec::new();
    for (index, decl) in callee_body.locals.iter().enumerate() {
        let parameter = index
            .checked_sub(1)
            .and_then(|parameter| binding.parameters.get(parameter));
        if let Some(Argument {
            operand: Some(operand),
            optional: false,
        }) = parameter
            && callee_defs[index] == 0
            && let Some(source) = argument_local(body, site, operand)
            && body.locals[source.0].ty == decl.ty
        {
            local_map.push(Some(source));
            continue;
        }
        let local = Local(body.locals.len());
        body.locals.push(LocalDecl {
            // The return place is `_0` only in a function of its own.
            name: if index == 0 { None } else { decl.name.clone() },
            // The debugger resolves scopes in the enclosing function's source.
            scope_span: None,
            ..decl.clone()
        });
        local_map.push(Some(local));
        if let Some(argument) = parameter {
            binds.push(Statement {
                kind: StatementKind::Assign {
                    destination: Place::Local(local),
                    value: Rvalue::Use(
                        argument
                            .operand
                            .clone()
                            .unwrap_or(Operand::Constant(Constant::OmittedArg)),
                    ),
                },
                span: call_span,
            });
        }
    }
    let return_local = local_map[0].unwrap_or_else(|| unreachable!("the return place is mapped"));

    let base = body.blocks.len();
    let block_map: Vec<Option<BlockId>> = (0..callee_body.blocks.len())
        .map(|index| Some(BlockId(base + index)))
        .collect();
    for source in &callee_body.blocks {
        let mut block = source.clone();
        block.id = BlockId(base + source.id.0);
        // A throw or panic in the copy unwinds as one in the call did.
        block.unwind = unwind;
        block.handling = handling;
        block.shielded = shielded;
        block.inline = Some(scope.clone());
        for statement in &mut block.statements {
            super::rewrite_locals_in_statement(statement, &local_map);
        }
        match block.terminator.as_mut() {
            Some(Terminator::Return) => {
                block.statements.push(Statement {
                    kind: StatementKind::Assign {
                        destination: destination.clone(),
                        value: Rvalue::Use(Operand::Copy(Place::Local(return_local))),
                    },
                    span: source.terminator_span,
                });
                block.terminator = Some(Terminator::Goto { target });
            }
            Some(terminator) => {
                super::rewrite_locals_in_terminator(terminator, &local_map);
                super::rewrite_block_ids_in_terminator(terminator, &block_map);
                if let Terminator::Call { unwind: edge, .. }
                | Terminator::VirtualCall { unwind: edge, .. }
                | Terminator::SysOp { unwind: edge, .. }
                | Terminator::Await { unwind: edge, .. }
                | Terminator::AwaitAny { unwind: edge, .. } = terminator
                {
                    *edge = unwind;
                }
            }
            None => unreachable!("a lowered block is terminated"),
        }
        body.blocks.push(block);
    }

    for (parameter, argument) in binding.parameters.iter().enumerate() {
        let omitted = match &argument.operand {
            None | Some(Operand::Constant(Constant::OmittedArg)) => true,
            Some(Operand::Constant(_)) => false,
            // A value computed at run time is tested at run time.
            Some(Operand::Copy(_) | Operand::Move(_)) => continue,
        };
        for (block, statement) in omission_tests(callee_body, Local(parameter + 1)) {
            let StatementKind::Assign { value, .. } =
                &mut body.blocks[base + block.0].statements[statement].kind
            else {
                unreachable!("an omission test is an assignment")
            };
            let Rvalue::BinaryOp { op, .. } = value else {
                unreachable!("an omission test is a comparison")
            };
            *value = Rvalue::Use(Operand::Constant(Constant::Bool(
                omitted == (*op == BinOp::Eq),
            )));
        }
    }

    let call_block = &mut body.blocks[site.0];
    call_block.statements.extend(binds);
    call_block.terminator = Some(Terminator::Goto {
        target: BlockId(base + callee_body.entry.0),
    });
}

/// The caller local whose value `operand` is at the call ending `site`, and
/// stays for as long as the callee's copy runs: the operand's own local, or
/// the local that one was copied from earlier in the block. The copy cannot
/// write a local of its caller, so a parameter may stand for it.
fn argument_local(
    body: &MirFunctionBody<'_>,
    site: BlockId,
    operand: &Operand<'_>,
) -> Option<Local> {
    let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = operand else {
        return None;
    };
    let mut local = *local;
    let statements = &body.blocks[site.0].statements;
    // An argument is usually a temporary holding a copy of a binding.
    if body.locals[local.0].name.is_none()
        && let Some(definition) = statements
            .iter()
            .rposition(|statement| defines(&statement.kind, local))
        && let StatementKind::Assign {
            destination: Place::Local(_),
            value:
                Rvalue::Use(Operand::Copy(Place::Local(source)) | Operand::Move(Place::Local(source))),
        } = &statements[definition].kind
        && !statements[definition + 1..]
            .iter()
            .any(|statement| defines(&statement.kind, *source))
    {
        local = *source;
    }
    (!body.locals[local.0].is_captured).then_some(local)
}

/// Whether the statement writes `local` or a place inside it.
fn defines(kind: &StatementKind<'_>, local: Local) -> bool {
    match kind {
        StatementKind::Assign { destination, .. } => destination.base_local() == Some(local),
        StatementKind::FreshCell { local: cell, .. } => *cell == local,
        StatementKind::Drop(_)
        | StatementKind::Intrinsic { .. }
        | StatementKind::VirtualFieldStore { .. }
        | StatementKind::Nop => false,
    }
}

/// Whether the terminator writes `local` or a place inside it.
fn terminator_defines(terminator: &Terminator<'_>, local: Local) -> bool {
    match terminator {
        Terminator::NarrowBind { destination, .. } => *destination == local,
        Terminator::Call { destination, .. }
        | Terminator::VirtualCall { destination, .. }
        | Terminator::SysOp { destination, .. }
        | Terminator::Await { destination, .. }
        | Terminator::AwaitAny { destination, .. }
        | Terminator::ShortCircuit { destination, .. }
        | Terminator::Spawn {
            future: destination,
            ..
        } => destination.base_local() == Some(local),
        Terminator::Goto { .. }
        | Terminator::Branch { .. }
        | Terminator::Switch { .. }
        | Terminator::Return
        | Terminator::Unreachable
        | Terminator::Throw { .. }
        | Terminator::Rethrow { .. } => false,
    }
}

/// The statements of `body` that compare `parameter` with the omission
/// sentinel while it still holds the value it was called with: no path from
/// the entry to the comparison writes it. A function's entry tests each
/// defaulted parameter this way before filling in its default.
fn omission_tests(body: &MirFunctionBody<'_>, parameter: Local) -> Vec<(BlockId, usize)> {
    let is_test = |kind: &StatementKind<'_>| {
        let StatementKind::Assign {
            value:
                Rvalue::BinaryOp {
                    op: BinOp::Eq | BinOp::Ne,
                    left,
                    right,
                },
            ..
        } = kind
        else {
            return false;
        };
        [(left, right), (right, left)].into_iter().any(|pair| {
            matches!(
                pair,
                (
                    Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)),
                    Operand::Constant(Constant::OmittedArg),
                ) if *local == parameter
            )
        })
    };
    let mut predecessors: Vec<Vec<BlockId>> = vec![Vec::new(); body.blocks.len()];
    for block in &body.blocks {
        for successor in block.terminator.iter().flat_map(Terminator::successors) {
            predecessors[successor.0].push(block.id);
        }
    }
    let writes = |block: &BasicBlock<'_>| {
        block
            .statements
            .iter()
            .any(|statement| defines(&statement.kind, parameter))
            || block
                .terminator
                .as_ref()
                .is_some_and(|terminator| terminator_defines(terminator, parameter))
    };
    let mut tests = Vec::new();
    for block in &body.blocks {
        for (index, statement) in block.statements.iter().enumerate() {
            if !is_test(&statement.kind) {
                continue;
            }
            // Every block some path runs before this one, itself included
            // when it is in a loop.
            let mut earlier: Vec<BlockId> = Vec::new();
            let mut pending = predecessors[block.id.0].clone();
            while let Some(next) = pending.pop() {
                if !earlier.contains(&next) {
                    earlier.push(next);
                    pending.extend(&predecessors[next.0]);
                }
            }
            let written = earlier
                .iter()
                .any(|earlier| writes(&body.blocks[earlier.0]))
                || block.statements[..index]
                    .iter()
                    .any(|statement| defines(&statement.kind, parameter));
            if !written {
                tests.push((block.id, index));
            }
        }
    }
    tests
}
