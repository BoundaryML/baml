//! Global variables: what a top-level binding needs before its initializer can
//! run, and the order `$init` runs the initializers in.
//!
//! # What a global is
//!
//! A top-level let binding — a global variable written in source (`const NAME
//! = …`), a `client Foo = …` declaration, or a Session's persisted binding — is
//! evaluated once, eagerly,
//! by its package's synthesized `$init`, before any request runs. Its value is
//! then frozen into the engine's shared global table, so an initializer that
//! reads another global must run after that global's own initializer.
//!
//! # The dependency rule
//!
//! This is Go's package-initialization rule, and for the same reason: it is
//! decidable from the source alone, it never depends on values, and its errors
//! are stable under edits elsewhere. A global `a` **depends on** a global `b`
//! of the same package when `a`'s initializer refers to `b`, or refers to a
//! function whose body (or parameter defaults) refers to `b`, transitively.
//!
//! - *Refers* is lexical. A lambda the initializer builds counts even though
//!   nothing may call it during `$init`: a stored thunk can be invoked by a
//!   later initializer, and deciding whether one is would need the values.
//! - References are taken from inference's resolution tables
//!   ([`InferenceResult::global_roots`] and the member resolutions), never from
//!   spellings, so a local that shadows a global is not an edge, and a
//!   qualified `root.ns.x` is.
//! - The walk stays inside the package. A dependency's bodies cannot name this
//!   package's globals, and its own globals are initialized first, by its own
//!   `$init` (the linker orders packages).
//! - Dynamic dispatch (a virtual interface call, a function value received as
//!   an argument) resolves to no body here, so it adds no edge. The engine
//!   reports a read of a global that has not been initialized yet, so such a
//!   program fails loudly at startup rather than reading a placeholder.
//!
//! Globals run in dependency order; independent globals run in source order
//! (file path, then position), which does not depend on the order files were
//! loaded in. A cycle is a compile error (E0181), reported once, at the first
//! global of the cycle in source order.

use std::collections::{BTreeSet, VecDeque};

use baml_base::{Name, SourceFile, SourceRoot};
use baml_compiler2_ast::ExprId;
use baml_compiler2_hir::{
    body::BodyOwnerId,
    item_data::{file_lets, let_data},
    loc::{FunctionLoc, LetLoc},
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::infer::InferenceResult;

/// Whether `binding` is a global variable written in source — as opposed to a
/// `client` declaration or a Session's persisted binding. Only source globals
/// can be annotated, and only their initializers are held to the
/// initializer-is-an-expression rules (no top-level `return`).
pub fn is_source_global(db: &dyn baml_compiler2_hir::Db, binding: LetLoc<'_>) -> bool {
    matches!(
        let_data(db, binding).origin,
        baml_compiler2_ast::LetOrigin::Source
    ) && !binding.file(db).is_session_submission(db)
}

/// The type every reader of global `binding` sees: its annotation when it has
/// one, else its initializer's type widened as at any binding site (`let n =
/// 5` is an `int`, not the literal `5`). `None` when the initializer is
/// missing or was not typed.
///
/// The one definition of a global's type: the checker's reads within the
/// package and the package interface's export both take it from here.
pub fn global_value_type(
    db: &dyn baml_compiler2_hir::Db,
    binding: LetLoc<'_>,
) -> Option<baml_type::Ty> {
    if let Some(declared) = crate::lower::let_declared_type(db, binding) {
        return Some(declared.clone());
    }
    let body = baml_compiler2_hir::body::let_body(db, binding);
    let baml_compiler2_hir::body::LetBody::Expr(body) = body.as_ref() else {
        return None;
    };
    let inference = crate::infer::infer_body(db, BodyOwnerId::Let(binding));
    let root_ty = inference.type_of_expr.get(&body.root_expr?)?;
    Some(root_ty.clone().widen_fresh())
}

/// What one body refers to that can matter to initialization order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InitRefs<'db> {
    /// Globals read, each at the first expression (arena order) that reads it.
    pub globals: Vec<(ExprId, LetLoc<'db>)>,
    /// Same-package functions referred to — called, or taken as a value —
    /// each at the first expression that does.
    pub functions: Vec<(ExprId, FunctionLoc<'db>)>,
}

// SAFETY: This type owns its data. Its database lifetime only appears in Salsa
// identities; it contains no references into query storage.
#[allow(unsafe_code)]
unsafe impl salsa::SalsaValue for InitRefs<'_> {}

fn body_init_refs<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    owner: BodyOwnerId<'db>,
    package: SourceRoot,
    out: &mut InitRefs<'db>,
) {
    let inference: &InferenceResult<'db> = crate::infer::infer_body(db, owner);
    let same_package =
        |file: SourceFile| baml_compiler2_hir::file_package::file_package(db, file).root == package;
    for (&expr, root) in &inference.global_roots {
        // A served package's global belongs to another package, initialized
        // by that package's `$init` first.
        if let baml_compiler2_hir::loc::DeclRef::Source(binding) = root.binding
            && same_package(binding.file(db))
        {
            out.globals.push((expr, binding));
        }
    }
    let mut push_function = |expr: ExprId, resolution: &crate::infer::MemberResolution<'db>| {
        if let Some(function) = crate::init_io::resolved_function(resolution)
            && same_package(function.file(db))
        {
            out.functions.push((expr, function));
        }
    };
    for (&expr, resolution) in &inference.member_resolutions {
        push_function(expr, resolution);
    }
    // A value-rooted ladder (`x.method().field`) resolves its intermediate
    // segments here rather than in `member_resolutions`.
    for (&expr, path) in &inference.path_resolutions {
        for segment in &path.segments {
            if let Some(resolution) = &segment.resolution {
                push_function(expr, resolution);
            }
        }
    }
}

fn finish_refs(mut refs: InitRefs<'_>) -> InitRefs<'_> {
    // Arena order is deterministic (and close to source order), so the anchor
    // kept for each target is the first place the body names it.
    refs.globals.sort_by_key(|(expr, _)| *expr);
    let mut seen = FxHashSet::default();
    refs.globals.retain(|(_, global)| seen.insert(*global));
    refs.functions.sort_by_key(|(expr, _)| *expr);
    let mut seen = FxHashSet::default();
    refs.functions
        .retain(|(_, function)| seen.insert(*function));
    refs
}

/// What a global's initializer refers to.
#[salsa::tracked(returns(ref))]
pub fn let_init_refs<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    binding: LetLoc<'db>,
) -> InitRefs<'db> {
    let package = baml_compiler2_hir::file_package::file_package(db, binding.file(db)).root;
    let mut refs = InitRefs::default();
    body_init_refs(db, BodyOwnerId::Let(binding), package, &mut refs);
    finish_refs(refs)
}

/// What a function refers to when it runs: its body and, because a call
/// evaluates them, its parameter defaults.
#[salsa::tracked(returns(ref))]
pub fn function_init_refs<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> InitRefs<'db> {
    let package = baml_compiler2_hir::file_package::file_package(db, function.file(db)).root;
    let mut refs = InitRefs::default();
    body_init_refs(db, BodyOwnerId::Function(function), package, &mut refs);
    let has_defaults = baml_compiler2_hir::signature::function_parameter_defaults(db, function)
        .params
        .iter()
        .any(Option::is_some);
    if has_defaults {
        body_init_refs(
            db,
            BodyOwnerId::ParameterDefaults(function),
            package,
            &mut refs,
        );
    }
    finish_refs(refs)
}

/// One global another global's initializer needs, and how it is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalDep<'db> {
    pub global: LetLoc<'db>,
    /// The expression in the DEPENDENT initializer the path starts at: the
    /// global's own read, or the call/reference of the first function.
    pub anchor: ExprId,
    /// The functions the path passes through, in call order (empty when the
    /// initializer reads the global directly).
    pub via: Vec<FunctionLoc<'db>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalDeps<'db>(pub Vec<GlobalDep<'db>>);

// SAFETY: This type owns its data. Its database lifetime only appears in Salsa
// identities; it contains no references into query storage.
#[allow(unsafe_code)]
unsafe impl salsa::SalsaValue for GlobalDeps<'_> {}

/// The same-package globals `binding`'s initializer needs initialized first,
/// each with the shortest path that needs it. May include `binding` itself.
///
/// A breadth-first walk over the memoized per-body reference tables, on an
/// explicit queue: recursion in user code is a revisit, not a query cycle.
#[salsa::tracked(returns(ref))]
pub fn global_init_deps<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    binding: LetLoc<'db>,
) -> GlobalDeps<'db> {
    let refs = let_init_refs(db, binding);
    let mut deps = Vec::new();
    let mut seen_globals = FxHashSet::default();
    for &(anchor, global) in &refs.globals {
        if seen_globals.insert(global) {
            deps.push(GlobalDep {
                global,
                anchor,
                via: Vec::new(),
            });
        }
    }
    let mut visited = FxHashSet::default();
    let mut queue = VecDeque::new();
    for &(anchor, function) in &refs.functions {
        if visited.insert(function) {
            queue.push_back((function, anchor, vec![function]));
        }
    }
    while let Some((function, anchor, via)) = queue.pop_front() {
        let refs = function_init_refs(db, function);
        for &(_, global) in &refs.globals {
            if seen_globals.insert(global) {
                deps.push(GlobalDep {
                    global,
                    anchor,
                    via: via.clone(),
                });
            }
        }
        for &(_, callee) in &refs.functions {
            if visited.insert(callee) {
                let mut path = via.clone();
                path.push(callee);
                queue.push_back((callee, anchor, path));
            }
        }
    }
    GlobalDeps(deps)
}

/// One initialization cycle, reported at its first global in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalCycle<'db> {
    /// The global the cycle is reported at.
    pub global: LetLoc<'db>,
    /// Where in that global's initializer the cycle starts.
    pub anchor: ExprId,
    /// The cycle as names, starting and ending at `global`, with every
    /// function it passes through.
    pub chain: Vec<Name>,
}

/// How a set of globals is initialized: the order, and any cycles.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalInitPlan<'db> {
    /// The order `$init` evaluates the initializers in. Every global of the
    /// set appears once; members of a cycle (a compile error) are placed in
    /// source order after everything they need outside the cycle.
    pub order: Vec<LetLoc<'db>>,
    pub cycles: Vec<GlobalCycle<'db>>,
}

// SAFETY: This type owns its data. Its database lifetime only appears in Salsa
// identities; it contains no references into query storage.
#[allow(unsafe_code)]
unsafe impl salsa::SalsaValue for GlobalInitPlan<'_> {}

/// The initialization plan for every global of `root` (the package `$init`).
#[salsa::tracked(returns(ref))]
pub fn package_global_init_plan<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    root: SourceRoot,
) -> GlobalInitPlan<'db> {
    let lets: Vec<LetLoc<'db>> = root
        .files(db)
        .iter()
        .flat_map(|&file| file_lets(db, file).iter().copied())
        .collect();
    plan_global_init(db, &lets)
}

/// Source order: file path, then position. Independent of the order files
/// were added to the database, which is not stable across processes.
fn source_key(db: &dyn baml_compiler2_hir::Db, binding: LetLoc<'_>) -> (std::path::PathBuf, u32) {
    let span = baml_compiler2_hir::item_data::let_source_map(db, binding).span;
    (binding.file(db).path(db), u32::from(span.start()))
}

/// The initialization plan for `lets`: dependencies on globals outside the
/// set are already satisfied (an earlier package's `$init`, an earlier
/// Session submission) and impose no order.
pub fn plan_global_init<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    lets: &[LetLoc<'db>],
) -> GlobalInitPlan<'db> {
    let mut sorted: Vec<LetLoc<'db>> = lets.to_vec();
    sorted.sort_by_cached_key(|&binding| source_key(db, binding));
    sorted.dedup();
    let index: FxHashMap<LetLoc<'db>, usize> = sorted
        .iter()
        .enumerate()
        .map(|(position, &binding)| (binding, position))
        .collect();
    // Edges in source-position space, restricted to the set.
    let edges: Vec<Vec<usize>> = sorted
        .iter()
        .map(|&binding| {
            global_init_deps(db, binding)
                .0
                .iter()
                .filter_map(|dep| index.get(&dep.global).copied())
                .collect()
        })
        .collect();

    let components = strongly_connected_components(&edges);
    let mut component_of = vec![0usize; sorted.len()];
    for (component, members) in components.iter().enumerate() {
        for &member in members {
            component_of[member] = component;
        }
    }

    // Kahn's algorithm over the condensation, always taking the ready
    // component whose first member comes first in source order.
    let mut needs = vec![FxHashSet::default(); components.len()];
    let mut needed_by = vec![Vec::new(); components.len()];
    for (from, targets) in edges.iter().enumerate() {
        for &to in targets {
            let (from_c, to_c) = (component_of[from], component_of[to]);
            if from_c != to_c && needs[from_c].insert(to_c) {
                needed_by[to_c].push(from_c);
            }
        }
    }
    let first_member = |component: usize| components[component][0];
    let mut ready: BTreeSet<(usize, usize)> = (0..components.len())
        .filter(|&component| needs[component].is_empty())
        .map(|component| (first_member(component), component))
        .collect();
    let mut order = Vec::with_capacity(sorted.len());
    let mut cycles = Vec::new();
    while let Some((_, component)) = ready.pop_first() {
        let members = &components[component];
        let cyclic = members.len() > 1 || edges[members[0]].contains(&members[0]);
        if cyclic {
            cycles.push(describe_cycle(db, &sorted, &index, members));
        }
        order.extend(members.iter().map(|&member| sorted[member]));
        for &dependent in &needed_by[component] {
            needs[dependent].remove(&component);
            if needs[dependent].is_empty() {
                ready.insert((first_member(dependent), dependent));
            }
        }
    }
    debug_assert_eq!(order.len(), sorted.len(), "the condensation is acyclic");
    GlobalInitPlan { order, cycles }
}

/// Tarjan's algorithm, iterative. Each component's members are sorted, so a
/// component's first member is its earliest global in source order.
fn strongly_connected_components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = edges.len();
    let mut index_of = vec![usize::MAX; n];
    let mut lowlink = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut components = Vec::new();
    let mut next_index = 0usize;
    for start in 0..n {
        if index_of[start] != usize::MAX {
            continue;
        }
        // (node, next edge to visit)
        let mut work = vec![(start, 0usize)];
        index_of[start] = next_index;
        lowlink[start] = next_index;
        next_index += 1;
        stack.push(start);
        on_stack[start] = true;
        while let Some(&mut (node, ref mut edge)) = work.last_mut() {
            if let Some(&target) = edges[node].get(*edge) {
                *edge += 1;
                if index_of[target] == usize::MAX {
                    index_of[target] = next_index;
                    lowlink[target] = next_index;
                    next_index += 1;
                    stack.push(target);
                    on_stack[target] = true;
                    work.push((target, 0));
                } else if on_stack[target] {
                    lowlink[node] = lowlink[node].min(index_of[target]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                lowlink[parent] = lowlink[parent].min(lowlink[node]);
            }
            if lowlink[node] == index_of[node] {
                let mut members = Vec::new();
                while let Some(member) = stack.pop() {
                    on_stack[member] = false;
                    members.push(member);
                    if member == node {
                        break;
                    }
                }
                members.sort_unstable();
                components.push(members);
            }
        }
    }
    components
}

/// The cycle through `members` (one strongly connected component) as the
/// path a reader can follow: from its first global, through every function
/// and global on the way, back to it.
fn describe_cycle<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    sorted: &[LetLoc<'db>],
    index: &FxHashMap<LetLoc<'db>, usize>,
    members: &[usize],
) -> GlobalCycle<'db> {
    let in_component: FxHashSet<usize> = members.iter().copied().collect();
    let start = members[0];
    // Shortest global-to-global path from `start` back to itself, within the
    // component: BFS over dependency hops, remembering the hop taken.
    let mut parent: FxHashMap<usize, (usize, GlobalDep<'db>)> = FxHashMap::default();
    let mut queue = VecDeque::from([start]);
    let mut closing: Option<(usize, GlobalDep<'db>)> = None;
    'search: while let Some(node) = queue.pop_front() {
        for dep in &global_init_deps(db, sorted[node]).0 {
            let Some(&target) = index.get(&dep.global) else {
                continue;
            };
            if !in_component.contains(&target) {
                continue;
            }
            if target == start {
                closing = Some((node, dep.clone()));
                break 'search;
            }
            if target != start && !parent.contains_key(&target) {
                parent.insert(target, (node, dep.clone()));
                queue.push_back(target);
            }
        }
    }
    let (last, closing_dep) =
        closing.unwrap_or_else(|| unreachable!("a cyclic component reaches its first member"));
    // Rebuild the hops start → … → last, then last → start.
    let mut hops = vec![(last, closing_dep)];
    let mut node = last;
    while node != start {
        let (previous, dep) = parent[&node].clone();
        hops.push((previous, dep));
        node = previous;
    }
    hops.reverse();
    let anchor = hops[0].1.anchor;
    let mut chain = vec![let_data(db, sorted[start]).name.clone()];
    for (_, dep) in &hops {
        chain.extend(
            dep.via
                .iter()
                .map(|&function| function_display_name(db, function)),
        );
        chain.push(let_data(db, dep.global).name.clone());
    }
    GlobalCycle {
        global: sorted[start],
        anchor,
        chain,
    }
}

/// A function as a reader finds it in source: `name`, or `Owner.name` for a
/// method.
fn function_display_name(db: &dyn baml_compiler2_hir::Db, function: FunctionLoc<'_>) -> Name {
    let name = baml_compiler2_hir::item_data::function_data(db, function)
        .name
        .clone();
    match baml_compiler2_hir::item_data::method_owner(db, function) {
        Some(baml_compiler2_hir::item_data::MethodOwner::Class(class)) => Name::new(format!(
            "{}.{name}",
            baml_compiler2_hir::item_data::class_data(db, class).name
        )),
        Some(baml_compiler2_hir::item_data::MethodOwner::Interface(interface)) => {
            Name::new(format!(
                "{}.{name}",
                baml_compiler2_hir::item_data::interface_data(db, interface).name
            ))
        }
        _ => name,
    }
}

/// E0181 for one global: the initialization cycle it is the reported member
/// of, if any.
pub fn let_init_cycle_diagnostics<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    binding: LetLoc<'db>,
) -> Vec<(text_size::TextRange, crate::diagnostics::TirTypeError)> {
    let package = baml_compiler2_hir::file_package::file_package(db, binding.file(db)).root;
    let plan = package_global_init_plan(db, package);
    let Some(cycle) = plan.cycles.iter().find(|cycle| cycle.global == binding) else {
        return Vec::new();
    };
    let span = baml_compiler2_hir::body::body_source_map(db, BodyOwnerId::Let(binding))
        .map(|source_map| source_map.expr_span(cycle.anchor))
        .unwrap_or_else(|| baml_compiler2_hir::item_data::let_source_map(db, binding).name_span);
    vec![(
        span,
        crate::diagnostics::TirTypeError::GlobalInitializationCycle {
            chain: cycle.chain.clone(),
        },
    )]
}

/// E0183 (a warning) for one global variable: its initializer can throw, and
/// nothing in it catches the throw.
///
/// An uncaught throw in `$init` has no caller to reach: it stops the engine
/// from being created. That is often what a program wants — a configuration
/// that does not parse should keep it from starting — so this is a warning
/// that names the risk, not an error; a `catch` in the initializer settles it
/// either way (`baml.sys.panic` keeps the refusal explicit).
///
/// Only source globals are judged. A `client` declaration's constructor may
/// validate its arguments (`ai.RoundRobin` of no clients), and a Session
/// binding's initializer runs inside `eval`, whose caller receives the throw
/// as an ordinary `EvaluationError`.
pub fn let_init_throw_diagnostics<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    binding: LetLoc<'db>,
) -> Vec<(text_size::TextRange, crate::diagnostics::TirTypeError)> {
    if !is_source_global(db, binding) {
        return Vec::new();
    }
    let inference = crate::infer::infer_body(db, BodyOwnerId::Let(binding));
    if matches!(inference.throws, baml_type::Ty::Never)
        || baml_type::contains_ty_where(&inference.throws, &|ty| matches!(ty, baml_type::Ty::Error))
    {
        return Vec::new();
    }
    let data = let_data(db, binding);
    vec![(
        baml_compiler2_hir::item_data::let_source_map(db, binding).name_span,
        crate::diagnostics::TirTypeError::InitThrowNotHandled {
            declaration: data.name.clone(),
            is_client: matches!(data.origin, baml_compiler2_ast::LetOrigin::Client),
            thrown: inference.throws.clone(),
        },
    )]
}
