//! Per-function analysis: how each local is held, the edge shape of each
//! block, and what the function calls or suspends on.

use std::collections::{BTreeSet, HashMap};

use baml_compiler2_mir::{
    Constant, MirFunction, MirFunctionBody, Operand, Place, Rvalue, StatementKind, SwitchKey,
    Terminator, function_display_name,
};
use baml_type::{Literal, TyTemplate};
use bex_vm_types::FunctionKind;

use crate::{
    admit::{ProgramIndex, Rejection},
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

pub(crate) struct Analysis {
    pub(crate) arity: usize,
    pub(crate) reprs: Vec<Repr>,
    /// The shape of the body's graph. Only an inline body structures it
    /// (`Generator::function`); a resumable body runs its blocks as a state
    /// machine and needs no structure.
    pub(crate) cfg: Cfg,
    /// Link names of the user functions this function calls directly.
    pub(crate) user_callees: BTreeSet<String>,
    /// Locals assigned a statically known type (`LoadType` of a template that
    /// needs no frame type arguments), with the `RealizedTy` expression that
    /// builds it. Such a local is only ever a call's type argument.
    pub(crate) static_types: HashMap<usize, StaticTy>,
    /// Whether the body itself suspends (await, spawn, a sys op, or a call
    /// compiled code cannot make directly); callers of suspending functions
    /// are found by the admission fixpoint.
    pub(crate) suspends: bool,
    /// Iterators over an array (`xs.iter()` of a local of array type), by
    /// iterator local → array local. Compiled as an index walk with the
    /// semantics `baml.iter.ArrayIterator` documents.
    pub(crate) array_iters: HashMap<usize, usize>,
}

impl Analysis {
    pub(crate) fn new<'db>(
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
        Ok(Self {
            arity: mir.arity,
            reprs,
            cfg,
            user_callees,
            static_types,
            suspends,
            array_iters,
        })
    }
}

/// `xs.iter()` on a local of array type, as `(iterator local, array local)`.
pub(crate) fn array_iter_call(
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
pub(crate) fn array_next_call(
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
pub(crate) fn suspends_directly<'db>(
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

pub(crate) fn user_callee_name<'db>(
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

/// `body`'s loops: the targets of its back edges (edges to a block on the
/// depth-first path from the entry, which for structured source are loop
/// headers), and whether each block lies in some loop (a header, or a block
/// that reaches a back edge's source without passing its header).
pub(crate) fn loops(body: &MirFunctionBody<'_>) -> (BTreeSet<usize>, Vec<bool>) {
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

/// VM-owned builtins that never yield to run a callback, by `native_key`.
/// Anything not listed is rejected: a yielding builtin cannot run inside a
/// compiled body. `primitive_types` says the call has type arguments, all
/// statically known primitives or arrays of them.
pub(crate) fn builtin_is_callable(key: &str, primitive_types: bool) -> bool {
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

/// A type argument known statically: the `RealizedTy` expression that builds
/// it, and whether it is a primitive (or an array of them), which can hold no
/// user-defined behavior.
#[derive(Clone, Debug)]
pub(crate) struct StaticTy {
    pub(crate) code: String,
    pub(crate) primitive: bool,
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
pub(crate) fn static_realized(
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
