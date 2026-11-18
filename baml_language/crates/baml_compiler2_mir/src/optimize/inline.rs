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
//!
//! A standard library method written in Rust may have a loop form: a sibling
//! method written in BAML that does exactly what it does ([`loop_form`]). A
//! call of the Rust method that passes a closure this pass can inline is
//! replaced by a copy of the loop form, so the closure's body ends up in the
//! loop. A stack trace still shows the Rust method's frame, which has no
//! file and no lines ([`crate::InlineScope::native`]).

use std::sync::Arc;

use baml_compiler2_hir::{
    item_data::{function_data, function_trace_hook},
    loc::{DeclRef, FunctionLoc},
};

use super::devirtualize;
use crate::{
    BasicBlock, BinOp, BlockId, CellId, Constant, InlineScope, Local, LocalDecl, MirFunction,
    MirFunctionBody, MirFunctionId, MirFunctionKind, Operand, OptLevel, Place, Rvalue, Statement,
    StatementKind, Terminator, TyTemplate, TypeTest, memory,
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
    let MirFunction {
        kind,
        lambdas,
        arity,
        ..
    } = function;
    if let MirFunctionKind::Bytecode(body) = kind {
        changed |= inline_body(db, root, body, *arity, lambdas, opt);
    }
    for lambda in lambdas {
        changed |= inline_function(db, root, lambda, opt);
    }
    changed
}

/// Inline eligible calls in one body, whose closures are made from
/// `lambdas`. Copied blocks are appended, so the scan reaches the calls they
/// contain.
fn inline_body<'db>(
    db: &'db dyn crate::Db,
    root: FunctionLoc<'db>,
    body: &mut MirFunctionBody<'db>,
    arity: usize,
    lambdas: &[MirFunction<'db>],
    opt: OptLevel,
) -> bool {
    let mut growth = 0;
    let mut index = 0;
    while index < body.blocks.len() {
        let site = BlockId(index);
        read_known_fields(db, body, arity, site, &mut growth);
        // An interface call on a receiver of a known class is tried as the
        // direct call of what that class provides, and put back if the
        // callee turns out to keep its call.
        let dispatched = resolve_dispatch(db, body, arity, site, &mut growth);
        let inlined = if let Some(callee) = callee_of(db, root, body, arity, site, lambdas, opt)
            && growth + callee.cost <= MAX_GROWTH
            && let Some(binding) = bind_arguments(&callee, &body.blocks[index])
        {
            growth += callee.cost;
            splice(body, site, &callee, &binding);
            true
        } else {
            false
        };
        if !inlined && let Some(dispatched) = dispatched {
            body.blocks[index].terminator = Some(dispatched);
        }
        index += 1;
    }
    growth > 0
}

/// Replace the interface call ending `site` with a direct call of the
/// function it reaches, when the receiver's class is known there. Returns
/// the interface call it replaced.
fn resolve_dispatch<'db>(
    db: &'db dyn crate::Db,
    body: &mut MirFunctionBody<'db>,
    arity: usize,
    site: BlockId,
    growth: &mut usize,
) -> Option<Terminator<'db>> {
    let Some(Terminator::VirtualCall {
        has_trace: false,
        iface,
        method,
        args,
        ntypeargs: 0,
        self_arg,
        ..
    }) = body.blocks[site.0].terminator.clone()
    else {
        return None;
    };
    let at = (site, body.blocks[site.0].statements.len());
    let class = devirtualize::class_of(body, arity, at, args.get(self_arg)?, growth, MAX_GROWTH)?;
    let function = devirtualize::method(db, &class, &iface, &method)?;
    // Finding the class may have renamed the receiver, so the call is
    // rebuilt from the terminator as it is now.
    let Some(Terminator::VirtualCall {
        argument_layout,
        args,
        destination,
        target,
        unwind,
        ..
    }) = body.blocks[site.0].terminator.clone()
    else {
        unreachable!("finding a receiver's class leaves its call in place")
    };
    body.blocks[site.0].terminator.replace(Terminator::Call {
        has_trace: false,
        argument_layout,
        callee: Operand::Constant(Constant::Function(DeclRef::Source(function))),
        args,
        ntypeargs: 0,
        destination,
        target,
        unwind,
    })
}

/// Turn each interface field read in `site` whose receiver's class is known
/// there into a read of the class field the interface field is linked to.
fn read_known_fields<'db>(
    db: &'db dyn crate::Db,
    body: &mut MirFunctionBody<'db>,
    arity: usize,
    site: BlockId,
    growth: &mut usize,
) {
    let mut index = 0;
    while index < body.blocks[site.0].statements.len() {
        if let StatementKind::Assign {
            value:
                Rvalue::VirtualFieldAccess {
                    iface,
                    receiver,
                    field,
                    ..
                },
            ..
        } = &body.blocks[site.0].statements[index].kind
        {
            let (iface, receiver, field) = (iface.clone(), receiver.clone(), field.clone());
            if let Some(class) =
                devirtualize::class_of(body, arity, (site, index), &receiver, growth, MAX_GROWTH)
                // A slot read names its base's class, and only a class
                // without type arguments is spelled without a frame.
                && class.args.is_empty()
                && let Some(slot) = devirtualize::field_slot(db, &class, &iface, &field)
            {
                let instance = Local(body.locals.len());
                body.locals.push(LocalDecl {
                    name: None,
                    ty: crate::RuntimeTy::Class(
                        baml_compiler2_hir_ty::layout::class_head(db, class.class),
                        Box::new([]),
                    ),
                    span: None,
                    scope_span: None,
                    is_captured: false,
                });
                let statements = &mut body.blocks[site.0].statements;
                let span = statements[index].span;
                // Finding the class may have renamed the receiver, so it is
                // read again from the statement as it is now.
                let StatementKind::Assign {
                    value: value @ Rvalue::VirtualFieldAccess { .. },
                    ..
                } = &mut statements[index].kind
                else {
                    unreachable!("matched as an interface field read above")
                };
                let Rvalue::VirtualFieldAccess { receiver, .. } = value.clone() else {
                    unreachable!("matched as an interface field read above")
                };
                *value = Rvalue::Use(Operand::Copy(Place::Field {
                    base: Box::new(Place::Local(instance)),
                    field: slot,
                }));
                statements.insert(
                    index,
                    Statement {
                        kind: StatementKind::Assign {
                            destination: Place::Local(instance),
                            value: Rvalue::Use(receiver),
                        },
                        span,
                    },
                );
                index += 1;
            }
        }
        index += 1;
    }
}

/// Give every captured local that no closure captures any more a plain slot
/// again, in `function` and the lambdas lowered inside it: once the calls
/// through a closure are inlined and the closure itself is gone, its
/// captured variables are ordinary locals. Returns whether any was.
pub(crate) fn release_cells(function: &mut MirFunction<'_>) -> bool {
    let mut released = false;
    if let MirFunctionKind::Bytecode(body) = &mut function.kind {
        released |= release_body_cells(body);
    }
    for lambda in &mut function.lambdas {
        released |= release_cells(lambda);
    }
    released
}

/// Rewrite a place that goes through the cell of a local in `plain` to name
/// the local itself.
fn value_place(place: &mut Place, plain: &[bool]) {
    match place {
        Place::Deref(CellId::Local(local)) if plain[local.0] => {
            *place = Place::Local(*local);
        }
        Place::Field { base, .. } | Place::Index { base, .. } => value_place(base, plain),
        Place::Local(_) | Place::Capture(_) | Place::Deref(_) => {}
    }
}

fn release_body_cells(body: &mut MirFunctionBody<'_>) -> bool {
    // A cell is shared exactly where a `MakeClosure` names its local bare.
    let mut plain: Vec<bool> = body.locals.iter().map(|local| local.is_captured).collect();
    for block in &body.blocks {
        for statement in &block.statements {
            if let StatementKind::Assign {
                value: Rvalue::MakeClosure { captures, .. },
                ..
            } = &statement.kind
            {
                for capture in captures {
                    if let Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) =
                        capture
                    {
                        plain[local.0] = false;
                    }
                }
            }
        }
    }
    if !plain.contains(&true) {
        return false;
    }
    for block in &mut body.blocks {
        // Every read is dominated by a write of the binding, so the cell a
        // `FreshCell` would have made holds nothing a read can see.
        block.statements.retain(|statement| {
            !matches!(
                &statement.kind,
                StatementKind::FreshCell { local, .. } if plain[local.0]
            )
        });
        for statement in &mut block.statements {
            super::for_each_place_in_statement(statement, &mut |place| value_place(place, &plain));
        }
        if let Some(terminator) = &mut block.terminator {
            super::for_each_place_in_terminator(terminator, &mut |place| {
                value_place(place, &plain);
            });
        }
    }
    for (local, plain) in body.locals.iter_mut().zip(&plain) {
        if *plain {
            local.is_captured = false;
        }
    }
    true
}

/// A callee whose body may replace one particular call.
struct Callee<'a, 'db> {
    identity: MirFunctionId<'db>,
    body: &'a MirFunctionBody<'db>,
    arity: usize,
    /// The value slots the callee's parameters read.
    declared: baml_type::CallLayout,
    /// The type each of its frame's type-argument slots holds at this call,
    /// where the call names one that is known where it is written.
    frame: Vec<Option<TyTemplate>>,
    /// Whether the body reads its frame's type arguments.
    reads_frame_types: bool,
    /// For a closure, the cell behind each of its captures, as the body the
    /// closure was made in names it.
    cells: Vec<CellId>,
    /// For a bound method, the receiver it was bound to, which the call
    /// passes ahead of its own arguments.
    receiver: Option<Operand<'db>>,
    /// Its size in statements plus terminators.
    cost: usize,
    /// Whether the body is the loop form of `identity`, a function written
    /// in Rust.
    native: bool,
}

/// The callee of the call ending `site`, when that call may be inlined.
fn callee_of<'a, 'db: 'a>(
    db: &'db dyn crate::Db,
    root: FunctionLoc<'db>,
    body: &MirFunctionBody<'db>,
    arity: usize,
    site: BlockId,
    lambdas: &'a [MirFunction<'db>],
    opt: OptLevel,
) -> Option<Callee<'a, 'db>> {
    let block = &body.blocks[site.0];
    let Some(Terminator::Call {
        has_trace: false,
        callee,
        args,
        ntypeargs,
        ..
    }) = &block.terminator
    else {
        return None;
    };
    // Code loaded at run time calls functions a later load may replace: an
    // eval session's submission redefines what an earlier one declared.
    let package = root.file(db).source_root(db);
    if package.kind(db) == baml_base::SourceRootKind::Dynamic {
        return None;
    }
    let callee = match callee {
        Operand::Constant(Constant::Function(DeclRef::Source(function))) => {
            let frame = args[..*ntypeargs]
                .iter()
                .map(|argument| type_argument(body, arity, argument))
                .collect();
            let passed = &args[*ntypeargs..];
            named_callee(db, package, *function, frame, opt, || {
                passes_a_closure_to_inline(body, arity, site, lambdas, passed)
            })?
        }
        Operand::Constant(Constant::GenericFunction {
            func: DeclRef::Source(function),
            type_args,
        }) if *ntypeargs == 0 => {
            let frame = type_args
                .iter()
                .cloned()
                .map(|argument| Some(TyTemplate::from(argument)))
                .collect();
            named_callee(db, package, *function, frame, opt, || {
                passes_a_closure_to_inline(body, arity, site, lambdas, args)
            })?
        }
        Operand::Copy(Place::Local(value)) | Operand::Move(Place::Local(value))
            if *ntypeargs == 0 =>
        {
            match callable_of(body, arity, site, *value)? {
                Callable::Closure(closure) => closure_callee(&lambdas[closure.lambda], closure)?,
                Callable::Function(function, frame) => {
                    declared_callee(db, package, function, frame, opt)?
                }
                Callable::BoundMethod(function, receiver) => Callee {
                    receiver: Some(receiver),
                    ..declared_callee(db, package, function, Vec::new(), opt)?
                },
            }
        }
        _ => return None,
    };
    // A function calling itself, directly or through the calls already
    // inlined here, would be copied without end.
    let chain = || block.inline.iter().flat_map(|scope| scope.chain());
    if callee.identity == MirFunctionId::Declared(root)
        || chain().any(|scope| scope.callee == callee.identity)
        || chain().count() >= MAX_DEPTH
        // A loop form is written to be copied, whatever its size.
        || (callee.cost > MAX_CALLEE_SIZE && !callee.native)
    {
        return None;
    }
    // Every type the body reads from its frame must come out whole.
    (!callee.reads_frame_types || substitutes(callee.body, &callee.frame)).then_some(callee)
}

/// The callee of a call that names `function`: the function itself, or its
/// loop form when it has one and the call passes a closure to inline.
fn named_callee<'db>(
    db: &'db dyn crate::Db,
    package: baml_base::SourceRoot,
    function: FunctionLoc<'db>,
    frame: Vec<Option<TyTemplate>>,
    opt: OptLevel,
    passes_a_closure: impl FnOnce() -> bool,
) -> Option<Callee<'db, 'db>> {
    match loop_form(db, function) {
        Some(form) if passes_a_closure() => Some(Callee {
            identity: MirFunctionId::Declared(function),
            native: true,
            ..declared_callee(db, package, form, frame, opt)?
        }),
        Some(_) => None,
        None => declared_callee(db, package, function, frame, opt),
    }
}

/// The loop form of a standard library method written in Rust: the method
/// of the same class named `_inlined_` and then its name. It takes the same
/// parameters and type parameters, and does exactly what the method does.
#[salsa::tracked(returns(clone))]
pub fn loop_form<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<FunctionLoc<'db>> {
    sibling(db, function, "_inlined_")
}

/// The driver of a standard library method written in Rust that calls a
/// closure: the method of the same class named `_driver_` and then its
/// name, which does exactly what the method does. It is for a backend that
/// cannot call a closure from Rust. The inliner does not use it: a method
/// has a driver and no loop form where the loop is no faster in the
/// interpreter than the Rust function.
#[salsa::tracked(returns(clone))]
pub fn driver_form<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
) -> Option<FunctionLoc<'db>> {
    sibling(db, function, "_driver_")
}

/// The method of a standard library method's class named `prefix` and then
/// the method's name.
fn sibling<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
    prefix: &str,
) -> Option<FunctionLoc<'db>> {
    use baml_compiler2_hir::item_data::{MethodOwner, class_data, method_owner};
    if function.file(db).source_root(db).kind(db) != baml_base::SourceRootKind::Stdlib {
        return None;
    }
    let MethodOwner::Class(class) = method_owner(db, function)? else {
        return None;
    };
    let name = format!("{prefix}{}", function_data(db, function).name);
    class_data(db, class)
        .methods
        .iter()
        .copied()
        .find(|&method| function_data(db, method).name.as_str() == name)
}

/// Whether one of the values the call ending `site` passes is a closure
/// made in this body that a call through it would inline.
fn passes_a_closure_to_inline<'db>(
    body: &MirFunctionBody<'db>,
    arity: usize,
    site: BlockId,
    lambdas: &[MirFunction<'db>],
    passed: &[Operand<'db>],
) -> bool {
    passed.iter().any(|argument| {
        let (Operand::Copy(Place::Local(value)) | Operand::Move(Place::Local(value))) = argument
        else {
            return false;
        };
        let Some(Callable::Closure(closure)) = callable_of(body, arity, site, *value) else {
            return false;
        };
        closure_callee(&lambdas[closure.lambda], closure)
            .is_some_and(|callee| callee.cost <= MAX_CALLEE_SIZE)
    })
}

/// A declared function as the callee of a call that fills its frame's
/// type-argument slots with `frame`.
fn declared_callee<'db>(
    db: &'db dyn crate::Db,
    package: baml_base::SourceRoot,
    function: FunctionLoc<'db>,
    frame: Vec<Option<TyTemplate>>,
    opt: OptLevel,
) -> Option<Callee<'db, 'db>> {
    // A package's code must not depend on the bodies of another package:
    // its cached output is keyed by its own sources and its dependencies'
    // interfaces. The standard library is part of the compiler build, which
    // is in every key.
    let callee_package = function.file(db).source_root(db);
    if callee_package != package && callee_package.kind(db) != baml_base::SourceRootKind::Stdlib {
        return None;
    }
    let cost = (*inline_cost(db, function, opt))?;
    let mir = crate::lower::lower_function_local(db, function, opt)
        .as_ref()
        .ok()?;
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return None;
    };
    Some(Callee {
        identity: MirFunctionId::Declared(function),
        body,
        arity: mir.arity,
        declared: baml_type::CallLayout::from_modes(
            baml_compiler2_hir_ty::callable::callable_signature(db, DeclRef::Source(function))
                .params
                .iter()
                .map(|param| (param.name.clone(), param.mode)),
        ),
        frame,
        reads_frame_types: cost.reads_frame_types,
        cells: Vec::new(),
        receiver: None,
        cost: cost.size,
        native: false,
    })
}

/// A lambda of the body being compiled as the callee of a call through a
/// closure known to be made from it.
fn closure_callee<'a, 'db>(
    lambda: &'a MirFunction<'db>,
    closure: KnownClosure,
) -> Option<Callee<'a, 'db>> {
    let MirFunctionKind::Bytecode(body) = &lambda.kind else {
        return None;
    };
    if !lambda.lambdas.is_empty() || body.locals.iter().any(|local| local.is_captured) {
        return None;
    }
    let signature = lambda.signature.as_ref()?;
    let (cost, reads_frame_types) = body_cost(None, body)?;
    // Its frame is the type arguments captured where it was made, which the
    // enclosing frame may have rebound since.
    if reads_frame_types {
        return None;
    }
    Some(Callee {
        identity: lambda.identity.clone(),
        body,
        arity: lambda.arity,
        declared: baml_type::CallLayout(
            signature
                .param_names
                .iter()
                .zip(&signature.param_has_default)
                .map(|(name, has_default)| has_default.then(|| baml_type::Name::new(name)))
                .collect(),
        ),
        frame: Vec::new(),
        reads_frame_types,
        cells: closure.cells,
        receiver: None,
        cost,
        native: false,
    })
}

/// What a local is known to hold at a call through it.
enum Callable<'db> {
    Closure(KnownClosure),
    /// A declared function, with the type arguments the value carries.
    Function(FunctionLoc<'db>, Vec<Option<TyTemplate>>),
    /// A method bound to a receiver.
    BoundMethod(FunctionLoc<'db>, Operand<'db>),
}

/// A closure a local is known to hold at a call.
struct KnownClosure {
    /// The lambda it was made from, as `MakeClosure` numbers it.
    lambda: usize,
    /// The cell behind each capture.
    cells: Vec<CellId>,
}

/// What `value` holds at the call ending `site`, when it can only be one
/// thing: `value`'s one definition, or the one definition of the local it
/// was copied from, is a function constant, a method bound to a receiver
/// that is not written again, or a `MakeClosure` each of whose captured
/// locals still has, at the call, the cell the closure captured.
///
/// A captured local gets a new cell where its binding is created
/// (`FreshCell`), once per loop iteration for a binding inside a loop. The
/// closure keeps the cell it was made with. So no `FreshCell` of a captured
/// local may run between the `MakeClosure` and the call: not after the
/// `MakeClosure` up to each copy that carries the closure on, and not after
/// that copy up to the next one or the call.
fn callable_of<'db>(
    body: &MirFunctionBody<'db>,
    arity: usize,
    site: BlockId,
    value: Local,
) -> Option<Callable<'db>> {
    let defs = super::count_local_defs(body);
    let mut value = value;
    // Where the closure is at each step, from the call back to where it was
    // made.
    let mut steps = vec![(site, body.blocks[site.0].statements.len())];
    // A copy chain cannot be longer than the body has locals.
    for _ in 0..body.locals.len() {
        // A parameter also holds what the caller passed.
        if defs[value.0] != 1 || value.0 <= arity {
            return None;
        }
        let (at, definition) = body.blocks.iter().find_map(|block| {
            block
                .statements
                .iter()
                .enumerate()
                .find_map(|(index, statement)| match &statement.kind {
                    StatementKind::Assign {
                        destination: Place::Local(local),
                        value: definition,
                    } if *local == value => Some(((block.id, index), definition)),
                    _ => None,
                })
        })?;
        steps.push(at);
        match definition {
            Rvalue::Use(
                Operand::Copy(Place::Local(source)) | Operand::Move(Place::Local(source)),
            ) => {
                value = *source;
            }
            Rvalue::MakeClosure {
                lambda_idx,
                captures,
                ..
            } => {
                let cells = captures
                    .iter()
                    .map(|capture| match capture {
                        Operand::Copy(place) | Operand::Move(place) => {
                            CellId::try_from(place.clone()).ok()
                        }
                        Operand::Constant(_) => None,
                    })
                    .collect::<Option<Vec<_>>>()?;
                let captured: Vec<Local> = cells.iter().filter_map(CellId::local).collect();
                if !captured.is_empty()
                    && steps
                        .windows(2)
                        .any(|step| fresh_cell_between(body, step[1], step[0], &captured))
                {
                    return None;
                }
                return Some(Callable::Closure(KnownClosure {
                    lambda: *lambda_idx,
                    cells,
                }));
            }
            Rvalue::Use(Operand::Constant(Constant::Function(DeclRef::Source(function)))) => {
                return Some(Callable::Function(*function, Vec::new()));
            }
            Rvalue::Use(Operand::Constant(Constant::GenericFunction {
                func: DeclRef::Source(function),
                type_args,
            })) => {
                let frame = type_args
                    .iter()
                    .cloned()
                    .map(|argument| Some(TyTemplate::from(argument)))
                    .collect();
                return Some(Callable::Function(*function, frame));
            }
            // The method's frame is the receiver's class arguments, which
            // are not written here: a body that reads them keeps its call.
            Rvalue::MakeBoundMethod {
                func: DeclRef::Source(function),
                receiver:
                    receiver @ (Operand::Copy(Place::Local(bound)) | Operand::Move(Place::Local(bound))),
                frame: crate::BoundFrame::Receiver,
            } => {
                // The value was bound to what the receiver's local held
                // then, so the local must hold it still: a parameter never
                // written, or a local written once.
                let written = defs[bound.0];
                let unchanged = if bound.0 <= arity {
                    written == 0
                } else {
                    written == 1
                };
                return (unchanged && !body.locals[bound.0].is_captured)
                    .then(|| Callable::BoundMethod(*function, receiver.clone()));
            }
            _ => return None,
        }
    }
    None
}

/// Whether some path from just after statement `from` to just before
/// statement `to` (a block and a statement index; the block's length for
/// its terminator) gives one of `locals` a new cell, without running `from`
/// again: a path that does has a newer `from` to start at.
fn fresh_cell_between(
    body: &MirFunctionBody<'_>,
    from: (BlockId, usize),
    to: (BlockId, usize),
    locals: &[Local],
) -> bool {
    let fresh = |statements: &[Statement<'_>]| {
        statements.iter().any(|statement| {
            matches!(
                &statement.kind,
                StatementKind::FreshCell { local, .. } if locals.contains(local)
            )
        })
    };
    let (start, end) = (&body.blocks[from.0.0], &body.blocks[to.0.0]);
    if from.0 == to.0 && from.1 < to.1 {
        return fresh(&start.statements[from.1 + 1..to.1]);
    }
    if fresh(&start.statements[from.1 + 1..]) || fresh(&end.statements[..to.1]) {
        return true;
    }
    // A throw or panic anywhere in a block continues at its handler.
    let successors = |block: &BasicBlock<'_>| {
        block
            .terminator
            .iter()
            .flat_map(Terminator::successors)
            .chain(block.unwind)
            .collect::<Vec<_>>()
    };
    let mut predecessors: Vec<Vec<BlockId>> = vec![Vec::new(); body.blocks.len()];
    for block in &body.blocks {
        for successor in successors(block) {
            predecessors[successor.0].push(block.id);
        }
    }
    let reach = |seeds: Vec<BlockId>, next: &dyn Fn(BlockId) -> Vec<BlockId>| {
        let mut seen = vec![false; body.blocks.len()];
        let mut pending = seeds;
        while let Some(block) = pending.pop() {
            if block != from.0 && !std::mem::replace(&mut seen[block.0], true) {
                pending.extend(next(block));
            }
        }
        seen
    };
    let after = reach(successors(start), &|block| {
        successors(&body.blocks[block.0])
    });
    let before = reach(predecessors[to.0.0].clone(), &|block| {
        predecessors[block.0].clone()
    });
    body.blocks
        .iter()
        .any(|block| after[block.id.0] && before[block.id.0] && fresh(&block.statements))
}

/// The type a call passes in `argument`, when it is known where the call is
/// written: the operand is a local whose one definition loads a type that
/// reads nothing from the caller's own frame.
fn type_argument(
    body: &MirFunctionBody<'_>,
    arity: usize,
    argument: &Operand<'_>,
) -> Option<TyTemplate> {
    let (Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local))) = argument else {
        return None;
    };
    // A parameter also holds what the caller passed.
    if super::count_local_defs(body)[local.0] != 1 || local.0 <= arity {
        return None;
    }
    body.blocks
        .iter()
        .flat_map(|block| &block.statements)
        .find_map(|statement| match &statement.kind {
            StatementKind::Assign {
                destination: Place::Local(defined),
                value: Rvalue::LoadType(template),
            } if defined == local && template.is_fully_concrete() => Some(template.clone()),
            _ => None,
        })
}

/// Rewrite the templates of a copied statement or terminator to read `frame`
/// where they read the callee's frame.
fn substitute(template: &mut TyTemplate, frame: &[Option<TyTemplate>]) {
    template.replace_type_arg_refs(&mut |slot| {
        frame
            .get(slot as usize)
            .cloned()
            .flatten()
            .unwrap_or(TyTemplate::TypeArgRef(slot))
    });
}

/// Whether every template of `body` that reads the frame becomes a whole
/// type once `frame` is put in its place. One that does not (a projection
/// the runtime reduces against the frame, or a slot the call does not fill)
/// keeps the call.
fn substitutes(body: &MirFunctionBody<'_>, frame: &[Option<TyTemplate>]) -> bool {
    let mut whole = true;
    let mut check = |template: &mut TyTemplate| {
        let mut reads = false;
        template.for_each_type_arg_ref(&mut |_| reads = true);
        if reads {
            let mut replaced = template.clone();
            substitute(&mut replaced, frame);
            whole &= replaced.is_fully_concrete();
        }
    };
    for block in &body.blocks {
        for statement in &block.statements {
            if let StatementKind::Assign {
                value: Rvalue::IsType { test, .. },
                ..
            } = &statement.kind
                && tests_a_frame_type(test)
            {
                return false;
            }
            memory::for_each_statement_template_mut(&mut statement.kind.clone(), &mut check);
        }
        if let Some(terminator) = &block.terminator {
            if let Terminator::NarrowBind { test, .. } = terminator
                && tests_a_frame_type(test)
            {
                return false;
            }
            memory::for_each_terminator_template_mut(&mut terminator.clone(), &mut check);
        }
    }
    whole
}

/// Whether `test` is against a type its frame decides. Lowering states a
/// test against a class or an enum by declaration, which a template with
/// the frame's type put in place does not do.
fn tests_a_frame_type(test: &TypeTest<'_>) -> bool {
    let mut reads = false;
    if let TypeTest::Template(template) = test {
        template.for_each_type_arg_ref(&mut |_| reads = true);
    }
    reads
}

/// What [`inline_cost`] answers for a function that may be inlined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InlineCost {
    /// The body's size in statements plus terminators.
    size: usize,
    /// Whether the body reads its frame's type arguments. A call must then
    /// name a known type for each slot it reads.
    reads_frame_types: bool,
}

/// The size of `function`'s body in statements plus terminators, when a call
/// of it may be replaced by a copy of that body: `None` for a function that
/// must keep a frame of its own.
#[salsa::tracked(returns(ref))]
pub(crate) fn inline_cost<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
    opt: OptLevel,
) -> Option<InlineCost> {
    if traced(db, function) {
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
    let (size, reads_frame_types) = body_cost(Some(function), body)?;
    Some(InlineCost {
        size,
        reads_frame_types,
    })
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

/// The size of `body` and whether it reads its frame's type arguments, or
/// `None` when the body needs a frame of its own. `function` is the declared
/// function it is the body of, if any.
fn body_cost<'db>(
    function: Option<FunctionLoc<'db>>,
    body: &MirFunctionBody<'db>,
) -> Option<(usize, bool)> {
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
            } if Some(*callee) == function => return None,
            _ => {}
        }
        memory::walk_terminator_type_slots(terminator, &mut |_| reads_frame_types = true);
        cost += block.statements.len() + 1;
    }
    Some((cost, reads_frame_types))
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
fn bind_arguments<'db>(callee: &Callee<'_, 'db>, block: &BasicBlock<'db>) -> Option<Binding<'db>> {
    let Some(Terminator::Call {
        argument_layout,
        args,
        ntypeargs,
        ..
    }) = &block.terminator
    else {
        return None;
    };
    // A bound method's receiver is its first argument, ahead of the slots
    // the call was checked against.
    let mut args: Vec<Operand<'db>> = args[*ntypeargs..].to_vec();
    let mut argument_layout = argument_layout.clone();
    if let Some(receiver) = &callee.receiver {
        args.insert(0, receiver.clone());
        if let Some(layout) = &mut argument_layout {
            layout.0.insert(0, None);
        }
    }
    let (declared, arity) = (&callee.declared, callee.arity);
    if declared.len() != arity {
        return None;
    }
    let slots: Vec<Option<usize>> = match &argument_layout {
        // A compiler-synthesized call is already in the callee's layout.
        None => (0..arity).map(Some).collect(),
        Some(layout) => layout.map_to(declared).ok()?,
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

/// Replace the call ending `site` with a copy of the callee's body.
fn splice<'db>(
    body: &mut MirFunctionBody<'db>,
    site: BlockId,
    callee: &Callee<'_, 'db>,
    binding: &Binding<'db>,
) {
    let callee_body = callee.body;
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
        callee: callee.identity.clone(),
        call_span,
        parent: call_block.inline.clone(),
        native: callee.native,
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
    let return_is_read = super::count_local_uses(callee_body)[0] > 0;
    // With no handler around the call, nothing can see the destination
    // between the callee's first write of its result and its return: a throw
    // leaves the frame. The callee then writes its result straight into the
    // destination, unless a parameter stands for that same local, which the
    // callee may still read after writing its result.
    if let Place::Local(destination) = &destination
        && unwind.is_none()
        && !return_is_read
        && !body.locals[destination.0].is_captured
        && body.locals[destination.0].ty == callee_body.locals[0].ty
        && !local_map[1..].contains(&Some(*destination))
    {
        local_map[0] = Some(*destination);
    }
    let return_local = local_map[0].unwrap_or_else(|| unreachable!("the return place is mapped"));
    let returns_in_place = destination == Place::Local(return_local);

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
            if !callee.cells.is_empty() {
                super::for_each_place_in_statement(statement, &mut |place| {
                    read_through_captured_cell(place, &callee.cells);
                });
            }
            if callee.reads_frame_types {
                memory::for_each_statement_template_mut(&mut statement.kind, &mut |template| {
                    substitute(template, &callee.frame);
                });
            }
        }
        match block.terminator.as_mut() {
            Some(Terminator::Return) => {
                // A value computed just before the return is computed into
                // the call's destination. Nothing runs between the two, so
                // the destination is written exactly when the call's result
                // would have been.
                if returns_in_place {
                    // Already written where the call's result goes.
                } else if !return_is_read
                    && let Some(Statement {
                        kind:
                            StatementKind::Assign {
                                destination: computed,
                                ..
                            },
                        ..
                    }) = block.statements.last_mut()
                    && *computed == Place::Local(return_local)
                {
                    *computed = destination.clone();
                } else {
                    block.statements.push(Statement {
                        kind: StatementKind::Assign {
                            destination: destination.clone(),
                            value: Rvalue::Use(Operand::Copy(Place::Local(return_local))),
                        },
                        span: source.terminator_span,
                    });
                }
                block.terminator = Some(Terminator::Goto { target });
            }
            Some(terminator) => {
                if callee.reads_frame_types {
                    memory::for_each_terminator_template_mut(terminator, &mut |template| {
                        substitute(template, &callee.frame);
                    });
                }
                super::rewrite_locals_in_terminator(terminator, &local_map);
                if !callee.cells.is_empty() {
                    super::for_each_place_in_terminator(terminator, &mut |place| {
                        read_through_captured_cell(place, &callee.cells);
                    });
                }
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

/// Rewrite a place of a closure's body that goes through the closure's
/// capture array to go through the cell the capture holds, as the body the
/// closure was made in names it.
fn read_through_captured_cell(place: &mut Place, cells: &[CellId]) {
    match place {
        Place::Deref(cell) => {
            if let CellId::Capture(index) = cell {
                *cell = cells[*index];
            }
        }
        Place::Field { base, .. } | Place::Index { base, .. } => {
            read_through_captured_cell(base, cells);
        }
        // A closure's body names a capture bare only to hand the cell to a
        // closure of its own, and such a body is never copied.
        Place::Local(_) | Place::Capture(_) => {}
    }
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
pub(super) fn defines(kind: &StatementKind<'_>, local: Local) -> bool {
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
pub(super) fn terminator_defines(terminator: &Terminator<'_>, local: Local) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeTy;

    fn block(
        id: usize,
        statements: Vec<StatementKind<'static>>,
        terminator: Terminator<'static>,
    ) -> BasicBlock<'static> {
        let mut block = BasicBlock::new(BlockId(id));
        block.statements = statements
            .into_iter()
            .map(|kind| Statement { kind, span: None })
            .collect();
        block.terminator = Some(terminator);
        block
    }

    fn goto(target: usize) -> Terminator<'static> {
        Terminator::Goto {
            target: BlockId(target),
        }
    }

    fn fresh(local: usize) -> StatementKind<'static> {
        StatementKind::FreshCell {
            local: Local(local),
            carry_value: false,
        }
    }

    /// `_2 = closure capturing the cell of _1`.
    fn make() -> StatementKind<'static> {
        StatementKind::Assign {
            destination: Place::Local(Local(2)),
            value: Rvalue::MakeClosure {
                lambda_idx: 0,
                captures: vec![Operand::Copy(Place::Local(Local(1)))],
                type_arg_templates: Vec::new(),
            },
        }
    }

    /// `_0 = call _2() -> next`.
    fn call(next: usize) -> Terminator<'static> {
        Terminator::Call {
            has_trace: false,
            argument_layout: Some(baml_type::CallLayout::default()),
            callee: Operand::copy_local(Local(2)),
            args: Vec::new(),
            ntypeargs: 0,
            destination: Place::Local(Local(0)),
            target: BlockId(next),
            unwind: None,
        }
    }

    fn body(blocks: Vec<BasicBlock<'static>>) -> MirFunctionBody<'static> {
        MirFunctionBody {
            blocks,
            entry: BlockId(0),
            locals: (0..3)
                .map(|index| LocalDecl {
                    name: None,
                    ty: RuntimeTy::int(),
                    span: None,
                    scope_span: None,
                    is_captured: index == 1,
                })
                .collect(),
        }
    }

    fn known(body: &MirFunctionBody<'static>, site: usize) -> bool {
        matches!(
            callable_of(body, 0, BlockId(site), Local(2)),
            Some(Callable::Closure(_))
        )
    }

    #[test]
    fn a_closure_called_with_its_captured_cell_still_in_place_is_known() {
        let body = body(vec![
            block(0, vec![fresh(1), make()], goto(1)),
            block(1, vec![], call(2)),
            block(2, vec![], Terminator::Return),
        ]);
        assert!(known(&body, 1));
    }

    #[test]
    fn a_new_cell_between_making_and_calling_hides_the_closure() {
        // Straight after the closure is made, in a block on the way, and
        // right before the call.
        for at in 0..3 {
            let mut blocks = vec![
                block(0, vec![fresh(1), make()], goto(1)),
                block(1, vec![], goto(2)),
                block(2, vec![], call(3)),
                block(3, vec![], Terminator::Return),
            ];
            blocks[at].statements.push(Statement {
                kind: fresh(1),
                span: None,
            });
            assert!(!known(&body(blocks), 2), "new cell in bb{at}");
        }
    }

    #[test]
    fn a_new_cell_each_iteration_before_the_closure_is_made_does_not() {
        // bb1 is a loop body: new cell, new closure, call, around again. The
        // only way from the closure to the new cell runs the `MakeClosure`
        // again before the next call.
        let body = body(vec![
            block(0, vec![], goto(1)),
            block(1, vec![fresh(1), make()], call(2)),
            block(
                2,
                vec![],
                Terminator::Branch {
                    condition: Operand::copy_local(Local(0)),
                    then_block: BlockId(1),
                    else_block: BlockId(3),
                },
            ),
            block(3, vec![], Terminator::Return),
        ]);
        assert!(known(&body, 1));
    }

    #[test]
    fn a_new_cell_on_the_way_around_a_loop_hides_a_closure_made_before_it() {
        // The closure is made once, then bb1 loops: call, new cell, again.
        let body = body(vec![
            block(0, vec![fresh(1), make()], goto(1)),
            block(1, vec![], call(2)),
            block(
                2,
                vec![fresh(1)],
                Terminator::Branch {
                    condition: Operand::copy_local(Local(0)),
                    then_block: BlockId(1),
                    else_block: BlockId(3),
                },
            ),
            block(3, vec![], Terminator::Return),
        ]);
        assert!(!known(&body, 1));
    }
}
