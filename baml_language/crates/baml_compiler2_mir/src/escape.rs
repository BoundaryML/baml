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
//!
//! The same graph answers a second, stricter question per allocation site:
//! can the object outlive the frame that allocated it? If not, a backend may
//! place it on the stack. An allocation *escapes the frame* when it may reach
//! the return value, a shared node, a global, a captured cell, a thrown
//! error, a spawned task, or an argument of a call that may keep it; see
//! [`FrameEscape`] for every cause. Sharing implies escaping, and escaping
//! flows to contents the way sharing does.

use baml_base::{LangPackage, SourceRoot, SourceRootKind};
use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
use baml_compiler2_hir_ty::extern_loc::{ClassRef, FunctionRef};
use baml_type::Literal;
use rustc_hash::FxHashMap;

use crate::{
    Constant, IntrinsicOp, Local, MirFunction, MirFunctionBody, MirFunctionKind, Operand, OptLevel,
    Place, RuntimeTy, Rvalue, StatementKind, Terminator,
};

/// Locality mask of a node that is shared whatever the caller passes.
pub const NEVER: u64 = u64::MAX;

/// Parameters past this many are always shared: a locality mask has one bit
/// per parameter.
pub const MASKED_PARAMS: usize = 64;

/// The thread-locality of every object a body names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EscapeFacts {
    /// Per block: the parameters its terminator's builtin call takes the
    /// thread-local twin given ([`Self::needs`]), or [`NEVER`].
    twin_needs: Vec<u64>,
    /// Per block: for a call to a source function, each value argument's
    /// [`EscapeFacts::operand_needs`], [`NEVER`] for an argument with nothing
    /// to synchronize; the caller passes the callee a locality mask from
    /// them. `None` for any other terminator.
    call_args: Vec<Option<Box<[u64]>>>,
    /// The node each local's value belongs to.
    local_node: Vec<usize>,
    /// Per node: whether another thread may reach what it holds, as far as
    /// this body knows: when a caller passes nothing it proved thread-local.
    shared: Vec<bool>,
    /// Per node: the node every field and element of its objects belongs to.
    contents: Vec<Option<usize>>,
    /// Per node: the parameters it is thread-local given, as a mask (bit `i`
    /// for parameter `_{i+1}`), or [`NEVER`]. A caller's bit for a parameter
    /// says its argument and everything reachable from it are thread-local;
    /// `0` is thread-local whatever the caller passes.
    needs: Vec<u64>,
    /// Per node: why what it holds may outlive this frame, or `None` when it
    /// cannot.
    escape: Vec<Option<FrameEscape>>,
    /// Every allocation site of the body, in statement order.
    sites: Vec<SiteFacts>,
    /// `(block, statement)` to its index in `sites`.
    site_index: FxHashMap<(usize, usize), usize>,
}

/// Why an allocation may outlive its frame, for stack placement and its
/// coverage report. Every cause but `Generic` is a property of the object's
/// alias group; `Generic` is the site's.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FrameEscape {
    /// Reaches the return value.
    Returned,
    /// Reaches a node another task, the runtime, or an opaque operation may
    /// hold (an await, a sys-op, a bound method, logging, a cell read).
    Shared,
    /// Reaches a global's value.
    Global,
    /// Reaches a cell a closure captures.
    Captured,
    /// Reaches a thrown error.
    Thrown,
    /// Reaches a spawned task.
    Spawned,
    /// Passed to a call that may keep it: the named source function, a
    /// function value, a virtual call, or a sys-op.
    KeepingCall(String),
    /// Passed to a native that is not modeled or that keeps the argument.
    Native(String),
    /// An instance of a class that may have a finalizer, which observes the
    /// object after the frame is gone.
    Finalizer,
    /// Held by a parameter: the caller owns it.
    Parameter,
    /// A generic class instance; not placed in v1.
    Generic,
}

/// An allocation site of a body: an [`Rvalue`] that builds an array, map,
/// instance or byte array. Obtained from [`EscapeFacts::sites`] or
/// [`EscapeFacts::site_at`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Site(usize);

/// The sites and locals that may hold the same object; the unit of a stack
/// slot. Stable within one [`EscapeFacts`] only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GroupId(usize);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SiteFacts {
    block: crate::BlockId,
    statement: usize,
    group: GroupId,
    /// A generic class instance: never placed, whatever its group says.
    generic: bool,
}

impl EscapeFacts {
    /// Every allocation site of the body, with its block and statement
    /// index, in statement order.
    pub fn sites(&self) -> impl Iterator<Item = (Site, crate::BlockId, usize)> + '_ {
        self.sites
            .iter()
            .enumerate()
            .map(|(index, site)| (Site(index), site.block, site.statement))
    }

    /// The allocation site at `statement` of `block`, if that statement
    /// allocates.
    pub fn site_at(&self, block: crate::BlockId, statement: usize) -> Option<Site> {
        self.site_index.get(&(block.0, statement)).map(|&i| Site(i))
    }

    /// Whether the object `site` allocates cannot outlive this frame, so a
    /// backend may place it on the stack. Slot reuse across loop iterations
    /// is the backend's to prove, with [`Self::group_may_reach`] and liveness.
    pub fn site_stays_in_frame(&self, site: Site) -> bool {
        self.site_escape(site).is_none()
    }

    /// Why `site`'s object may outlive this frame; `None` when it cannot.
    pub fn site_escape(&self, site: Site) -> Option<&FrameEscape> {
        let facts = &self.sites[site.0];
        if facts.generic {
            return Some(&FrameEscape::Generic);
        }
        self.escape[facts.group.0].as_ref()
    }

    /// The alias group of the object `site` allocates: every site and local
    /// that may hold the same object.
    pub fn alias_group(&self, site: Site) -> GroupId {
        self.sites[site.0].group
    }

    /// The alias group of every object `local` can hold.
    pub fn local_group(&self, local: Local) -> Option<GroupId> {
        self.local_node.get(local.0).map(|&node| GroupId(node))
    }

    /// Why what `local` holds may outlive this frame; `None` when it cannot.
    pub fn local_escape(&self, local: Local) -> Option<&FrameEscape> {
        self.escape[self.local_group(local)?.0].as_ref()
    }

    /// The group every field and element of `group`'s objects belongs to.
    pub fn contents_group(&self, group: GroupId) -> Option<GroupId> {
        self.contents[group.0].map(GroupId)
    }

    /// Whether an object of `a`'s group may hold, directly or through
    /// fields and elements, an object of `b`'s group (or be it).
    pub fn may_reach(&self, a: Site, b: Site) -> bool {
        self.group_may_reach(self.alias_group(a), self.alias_group(b))
    }

    /// Whether an object of `from` may be, or hold through any depth of
    /// fields and elements, an object of `to`.
    pub fn group_may_reach(&self, from: GroupId, to: GroupId) -> bool {
        let mut node = Some(from.0);
        let mut seen = Vec::new();
        while let Some(current) = node {
            if current == to.0 {
                return true;
            }
            if seen.contains(&current) {
                return false;
            }
            seen.push(current);
            node = self.contents[current];
        }
        false
    }

    /// Whether `block`'s modeled builtin receiver is thread-local.
    /// Consumers check the function object for an available lock-free twin.
    pub fn calls_thread_local_twin(&self, block: crate::BlockId) -> bool {
        self.twin_needs(block) == 0
    }

    /// The parameters `block`'s builtin call takes its lock-free twin given:
    /// see [`Self::object_needs`]. [`NEVER`] when the call has no modeled receiver.
    /// Twin availability belongs to the consumer's function object.
    pub fn twin_needs(&self, block: crate::BlockId) -> u64 {
        self.twin_needs.get(block.0).copied().unwrap_or(NEVER)
    }

    /// For a call to a source function in `block`, each value argument's
    /// needs, from which the caller's locality mask for the callee follows:
    /// bit `i` is set when argument `i`'s needs are not [`NEVER`] and the
    /// caller's own mask covers them.
    pub fn call_arg_needs(&self, block: crate::BlockId) -> Option<&[u64]> {
        self.call_args.get(block.0)?.as_deref()
    }

    /// The parameters every object held in a field or element of `local`'s
    /// objects is thread-local given: an array iterator's array, for one.
    pub fn contents_needs(&self, local: Local) -> u64 {
        self.local_node
            .get(local.0)
            .and_then(|&node| self.contents[node])
            .map_or(NEVER, |contents| self.needs[contents])
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

    /// The parameters the object `place` evaluates to is thread-local given:
    /// bit `i` requires parameter `_{i+1}` and everything reachable from it to
    /// be thread-local; `0` needs no caller proof. [`NEVER`] for an unnamed place.
    pub fn object_needs(&self, place: &Place) -> u64 {
        self.object_node(place)
            .map_or(NEVER, |node| self.needs[node])
    }

    /// The parameters `operand`'s value and everything reachable from it are
    /// thread-local given, the condition for passing it as a thread-local
    /// argument. `0` for a value that is no object.
    pub fn operand_needs(&self, operand: &Operand<'_>) -> u64 {
        let (Operand::Copy(place) | Operand::Move(place)) = operand else {
            return 0;
        };
        let Some(mut node) = self.object_node(place) else {
            return NEVER;
        };
        let mut needs = 0;
        let mut seen = Vec::new();
        loop {
            needs |= self.needs[node];
            seen.push(node);
            match self.contents[node] {
                Some(inner) if !seen.contains(&inner) => node = inner,
                _ => return needs,
            }
        }
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

/// Whether a backend compiles a source callee itself. A callee the backend
/// cannot admit runs in the interpreter, which may keep its arguments, so
/// its summary does not clear them for stack placement.
pub type Compiled<'a, 'db> = &'a dyn Fn(FunctionLoc<'db>) -> bool;

/// Classify every object `body` names. `arity` is the parameter count;
/// parameters are `_1..=_arity`. Calls to functions with a summary
/// ([`escape_summary`]) in `root` or the stdlib are seen through, at `opt`.
/// For the frame verdict every summarized callee keeps its arguments, as if
/// interpreted: [`analyze_for`] takes the backend's word instead.
pub fn analyze<'db>(
    db: &'db dyn crate::Db,
    root: SourceRoot,
    body: &MirFunctionBody<'db>,
    arity: usize,
    opt: OptLevel,
) -> EscapeFacts {
    analyze_for(db, root, body, arity, opt, &|_| false)
}

/// [`analyze`], with `compiled` saying which source callees the consuming
/// backend compiles: a call to one keeps only the arguments its summary
/// keeps. Thread locality does not depend on `compiled`.
pub fn analyze_for<'db>(
    db: &'db dyn crate::Db,
    root: SourceRoot,
    body: &MirFunctionBody<'db>,
    arity: usize,
    opt: OptLevel,
    compiled: Compiled<'_, 'db>,
) -> EscapeFacts {
    let types = Types::new(db, root, body);
    let mut facts = build(
        db,
        body,
        arity,
        opt,
        &types,
        Callees {
            compiled: Some(compiled),
            ..Callees::default()
        },
    )
    .finish(arity);
    let builtins = Builtins::new(db);
    let calls: Vec<_> = body
        .blocks
        .iter()
        .map(|block| match &block.terminator {
            Some(Terminator::Call {
                has_trace: false,
                argument_layout,
                callee,
                args,
                ntypeargs,
                ..
            }) if argument_layout
                .as_ref()
                .is_none_or(|layout| layout.0.iter().all(Option::is_none)) =>
            {
                Some((callee, args.get(*ntypeargs..).unwrap_or_default()))
            }
            _ => None,
        })
        .collect();
    facts.twin_needs = calls
        .iter()
        .map(|call| match call {
            Some((callee, values))
                if builtins
                    .effect(callee)
                    .is_some_and(|effect| effect.arity > 0) =>
            {
                match values.first() {
                    Some(Operand::Copy(receiver) | Operand::Move(receiver)) => {
                        facts.object_needs(receiver)
                    }
                    _ => NEVER,
                }
            }
            _ => NEVER,
        })
        .collect();
    facts.call_args = calls
        .iter()
        .map(|call| {
            let (callee, values) = call.as_ref()?;
            let Some(DeclRef::Source(_)) = static_callee(callee) else {
                return None;
            };
            if builtins.effect(callee).is_some() {
                return None;
            }
            let needs: Box<[u64]> = values
                .iter()
                .map(|value| match value {
                    Operand::Copy(place) | Operand::Move(place) if !types.immutable(place) => {
                        facts.operand_needs(value)
                    }
                    _ => NEVER,
                })
                .collect();
            needs.iter().any(|&needs| needs != NEVER).then_some(needs)
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
    let root = function.file(db).source_root(db);
    if let Some(&component) = recursive_functions(db, root, opt).get(&function) {
        return component_summaries(db, root, opt, component)
            .get(&function)
            .cloned();
    }
    let mir = crate::lower_function(db, function, opt).as_ref().ok()?;
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        return None;
    };
    let types = Types::new(db, root, body);
    Some(
        build(
            db,
            body,
            mir.arity,
            opt,
            &types,
            Callees {
                current: Some(function),
                ..Callees::default()
            },
        )
        .summary(0, mir.arity),
    )
}

/// The summaries of one component of [`recursive_functions`], from one graph
/// of all its bodies: a call within the component joins its arguments to the
/// callee's parameters and its destination to the callee's return value, as
/// every call of that callee in the component does. A member whose body the
/// analysis cannot see has no summary, and calls to it are opaque.
#[salsa::tracked(returns(ref))]
fn component_summaries<'db>(
    db: &'db dyn crate::Db,
    root: SourceRoot,
    opt: OptLevel,
    component: usize,
) -> FxHashMap<FunctionLoc<'db>, Summary> {
    let mut members: Vec<(
        FunctionLoc<'db>,
        &'db MirFunction<'db>,
        &'db MirFunctionBody<'db>,
    )> = recursive_functions(db, root, opt)
        .iter()
        .filter(|&(_, &id)| id == component)
        .filter_map(|(&function, _)| {
            let mir = crate::lower_function(db, function, opt).as_ref().ok()?;
            let MirFunctionKind::Bytecode(body) = &mir.kind else {
                return None;
            };
            Some((function, mir, body))
        })
        .collect();
    // Node numbering, and so the summaries, must not depend on map order.
    members.sort_by_key(|&(function, ..)| salsa::plumbing::AsId::as_id(&function));
    let types: Vec<Types<'_, 'db>> = members
        .iter()
        .map(|&(_, _, body)| Types::new(db, root, body))
        .collect();
    let mut offsets = FxHashMap::default();
    let mut total = 0;
    for &(function, mir, body) in &members {
        offsets.insert(function, (total, mir.arity));
        total += body.locals.len();
    }
    let Some(first) = types.first() else {
        return FxHashMap::default();
    };
    let mut graph = Graph::new(total, first);
    for (index, &(function, mir, body)) in members.iter().enumerate() {
        graph.offset = offsets[&function].0;
        graph.types = &types[index];
        walk(
            &mut graph,
            db,
            body,
            mir.arity,
            opt,
            Callees {
                component: Some(&offsets),
                ..Callees::default()
            },
        );
    }
    members
        .iter()
        .map(|&(function, mir, _)| (function, graph.summary(offsets[&function].0, mir.arity)))
        .collect()
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
                    if callee.file(db).source_root(db) == root
                        && let Some(&callee) = index.get(&callee)
                    {
                        callees.push(callee);
                    }
                });
            }
            callees
        })
        .collect();
    baml_compiler2_hir_ty::globals::strongly_connected_components(&edges)
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

/// User dependency bodies must not affect a caller's artifact: a mounted
/// dependency exposes the same declarations but no MIR. Embedded stdlib source
/// remains eligible in source compiles; runtime compilation can mount it opaque.
fn can_summarize(db: &dyn crate::Db, caller: SourceRoot, callee: FunctionLoc<'_>) -> bool {
    let root = callee.file(db).source_root(db);
    root == caller || root.kind(db) == SourceRootKind::Stdlib
}

/// Whether an instance of `class` built in `caller` may have a finalizer.
/// Read from source only where [`can_summarize`] sees bodies, so a user
/// dependency answers the same mounted or not: anything else may.
fn may_finalize(db: &dyn crate::Db, caller: SourceRoot, class: &ClassRef<'_>) -> bool {
    let DeclRef::Source(class) = *class else {
        return true;
    };
    let root = class.file(db).source_root(db);
    if root != caller && root.kind(db) != SourceRootKind::Stdlib {
        return true;
    }
    baml_compiler2_hir::item_data::class_data(db, class)
        .methods
        .iter()
        .any(|&method| baml_compiler2_hir_ty::cleanup::cleanup_method(db, method).is_some())
}

/// The constraint graph of one body, before its parameters are classified.
fn build<'a, 'db>(
    db: &'db dyn crate::Db,
    body: &'a MirFunctionBody<'db>,
    arity: usize,
    opt: OptLevel,
    types: &'a Types<'a, 'db>,
    callees: Callees<'a, 'db>,
) -> Graph<'a, 'db> {
    let mut graph = Graph::new(body.locals.len(), types);
    walk(&mut graph, db, body, arity, opt, callees);
    graph
}

/// How a walk sees the functions a body calls.
#[derive(Clone, Copy, Default)]
struct Callees<'a, 'db> {
    /// The function whose summary is being built, if any: calls within its
    /// recursive component are opaque.
    current: Option<FunctionLoc<'db>>,
    /// The bodies of a recursive component sharing one graph: each callee's
    /// node offset and arity. A call to one joins the callee's nodes directly.
    component: Option<&'a FxHashMap<FunctionLoc<'db>, (usize, usize)>>,
    /// Which summarized callees run compiled, so their summaries' `kept`
    /// bits hold. `None` trusts every summary: a summary being built sees
    /// its callees through theirs, and the backend's hook applies once, at
    /// the direct call.
    compiled: Option<Compiled<'a, 'db>>,
}

/// Add `body`'s constraints to `graph`, whose locals for it start at
/// `graph.offset` and whose types are `body`'s.
fn walk<'a, 'db>(
    graph: &mut Graph<'a, 'db>,
    db: &'db dyn crate::Db,
    body: &'a MirFunctionBody<'db>,
    arity: usize,
    opt: OptLevel,
    callees: Callees<'a, 'db>,
) {
    let builtins = Builtins::new(db);
    let array_iterators = array_iterators(db, body, arity);
    let walker = Walker {
        db,
        opt,
        callees,
        types: graph.types,
        builtins: &builtins,
        array_iterators: &array_iterators,
    };
    if !body.locals.is_empty() {
        // The return value outlives the frame; the caller holds it.
        let ret = graph.local(0);
        graph.mark_frame(ret, FrameCause::Returned);
    }
    graph.cause = Reason::Captured;
    graph.frame_cause = FrameCause::Captured;
    for (index, decl) in body.locals.iter().enumerate() {
        // A captured local's slot holds its cell, which closures share.
        if decl.is_captured {
            let node = graph.local(index);
            graph.mark_shared(node);
        }
    }
    graph.cause = Reason::Handler;
    graph.frame_cause = FrameCause::Thrown;
    for (_, landing) in body.handlers() {
        // The VM writes these: an in-flight error and its context.
        let (error, context) = (
            graph.local(landing.error_local.0),
            graph.local(landing.context_local.0),
        );
        graph.mark_shared(error);
        graph.mark_shared(context);
    }
    for (block_index, block) in body.blocks.iter().enumerate() {
        for (statement_index, statement) in block.statements.iter().enumerate() {
            graph.cause = Reason::At {
                block: block_index,
                statement: Some(statement_index),
            };
            graph.frame_cause = FrameCause::Shared;
            graph.statement(&statement.kind);
        }
        if let Some(terminator) = &block.terminator {
            graph.cause = Reason::At {
                block: block_index,
                statement: None,
            };
            graph.frame_cause = FrameCause::Shared;
            walker.terminator(graph, terminator);
        }
    }
    graph.cause = Reason::Parameter;
    graph.frame_cause = FrameCause::Parameter;
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

/// [`FrameEscape`] before its names are rendered: what the graph records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameCause<'db> {
    Returned,
    Shared,
    Global,
    Captured,
    Thrown,
    Spawned,
    /// A call to a source function that may keep the argument.
    Call(FunctionRef<'db>),
    /// A call through a function value or a virtual call.
    FunctionValue,
    VirtualCall,
    /// A native not in [`EFFECTS`], or one whose effect keeps the argument.
    Native(FunctionRef<'db>),
    Finalizer,
    Parameter,
}

impl<'db> FrameCause<'db> {
    fn describe(self, db: &'db dyn crate::Db) -> FrameEscape {
        match self {
            Self::Returned => FrameEscape::Returned,
            Self::Shared => FrameEscape::Shared,
            Self::Global => FrameEscape::Global,
            Self::Captured => FrameEscape::Captured,
            Self::Thrown => FrameEscape::Thrown,
            Self::Spawned => FrameEscape::Spawned,
            Self::Call(func) => FrameEscape::KeepingCall(crate::function_display_name(db, func)),
            Self::FunctionValue => FrameEscape::KeepingCall("a function value".into()),
            Self::VirtualCall => FrameEscape::KeepingCall("a virtual call".into()),
            Self::Native(func) => FrameEscape::Native(
                native_key(db, func).unwrap_or_else(|| crate::function_display_name(db, func)),
            ),
            Self::Finalizer => FrameEscape::Finalizer,
            Self::Parameter => FrameEscape::Parameter,
        }
    }
}

/// The [`EFFECTS`] key of a native: a VM builtin's native key, or an
/// external function's display name. `None` for a function with a body.
fn native_key<'db>(db: &'db dyn crate::Db, func: FunctionRef<'db>) -> Option<String> {
    match func {
        DeclRef::Source(loc) => {
            let body = baml_compiler2_hir::body::function_body(db, loc);
            matches!(
                body.as_ref(),
                baml_compiler2_hir::body::FunctionBody::Builtin(
                    baml_compiler2_ast::ast::BuiltinKind::Vm
                )
            )
            .then(|| crate::native_key_for(db, loc))
        }
        DeclRef::External(_) => Some(crate::function_display_name(db, func)),
    }
}

/// Whether `func` runs native code: a builtin of any kind, or external.
fn is_native<'db>(db: &'db dyn crate::Db, func: FunctionRef<'db>) -> bool {
    match func {
        DeclRef::Source(loc) => matches!(
            baml_compiler2_hir::body::function_body(db, loc).as_ref(),
            baml_compiler2_hir::body::FunctionBody::Builtin(_)
        ),
        DeclRef::External(_) => true,
    }
}

/// One container or field access in a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    pub kind: AccessKind,
    /// Whether the accessed object is thread-local.
    pub thread_local: bool,
    /// For a shared object, what made it shared.
    pub shared_because: Option<String>,
    /// The parameters the accessed object is thread-local given
    /// ([`EscapeFacts::object_needs`]).
    pub needs: u64,
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
    root: SourceRoot,
    body: &MirFunctionBody<'db>,
    arity: usize,
    opt: OptLevel,
) -> Vec<Access> {
    let types = Types::new(db, root, body);
    let mut graph = build(db, body, arity, opt, &types, Callees::default());
    for local in 1..=arity.min(body.locals.len().saturating_sub(1)) {
        graph.mark_shared(local);
    }
    let resolved = graph.resolve();
    // The same graph with its parameters left unclassified: union-find roots
    // do not depend on sharing, so its nodes are this graph's nodes.
    let facts = build(db, body, arity, opt, &types, Callees::default()).finish(arity);
    let builtins = Builtins::new(db);
    let array_iterators = array_iterators(db, body, arity);
    let mut accesses = Vec::new();
    let mut record = |kind: AccessKind, node: Option<usize>| {
        let thread_local = node.is_some_and(|node| !facts.shared[node]);
        let needs = node.map_or(NEVER, |node| facts.needs[node]);
        let shared_because = (!thread_local).then(|| match node {
            Some(node) => describe(db, root, body, opt, resolved.reasons[node]),
            None => "not an object the analysis tracks".to_string(),
        });
        accesses.push(Access {
            kind,
            thread_local,
            shared_because,
            needs,
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
            Terminator::VirtualCall { .. } => {
                if let Some(("next", iterator, _)) =
                    crate::iteration::unary_iteration_call(db, terminator)
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
    root: SourceRoot,
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
            Some(DeclRef::Source(loc))
                if can_summarize(db, root, loc) && escape_summary(db, loc, opt).is_some() =>
            {
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
#[derive(Debug, Clone, PartialEq, Eq, salsa::SalsaValue)]
pub struct Summary {
    /// Per node: whether another thread may reach it, and its contents node.
    nodes: Vec<(bool, Option<usize>)>,
    /// The node of each parameter, in declaration order.
    params: Vec<usize>,
    /// The node of the return value.
    ret: usize,
    /// Per parameter: whether the callee may keep its argument past the
    /// call, for the frame verdict: it reaches the return value, a shared
    /// node, a keeping call or native of its own, or another parameter's
    /// contents (a store the caller cannot see).
    kept: Vec<bool>,
}

/// Static types of a body's places, as far as escape analysis needs them.
struct Types<'a, 'db> {
    db: &'db dyn crate::Db,
    body: &'a MirFunctionBody<'db>,
    /// Package owning this body, including lambdas and initializers.
    root: SourceRoot,
    /// Per class-typed local: its class's field types, by slot.
    fields: std::cell::RefCell<FxHashMap<usize, Option<Vec<baml_type::Ty>>>>,
}

impl<'a, 'db> Types<'a, 'db> {
    /// [`may_finalize`] for a class built in this body.
    fn may_finalize(&self, class: &ClassRef<'_>) -> bool {
        may_finalize(self.db, self.root, class)
    }

    fn new(db: &'db dyn crate::Db, root: SourceRoot, body: &'a MirFunctionBody<'db>) -> Self {
        Self {
            db,
            body,
            root,
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
    callees: Callees<'a, 'db>,
    types: &'a Types<'a, 'db>,
    builtins: &'a Builtins<'db>,
    array_iterators: &'a [bool],
}

impl<'a, 'db> Walker<'a, 'db> {
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
                let func = static_callee(callee);
                if positional && self.compares_against_immutable(callee, values) {
                    // Reads both operands; the result is a bool.
                } else if let Some(effect) = self
                    .builtins
                    .effect(callee)
                    .filter(|effect| positional && self.matches_builtin(effect, values))
                {
                    // The native holds a kept argument in its receiver, which
                    // the graph tracks; for placement, the native keeps it.
                    graph.frame_cause = FrameCause::Native(func.expect("modeled callee"));
                    for &kept in effect.keeps {
                        graph.keep(&values[kept]);
                    }
                    let result = graph.apply_builtin(effect, values);
                    graph.write(destination, result);
                } else if let Some(&(offset, arity)) = self
                    .component_member(callee)
                    .filter(|&&(_, arity)| positional && arity == values.len())
                {
                    // The callee's parameters are `_1..=_arity`, its return
                    // value `_0`, in its part of this graph.
                    for (index, value) in values.iter().take(arity).enumerate() {
                        let value = graph.operand(value);
                        graph.flow(Some(offset + 1 + index), value);
                    }
                    graph.write(destination, Some(offset));
                } else if let Some(summary) = self
                    .summary(callee)
                    .filter(|summary| positional && summary.params.len() == values.len())
                {
                    let func = func.expect("summarized callee");
                    graph.frame_cause = FrameCause::Call(func);
                    // An interpreted callee may keep anything it is passed.
                    let interpreted = match (self.callees.compiled, func) {
                        (Some(compiled), DeclRef::Source(loc)) => !compiled(loc),
                        (Some(_), DeclRef::External(_)) => true,
                        (None, _) => false,
                    };
                    for (value, &kept) in values.iter().zip(&summary.kept) {
                        if kept || interpreted {
                            graph.keep(value);
                        }
                    }
                    graph.apply_summary(summary, values, destination);
                } else {
                    graph.frame_cause = match func {
                        Some(func) if is_native(self.db, func) => FrameCause::Native(func),
                        Some(func) => FrameCause::Call(func),
                        None => FrameCause::FunctionValue,
                    };
                    graph.unknown(args, destination, Some(callee));
                }
            }
            Terminator::VirtualCall {
                args, destination, ..
            } => match self.array_iteration(terminator) {
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
                None => {
                    graph.frame_cause = FrameCause::VirtualCall;
                    graph.unknown(args, destination, None);
                }
            },
            Terminator::SysOp {
                callee,
                args,
                destination,
                ..
            } => {
                graph.frame_cause = match static_callee(callee) {
                    Some(func) => FrameCause::Call(func),
                    None => FrameCause::FunctionValue,
                };
                graph.unknown(args, destination, Some(callee));
            }
            Terminator::Spawn { plan, future, .. } => {
                graph.frame_cause = FrameCause::Spawned;
                graph.leak(plan);
                graph.write_shared(future);
            }
            Terminator::Await { destination, .. } => graph.write_shared(destination),
            Terminator::AwaitAny {
                futures,
                destination,
                ..
            } => {
                graph.frame_cause = FrameCause::Spawned;
                graph.leak(futures);
                graph.write_shared(destination);
            }
            // A thrown value reaches the VM's in-flight error, and from there
            // any handler or caller.
            Terminator::Throw { value } => {
                graph.frame_cause = FrameCause::Thrown;
                graph.leak(value);
            }
            Terminator::Rethrow { value, context } => {
                graph.frame_cause = FrameCause::Thrown;
                graph.leak(value);
                graph.leak(context);
            }
        }
    }

    /// The offset and arity of a call's target within this graph, when it is
    /// a body of the component being analyzed together.
    fn component_member(&self, callee: &Operand<'db>) -> Option<&'a (usize, usize)> {
        let DeclRef::Source(callee) = static_callee(callee)? else {
            return None;
        };
        self.callees.component?.get(&callee)
    }

    /// The summary of a call's target, unless it is opaque or in the same
    /// recursive component as the function being summarized.
    fn summary(&self, callee: &Operand<'db>) -> Option<&'db Summary> {
        let DeclRef::Source(callee) = static_callee(callee)? else {
            return None;
        };
        if !can_summarize(self.db, self.types.root, callee) {
            return None;
        }
        // A component member the call could not join is opaque: its summary
        // is the one being built.
        if self
            .callees
            .component
            .is_some_and(|component| component.contains_key(&callee))
        {
            return None;
        }
        if let Some(current) = self.callees.current {
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

    fn array_iteration(&self, terminator: &Terminator<'db>) -> Option<Iteration> {
        let (method, receiver, destination) =
            crate::iteration::unary_iteration_call(self.db, terminator)?;
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

/// Iterator locals a backend may compile as an index walk over the array:
/// defined once, by `xs.iter()` on an array local, not captured, and
/// mentioned nowhere but as the receiver of `next`. Such an iterator is never
/// observed, so the walk is the same loop without the object.
pub fn array_walks(db: &dyn crate::Db, body: &MirFunctionBody<'_>, arity: usize) -> Vec<bool> {
    let mut walks = array_iterators(db, body, arity);
    for (local, walk) in walks.iter_mut().enumerate() {
        *walk &= local > arity && !body.locals[local].is_captured;
    }
    let mut disqualify = |place: &Place| {
        let mut mentioned = |local: Local| {
            if let Some(walk) = walks.get_mut(local.0) {
                *walk = false;
            }
        };
        if let Some(local) = place.base_local() {
            mentioned(local);
        }
        let mut indices = place;
        while let Place::Field { base, .. } | Place::Index { base, .. } = indices {
            if let Place::Index { index, .. } = indices {
                mentioned(*index);
            }
            indices = base;
        }
    };
    for block in &body.blocks {
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign { destination, value } => {
                    disqualify(destination);
                    crate::memory::walk_rvalue_places(value, &mut |read| disqualify(read));
                }
                StatementKind::Drop(place) => disqualify(place),
                StatementKind::FreshCell { local, .. } => disqualify(&Place::Local(*local)),
                StatementKind::Intrinsic { args, .. } => {
                    for arg in args {
                        crate::memory::walk_operand_places(arg, &mut |read| disqualify(read));
                    }
                }
                StatementKind::VirtualFieldStore {
                    receiver, value, ..
                } => {
                    for operand in [receiver, value] {
                        crate::memory::walk_operand_places(operand, &mut |read| disqualify(read));
                    }
                }
                StatementKind::Nop => {}
            }
        }
        if let Some(landing) = block.landing {
            disqualify(&Place::Local(landing.error_local));
            disqualify(&Place::Local(landing.context_local));
        }
        let Some(terminator) = &block.terminator else {
            continue;
        };
        // The walk's own `iter` and `next` calls do not observe the iterator.
        if let Some((method, receiver, result)) =
            crate::iteration::unary_iteration_call(db, terminator)
        {
            match method {
                "iter" => {
                    disqualify(&Place::Local(Local(receiver)));
                    continue;
                }
                "next" => {
                    disqualify(&Place::Local(Local(result)));
                    continue;
                }
                _ => {}
            }
        }
        let (operands, destinations) = terminator_places(terminator);
        for operand in operands {
            crate::memory::walk_operand_places(operand, &mut |read| disqualify(read));
        }
        for destination in destinations {
            disqualify(destination);
        }
    }
    walks
}

/// Locals whose only definition is `xs.iter()` on an array: the runtime's
/// `ArrayIterator`, whose `next` reads the array and nothing else.
fn array_iterators(db: &dyn crate::Db, body: &MirFunctionBody<'_>, arity: usize) -> Vec<bool> {
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
            Terminator::VirtualCall { destination, .. } => {
                define(destination);
                if let Some(("iter", array, iterator)) =
                    crate::iteration::unary_iteration_call(db, terminator)
                    && iterator > arity
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

/// What a builtin does with its arguments (receiver at argument 0).
#[derive(Debug)]
pub struct Effect {
    /// Value arguments, receiver included.
    pub arity: usize,
    /// Arguments that become elements or entries of the receiver.
    pub keeps: &'static [usize],
    pub returns: Returns,
    /// It hashes or compares the receiver's keys.
    compares_keys: bool,
    /// Passes receiver elements to user code, which may retain them.
    leaks_contents: bool,
}

#[derive(Debug)]
pub enum Returns {
    Nothing,
    /// An element or entry of the receiver.
    Element,
    /// A new container of the receiver's elements and those of these
    /// arguments.
    Fresh(&'static [usize]),
}

/// Effects by native key, with argument 0 the receiver. `keeps` flows the
/// listed arguments into its contents; `Element` returns those contents;
/// `Fresh` returns a new container holding its contents and the listed
/// arguments' contents. `compares_keys` allows user hash/equality calls on
/// keys, which must be treated as escaping. Empty clauses only read values.
/// An unlisted builtin is opaque, which is sound.
pub const EFFECTS: &[(&str, Effect)] = &[
    // String.length delegates here: reads the cached character count and returns an int.
    (
        "baml.String.char_count",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Reads Vec length without retaining values.
    (
        "baml.Array.length",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Clones the selected element from the receiver.
    (
        "baml.Array.at",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Element,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Appends the argument to the receiver Vec.
    (
        "baml.Array.push",
        Effect {
            arity: 2,
            keeps: &[1],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Removes and returns the last receiver element.
    (
        "baml.Array.pop",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Element,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Removes and returns the indexed receiver element.
    (
        "baml.Array.remove_at",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Element,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Copies both input arrays into a fresh Vec.
    (
        "baml.Array.concat",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Fresh(&[1]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Copies receiver elements into a fresh reversed Vec.
    (
        "baml.Array.reverse",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Copies the selected receiver range into a fresh Vec.
    (
        "baml.Array.slice",
        Effect {
            arity: 3,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Drops receiver elements without retaining them.
    (
        "baml.Array.clear",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Removes and returns the first receiver element.
    (
        "baml.Array.shift",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Element,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Inserts the argument at the start of the receiver Vec.
    (
        "baml.Array.unshift",
        Effect {
            arity: 2,
            keeps: &[1],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Inserts the value argument at the given receiver index.
    (
        "baml.Array.insert",
        Effect {
            arity: 3,
            keeps: &[1],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Reads IndexMap length without retaining values.
    (
        "baml.Map.length",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Hashes the key and reads receiver membership.
    (
        "baml.Map.has",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Nothing,
            compares_keys: true,
            leaks_contents: false,
        },
    ),
    // Copies receiver keys into a fresh array.
    (
        "baml.Map.keys",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Copies receiver values into a fresh array.
    (
        "baml.Map.values",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Hashes the key and stores key and value in the receiver.
    (
        "baml.Map.set",
        Effect {
            arity: 3,
            keeps: &[1, 2],
            returns: Returns::Element,
            compares_keys: true,
            leaks_contents: false,
        },
    ),
    // Hashes the key and clones the matching receiver value.
    (
        "baml.Map.get",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Element,
            compares_keys: true,
            leaks_contents: false,
        },
    ),
    // Hashes the key and clones the indexed receiver value.
    (
        "baml.Map.index",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Element,
            compares_keys: true,
            leaks_contents: false,
        },
    ),
    // Hashes the key and removes its receiver entry.
    (
        "baml.Map.delete",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Element,
            compares_keys: true,
            leaks_contents: false,
        },
    ),
    // Hashes the key and stores the default if absent, returning an entry.
    (
        "baml.Map.get_or_insert",
        Effect {
            arity: 3,
            keeps: &[1, 2],
            returns: Returns::Element,
            compares_keys: true,
            leaks_contents: false,
        },
    ),
    // Drops receiver entries without retaining them.
    (
        "baml.Map.clear",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Allocates an array of immutable string pieces.
    (
        "baml.String.split",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Allocates an array of immutable character strings.
    (
        "baml.String.chars",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Allocates an array of immutable line strings.
    (
        "baml.String.lines",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Allocates an array of scalar UTF-8 bytes.
    (
        "baml.String.to_utf8",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // Allocates an array of scalar Unicode code points.
    (
        "baml.String.to_code_points",
        Effect {
            arity: 1,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: false,
        },
    ),
    // ToString overrides receive elements, while join retains only a copied Vec.
    (
        "baml.Array.join",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Nothing,
            compares_keys: false,
            leaks_contents: true,
        },
    ),
    // Callbacks receive elements; the continuation allocates the result array after they return.
    (
        "baml.Array.map",
        Effect {
            arity: 2,
            keeps: &[],
            returns: Returns::Fresh(&[]),
            compares_keys: false,
            leaks_contents: true,
        },
    ),
];

/// Whether `rvalue` allocates an object a backend could place, and if so
/// whether it is a generic class instance.
fn allocates(rvalue: &Rvalue<'_>) -> Option<bool> {
    match rvalue {
        Rvalue::Aggregate {
            kind: crate::AggregateKind::Class {
                type_arg_templates, ..
            },
            ..
        } => Some(!type_arg_templates.is_empty()),
        Rvalue::Aggregate {
            kind: crate::AggregateKind::Array,
            ..
        }
        | Rvalue::Array(..)
        | Rvalue::Map(..)
        | Rvalue::Uint8Array(_) => Some(false),
        _ => None,
    }
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
            // An inherent builtin method's key is its display name; any
            // other external function has no declared effect.
            let key = native_key(self.db, func)?;
            EFFECTS
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, effect)| effect)
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
    /// Per root: why what it holds may outlive the frame. Set with sharing
    /// and by the frame-only causes; `None` until then.
    frame: Vec<Option<FrameCause<'db>>>,
    /// The frame cause recorded for anything made shared or kept next.
    frame_cause: FrameCause<'db>,
    /// Every allocation site walked: its block, statement, object node, and
    /// whether it builds a generic class instance.
    sites: Vec<(usize, usize, usize, bool)>,
    locals: usize,
    /// Where the locals of the body being walked start: nonzero only for the
    /// later bodies of a recursive component.
    offset: usize,
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
            frame: vec![None; locals],
            frame_cause: FrameCause::Parameter,
            sites: Vec::new(),
            locals,
            offset: 0,
        }
    }

    /// What `node` holds may outlive the frame for `cause`, unless an
    /// earlier cause already says so.
    fn mark_frame(&mut self, node: usize, cause: FrameCause<'db>) {
        let root = self.find(node);
        if self.frame[root].is_none() {
            self.frame[root] = Some(cause);
        }
    }

    /// A call keeps `operand`'s value past its return, for the current
    /// frame cause. Thread locality is unaffected: the graph still says
    /// where the value goes.
    fn keep(&mut self, operand: &Operand<'_>) {
        if let Some(node) = self.operand(operand) {
            self.mark_frame(node, self.frame_cause);
        }
    }

    fn local(&self, local: usize) -> usize {
        debug_assert!(self.offset + local < self.locals);
        self.offset + local
    }

    fn fresh(&mut self) -> usize {
        let node = self.parent.len();
        self.parent.push(node);
        self.shared.push(false);
        self.contents.push(None);
        self.reason.push(self.cause);
        self.frame.push(None);
        node
    }

    /// A node for a value that may already be reachable from anywhere.
    fn shared(&mut self) -> usize {
        let node = self.fresh();
        self.shared[node] = true;
        self.frame[node] = Some(self.frame_cause);
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
        self.mark_frame(root, self.frame_cause);
    }

    /// [`Self::shared`], for a value shared for `reason` rather than the cause.
    fn shared_because(&mut self, reason: Reason, frame: FrameCause<'db>) -> usize {
        let cause = std::mem::replace(&mut self.cause, reason);
        let frame_cause = std::mem::replace(&mut self.frame_cause, frame);
        let node = self.shared();
        self.cause = cause;
        self.frame_cause = frame_cause;
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
                Constant::GlobalItem(_) => {
                    Some(self.shared_because(Reason::Constant, FrameCause::Global))
                }
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
            Place::Capture(_) | Place::Deref(_) => {
                self.shared_because(Reason::Cell, FrameCause::Captured)
            }
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
            Place::Capture(_) | Place::Deref(_) => {
                self.shared_because(Reason::Cell, FrameCause::Captured)
            }
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
                let object = self.rvalue(value);
                if let (
                    Some(object),
                    Reason::At {
                        block,
                        statement: Some(statement),
                    },
                ) = (object, self.cause)
                    && let Some(generic) = allocates(value)
                {
                    self.sites.push((block, statement, object, generic));
                }
                self.write(destination, object);
            }
            StatementKind::Drop(_) | StatementKind::Nop => {}
            StatementKind::FreshCell { local, .. } => {
                let node = self.local(local.0);
                self.frame_cause = FrameCause::Captured;
                self.mark_shared(node);
            }
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
            Rvalue::Aggregate { kind, fields } => {
                let object = self.fresh();
                let contents = Some(self.contents_of(Some(object)));
                for field in fields {
                    let field = self.operand(field);
                    self.flow(contents, field);
                }
                // The collector runs a garbage instance's finalizer in a VM
                // of its own, while the task that built the instance may
                // still hold what its fields hold.
                if let crate::AggregateKind::Class { class, .. } = kind
                    && self.types.may_finalize(class)
                {
                    let cause = std::mem::replace(&mut self.frame_cause, FrameCause::Finalizer);
                    // The finalizer observes the instance itself after the
                    // frame is gone.
                    self.mark_frame(object, FrameCause::Finalizer);
                    let shared = self.shared();
                    self.flow(Some(shared), contents);
                    self.frame_cause = cause;
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
                self.frame_cause = FrameCause::Captured;
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
            | Rvalue::ConcatStrings(_)
            | Rvalue::SatisfiesBound { .. } => None,
        }
    }

    /// Apply a builtin's effect to arguments [`Walker::matches_builtin`]
    /// accepted, returning the node its result belongs to.
    fn apply_builtin(&mut self, effect: &Effect, args: &[Operand<'_>]) -> Option<usize> {
        // An immutable receiver (a string) holds no objects: no contents.
        let receiver = self.operand(&args[0]);
        let contents = receiver.map(|receiver| self.contents_of(Some(receiver)));
        if effect.leaks_contents
            && let Some(contents) = contents
        {
            self.mark_shared(contents);
        }
        for &kept in effect.keeps {
            if contents.is_some() {
                let value = self.operand(&args[kept]);
                self.flow(contents, value);
            } else {
                // Kept by something this graph does not name.
                self.leak(&args[kept]);
            }
        }
        match &effect.returns {
            Returns::Nothing => None,
            Returns::Element => contents,
            Returns::Fresh(sources) => {
                let copy = self.fresh();
                let copy_contents = Some(self.contents_of(Some(copy)));
                self.flow(copy_contents, contents);
                for &source in *sources {
                    let source = self.operand(&args[source]);
                    let source_contents = Some(self.contents_of(source));
                    self.flow(copy_contents, source_contents);
                }
                Some(copy)
            }
        }
    }

    /// The summary of the body whose locals start at `offset`, as a callee
    /// with `arity` parameters, before its parameters are classified.
    fn summary(&mut self, offset: usize, arity: usize) -> Summary {
        let resolved = self.resolve();
        let roots: Vec<usize> = (0..=arity.min(self.locals.saturating_sub(offset + 1)))
            .map(|local| resolved.roots[offset + local])
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
        // Stored into another parameter's object: a keep the graph alone
        // would let a caller holding that object on its stack miss.
        let stored = |node: usize| {
            roots[1..].iter().any(|&param| {
                let mut current = resolved.contents[param];
                let mut seen = Vec::new();
                while let Some(inner) = current {
                    if inner == node {
                        return true;
                    }
                    if seen.contains(&inner) {
                        return false;
                    }
                    seen.push(inner);
                    current = resolved.contents[inner];
                }
                false
            })
        };
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
            kept: roots[1..]
                .iter()
                .map(|&param| resolved.frame[param].is_some() || stored(param))
                .collect(),
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
            if self.frame[a].is_none() {
                self.frame[a] = self.frame[b];
            }
            match (self.contents[a], self.contents[b]) {
                (Some(x), Some(y)) => pending.push((x, y)),
                (None, Some(y)) => self.contents[a] = Some(y),
                (_, None) => {}
            }
        }
    }

    /// Resolve every node to its root and push sharing into contents, with
    /// each node's locality conditioned on the `arity` parameters.
    fn finish(mut self, arity: usize) -> EscapeFacts {
        let params = arity.min(self.locals.saturating_sub(1));
        // Beyond the mask, a parameter's argument may already be shared.
        for local in MASKED_PARAMS + 1..=params {
            self.mark_shared(local);
        }
        // The caller owns what its arguments hold.
        for local in 1..=params {
            self.mark_frame(local, FrameCause::Parameter);
        }
        let resolved = self.resolve();
        let needs = resolved.param_needs(params.min(MASKED_PARAMS));
        let db = self.types.db;
        let sites: Vec<SiteFacts> = self
            .sites
            .iter()
            .map(|&(block, statement, node, generic)| SiteFacts {
                block: crate::BlockId(block),
                statement,
                group: GroupId(resolved.roots[node]),
                generic,
            })
            .collect();
        let site_index = sites
            .iter()
            .enumerate()
            .map(|(index, site)| ((site.block.0, site.statement), index))
            .collect();
        EscapeFacts {
            twin_needs: Vec::new(),
            call_args: Vec::new(),
            local_node: resolved.roots[..self.locals].to_vec(),
            shared: needs.iter().map(|&needs| needs != 0).collect(),
            contents: resolved.contents,
            needs,
            escape: resolved
                .frame
                .iter()
                .map(|cause| cause.map(|cause| cause.describe(db)))
                .collect(),
            sites,
            site_index,
        }
    }

    /// Every node's root, and per root its contents and whether sharing
    /// reaches it, from itself or from any node whose contents it is.
    fn resolve(&mut self) -> Resolved<'db> {
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
        // Likewise, what an escaping object holds escapes with it.
        let mut frame: Vec<Option<FrameCause<'db>>> = vec![None; nodes];
        let mut pending: Vec<usize> = (0..nodes)
            .filter(|&node| roots[node] == node && self.frame[node].is_some())
            .collect();
        for &node in &pending {
            frame[node] = self.frame[node];
        }
        while let Some(node) = pending.pop() {
            if let Some(inner) = contents[node]
                && frame[inner].is_none()
            {
                frame[inner] = frame[node];
                pending.push(inner);
            }
        }
        Resolved {
            roots,
            shared,
            contents,
            reasons,
            frame,
        }
    }
}

/// A graph's nodes after union-find, with sharing propagated.
struct Resolved<'db> {
    roots: Vec<usize>,
    shared: Vec<bool>,
    contents: Vec<Option<usize>>,
    reasons: Vec<Reason>,
    /// Per root: why what it holds may outlive the frame, from itself or
    /// from any node whose contents it is.
    frame: Vec<Option<FrameCause<'db>>>,
}

impl Resolved<'_> {
    /// Per node, the first `params` parameters it is thread-local given: a
    /// node is reachable from a parameter's argument when it is the
    /// parameter's node or within its contents, and marking that parameter
    /// shared would make exactly those nodes shared.
    fn param_needs(&self, params: usize) -> Vec<u64> {
        let mut needs: Vec<u64> = self
            .shared
            .iter()
            .map(|&shared| if shared { NEVER } else { 0 })
            .collect();
        for param in 1..=params {
            let bit = 1 << (param - 1);
            let mut node = Some(self.roots[param]);
            while let Some(current) = node {
                if needs[current] & bit != 0 {
                    break;
                }
                needs[current] |= bit;
                node = self.contents[current];
            }
        }
        needs
    }
}

#[cfg(test)]
mod effects_mirror_tests {
    /// The builtin generator's copy of the EFFECTS keys, read by path so the
    /// compiler takes no dependency on the generator.
    const GENERATOR_LIST: &str = include_str!("../../baml_builtins2_codegen/src/escape_modeled.rs");

    /// Every quoted string in the generator's list, in order.
    fn generator_names() -> Vec<&'static str> {
        GENERATOR_LIST.split('"').skip(1).step_by(2).collect()
    }

    #[test]
    fn effects_table_matches_the_generators_escape_modeled_list() {
        let effects: Vec<&str> = super::EFFECTS.iter().map(|(path, _)| *path).collect();
        assert_eq!(
            effects,
            generator_names(),
            "update baml_builtins2_codegen/src/escape_modeled.rs to match EFFECTS"
        );
    }
}
