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

use baml_base::LangPackage;
use baml_compiler2_hir_ty::{callable::lang_class_method, extern_loc::FunctionRef};
use baml_type::Literal;

use crate::{
    Constant, IntrinsicOp, Local, MirFunctionBody, Operand, Place, RuntimeTy, Rvalue,
    StatementKind, Terminator,
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
/// parameters are `_1..=_arity`.
pub fn analyze<'db>(
    db: &'db dyn crate::Db,
    body: &MirFunctionBody<'db>,
    arity: usize,
) -> EscapeFacts {
    let mut graph = Graph::new(body.locals.len());
    let builtins = Builtins::new(db);
    let array_iterators = array_iterators(body);
    let walker = Walker {
        body,
        builtins: &builtins,
        array_iterators: &array_iterators,
    };

    for local in 1..=arity.min(body.locals.len().saturating_sub(1)) {
        graph.mark_shared(local);
    }
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
    graph.finish()
}

struct Walker<'a, 'db> {
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
                let effect = (!has_trace
                    && argument_layout
                        .as_ref()
                        .is_none_or(|layout| layout.0.iter().all(Option::is_none)))
                .then(|| self.builtins.effect(callee))
                .flatten();
                let values = args.get(*ntypeargs..).unwrap_or_default();
                match effect.filter(|&effect| self.matches_builtin(effect, values)) {
                    Some(effect) => {
                        let result = graph.apply_builtin(effect, values);
                        graph.write(destination, result);
                    }
                    None => graph.unknown(args, destination, Some(callee)),
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
        let Operand::Constant(Constant::Function(func) | Constant::GenericFunction { func, .. }) =
            callee
        else {
            return None;
        };
        self.0
            .iter()
            .find(|(builtin, _)| builtin == func)
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
        EscapeFacts {
            local_node: roots[..self.locals].to_vec(),
            shared,
            contents,
        }
    }
}
