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
use baml_compiler2_hir_ty::{callable::lang_class_method, extern_loc::FunctionRef};
use baml_type::Literal;
use rustc_hash::FxHashMap;

use crate::{
    Constant, IntrinsicOp, Local, MirFunction, MirFunctionBody, MirFunctionKind, Operand, OptLevel,
    Place, RuntimeTy, Rvalue, StatementKind, Terminator,
};

/// The thread-locality of every object a body names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapeFacts {
    /// The node each local's value belongs to.
    local_node: Vec<usize>,
    /// Per node: whether another thread may reach what it holds.
    shared: Vec<bool>,
    /// Per node: the node every field and element of its objects belongs to.
    contents: Vec<Option<usize>>,
}

impl EscapeFacts {
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
    let mut graph = build(db, body, opt, None);
    // A parameter's argument may already be shared, as far as this body knows.
    for local in 1..=arity.min(body.locals.len().saturating_sub(1)) {
        graph.mark_shared(local);
    }
    graph.finish()
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
    Some(build(db, body, opt, Some(function)).summary(mir.arity))
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
fn build<'db>(
    db: &'db dyn crate::Db,
    body: &MirFunctionBody<'db>,
    opt: OptLevel,
    current: Option<FunctionLoc<'db>>,
) -> Graph {
    let mut graph = Graph::new(body.locals.len());
    let builtins = Builtins::new(db);
    let array_iterators = array_iterators(body);
    let walker = Walker {
        db,
        opt,
        current,
        body,
        builtins: &builtins,
        array_iterators: &array_iterators,
    };
    for (index, decl) in body.locals.iter().enumerate() {
        // A captured local's slot holds its cell, which closures share.
        if decl.is_captured {
            graph.mark_shared(index);
        }
    }
    for (_, landing) in body.handlers() {
        // The VM writes these: an in-flight error and its context.
        graph.mark_shared(landing.error_local.0);
        graph.mark_shared(landing.context_local.0);
    }
    for block in &body.blocks {
        for statement in &block.statements {
            graph.statement(&statement.kind);
        }
        if let Some(terminator) = &block.terminator {
            walker.terminator(&mut graph, terminator);
        }
    }
    graph
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

struct Walker<'a, 'db> {
    db: &'db dyn crate::Db,
    opt: OptLevel,
    /// The function whose summary is being built, if any: calls within its
    /// recursive component are opaque.
    current: Option<FunctionLoc<'db>>,
    body: &'a MirFunctionBody<'db>,
    builtins: &'a Builtins<'db>,
    array_iterators: &'a [bool],
}

impl<'db> Walker<'_, 'db> {
    fn terminator(&self, graph: &mut Graph, terminator: &Terminator<'db>) {
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
                if let Some(effect) = self
                    .builtins
                    .effect(callee)
                    .filter(|&effect| positional && self.matches_builtin(effect, values))
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

    /// Whether a call's value arguments have the shape `effect` describes:
    /// a receiver of the builtin's type, then its other arguments.
    fn matches_builtin(&self, effect: Effect, args: &[Operand<'db>]) -> bool {
        let Some((receiver, rest)) = args.split_first() else {
            return false;
        };
        if rest.len() != effect.arity() {
            return false;
        }
        match (effect, self.operand_ty(receiver)) {
            (Effect::Store | Effect::Load | Effect::Take | Effect::Read, Some(ty)) => {
                matches!(ty, RuntimeTy::List(_))
            }
            // A key hashes and compares through `Hash` and `Equals`, which
            // run user code for anything but a primitive key.
            (
                Effect::MapStore | Effect::MapLoad | Effect::MapRead,
                Some(RuntimeTy::Map { key, .. }),
            ) => is_primitive(key),
            (_, _) => false,
        }
    }

    fn operand_ty(&self, operand: &Operand<'db>) -> Option<&RuntimeTy> {
        match operand {
            Operand::Copy(Place::Local(local)) | Operand::Move(Place::Local(local)) => {
                Some(&self.body.locals[local.0].ty)
            }
            _ => None,
        }
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

/// What a builtin does with the objects it is given, beyond its receiver.
#[derive(Clone, Copy)]
enum Effect {
    /// `xs.push(item)`: the item becomes an element.
    Store,
    /// `xs.at(i)`: returns an element.
    Load,
    /// `xs.pop()`: returns an element.
    Take,
    /// Reads the array; returns no object.
    Read,
    /// `m.set(key, value)`: the value becomes an entry; returns the old one.
    MapStore,
    /// `m.get(key)`, `m.index(key)`, `m.delete(key)`: returns an entry.
    MapLoad,
    /// `m.has(key)`, `m.length()`: returns no object.
    MapRead,
}

impl Effect {
    /// Value arguments after the receiver.
    fn arity(self) -> usize {
        match self {
            Effect::Store | Effect::Load => 1,
            Effect::Take | Effect::Read => 0,
            Effect::MapStore => 2,
            Effect::MapLoad | Effect::MapRead => 1,
        }
    }
}

/// The builtins whose effect on their arguments the analysis knows. Every
/// other callee is opaque.
struct Builtins<'db>(Vec<(FunctionRef<'db>, Effect)>);

impl<'db> Builtins<'db> {
    /// `(class, method, effect)`.
    const TABLE: &'static [(&'static str, &'static str, Effect)] = &[
        ("Array", "push", Effect::Store),
        ("Array", "at", Effect::Load),
        ("Array", "pop", Effect::Take),
        ("Array", "length", Effect::Read),
        ("Map", "set", Effect::MapStore),
        ("Map", "get", Effect::MapLoad),
        ("Map", "index", Effect::MapLoad),
        ("Map", "delete", Effect::MapLoad),
        ("Map", "has", Effect::MapRead),
    ];

    fn new(db: &'db dyn crate::Db) -> Self {
        Self(
            Self::TABLE
                .iter()
                .filter_map(|&(class, method, effect)| {
                    Some((
                        lang_class_method(db, LangPackage::Baml, class, method)?,
                        effect,
                    ))
                })
                .collect(),
        )
    }

    fn effect(&self, callee: &Operand<'db>) -> Option<Effect> {
        let func = static_callee(callee)?;
        self.0
            .iter()
            .find(|(builtin, _)| *builtin == func)
            .map(|&(_, effect)| effect)
    }
}

/// Union-find over abstract nodes, each with a shared bit and a contents node.
struct Graph {
    parent: Vec<usize>,
    shared: Vec<bool>,
    contents: Vec<Option<usize>>,
    locals: usize,
}

impl Graph {
    fn new(locals: usize) -> Self {
        Self {
            parent: (0..locals).collect(),
            shared: vec![false; locals],
            contents: vec![None; locals],
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
        self.shared[root] = true;
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
            Operand::Copy(place) | Operand::Move(place) => Some(self.read(place)),
            Operand::Constant(constant) => match constant {
                Constant::Int(_)
                | Constant::Float(_)
                | Constant::Bool(_)
                | Constant::Null
                | Constant::OmittedArg => None,
                // Pooled or global: shared by every invocation.
                Constant::Bigint(_)
                | Constant::String(_)
                | Constant::Function(_)
                | Constant::GlobalItem(_)
                | Constant::GenericFunction { .. }
                | Constant::EnumVariant { .. } => Some(self.shared()),
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
            Place::Capture(_) | Place::Deref(_) => self.shared(),
        }
    }

    fn write(&mut self, place: &Place, value: Option<usize>) {
        let into = match place {
            Place::Local(local) => self.local(local.0),
            Place::Field { base, .. } | Place::Index { base, .. } => {
                let base = self.read(base);
                self.contents_of(Some(base))
            }
            Place::Capture(_) | Place::Deref(_) => self.shared(),
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
    fn apply_builtin(&mut self, effect: Effect, args: &[Operand<'_>]) -> Option<usize> {
        let receiver = self.operand(&args[0]);
        let contents = Some(self.contents_of(receiver));
        match effect {
            Effect::Store => {
                let item = self.operand(&args[1]);
                self.flow(contents, item);
                None
            }
            Effect::MapStore => {
                let value = self.operand(&args[2]);
                self.flow(contents, value);
                contents
            }
            Effect::Load | Effect::Take | Effect::MapLoad => contents,
            Effect::Read | Effect::MapRead => None,
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
        let mut pending: Vec<usize> = (0..nodes)
            .filter(|&node| self.shared[node])
            .map(|node| roots[node])
            .collect();
        while let Some(node) = pending.pop() {
            if std::mem::replace(&mut shared[node], true) {
                continue;
            }
            if let Some(inner) = contents[node] {
                pending.push(inner);
            }
        }
        Resolved {
            roots,
            shared,
            contents,
        }
    }
}

/// A graph's nodes after union-find, with sharing propagated.
struct Resolved {
    roots: Vec<usize>,
    shared: Vec<bool>,
    contents: Vec<Option<usize>>,
}
