//! Per-function analysis: how each local is held, the edge shape of each
//! block, and what the function calls or suspends on.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::OnceLock,
};

use baml_compiler2_mir::{
    CellId, Constant, IntrinsicOp, MirFunction, MirFunctionBody, Operand, Place, RuntimeTy, Rvalue,
    StatementKind, Terminator, UnaryOp,
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
    /// How each local's slot is held. A captured local's slot holds its
    /// cell, a `Value`; see [`Self::values`] for what the cell holds.
    pub(crate) reprs: Vec<Repr>,
    /// How the value each local names is held: for a captured local, the
    /// value in its cell (what a `Deref` of it reads or writes), otherwise
    /// the slot itself ([`Self::reprs`]).
    pub(crate) values: Vec<Repr>,
    /// For a lambda, how the value behind each of its closure's captures is
    /// held (what `Deref(Capture(i))` reads or writes): the repr of the
    /// enclosing function's local the capture is the cell of.
    pub(crate) captures: Vec<Repr>,
    /// The shape of the body's graph. Only an inline body structures it
    /// (`Generator::function`); a resumable body runs its blocks as a state
    /// machine and needs no structure.
    pub(crate) cfg: Cfg,
    /// Objects of the user functions this function calls directly.
    pub(crate) user_callees: BTreeSet<usize>,
    /// Locals assigned a statically known type (`LoadType` of a template that
    /// needs no frame type arguments), with the `RealizedTy` expression that
    /// builds it. Such a local is only ever a call's type argument.
    pub(crate) static_types: HashMap<usize, StaticTy>,
    /// Locals assigned a type derived from the frame's type arguments
    /// (`LoadType` of a template with a `TypeArgRef`), by the template: a
    /// call's type argument, or a `type` value (`Generator::realized`). None
    /// in a body that binds types, where each is an ordinary binding.
    pub(crate) frame_types: HashMap<usize, baml_compiler2_mir::TyTemplate>,
    /// Whether the body itself suspends (await, spawn, a sys op, or a call
    /// compiled code cannot make directly); callers of suspending functions
    /// are found by the admission fixpoint.
    pub(crate) suspends: bool,
    /// Iterators over an array (`xs.iter()` of a local of array type), by
    /// iterator local → array local. Compiled as an index walk with the
    /// semantics `baml.iter.ArrayIterator` documents.
    pub(crate) array_iters: HashMap<usize, usize>,
    /// Whether a block inside a loop may allocate: the loop's header is where
    /// an inline body checks for a collection.
    pub(crate) loop_allocates: bool,
    /// Whether the body may allocate other than through its calls to user
    /// functions: a recursive function that does checks for a collection on
    /// entry, so the recursion is bounded in memory as a loop is.
    pub(crate) allocates: bool,
}

impl Analysis {
    pub(crate) fn new<'db>(
        db: &'db dyn baml_compiler2_mir::Db,
        index: &ProgramIndex,
        mir: &MirFunction<'db>,
        body: &MirFunctionBody<'db>,
        captures: Vec<Repr>,
    ) -> Result<Self, Rejection> {
        if mir.arity + 1 > body.locals.len() {
            return Err(Rejection::Invalid("fewer locals than parameters".into()));
        }
        let values = value_reprs(body);
        // A captured local's slot holds its cell from its `FreshCell` on (a
        // parameter's from frame entry, where the body wraps it).
        let reprs: Vec<Repr> = values
            .iter()
            .zip(&body.locals)
            .map(|(&value, local)| {
                if local.is_captured {
                    Repr::Value
                } else {
                    value
                }
            })
            .collect();
        let mut static_types = HashMap::new();
        let mut frame_types = HashMap::new();
        for block in &body.blocks {
            for statement in &block.statements {
                let StatementKind::Assign { destination, value } = &statement.kind else {
                    continue;
                };
                if let Rvalue::LoadType(template) = value {
                    let Place::Local(local) = destination else {
                        continue;
                    };
                    if let Some(code) = static_realized(index, template) {
                        let primitive = is_primitive(template);
                        static_types.insert(local.0, StaticTy { code, primitive });
                    } else {
                        // Built where it is used: from its parts when this
                        // backend builds them, else from the constant
                        // bytecode's own `LoadType` reads
                        // (`Generator::realized`).
                        frame_types.insert(local.0, template.clone());
                    }
                }
            }
        }
        // MIR may reuse a type temp for a runtime value; such a local is an
        // ordinary binding, its type reads built from the value it holds.
        let mut written = HashSet::new();
        for block in &body.blocks {
            for statement in &block.statements {
                if let StatementKind::Assign {
                    destination: Place::Local(local),
                    value,
                } = &statement.kind
                    && !matches!(value, Rvalue::LoadType(_))
                {
                    written.insert(local.0);
                }
            }
            if let Some(
                Terminator::Call { destination, .. }
                | Terminator::VirtualCall { destination, .. }
                | Terminator::SysOp { destination, .. }
                | Terminator::Await { destination, .. }
                | Terminator::AwaitAny { destination, .. }
                | Terminator::ShortCircuit { destination, .. },
            ) = &block.terminator
                && let Place::Local(local) = destination
            {
                written.insert(local.0);
            }
        }
        static_types.retain(|local, _| !written.contains(local));
        frame_types.retain(|local, _| !written.contains(local));
        // A body that binds a frame slot (`type T = ..`) can rebind it
        // between a `LoadType` and a read of its local, so each such type is
        // built at its `LoadType`, as the interpreter evaluates it, and held
        // as a `type` value.
        let binds_types = body.blocks.iter().any(|block| {
            block.statements.iter().any(|statement| {
                matches!(
                    statement.kind,
                    StatementKind::Intrinsic {
                        op: IntrinsicOp::BindType(_),
                        ..
                    }
                )
            })
        });
        if binds_types {
            frame_types.clear();
        }

        let mut array_iters = HashMap::new();
        for block in &body.blocks {
            if let Some(terminator) = &block.terminator
                && let Some((iter, array)) = array_iter_call(body, mir.arity, terminator)
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
            let array_iteration = array_iter_call(body, mir.arity, terminator).is_some()
                || array_next_call(terminator, &array_iters).is_some();
            flows.push(flow_of(terminator)?);
            unwinds.push(block.unwind.map(|b| b.0));
            if let Terminator::Call { callee, .. } = terminator
                && let Some(object) = user_callee(db, index, callee)
            {
                user_callees.insert(object);
            }
            // An interface method call resolves its callee at run time, so
            // the body requests it from the VM (`Step::Call`); only array
            // iteration is compiled in place.
            suspends |= suspends_directly(db, index, body, terminator, &static_types)
                || (matches!(terminator, Terminator::VirtualCall { .. }) && !array_iteration);
        }
        let cfg = Cfg {
            entry: body.entry.0,
            flows,
            unwinds,
        };
        let (_, in_loop) = loops(body);
        let loop_allocates = body
            .blocks
            .iter()
            .any(|block| in_loop[block.id.0] && block_allocates(db, index, &reprs, block, true));
        let allocates = body
            .blocks
            .iter()
            .any(|block| block_allocates(db, index, &reprs, block, false));
        Ok(Self {
            arity: mir.arity,
            reprs,
            values,
            captures,
            cfg,
            user_callees,
            static_types,
            frame_types,
            suspends,
            array_iters,
            loop_allocates,
            allocates,
        })
    }

    /// Whether local `i` holds a type (a `LoadType` temp this backend
    /// builds as a `RealizedTy`), with no `Value` of its own.
    pub(crate) fn is_type_local(&self, i: usize) -> bool {
        self.static_types.contains_key(&i) || self.frame_types.contains_key(&i)
    }
}

/// Whether `block` may allocate on the heap: anything but a scalar computed
/// into a scalar local, a value read into a value local, or a branch. Calls
/// count, except calls to user functions unless `user_calls`. Conservative:
/// a check where none was needed costs a counter; a missing one leaves a
/// loop's garbage uncollected.
fn block_allocates(
    db: &dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    reprs: &[Repr],
    block: &baml_compiler2_mir::BasicBlock<'_>,
    user_calls: bool,
) -> bool {
    let statement_allocates = |kind: &StatementKind<'_>| match kind {
        StatementKind::Nop | StatementKind::Drop(_) => false,
        StatementKind::Assign {
            destination: Place::Local(local),
            value,
        } => {
            if reprs[local.0] != Repr::Value {
                !matches!(
                    value,
                    Rvalue::Use(_)
                        | Rvalue::BinaryOp { .. }
                        | Rvalue::UnaryOp { .. }
                        | Rvalue::IsType { .. }
                        | Rvalue::IsTypeTag { .. }
                        | Rvalue::TypeTag(_)
                        | Rvalue::Discriminant(_)
                        | Rvalue::Len(_)
                        | Rvalue::LoadType(_)
                )
            } else {
                !match value {
                    Rvalue::Use(Operand::Copy(place) | Operand::Move(place)) => match place {
                        Place::Local(source) => reprs[source.0] == Repr::Value,
                        _ => true,
                    },
                    Rvalue::Use(Operand::Constant(constant)) => {
                        !matches!(constant, Constant::Float(_))
                    }
                    Rvalue::LoadType(_) => true,
                    _ => false,
                }
            }
        }
        _ => true,
    };
    if block.statements.iter().any(|statement| statement_allocates(&statement.kind)) {
        return true;
    }
    match &block.terminator {
        Some(Terminator::Call { callee, .. }) => {
            user_calls || user_callee(db, index, callee).is_none()
        }
        Some(Terminator::SysOp { .. } | Terminator::Spawn { .. }) => true,
        _ => false,
    }
}

/// How the value each local of `body` names is held: its declared type's
/// repr. A parameter with a default arrives as `OmittedArg` and MIR's
/// prologue tests for that sentinel in the body; such a local stays a
/// `Value` so the test sees the sentinel instead of an unboxed zero. A
/// `bool` local MIR coerces in place (`x = Truthy(x)`, the result of `a &&
/// b` with a non-bool `b`) first holds the uncoerced value, so it is a
/// `Value` too.
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

/// How the value behind each capture of the `lambda_idx`-th lambda of the
/// function whose body is `parent` is held: each capture is the cell of one
/// of the parent's locals (`values`, see [`value_reprs`]) or one of the
/// parent's own captures (`captures`), forwarded.
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
        // The panic case leaves through the block's unwind edge or the
        // function; the edge here is the not-a-panic continuation.
        Terminator::ThrowIfPanic { otherwise, .. } => Flow::Goto(otherwise.0),
    })
}

/// Whether `terminator` suspends whatever it calls: the suspending
/// instructions, a call through a function value, a sys op called as a
/// function, a builtin compiled code cannot call directly, and a call to a
/// pass-through function. A call to any other user function suspends when
/// that function does, which the admission fixpoint decides.
pub(crate) fn suspends_directly<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    body: &MirFunctionBody<'db>,
    terminator: &Terminator<'db>,
    static_types: &HashMap<usize, StaticTy>,
) -> bool {
    match terminator {
        Terminator::SysOp { .. }
        | Terminator::Spawn { .. }
        | Terminator::Await { .. }
        | Terminator::AwaitAny { .. } => true,
        // A `$trace` attachment is applied by the VM's call path, which the
        // call reaches as a request.
        Terminator::Call {
            has_trace: true, ..
        } => true,
        Terminator::Call {
            callee,
            args,
            ntypeargs,
            ..
        } => {
            let Operand::Constant(Constant::Function(func)) = callee else {
                return true;
            };
            // Not in the program, or ambiguous in it: generation rejects the
            // call, so it does not matter what this says.
            let Ok(Some((_, info))) = index.function(db, *func) else {
                return false;
            };
            match info.kind {
                // A pass-through callee holds the call's `$trace` controls
                // and scopes its span activations to its frame, which the
                // VM's call path sets up and ends.
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

/// The object of the user (bytecode) function `callee` names, if it names one.
pub(crate) fn user_callee<'db>(
    db: &'db dyn baml_compiler2_mir::Db,
    index: &ProgramIndex,
    callee: &Operand<'db>,
) -> Option<usize> {
    let Operand::Constant(Constant::Function(func)) = callee else {
        return None;
    };
    let (object, info) = index.function(db, *func).ok().flatten()?;
    matches!(info.kind, FunctionKind::Bytecode).then_some(object)
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

/// What is statically known about a call to a builtin that decides whether
/// compiled code can call it directly; see [`builtin_is_callable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CallShape {
    /// The call has type arguments, all statically known primitives or
    /// arrays of them.
    pub(crate) type_args_primitive: bool,
    /// The call's receiver is a map whose declared key type is primitive:
    /// its keys hash and compare in Rust, never through a user `hash` or
    /// `eq`.
    pub(crate) map_key_primitive: bool,
}

/// The [`CallShape`] of a call with arguments `args`, the first `ntypeargs`
/// of them type arguments.
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
    CallShape {
        type_args_primitive,
        map_key_primitive,
    }
}

/// Whether every value of the map key type `ty` hashes and compares in Rust
/// without yielding: a string, a string literal, or a union of those. The
/// map's operations hash a string key and compare two string keys directly;
/// every other key (an `int` too) hashes through its `baml.Hash`
/// implementation and compares through the structural default, both of
/// which can run BAML code, so the builtin yields.
fn is_primitive_key(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::String | RuntimeTy::Literal(Literal::String(_), _) => true,
        RuntimeTy::Union(members) => members.iter().all(is_primitive_key),
        _ => false,
    }
}

/// The stdlib packages whose `$rust_function` bodies the VM implements
/// (`bex_vm::package_baml::VM_NATIVE_PACKAGES`): the only builtins compiled
/// code can bind by key.
const VM_PACKAGES: &[&str] = &["baml", "testing", "ai", "reflect", "trace"];

/// Whether the VM-owned builtin `key` may yield to run a BAML callback,
/// read off the `//baml:may_yield` directive on its declaration in the
/// stdlib's own source — the same declaration that shapes the builtin's
/// glue, so a builtin that can yield cannot be declared without it. `None`
/// for a key no VM package declares.
fn builtin_may_yield(key: &str) -> Option<bool> {
    static TABLE: OnceLock<HashMap<String, bool>> = OnceLock::new();
    TABLE
        .get_or_init(|| {
            let mut table = HashMap::new();
            for package in VM_PACKAGES {
                let (builtins, _, _) = baml_builtins2_codegen::extract_native_builtins_for(package)
                    .unwrap_or_else(|error| {
                        panic!("the stdlib's `{package}` builtins do not extract: {error}")
                    });
                for builtin in builtins {
                    table.insert(builtin.path, builtin.may_yield);
                }
            }
            table
        })
        .get(key)
        .copied()
}

/// The `baml.Map` methods that yield only to hash or compare keys.
const MAP_KEY_METHODS: &[&str] = &["has", "set", "get", "index", "delete", "get_or_insert"];

/// Whether compiled code can call the VM-owned builtin `key` directly, with
/// a call of `shape`: a builtin that cannot yield always can; one that
/// yields only to run a callback a value of some type might carry can when
/// the static types rule such a value out. Anything else — a builtin that
/// takes a callback, one no VM package implements — is called through the
/// VM as a suspension.
pub(crate) fn builtin_is_callable(key: &str, shape: CallShape) -> bool {
    match builtin_may_yield(key) {
        None => false,
        Some(false) => true,
        // Structural rendering yields only to run a user `to_string`
        // override, which a value of a primitive type cannot contain.
        Some(true) if key == "baml._to_string_default" => shape.type_args_primitive,
        // A map's key operations yield only to run a user `hash` or `eq` on
        // a key; a primitive key has neither (strings hash in Rust, the
        // rest take the structural default).
        Some(true) => key
            .strip_prefix("baml.Map.")
            .is_some_and(|method| MAP_KEY_METHODS.contains(&method) && shape.map_key_primitive),
    }
}

/// A type argument known statically: the `RealizedTy` expression that builds
/// it, and whether it is a primitive (or an array of them), which can hold no
/// user-defined behavior.
#[derive(Clone, Debug)]
pub(crate) struct StaticTy {
    pub(crate) code: String,
    pub(crate) primitive: bool,
}

/// Whether `template` is a primitive (`null` and `never` included: a throws
/// type of `never` is the common type argument of a callback-taking builtin)
/// or an array of primitives.
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

/// The `RealizedTy` expression for a type template that needs no frame type
/// arguments: primitives, declared types, and containers of them.
pub(crate) fn static_realized(
    index: &ProgramIndex,
    template: &baml_compiler2_mir::TyTemplate,
) -> Option<String> {
    realized_expr(index, template, false)
}

/// Whether `template` reads the frame's type arguments (has a `TypeArgRef`).
pub(crate) fn reads_frame(template: &baml_compiler2_mir::TyTemplate) -> bool {
    let mut reads = false;
    template.for_each_type_arg_ref(&mut |_| reads = true);
    reads
}

/// The expression building the `RealizedTy` of `template`, as
/// `TyTemplate::substitute` builds it: a frame reference reads the running
/// function's type argument (`vm.frame_type_arg(depth, n)?`, so only inside
/// a body, with `frame`), and each composite is built from its parts.
/// `None` for a shape this backend does not build (see
/// [`template_blocker`]).
pub(crate) fn realized_expr(
    index: &ProgramIndex,
    template: &baml_compiler2_mir::TyTemplate,
    frame: bool,
) -> Option<String> {
    let part = |part: &baml_compiler2_mir::TyTemplate| realized_expr(index, part, frame);
    let parts = |parts: &[baml_compiler2_mir::TyTemplate]| {
        parts
            .iter()
            .map(part)
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.join(", "))
    };
    Some(match template {
        TyTemplate::Int => "RealizedTy::Int".to_string(),
        TyTemplate::Float => "RealizedTy::Float".to_string(),
        TyTemplate::Bool => "RealizedTy::Bool".to_string(),
        TyTemplate::String => "RealizedTy::String".to_string(),
        TyTemplate::Null => "RealizedTy::Null".to_string(),
        TyTemplate::Unknown => "RealizedTy::unknown()".to_string(),
        TyTemplate::Never => "RealizedTy::Never".to_string(),
        TyTemplate::TypeArgRef(slot) => {
            if !frame {
                return None;
            }
            format!("vm.frame_type_arg(depth, {slot})?")
        }
        TyTemplate::List(element) => format!("RealizedTy::List(Box::new({}))", part(element)?),
        TyTemplate::Map { key, value } => format!(
            "RealizedTy::Map {{ key: Box::new({}), value: Box::new({}) }}",
            part(key)?,
            part(value)?
        ),
        TyTemplate::Union(members) => format!("RealizedTy::Union(Box::new([{}]))", parts(members)?),
        TyTemplate::Class(name, args) => {
            let (object, _) = index.class_at(name).ok().flatten()?;
            if args.is_empty() {
                format!("vm.class_type(vm.idx_to_ptr(ObjectIndex::from_raw({object})))")
            } else {
                format!(
                    "vm.class_type_at(vm.idx_to_ptr(ObjectIndex::from_raw({object})), Box::new([{}]))",
                    parts(args)?
                )
            }
        }
        TyTemplate::Enum(name) => {
            let object = index.enum_at(name).ok().flatten()?;
            format!("vm.enum_type(vm.idx_to_ptr(ObjectIndex::from_raw({object})))")
        }
        // The runtime boundary normalizes a literal's freshness to `Regular`.
        TyTemplate::Literal(literal, _) => {
            let literal = match literal {
                Literal::Int(n) => format!("Literal::Int({n}i64)"),
                Literal::Bool(b) => format!("Literal::Bool({b})"),
                Literal::String(s) => format!("Literal::String(String::from({s:?}))"),
                Literal::Float(f) => format!("Literal::Float(String::from({f:?}))"),
                Literal::Bigint(_) => return None,
            };
            format!(
                "RealizedTy::Literal(bex_vm::compiled_rt::{literal}, bex_vm::compiled_rt::Freshness::Regular)"
            )
        }
        _ => return None,
    })
}

/// The first leaf of `template` this backend does not build, for the
/// rejection naming it.
pub(crate) fn template_blocker(
    index: &ProgramIndex,
    template: &baml_compiler2_mir::TyTemplate,
) -> &'static str {
    fn blocker_of(
        index: &ProgramIndex,
        template: &baml_compiler2_mir::TyTemplate,
    ) -> Option<&'static str> {
        let parts = |parts: &[baml_compiler2_mir::TyTemplate]| {
            parts.iter().find_map(|part| blocker_of(index, part))
        };
        Some(match template {
            TyTemplate::List(element) => return blocker_of(index, element),
            TyTemplate::Map { key, value } => {
                return blocker_of(index, key).or_else(|| blocker_of(index, value));
            }
            TyTemplate::Union(members) => return parts(members),
            TyTemplate::Class(name, args) => {
                if index.class_at(name).ok().flatten().is_none() {
                    return Some("a class not in the program");
                }
                return parts(args);
            }
            TyTemplate::Enum(name) => {
                if index.enum_at(name).ok().flatten().is_none() {
                    return Some("an enum not in the program");
                }
                return None;
            }
            TyTemplate::Interface(..) => "an interface type",
            TyTemplate::Function { .. } => "a function type",
            TyTemplate::Future(..) => "a future type",
            TyTemplate::AssociatedTypeProjection { .. } => "a projection",
            TyTemplate::TypeAlias(_) => "a recursive alias",
            TyTemplate::EnumVariant(..) => "an enum variant type",
            TyTemplate::Literal(Literal::Bigint(_), _) => "a bigint literal type",
            TyTemplate::Bigint
            | TyTemplate::Uint8Array
            | TyTemplate::Media(_)
            | TyTemplate::RustType
            | TyTemplate::Type
            | TyTemplate::Resource
            | TyTemplate::PromptAst => "an opaque type",
            _ => return None,
        })
    }
    blocker_of(index, template).unwrap_or("a type this backend does not build")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIRECT: CallShape = CallShape {
        type_args_primitive: false,
        map_key_primitive: false,
    };

    /// The allowlist is the stdlib's own `//baml:may_yield` directives: a
    /// builtin without one is callable, one with it only when the static
    /// types rule its callbacks out, and a key no VM package declares never.
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
        };
        assert!(builtin_is_callable(
            "baml._to_string_default",
            primitive_types
        ));
        assert!(!builtin_is_callable("baml._to_string_default", DIRECT));
        let primitive_keys = CallShape {
            type_args_primitive: false,
            map_key_primitive: true,
        };
        for method in MAP_KEY_METHODS {
            let key = format!("baml.Map.{method}");
            assert_eq!(builtin_may_yield(&key), Some(true), "{key}");
            assert!(builtin_is_callable(&key, primitive_keys), "{key}");
            assert!(!builtin_is_callable(&key, DIRECT), "{key}");
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
