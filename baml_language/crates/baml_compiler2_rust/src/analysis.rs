//! Per-function analysis: how each local is held, the edge shape of each
//! block, and what the function calls or suspends on.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
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
    /// Deferred type locals; empty when the body can rebind frame types.
    pub(crate) frame_types: HashMap<usize, baml_compiler2_mir::TyTemplate>,
    /// Direct suspension; admission propagates it through user calls.
    pub(crate) suspends: bool,
    /// Iterator local to array local for iteration compiled as an index walk.
    pub(crate) array_iters: BTreeMap<usize, usize>,
    /// The body has a loop, which polls for a yield at its back edge.
    pub(crate) loops: bool,
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
        let reprs = slot_reprs(body, &values);
        let TypeLocals {
            static_types,
            frame_types,
        } = type_locals(index, body);
        let array_iters = array_iterators(body, mir.arity);
        let BodyFlow {
            cfg,
            user_callees,
            suspends,
        } = body_flow(db, index, body, mir.arity, &static_types, &array_iters)?;
        let loops = !loops(body).0.is_empty();
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
            loops,
        })
    }

    /// Whether the body may suspend on its own, before its callees are known.
    pub(crate) fn pauses(&self) -> bool {
        self.suspends || self.loops
    }

    /// Type locals have no runtime `Value` binding.
    pub(crate) fn is_type_local(&self, i: usize) -> bool {
        self.static_types.contains_key(&i) || self.frame_types.contains_key(&i)
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

struct TypeLocals {
    static_types: HashMap<usize, StaticTy>,
    frame_types: HashMap<usize, baml_compiler2_mir::TyTemplate>,
}

fn type_locals(index: &ProgramIndex, body: &MirFunctionBody<'_>) -> TypeLocals {
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
                    frame_types.insert(local.0, template.clone());
                }
            }
        }
    }
    // Reused type temporaries need ordinary value bindings.
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
    // Frame types can change between a LoadType and its use.
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

    TypeLocals {
        static_types,
        frame_types,
    }
}

fn array_iterators(body: &MirFunctionBody<'_>, arity: usize) -> BTreeMap<usize, usize> {
    let mut array_iters = BTreeMap::new();
    for block in &body.blocks {
        if let Some(terminator) = &block.terminator
            && let Some((iter, array)) = array_iter_call(body, arity, terminator)
        {
            array_iters.insert(iter, array);
        }
    }

    array_iters
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
    array_iters: &BTreeMap<usize, usize>,
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
        let array_iteration = array_iter_call(body, arity, terminator).is_some()
            || array_next_call(terminator, array_iters).is_some();
        flows.push(flow_of(terminator));
        unwinds.push(block.unwind.map(|b| b.0));
        if let Terminator::Call { callee, .. } = terminator
            && let Some(object) = user_callee(db, index, callee)
        {
            user_callees.insert(object);
        }
        // Virtual calls need VM dispatch except for array iteration.
        suspends |= suspends_directly(db, index, body, terminator, static_types)
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

/// `it.next()` on an array iterator, as the iterator local.
pub(crate) fn array_next_call(
    terminator: &Terminator<'_>,
    array_iters: &BTreeMap<usize, usize>,
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
        // The panic case leaves through the block's unwind edge or the
        // function; the edge here is the not-a-panic continuation.
        Terminator::ThrowIfPanic { otherwise, .. } => Flow::Goto(otherwise.0),
    }
}

/// Direct suspension excludes user callees, handled by the admission fixpoint.
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
        // Trace attachments require the VM's call path.
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
            // Generation rejects missing or ambiguous callees.
            let Ok(Some((_, info))) = index.function(db, *func) else {
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
    let (object, info) = index.function(db, *func).ok().flatten()?;
    matches!(info.kind, FunctionKind::Bytecode).then_some(object)
}

/// Return back-edge targets and loop membership for each block.
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

/// Static constraints that can rule out builtin callbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CallShape {
    /// Nonempty type arguments containing only primitives or arrays of them.
    pub(crate) type_args_primitive: bool,
    /// Map keys hash and compare without user code.
    pub(crate) map_key_primitive: bool,
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
    CallShape {
        type_args_primitive,
        map_key_primitive,
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

// These map methods yield only to hash or compare keys.
const MAP_KEY_METHODS: &[&str] = &["has", "set", "get", "index", "delete", "get_or_insert"];

/// Yielding builtins need static types that exclude their callbacks.
pub(crate) fn builtin_is_callable(key: &str, shape: CallShape) -> bool {
    match builtin_may_yield(key) {
        None => false,
        Some(false) => true,
        Some(true) if PRIMITIVE_TYPE_BUILTINS.contains(&key) => shape.type_args_primitive,
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

/// Frame references require a running function; unsupported templates return `None`.
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
