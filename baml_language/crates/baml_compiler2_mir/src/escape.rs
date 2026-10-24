//! Which heap objects another thread may reach.
//!
//! BEP-034 `spawn` lets several VMs share one heap, so every container access
//! takes a lock and every field access is atomic. An object only this thread
//! can reach needs neither. This analysis finds those objects, per function
//! body, so each backend can pick the unsynchronized accessor at an access
//! site whose object is *thread-local*.
//!
//! An object is thread-local when it was allocated by this invocation and no
//! reference to it can have left the invocation's thread: it never reached a
//! call the analysis cannot see through, a spawn, a sys-op, a cell, a closure,
//! the runtime, or a heap object that is itself not thread-local. Returning it
//! is not a leak: the invocation is over by then, and the caller sees an
//! unknown call result, which is never thread-local.
//!
//! The model is unification-based points-to (Steensgaard): one abstract node
//! per local, merged by every copy, with one `contents` node standing for
//! every field and element of the objects a node holds. Field- and
//! flow-insensitive, so linear in the size of the body. A node is *shared*
//! when anything it holds may be reachable from another thread, or did not
//! come from an allocation this analysis saw; sharing flows from a node to its
//! contents. Everything the analysis cannot name is shared, so a missed case
//! costs a lock, never a race.

use baml_base::{LangPackage, SourceRoot};
use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_hir_ty::extern_loc::FunctionRef;
use baml_type::Literal;
use rustc_hash::FxHashMap;

use crate::{
    Constant, IntrinsicOp, Local, MirFunction, MirFunctionBody, MirFunctionKind, Operand, OptLevel,
    Place, RuntimeTy, Rvalue, StatementKind, Terminator,
};

/// The thread-locality of every object a body names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapeFacts {
    /// Per block: whether its terminator calls a builtin's thread-local twin.
    local_calls: Vec<bool>,
    /// The node each local's value belongs to.
    local_node: Vec<usize>,
    /// Per node: whether another thread may reach what it holds.
    shared: Vec<bool>,
    /// Per node: the node every field and element of its objects belongs to.
    contents: Vec<Option<usize>>,
}

impl EscapeFacts {
    /// Whether `block`'s terminator is a builtin call whose receiver is
    /// thread-local, so it takes the builtin's lock-free twin.
    pub fn calls_thread_local_twin(&self, block: crate::BlockId) -> bool {
        self.local_calls.get(block.0).copied().unwrap_or(false)
    }

    /// Whether the object `place` evaluates to is thread-local.
    pub fn object_is_thread_local(&self, place: &Place) -> bool {
        self.object_node(place)
            .is_some_and(|node| !self.shared[node])
    }

    /// Whether the object a field or element access goes through is
    /// thread-local: the base of `place`. `false` for a place that is not a
    /// projection.
    pub fn base_is_thread_local(&self, place: &Place) -> bool {
        match place {
            Place::Field { base, .. } | Place::Index { base, .. } => {
                self.object_is_thread_local(base)
            }
            Place::Local(_) | Place::Capture(_) | Place::Deref(_) => false,
        }
    }

    /// Whether every object `local` can hold is thread-local.
    pub fn local_is_thread_local(&self, local: Local) -> bool {
        self.object_is_thread_local(&Place::Local(local))
    }

    /// Whether every object held in a field or element of `local`'s objects
    /// is thread-local: an array iterator's array, for one.
    pub fn contents_are_thread_local(&self, local: Local) -> bool {
        self.local_node
            .get(local.0)
            .and_then(|&node| self.contents[node])
            .is_some_and(|contents| !self.shared[contents])
    }

    fn object_node(&self, place: &Place) -> Option<usize> {
        match place {
            Place::Local(local) => self.local_node.get(local.0).copied(),
            Place::Field { base, .. } | Place::Index { base, .. } => {
                self.contents[self.object_node(base)?]
            }
            Place::Capture(_) | Place::Deref(_) => None,
        }
    }
}

/// Classify every object `body` names. `arity` is the parameter count;
/// parameters are `_1..=_arity`. Calls to functions with a summary
/// ([`escape_summary`]) are seen through, at `opt`.
pub fn analyze<'db>(
    db: &'db dyn crate::Db,
    body: &MirFunctionBody<'db>,
    arity: usize,
    opt: OptLevel,
) -> EscapeFacts {
    let types = Types::new(db, body);
    let mut graph = build(db, body, opt, None, &types);
    // A parameter's argument may already be shared, as far as this body knows.
    for local in 1..=arity.min(body.locals.len().saturating_sub(1)) {
        graph.mark_shared(local);
    }
    let mut facts = graph.finish();
    let builtins = Builtins::new(db);
    facts.local_calls = body
        .blocks
        .iter()
        .map(|block| {
            let Some(Terminator::Call {
                has_trace: false,
                argument_layout,
                callee,
                args,
                ntypeargs,
                ..
            }) = &block.terminator
            else {
                return false;
            };
            argument_layout
                .as_ref()
                .is_none_or(|layout| layout.0.iter().all(Option::is_none))
                && builtins.effect(callee).is_some_and(|effect| effect.twin)
                && matches!(
                    args.get(*ntypeargs),
                    Some(Operand::Copy(receiver) | Operand::Move(receiver))
                        if facts.object_is_thread_local(receiver)
                )
        })
        .collect();
    facts
}

/// What calling `function` does with its arguments and result, for callers:
/// its graph, restricted to what its parameters and return value reach, with
/// parameters assumed thread-local. `None` when the analysis cannot see the
/// body (a builtin, or lowering failed), so callers treat the call as opaque.
#[salsa::tracked(returns(ref), cycle_result = escape_summary_cycle)]
pub fn escape_summary<'db>(
    db: &'db dyn crate::Db,
    function: FunctionLoc<'db>,
    opt: OptLevel,
) -> Option<Summary> {
    let mir = crate::lower_function(db, function, opt).as_ref().ok()?;
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return None;
    };
    let types = Types::new(db, body);
    Some(build(db, body, opt, Some(function), &types).summary(mir.arity))
}

/// A cycle [`recursive_functions`] did not see (a runtime-compiled
/// submission): opaque, which is always sound.
fn escape_summary_cycle<'db>(
    _db: &'db dyn crate::Db,
    _id: salsa::Id,
    _function: FunctionLoc<'db>,
    _opt: OptLevel,
) -> Option<Summary> {
    None
}

/// The functions of `root` that can reach themselves through direct calls,
/// by strongly connected component. A call within one component is opaque
/// to [`escape_summary`], so summaries never depend on themselves.
#[salsa::tracked(returns(ref))]
pub fn recursive_functions<'db>(
    db: &'db dyn crate::Db,
    root: SourceRoot,
    opt: OptLevel,
) -> FxHashMap<FunctionLoc<'db>, usize> {
    let mut functions = Vec::new();
    for &file in root.files(db) {
        functions.extend(
            baml_compiler2_hir::item_data::file_functions(db, file)
                .iter()
                .copied(),
        );
    }
    let index: FxHashMap<FunctionLoc<'db>, usize> = functions
        .iter()
        .enumerate()
        .map(|(i, &function)| (function, i))
        .collect();
    let edges: Vec<Vec<usize>> = functions
        .iter()
        .map(|&function| {
            let mut callees = Vec::new();
            if let Ok(mir) = crate::lower_function(db, function, opt) {
                direct_callees(mir, &mut |callee| {
                    if let Some(&callee) = index.get(&callee) {
                        callees.push(callee);
                    }
                });
            }
            callees
        })
        .collect();
    strongly_connected(&edges)
        .into_iter()
        .enumerate()
        .filter(|(_, component)| component.len() > 1 || edges[component[0]].contains(&component[0]))
        .flat_map(|(id, component)| {
            let functions = &functions;
            component.into_iter().map(move |f| (functions[f], id))
        })
        .collect()
}

/// Every source function `function`'s body or lambdas call directly.
fn direct_callees<'db>(function: &MirFunction<'db>, visit: &mut impl FnMut(FunctionLoc<'db>)) {
    if let MirFunctionKind::Bytecode(body) = &function.kind {
        for block in &body.blocks {
            if let Some(Terminator::Call { callee, .. }) = &block.terminator
                && let Some(DeclRef::Source(callee)) = static_callee(callee)
            {
                visit(callee);
            }
        }
    }
    for lambda in &function.lambdas {
        direct_callees(lambda, visit);
    }
}

fn static_callee<'db>(callee: &Operand<'db>) -> Option<FunctionRef<'db>> {
    match callee {
        Operand::Constant(Constant::Function(func) | Constant::GenericFunction { func, .. }) => {
            Some(*func)
        }
        _ => None,
    }
}

/// Tarjan's algorithm, iteratively: the strongly connected components of
/// `edges`, each as its node indices.
fn strongly_connected(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    const UNVISITED: usize = usize::MAX;
    let mut order = vec![UNVISITED; edges.len()];
    let mut low = vec![0; edges.len()];
    let mut on_stack = vec![false; edges.len()];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    let mut next = 0;
    for start in 0..edges.len() {
        if order[start] != UNVISITED {
            continue;
        }
        // (node, index of its next edge to explore)
        let mut work = vec![(start, 0)];
        while let Some(&mut (node, ref mut edge)) = work.last_mut() {
            if *edge == 0 && order[node] == UNVISITED {
                order[node] = next;
                low[node] = next;
                next += 1;
                stack.push(node);
                on_stack[node] = true;
            }
            if let Some(&target) = edges[node].get(*edge) {
                *edge += 1;
                if order[target] == UNVISITED {
                    work.push((target, 0));
                } else if on_stack[target] {
                    low[node] = low[node].min(order[target]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low[parent] = low[parent].min(low[node]);
            }
            if low[node] == order[node] {
                let mut component = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                components.push(component);
            }
        }
    }
    components
}

/// The constraint graph of one body, before its parameters are classified.
fn build<'a, 'db>(
    db: &'db dyn crate::Db,
    body: &'a MirFunctionBody<'db>,
    opt: OptLevel,
    current: Option<FunctionLoc<'db>>,
    types: &'a Types<'a, 'db>,
) -> Graph<'a, 'db> {
    let mut graph = Graph::new(body.locals.len(), types);
    let builtins = Builtins::new(db);
    let array_iterators = array_iterators(body);
    let walker = Walker {
        db,
        opt,
        current,
        types,
        builtins: &builtins,
        array_iterators: &array_iterators,
    };
    graph.cause = Reason::Captured;
    for (index, decl) in body.locals.iter().enumerate() {
        // A captured local's slot holds its cell, which closures share.
        if decl.is_captured {
            graph.mark_shared(index);
        }
    }
    graph.cause = Reason::Handler;
    for (_, landing) in body.handlers() {
        // The VM writes these: an in-flight error and its context.
        graph.mark_shared(landing.error_local.0);
        graph.mark_shared(landing.context_local.0);
    }
    for (block_index, block) in body.blocks.iter().enumerate() {
        for (statement_index, statement) in block.statements.iter().enumerate() {
            graph.cause = Reason::At {
                block: block_index,
                statement: Some(statement_index),
            };
            graph.statement(&statement.kind);
        }
        if let Some(terminator) = &block.terminator {
            graph.cause = Reason::At {
                block: block_index,
                statement: None,
            };
            walker.terminator(&mut graph, terminator);
        }
    }
    graph.cause = Reason::Parameter;
    graph
}

/// Why an object is shared, for the coverage report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reason {
    Parameter,
    Captured,
    Handler,
    Constant,
    Cell,
    /// The statement (or, with no statement, the terminator) of a block.
    At {
        block: usize,
        statement: Option<usize>,
    },
}

/// One container or field access in a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub kind: AccessKind,
    /// Whether the accessed object is thread-local.
    pub thread_local: bool,
    /// For a shared object, what made it shared.
    pub shared_because: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AccessKind {
    /// `xs[i]`, read or written.
    Element,
    /// `xs.length()` lowered to a length read.
    Length,
    /// `x.field`, read or written.
    Field,
    /// A call to a builtin array or map method on a receiver.
    ContainerMethod,
    /// A step of `for` over an array.
    Iteration,
}

/// Every container and field access in `body`, classified as [`analyze`]
/// classifies it, with the reason for each shared one. For measuring the
/// analysis, not for compiling.
pub fn coverage<'db>(
    db: &'db dyn crate::Db,
    body: &MirFunctionBody<'db>,
    arity: usize,
    opt: OptLevel,
) -> Vec<Access> {
    let types = Types::new(db, body);
    let mut graph = build(db, body, opt, None, &types);
    for local in 1..=arity.min(body.locals.len().saturating_sub(1)) {
        graph.mark_shared(local);
    }
    let resolved = graph.resolve();
    let locals = graph.locals;
    let facts = EscapeFacts {
        local_calls: Vec::new(),
        local_node: resolved.roots[..locals].to_vec(),
        shared: resolved.shared.clone(),
        contents: resolved.contents.clone(),
    };
    let builtins = Builtins::new(db);
    let array_iterators = array_iterators(body);
    let mut accesses = Vec::new();
    let mut record = |kind: AccessKind, node: Option<usize>| {
        let thread_local = node.is_some_and(|node| !facts.shared[node]);
        let shared_because = (!thread_local).then(|| match node {
            Some(node) => describe(db, body, opt, resolved.reasons[node]),
            None => "not an object the analysis tracks".to_string(),
        });
        accesses.push(Access {
            kind,
            thread_local,
            shared_because,
        });
    };
    let place = |place: &Place, record: &mut dyn FnMut(AccessKind, Option<usize>)| {
        projections(place, &mut |kind, base| {
            record(kind, facts.object_node(base));
        });
    };
    for block in &body.blocks {
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign { destination, value } => {
                    place(destination, &mut record);
                    crate::memory::walk_rvalue_places(value, &mut |read| place(read, &mut record));
                    if let Rvalue::Len(container) = value {
                        record(AccessKind::Length, facts.object_node(container));
                    }
                }
                StatementKind::Drop(read) => place(read, &mut record),
                StatementKind::Intrinsic { args, .. } => {
                    for arg in args {
                        crate::memory::walk_operand_places(arg, &mut |read| {
                            place(read, &mut record);
                        });
                    }
                }
                StatementKind::VirtualFieldStore {
                    receiver, value, ..
                } => {
                    for operand in [receiver, value] {
                        crate::memory::walk_operand_places(operand, &mut |read| {
                            place(read, &mut record);
                        });
                    }
                }
                StatementKind::FreshCell { .. } | StatementKind::Nop => {}
            }
        }
        let Some(terminator) = &block.terminator else {
            continue;
        };
        let (operands, destinations) = terminator_places(terminator);
        for operand in operands {
            crate::memory::walk_operand_places(operand, &mut |read| place(read, &mut record));
        }
        for destination in destinations {
            place(destination, &mut record);
        }
        match terminator {
            Terminator::Call {
                callee,
                args,
                ntypeargs,
                ..
            } if builtins.effect(callee).is_some() => {
                let receiver = args.get(*ntypeargs).and_then(|receiver| match receiver {
                    Operand::Copy(receiver) | Operand::Move(receiver) => {
                        facts.object_node(receiver)
                    }
                    Operand::Constant(_) => None,
                });
                record(AccessKind::ContainerMethod, receiver);
            }
            Terminator::VirtualCall {
                method,
                args,
                destination,
                ..
            } if method == "next" => {
                if let Some((iterator, _)) = unary_local_call(args, destination)
                    && array_iterators[iterator]
                {
                    let array = facts.local_node[iterator];
                    record(AccessKind::Iteration, facts.contents[array]);
                }
            }
            _ => {}
        }
    }
    accesses
}

/// Each field or element access within `place`, with the place of the object
/// it goes through.
fn projections(place: &Place, visit: &mut impl FnMut(AccessKind, &Place)) {
    match place {
        Place::Field { base, .. } => {
            visit(AccessKind::Field, base);
            projections(base, visit);
        }
        Place::Index { base, kind, .. } => {
            if matches!(kind, crate::IndexKind::Array) {
                visit(AccessKind::Element, base);
            }
            projections(base, visit);
        }
        Place::Local(_) | Place::Capture(_) | Place::Deref(_) => {}
    }
}

/// The operands a terminator reads and the places it writes.
fn terminator_places<'a, 'db>(
    terminator: &'a Terminator<'db>,
) -> (Vec<&'a Operand<'db>>, Vec<&'a Place>) {
    match terminator {
        Terminator::Goto { .. } | Terminator::Return | Terminator::Unreachable => {
            (Vec::new(), Vec::new())
        }
        Terminator::Branch { condition, .. } => (vec![condition], Vec::new()),
        Terminator::Switch { discriminant, .. } => (vec![discriminant], Vec::new()),
        Terminator::NarrowBind { source, .. } => (vec![source], Vec::new()),
        Terminator::ShortCircuit {
            operand,
            destination,
            ..
        } => (vec![operand], vec![destination]),
        Terminator::Call {
            callee,
            args,
            destination,
            ..
        }
        | Terminator::SysOp {
            callee,
            args,
            destination,
            ..
        } => (
            std::iter::once(callee).chain(args).collect(),
            vec![destination],
        ),
        Terminator::VirtualCall {
            args, destination, ..
        } => (args.iter().collect(), vec![destination]),
        Terminator::Spawn { plan, future, .. } => (vec![plan], vec![future]),
        Terminator::Await {
            future,
            destination,
            ..
        } => (Vec::new(), vec![future, destination]),
        Terminator::AwaitAny {
            futures,
            destination,
            ..
        } => (vec![futures], vec![destination]),
        Terminator::Throw { value } => (vec![value], Vec::new()),
        Terminator::Rethrow { value, context } => (vec![value, context], Vec::new()),
    }
}

fn describe<'db>(
    db: &'db dyn crate::Db,
    body: &MirFunctionBody<'db>,
    opt: OptLevel,
    reason: Reason,
) -> String {
    let (block, statement) = match reason {
        Reason::Parameter => return "parameter".into(),
        Reason::Captured => return "captured by a closure".into(),
        Reason::Handler => return "caught error".into(),
        Reason::Constant => return "constant or global".into(),
        Reason::Cell => return "read from a cell".into(),
        Reason::At { block, statement } => (block, statement),
    };
    let block = &body.blocks[block];
    if let Some(statement) = statement {
        return match &block.statements[statement].kind {
            StatementKind::Intrinsic {
                op: IntrinsicOp::Log(_),
                ..
            } => "logged".into(),
            StatementKind::Intrinsic { .. } => "trace hook".into(),
            StatementKind::FreshCell { .. } => "captured by a closure".into(),
            StatementKind::Assign {
                value: Rvalue::MakeClosure { .. },
                ..
            } => "closure".into(),
            StatementKind::Assign {
                value: Rvalue::MakeBoundMethod { .. } | Rvalue::MakeVirtualBoundMethod { .. },
                ..
            } => "bound method".into(),
            StatementKind::Assign { .. }
            | StatementKind::Drop(_)
            | StatementKind::VirtualFieldStore { .. }
            | StatementKind::Nop => "runtime value".into(),
        };
    }
    let name = |callee: &Operand<'db>| match static_callee(callee) {
        Some(func) => crate::function_display_name(db, func),
        None => "a function value".into(),
    };
    match block.terminator.as_ref() {
        Some(Terminator::Call { callee, .. }) => match static_callee(callee) {
            Some(DeclRef::Source(loc)) if escape_summary(db, loc, opt).is_some() => {
                format!("leaked by {}", name(callee))
            }
            _ => format!("opaque call {}", name(callee)),
        },
        Some(Terminator::VirtualCall { method, .. }) => format!("virtual call .{method}"),
        Some(Terminator::SysOp { callee, .. }) => format!("sys-op {}", name(callee)),
        Some(Terminator::Spawn { .. }) => "spawn".into(),
        Some(Terminator::Await { .. } | Terminator::AwaitAny { .. }) => "await".into(),
        Some(Terminator::Throw { .. } | Terminator::Rethrow { .. }) => "throw".into(),
        _ => "runtime value".into(),
    }
}

/// A callee's graph as seen from its call sites: the nodes its parameters
/// and return value reach, with sharing and contents edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Per node: whether another thread may reach it, and its contents node.
    nodes: Vec<(bool, Option<usize>)>,
    /// The node of each parameter, in declaration order.
    params: Vec<usize>,
    /// The node of the return value.
    ret: usize,
}

/// Static types of a body's places, as far as escape analysis needs them.
struct Types<'a, 'db> {
    db: &'db dyn crate::Db,
    body: &'a MirFunctionBody<'db>,
    /// Per class-typed local: its class's field types, by slot.
    fields: std::cell::RefCell<FxHashMap<usize, Option<Vec<baml_type::Ty>>>>,
}

impl<'a, 'db> Types<'a, 'db> {
    fn new(db: &'db dyn crate::Db, body: &'a MirFunctionBody<'db>) -> Self {
        Self {
            db,
            body,
            fields: std::cell::RefCell::default(),
        }
    }

    /// Whether every value `place` can hold is immutable: a scalar, string,
    /// bigint, enum or type, which holds no reference to anything mutable.
    fn immutable(&self, place: &Place) -> bool {
        match place {
            // A captured local's slot holds its cell.
            Place::Local(local) => {
                let decl = &self.body.locals[local.0];
                !decl.is_captured && immutable(&decl.ty)
            }
            Place::Deref(crate::CellId::Local(local)) => immutable(&self.body.locals[local.0].ty),
            Place::Field { base, field } => self
                .field_ty(base, *field)
                .is_some_and(|ty| immutable_ty(&ty)),
            Place::Index { base, .. } => match self.local_ty(base) {
                Some(RuntimeTy::List(element)) => immutable(element),
                Some(RuntimeTy::Map { value, .. }) => immutable(value),
                _ => false,
            },
            Place::Capture(_) | Place::Deref(crate::CellId::Capture(_)) => false,
        }
    }

    /// Whether the map `place` holds has primitive keys, which hash and
    /// compare without running user code.
    fn primitive_map_keys(&self, place: &Place) -> bool {
        match place {
            Place::Field { base, field } => self.field_ty(base, *field).is_some_and(
                |ty| matches!(ty, baml_type::Ty::Map { key, .. } if primitive_ty(&key)),
            ),
            _ => {
                matches!(self.local_ty(place), Some(RuntimeTy::Map { key, .. }) if is_primitive(key))
            }
        }
    }

    fn local_ty(&self, place: &Place) -> Option<&'a RuntimeTy> {
        match place {
            Place::Local(local) if !self.body.locals[local.0].is_captured => {
                Some(&self.body.locals[local.0].ty)
            }
            _ => None,
        }
    }

    /// The declared type of slot `field` of the class `base` holds, when
    /// `base` is a local of a known class.
    fn field_ty(&self, base: &Place, field: usize) -> Option<baml_type::Ty> {
        let Place::Local(local) = base else {
            return None;
        };
        let RuntimeTy::Class(name, _) = self.local_ty(base)? else {
            return None;
        };
        self.fields
            .borrow_mut()
            .entry(local.0)
            .or_insert_with(|| {
                let class = baml_compiler2_hir_ty::layout::class_ref_of(self.db, name)?;
                Some(
                    baml_compiler2_hir_ty::layout::class_fields(self.db, class)
                        .into_iter()
                        .map(|(_, ty)| ty)
                        .collect(),
                )
            })
            .as_ref()?
            .get(field)
            .cloned()
    }
}

/// Lowering types some temporaries `null` while they hold an object (an
/// awaited future, a sys-op result), so a bare `null` proves nothing.
fn immutable(ty: &RuntimeTy) -> bool {
    !matches!(ty, RuntimeTy::Null) && immutable_value(ty)
}

fn immutable_value(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Int
        | RuntimeTy::Bigint
        | RuntimeTy::Float
        | RuntimeTy::String
        | RuntimeTy::Bool
        | RuntimeTy::Null
        | RuntimeTy::Literal(..)
        | RuntimeTy::Enum(_)
        | RuntimeTy::EnumVariant(..) => true,
        RuntimeTy::Union(members) => members.iter().all(immutable_value),
        _ => false,
    }
}

fn immutable_ty(ty: &baml_type::Ty) -> bool {
    use baml_type::Ty;
    match ty {
        Ty::Int
        | Ty::Bigint
        | Ty::Float
        | Ty::String
        | Ty::Bool
        | Ty::Null
        | Ty::Literal(..)
        | Ty::Enum(_)
        | Ty::EnumVariant(..) => true,
        Ty::Union(members) => members.iter().all(immutable_ty),
        _ => false,
    }
}

fn primitive_ty(ty: &baml_type::Ty) -> bool {
    use baml_type::Ty;
    match ty {
        Ty::Int | Ty::String | Ty::Bool => true,
        Ty::Literal(literal, _) => {
            matches!(
                literal,
                Literal::Int(_) | Literal::String(_) | Literal::Bool(_)
            )
        }
        _ => false,
    }
}

struct Walker<'a, 'db> {
    db: &'db dyn crate::Db,
    opt: OptLevel,
    /// The function whose summary is being built, if any: calls within its
    /// recursive component are opaque.
    current: Option<FunctionLoc<'db>>,
    types: &'a Types<'a, 'db>,
    builtins: &'a Builtins<'db>,
    array_iterators: &'a [bool],
}

impl<'db> Walker<'_, 'db> {
    fn terminator(&self, graph: &mut Graph<'_, 'db>, terminator: &Terminator<'db>) {
        match terminator {
            Terminator::Goto { .. }
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return
            | Terminator::Unreachable => {}
            Terminator::NarrowBind {
                source,
                destination,
                ..
            } => {
                let value = graph.operand(source);
                let destination = graph.local(destination.0);
                graph.flow(Some(destination), value);
            }
            Terminator::ShortCircuit {
                operand,
                destination,
                ..
            } => {
                let value = graph.operand(operand);
                graph.write(destination, value);
            }
            Terminator::Call {
                has_trace,
                argument_layout,
                callee,
                args,
                ntypeargs,
                destination,
                ..
            } => {
                // Value arguments in parameter order, or the call is opaque.
                let positional = !has_trace
                    && argument_layout
                        .as_ref()
                        .is_none_or(|layout| layout.0.iter().all(Option::is_none));
                let values = args.get(*ntypeargs..).unwrap_or_default();
                if positional && self.compares_against_immutable(callee, values) {
                    // Reads both operands; the result is a bool.
                } else if let Some(effect) = self
                    .builtins
                    .effect(callee)
                    .filter(|effect| positional && self.matches_builtin(effect, values))
                {
                    let result = graph.apply_builtin(effect, values);
                    graph.write(destination, result);
                } else if let Some(summary) = self
                    .summary(callee)
                    .filter(|summary| positional && summary.params.len() == values.len())
                {
                    graph.apply_summary(summary, values, destination);
                } else {
                    graph.unknown(args, destination, Some(callee));
                }
            }
            Terminator::VirtualCall {
                method,
                args,
                destination,
                ..
            } => match self.array_iteration(method, args, destination) {
                Some(Iteration::Iter { array, iterator }) => {
                    // The iterator holds the array.
                    let (array, iterator) = (graph.local(array), graph.local(iterator));
                    let contents = Some(graph.contents_of(Some(iterator)));
                    graph.flow(contents, Some(array));
                }
                Some(Iteration::Next { iterator, item }) => {
                    let (iterator, item) = (graph.local(iterator), graph.local(item));
                    let array = Some(graph.contents_of(Some(iterator)));
                    let element = Some(graph.contents_of(array));
                    graph.flow(Some(item), element);
                }
                None => graph.unknown(args, destination, None),
            },
            Terminator::SysOp {
                callee,
                args,
                destination,
                ..
            } => graph.unknown(args, destination, Some(callee)),
            Terminator::Spawn { plan, future, .. } => {
                graph.leak(plan);
                graph.write_shared(future);
            }
            Terminator::Await { destination, .. } => graph.write_shared(destination),
            Terminator::AwaitAny {
                futures,
                destination,
                ..
            } => {
                graph.leak(futures);
                graph.write_shared(destination);
            }
            // A thrown value reaches the VM's in-flight error, and from there
            // any handler or caller.
            Terminator::Throw { value } => graph.leak(value),
            Terminator::Rethrow { value, context } => {
                graph.leak(value);
                graph.leak(context);
            }
        }
    }

    /// The summary of a call's target, unless it is opaque or in the same
    /// recursive component as the function being summarized.
    fn summary(&self, callee: &Operand<'db>) -> Option<&'db Summary> {
        let DeclRef::Source(callee) = static_callee(callee)? else {
            return None;
        };
        if let Some(current) = self.current {
            let root = current.file(self.db).source_root(self.db);
            let components = recursive_functions(self.db, root, self.opt);
            if callee == current
                || components
                    .get(&current)
                    .is_some_and(|component| components.get(&callee) == Some(component))
            {
                return None;
            }
        }
        escape_summary(self.db, callee, self.opt).as_ref()
    }

    /// `a == b` through `baml.ops.equals_equals` with an immutable operand:
    /// operands of different kinds compare unequal and only two instances of
    /// one class run a user `Equals`, so it runs no user code.
    fn compares_against_immutable(&self, callee: &Operand<'db>, values: &[Operand<'db>]) -> bool {
        static_callee(callee).is_some_and(|func| Some(func) == self.builtins.equals_equals)
            && values.len() == 2
            && values.iter().any(|value| match value {
                Operand::Constant(Constant::GlobalItem(_)) => false,
                Operand::Constant(_) => true,
                Operand::Copy(place) | Operand::Move(place) => self.types.immutable(place),
            })
    }

    /// Whether a call's value arguments fit `effect`: the declared arity and
    /// a receiver place, whose keys, when it compares them, are primitive.
    fn matches_builtin(&self, effect: &Effect, args: &[Operand<'db>]) -> bool {
        let Some(Operand::Copy(receiver) | Operand::Move(receiver)) = args.first() else {
            return false;
        };
        // A key hashes and compares through `Hash` and `Equals`, which run
        // user code for anything but a primitive key.
        args.len() == effect.arity
            && (!effect.compares_keys || self.types.primitive_map_keys(receiver))
    }

    fn array_iteration(
        &self,
        method: &str,
        args: &[Operand<'db>],
        destination: &Place,
    ) -> Option<Iteration> {
        let (receiver, destination) = unary_local_call(args, destination)?;
        match method {
            "iter" if self.array_iterators[destination] => Some(Iteration::Iter {
                array: receiver,
                iterator: destination,
            }),
            "next" if self.array_iterators[receiver] => Some(Iteration::Next {
                iterator: receiver,
                item: destination,
            }),
            _ => None,
        }
    }
}

enum Iteration {
    Iter { array: usize, iterator: usize },
    Next { iterator: usize, item: usize },
}

/// Locals whose only definition is `xs.iter()` on an array: the runtime's
/// `ArrayIterator`, whose `next` reads the array and nothing else.
fn array_iterators(body: &MirFunctionBody<'_>) -> Vec<bool> {
    let mut definitions = vec![0usize; body.locals.len()];
    let mut array_iter = vec![false; body.locals.len()];
    let mut define = |place: &Place| {
        if let Place::Local(local) = place {
            definitions[local.0] += 1;
        }
    };
    for block in &body.blocks {
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign { destination, .. } => define(destination),
                StatementKind::FreshCell { local, .. } => define(&Place::Local(*local)),
                StatementKind::Drop(_)
                | StatementKind::Intrinsic { .. }
                | StatementKind::VirtualFieldStore { .. }
                | StatementKind::Nop => {}
            }
        }
        if let Some(landing) = block.landing {
            define(&Place::Local(landing.error_local));
            define(&Place::Local(landing.context_local));
        }
        let Some(terminator) = &block.terminator else {
            continue;
        };
        match terminator {
            Terminator::VirtualCall {
                method,
                args,
                destination,
                ..
            } => {
                define(destination);
                if method == "iter"
                    && let Some((array, iterator)) = unary_local_call(args, destination)
                    && matches!(body.locals[array].ty, RuntimeTy::List(_))
                {
                    array_iter[iterator] = true;
                }
            }
            Terminator::NarrowBind { destination, .. } => define(&Place::Local(*destination)),
            Terminator::Call { destination, .. }
            | Terminator::SysOp { destination, .. }
            | Terminator::Await { destination, .. }
            | Terminator::AwaitAny { destination, .. }
            | Terminator::ShortCircuit { destination, .. } => define(destination),
            Terminator::Spawn { future, .. } => define(future),
            Terminator::Goto { .. }
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return
            | Terminator::Unreachable
            | Terminator::Throw { .. }
            | Terminator::Rethrow { .. } => {}
        }
    }
    array_iter
        .iter()
        .zip(&definitions)
        .map(|(&iter, &count)| iter && count == 1)
        .collect()
}

/// A virtual call with one local receiver, a local destination, and no type
/// arguments or trace: `(receiver, destination)`.
fn unary_local_call(args: &[Operand<'_>], destination: &Place) -> Option<(usize, usize)> {
    let (
        [Operand::Copy(Place::Local(receiver)) | Operand::Move(Place::Local(receiver))],
        Place::Local(destination),
    ) = (args, destination)
    else {
        return None;
    };
    Some((receiver.0, destination.0))
}

fn is_primitive(ty: &RuntimeTy) -> bool {
    match ty {
        RuntimeTy::Int | RuntimeTy::String | RuntimeTy::Bool => true,
        RuntimeTy::Literal(literal, _) => {
            matches!(
                literal,
                Literal::Int(_) | Literal::String(_) | Literal::Bool(_)
            )
        }
        _ => false,
    }
}

/// What a builtin does with the objects it is given, from its
/// `//baml:effects(...)` declaration, by argument position (the receiver is
/// argument 0).
#[derive(Debug)]
struct Effect {
    /// Value arguments, receiver included.
    arity: usize,
    /// Arguments that become elements or entries of the receiver.
    keeps: Vec<usize>,
    returns: Returns,
    /// It hashes or compares the receiver's keys.
    compares_keys: bool,
    /// It has a `$local` glue twin for a thread-local receiver.
    twin: bool,
}

#[derive(Debug)]
enum Returns {
    Nothing,
    /// An element or entry of the receiver.
    Element,
    /// A new container of the receiver's elements and those of these
    /// arguments.
    Fresh(Vec<usize>),
}

/// Every declared builtin effect, by native key.
fn declared_effects() -> &'static FxHashMap<String, Effect> {
    /// Keep aligned with `bex_vm::package_baml::VM_NATIVE_PACKAGES`.
    const VM_PACKAGES: &[&str] = &["baml", "testing", "ai", "reflect", "trace"];
    static EFFECTS: std::sync::OnceLock<FxHashMap<String, Effect>> = std::sync::OnceLock::new();
    EFFECTS.get_or_init(|| {
        let mut effects = FxHashMap::default();
        for package in VM_PACKAGES {
            // The build already extracted these to generate the VM's glue,
            // so a failure here is unreachable; an opaque builtin is sound.
            let Ok((builtins, _, _)) = baml_builtins2_codegen::extract_native_builtins_for(package)
            else {
                continue;
            };
            for builtin in builtins {
                let Some(declared) = &builtin.effects else {
                    continue;
                };
                let position = |name: &str| {
                    1 + builtin
                        .params
                        .iter()
                        .position(|param| param.name == name)
                        .expect("extraction checks that effects name parameters")
                };
                effects.insert(
                    builtin.path.clone(),
                    Effect {
                        arity: 1 + builtin.params.len(),
                        keeps: declared.keeps.iter().map(|name| position(name)).collect(),
                        returns: match &declared.returns {
                            baml_builtins2_codegen::EffectReturn::Nothing => Returns::Nothing,
                            baml_builtins2_codegen::EffectReturn::Element => Returns::Element,
                            baml_builtins2_codegen::EffectReturn::Fresh(params) => {
                                Returns::Fresh(params.iter().map(|name| position(name)).collect())
                            }
                        },
                        compares_keys: declared.compares_keys,
                        twin: builtin.has_thread_local_twin(),
                    },
                );
            }
        }
        effects
    })
}

/// The builtins whose effect on their arguments is declared. Every other
/// callee is opaque.
struct Builtins<'db> {
    db: &'db dyn crate::Db,
    effects: std::cell::RefCell<FxHashMap<FunctionRef<'db>, Option<&'static Effect>>>,
    /// `baml.ops.equals_equals`, the `==` driver.
    equals_equals: Option<FunctionRef<'db>>,
}

impl<'db> Builtins<'db> {
    fn new(db: &'db dyn crate::Db) -> Self {
        Self {
            db,
            effects: std::cell::RefCell::default(),
            equals_equals: baml_compiler2_hir_ty::callable::lang_function(
                db,
                LangPackage::Baml,
                &["ops"],
                "equals_equals",
            ),
        }
    }

    fn effect(&self, callee: &Operand<'db>) -> Option<&'static Effect> {
        let func = static_callee(callee)?;
        *self.effects.borrow_mut().entry(func).or_insert_with(|| {
            let key = match func {
                DeclRef::Source(loc) => {
                    let body = baml_compiler2_hir::body::function_body(self.db, loc);
                    if !matches!(
                        body.as_ref(),
                        baml_compiler2_hir::body::FunctionBody::Builtin(
                            baml_compiler2_ast::ast::BuiltinKind::Vm
                        )
                    ) {
                        return None;
                    }
                    crate::native_key_for(self.db, loc)
                }
                // An inherent builtin method's key is its display name; any
                // other external function has no declared effect.
                DeclRef::External(_) => crate::function_display_name(self.db, func),
            };
            declared_effects().get(&key)
        })
    }
}

/// Union-find over abstract nodes, each with a shared bit and a contents node.
struct Graph<'a, 'db> {
    /// Static types: values of immutable types need no tracking.
    types: &'a Types<'a, 'db>,
    parent: Vec<usize>,
    shared: Vec<bool>,
    contents: Vec<Option<usize>>,
    /// Per node: what made it shared, read only by [`coverage`].
    reason: Vec<Reason>,
    /// The reason recorded for anything made shared next.
    cause: Reason,
    locals: usize,
}

impl<'a, 'db> Graph<'a, 'db> {
    fn new(locals: usize, types: &'a Types<'a, 'db>) -> Self {
        Self {
            types,
            parent: (0..locals).collect(),
            shared: vec![false; locals],
            contents: vec![None; locals],
            reason: vec![Reason::Parameter; locals],
            cause: Reason::Parameter,
            locals,
        }
    }

    fn local(&self, local: usize) -> usize {
        debug_assert!(local < self.locals);
        local
    }

    fn fresh(&mut self) -> usize {
        let node = self.parent.len();
        self.parent.push(node);
        self.shared.push(false);
        self.contents.push(None);
        self.reason.push(self.cause);
        node
    }

    /// A node for a value that may already be reachable from anywhere.
    fn shared(&mut self) -> usize {
        let node = self.fresh();
        self.shared[node] = true;
        node
    }

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            let grandparent = self.parent[self.parent[node]];
            self.parent[node] = grandparent;
            node = grandparent;
        }
        node
    }

    fn mark_shared(&mut self, node: usize) {
        let root = self.find(node);
        if !self.shared[root] {
            self.reason[root] = self.cause;
        }
        self.shared[root] = true;
    }

    /// [`Self::shared`], for a value shared for `reason` rather than the cause.
    fn shared_because(&mut self, reason: Reason) -> usize {
        let cause = std::mem::replace(&mut self.cause, reason);
        let node = self.shared();
        self.cause = cause;
        node
    }

    /// The contents node of `node`'s objects, created on first use. A value
    /// that is no object has no contents; reading through one is a type
    /// error the checker already rejected, so it yields a shared node.
    fn contents_of(&mut self, node: Option<usize>) -> usize {
        let Some(node) = node else {
            return self.shared();
        };
        let root = self.find(node);
        if let Some(contents) = self.contents[root] {
            return self.find(contents);
        }
        let contents = self.fresh();
        self.contents[root] = Some(contents);
        contents
    }

    /// The node an operand's value belongs to; `None` when it is no object.
    fn operand(&mut self, operand: &Operand<'_>) -> Option<usize> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                (!self.types.immutable(place)).then(|| self.read(place))
            }
            Operand::Constant(constant) => match constant {
                // Immutable: nothing reached through them can change.
                Constant::Int(_)
                | Constant::Float(_)
                | Constant::Bool(_)
                | Constant::Null
                | Constant::OmittedArg
                | Constant::Bigint(_)
                | Constant::String(_)
                | Constant::Function(_)
                | Constant::GenericFunction { .. }
                | Constant::EnumVariant { .. } => None,
                // A global's value is shared by every task.
                Constant::GlobalItem(_) => Some(self.shared_because(Reason::Constant)),
            },
        }
    }

    fn read(&mut self, place: &Place) -> usize {
        match place {
            Place::Local(local) => self.local(local.0),
            Place::Field { base, .. } | Place::Index { base, .. } => {
                let base = self.read(base);
                self.contents_of(Some(base))
            }
            Place::Capture(_) | Place::Deref(_) => self.shared_because(Reason::Cell),
        }
    }

    fn write(&mut self, place: &Place, value: Option<usize>) {
        if self.types.immutable(place) {
            return;
        }
        let into = match place {
            Place::Local(local) => self.local(local.0),
            Place::Field { base, .. } | Place::Index { base, .. } => {
                let base = self.read(base);
                self.contents_of(Some(base))
            }
            Place::Capture(_) | Place::Deref(_) => self.shared_because(Reason::Cell),
        };
        self.flow(Some(into), value);
    }

    fn write_shared(&mut self, place: &Place) {
        let shared = self.shared();
        self.write(place, Some(shared));
    }

    /// `operand`'s value may now be reachable from anywhere.
    fn leak(&mut self, operand: &Operand<'_>) {
        let value = self.operand(operand);
        let shared = self.shared();
        self.flow(Some(shared), value);
    }

    /// A call the analysis cannot see through: its arguments and callee leak,
    /// and its result comes from anywhere.
    fn unknown(&mut self, args: &[Operand<'_>], destination: &Place, callee: Option<&Operand<'_>>) {
        for arg in args.iter().chain(callee) {
            self.leak(arg);
        }
        self.write_shared(destination);
    }

    fn statement(&mut self, kind: &StatementKind<'_>) {
        match kind {
            StatementKind::Assign { destination, value } => {
                let value = self.rvalue(value);
                self.write(destination, value);
            }
            StatementKind::Drop(_) | StatementKind::Nop => {}
            StatementKind::FreshCell { local, .. } => self.mark_shared(local.0),
            StatementKind::Intrinsic { op, args } => match op {
                // Logged data goes to the runtime.
                IntrinsicOp::Log(_) | IntrinsicOp::ApplyTraceHook => {
                    for arg in args {
                        self.leak(arg);
                    }
                }
                IntrinsicOp::BindType(_) | IntrinsicOp::BuiltinTraceHook(_) => {}
            },
            StatementKind::VirtualFieldStore {
                receiver, value, ..
            } => {
                // Field-insensitive: the slot resolved at run time is one of
                // the receiver's fields, all of which are its contents.
                let receiver = self.operand(receiver);
                let value = self.operand(value);
                let contents = Some(self.contents_of(receiver));
                self.flow(contents, value);
            }
        }
    }

    /// The node an rvalue's result belongs to; `None` when it is no object.
    fn rvalue(&mut self, rvalue: &Rvalue<'_>) -> Option<usize> {
        match rvalue {
            Rvalue::Use(operand) => self.operand(operand),
            Rvalue::Array(_, elements) => {
                let array = self.fresh();
                let contents = Some(self.contents_of(Some(array)));
                for element in elements {
                    let element = self.operand(element);
                    self.flow(contents, element);
                }
                Some(array)
            }
            Rvalue::Map(_, _, entries) => {
                let map = self.fresh();
                let contents = Some(self.contents_of(Some(map)));
                for (key, value) in entries {
                    let key = self.operand(key);
                    let value = self.operand(value);
                    self.flow(contents, key);
                    self.flow(contents, value);
                }
                Some(map)
            }
            Rvalue::Aggregate { fields, .. } => {
                let object = self.fresh();
                let contents = Some(self.contents_of(Some(object)));
                for field in fields {
                    let field = self.operand(field);
                    self.flow(contents, field);
                }
                Some(object)
            }
            Rvalue::Uint8Array(_) => Some(self.fresh()),
            Rvalue::VirtualFieldAccess { receiver, .. } => {
                let receiver = self.operand(receiver);
                Some(self.contents_of(receiver))
            }
            // The receiver or wrapped value travels with a callable whose
            // invocation this analysis cannot see.
            Rvalue::MakeBoundMethod { receiver, .. }
            | Rvalue::MakeVirtualBoundMethod { receiver, .. }
            | Rvalue::MakeGenericFunctionFromValue {
                value: receiver, ..
            } => {
                self.leak(receiver);
                Some(self.shared())
            }
            // Captures are cells, already shared.
            Rvalue::MakeClosure { .. }
            | Rvalue::MakeVirtualFunction { .. }
            | Rvalue::MakeGenericFunction { .. }
            | Rvalue::LoadType(_)
            | Rvalue::CurrentPackage
            | Rvalue::TraceHookSettings { .. } => Some(self.shared()),
            // Scalars, or a fresh immutable string or bigint, which no access
            // site synchronizes on.
            Rvalue::BinaryOp { .. }
            | Rvalue::UnaryOp { .. }
            | Rvalue::Discriminant(_)
            | Rvalue::TypeTag(_)
            | Rvalue::Len(_)
            | Rvalue::IsType { .. }
            | Rvalue::IsTypeTag { .. }
            | Rvalue::SatisfiesBound { .. } => None,
        }
    }

    /// Apply a builtin's effect to arguments [`Walker::matches_builtin`]
    /// accepted, returning the node its result belongs to.
    fn apply_builtin(&mut self, effect: &Effect, args: &[Operand<'_>]) -> Option<usize> {
        let receiver = self.operand(&args[0]);
        let contents = Some(self.contents_of(receiver));
        for &kept in &effect.keeps {
            let value = self.operand(&args[kept]);
            self.flow(contents, value);
        }
        match &effect.returns {
            Returns::Nothing => None,
            Returns::Element => contents,
            Returns::Fresh(sources) => {
                let copy = self.fresh();
                let copy_contents = Some(self.contents_of(Some(copy)));
                self.flow(copy_contents, contents);
                for &source in sources {
                    let source = self.operand(&args[source]);
                    let source_contents = Some(self.contents_of(source));
                    self.flow(copy_contents, source_contents);
                }
                Some(copy)
            }
        }
    }

    /// The summary of this graph as a callee with `arity` parameters, before
    /// its parameters are classified.
    fn summary(&mut self, arity: usize) -> Summary {
        let resolved = self.resolve();
        let roots: Vec<usize> = (0..=arity.min(self.locals.saturating_sub(1)))
            .map(|local| resolved.roots[local])
            .collect();
        let mut compact: FxHashMap<usize, usize> = FxHashMap::default();
        let mut order = Vec::new();
        let mut pending = roots.clone();
        while let Some(node) = pending.pop() {
            if compact.contains_key(&node) {
                continue;
            }
            compact.insert(node, order.len());
            order.push(node);
            pending.extend(resolved.contents[node]);
        }
        Summary {
            nodes: order
                .iter()
                .map(|&node| {
                    (
                        resolved.shared[node],
                        resolved.contents[node].map(|contents| compact[&contents]),
                    )
                })
                .collect(),
            params: roots[1..].iter().map(|root| compact[root]).collect(),
            ret: compact[&roots[0]],
        }
    }

    /// A call to a summarized callee: a fresh copy of its graph, joined to the
    /// arguments and the destination.
    fn apply_summary(&mut self, summary: &Summary, args: &[Operand<'_>], destination: &Place) {
        let copies: Vec<usize> = summary
            .nodes
            .iter()
            .map(|&(shared, _)| if shared { self.shared() } else { self.fresh() })
            .collect();
        for (node, &(_, contents)) in summary.nodes.iter().enumerate() {
            if let Some(contents) = contents {
                let inner = self.contents_of(Some(copies[node]));
                self.union(inner, copies[contents]);
            }
        }
        for (arg, &param) in args.iter().zip(&summary.params) {
            let value = self.operand(arg);
            self.flow(Some(copies[param]), value);
        }
        self.write(destination, Some(copies[summary.ret]));
    }

    /// `value` may now be held wherever `into` is.
    fn flow(&mut self, into: Option<usize>, value: Option<usize>) {
        if let (Some(into), Some(value)) = (into, value) {
            self.union(into, value);
        }
    }

    fn union(&mut self, a: usize, b: usize) {
        let mut pending = vec![(a, b)];
        while let Some((a, b)) = pending.pop() {
            let (a, b) = (self.find(a), self.find(b));
            if a == b {
                continue;
            }
            self.parent[b] = a;
            if !self.shared[a] && self.shared[b] {
                self.reason[a] = self.reason[b];
            }
            self.shared[a] |= self.shared[b];
            match (self.contents[a], self.contents[b]) {
                (Some(x), Some(y)) => pending.push((x, y)),
                (None, Some(y)) => self.contents[a] = Some(y),
                (_, None) => {}
            }
        }
    }

    /// Resolve every node to its root and push sharing into contents.
    fn finish(mut self) -> EscapeFacts {
        let resolved = self.resolve();
        EscapeFacts {
            local_calls: Vec::new(),
            local_node: resolved.roots[..self.locals].to_vec(),
            shared: resolved.shared,
            contents: resolved.contents,
        }
    }

    /// Every node's root, and per root its contents and whether sharing
    /// reaches it, from itself or from any node whose contents it is.
    fn resolve(&mut self) -> Resolved {
        let nodes = self.parent.len();
        let roots: Vec<usize> = (0..nodes).map(|node| self.find(node)).collect();
        let mut contents: Vec<Option<usize>> = vec![None; nodes];
        for node in 0..nodes {
            if let Some(inner) = self.contents[node] {
                contents[roots[node]] = Some(roots[inner]);
            }
        }
        let mut shared = vec![false; nodes];
        let mut reasons = self.reason.clone();
        let mut pending: Vec<usize> = (0..nodes)
            .filter(|&node| self.shared[node])
            .map(|node| roots[node])
            .collect();
        for &node in &pending {
            shared[node] = true;
        }
        // What an object holds is shared for the reason the object is.
        while let Some(node) = pending.pop() {
            if let Some(inner) = contents[node]
                && !std::mem::replace(&mut shared[inner], true)
            {
                reasons[inner] = reasons[node];
                pending.push(inner);
            }
        }
        Resolved {
            roots,
            shared,
            contents,
            reasons,
        }
    }
}

/// A graph's nodes after union-find, with sharing propagated.
struct Resolved {
    roots: Vec<usize>,
    shared: Vec<bool>,
    contents: Vec<Option<usize>>,
    reasons: Vec<Reason>,
}
