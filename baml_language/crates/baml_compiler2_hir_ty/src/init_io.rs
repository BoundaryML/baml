//! Check-time detection of io and suspension reachable from `$init` (E0163,
//! E0182).
//!
//! # What `$init` is
//!
//! Every **top-level binding with an initializer** is evaluated once, eagerly,
//! by the synthesized `$init` chainer that `baml_compiler2_emit` appends to the
//! program tail. Two surfaces produce one: a global variable (`const NAME =
//! <expr>;` at file scope) and `client Foo = <expr>;`, which
//! `lower_client_value_def` lowers to a
//! [`LetOrigin::Client`][baml_compiler2_ast::ast::LetOrigin] top-level let.
//! The analysis is keyed on the *item* (every [`LetLoc`]), not on either
//! keyword.
//!
//! `$init` runs when the engine is *created*, before any host request, on a
//! path that cannot suspend: the VM has no scheduler turn to yield into yet.
//!
//! So an io sysop reached from a top-level initializer does not fail with a
//! BAML error the user can catch — it kills engine construction with an
//! opaque `InitFailed("$init yielded unexpectedly: SysOp BamlEnvGet")`. A
//! `spawn` (there is no scheduler to launch onto) or an `await` (nothing to
//! wait on) fails the same way. Those are check-time-detectable mistakes,
//! and this module is the check.
//!
//! # The analysis
//!
//! A body is **tainted** when its evaluated path performs an init-unsafe
//! effect directly — calls an io sysop (a `$rust_io_function` builtin,
//! [`BuiltinKind::Io`]), launches a task (`baml.spawn.__spawn`, which `spawn
//! { … }` lowers to), or suspends (`await`, `baml.future.__await_any`) — or
//! calls a function that is itself tainted. Two memoized layers:
//!
//! 1. [`body_evaluated_calls`] — per body owner, the callees resolved on the
//!    body's *evaluated* path. Salsa-tracked, so the expensive half (reading
//!    inference's `member_resolutions`) is paid once per body no matter how
//!    many declarations reach it.
//! 2. [`let_init_io_diagnostics`] — per top-level binding, a depth-first walk
//!    over that memoized graph with its own visited set. Doing the traversal
//!    *outside* salsa is deliberate: recursion in user code would otherwise
//!    be a query cycle, and the visited set makes it a non-event.
//!
//! # Closures
//!
//! "Evaluated path" excludes lambda bodies
//! ([`ExprBody::reachable_excluding_lambdas`]), with one exception: a lambda
//! in *callee* position (`((…) -> { … })()`) is immediately invoked, so its
//! body is walked. A lambda merely *stored* — passed to a constructor, put in
//! a field, returned — contributes nothing, because nothing runs it during
//! `$init`. This is load-bearing, not a shortcut: a stored thunk that reads
//! the environment when the runner later calls it is exactly the pattern this
//! feature wants people to use.
//!
//! A `spawn { … }` body IS walked, unlike a lambda: the two look alike but a
//! spawn is not a stored thunk — it is a launch, and `$init` has no scheduler
//! turn to launch onto, so io inside one is just as fatal as io written
//! inline.
//!
//! The analysis is therefore a deliberate UNDER-approximation. It never
//! guesses: an indirect call through a local (`let f = …; f()`), a virtual
//! interface method, or a lambda handed to a higher-order function resolves
//! to no `FunctionLoc` and is skipped. False positives here would block valid
//! programs; a false negative merely leaves today's runtime failure in place.

use std::collections::HashSet;

use baml_base::Name;
use baml_compiler2_ast::{BodyNode, BuiltinKind, Expr, ExprBody, ExprId};
use baml_compiler2_hir::{
    body::{BodyOwnerId, FunctionBody, LetBody, OwnerBody},
    loc::{FunctionLoc, LetLoc},
};
use text_size::TextRange;

use crate::diagnostics::TirTypeError;

/// One resolved call on a body's evaluated path: the callee expression it was
/// written at (for span anchoring) and the function it resolved to.
pub type EvaluatedCall<'db> = (ExprId, FunctionLoc<'db>);

/// A body's evaluated path: the calls it makes, and the `await`s it performs
/// itself. Wrapper so the tracked query can return by reference under salsa's
/// retention rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedCalls<'db> {
    pub calls: Vec<EvaluatedCall<'db>>,
    /// `await` expressions on the evaluated path, in arena order.
    pub awaits: Vec<ExprId>,
}

impl EvaluatedCalls<'_> {
    const EMPTY: Self = EvaluatedCalls {
        calls: Vec::new(),
        awaits: Vec::new(),
    };
}

// SAFETY: This type owns its data. Its database lifetime only appears
// in Salsa identities; it contains no references into query storage.
#[allow(unsafe_code)]
unsafe impl salsa::SalsaValue for EvaluatedCalls<'_> {}

/// The callees a body invokes on its evaluated path, in arena order.
///
/// Resolution comes from inference's `member_resolutions` (keyed on the CALLEE
/// expression) and, for value-rooted paths, the last rung of the
/// `path_resolutions` ladder — the same tables MIR lowers calls from, so an
/// edge here is a call the VM will really make.
///
/// A plain dispatcher over the per-loc tracked queries, mirroring
/// [`crate::infer::infer_body`]: `BodyOwnerId` is an ordinary enum, not a
/// salsa struct, so it cannot key a tracked function itself.
pub fn body_evaluated_calls<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    owner: BodyOwnerId<'db>,
) -> &'db EvaluatedCalls<'db> {
    static EMPTY: EvaluatedCalls<'static> = EvaluatedCalls::EMPTY;
    match owner {
        BodyOwnerId::Function(function) => function_evaluated_calls(db, function),
        BodyOwnerId::Let(let_binding) => let_evaluated_calls(db, let_binding),
        // Parameter defaults are evaluated per CALL, not at `$init`.
        BodyOwnerId::ParameterDefaults(_) => &EMPTY,
    }
}

#[salsa::tracked(returns(ref))]
fn function_evaluated_calls<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    function: FunctionLoc<'db>,
) -> EvaluatedCalls<'db> {
    evaluated_calls_impl(db, BodyOwnerId::Function(function))
}

#[salsa::tracked(returns(ref))]
fn let_evaluated_calls<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    let_binding: LetLoc<'db>,
) -> EvaluatedCalls<'db> {
    evaluated_calls_impl(db, BodyOwnerId::Let(let_binding))
}

fn evaluated_calls_impl<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    owner: BodyOwnerId<'db>,
) -> EvaluatedCalls<'db> {
    let owner_body = baml_compiler2_hir::body::body(db, owner);
    let body: &ExprBody = match &owner_body {
        OwnerBody::Function(function) => match function.as_ref() {
            FunctionBody::Expr(body) => body,
            // A builtin has no BAML body: its io-ness is a property of the
            // declaration, read by `io_sysop_of` — not an edge.
            FunctionBody::Builtin(_) | FunctionBody::Missing => return EvaluatedCalls::EMPTY,
        },
        OwnerBody::Let(let_body) => match let_body.as_ref() {
            LetBody::Expr(body) => body,
            LetBody::Missing => return EvaluatedCalls::EMPTY,
        },
        // Parameter defaults are evaluated per CALL, not at `$init`.
        OwnerBody::ParameterDefaults(_) => return EvaluatedCalls::EMPTY,
    };
    let Some(root) = body.root_expr else {
        return EvaluatedCalls::EMPTY;
    };

    let inference = crate::infer::infer_body(db, owner);

    // Callee expressions on the evaluated path. Walking from the root with
    // lambdas excluded is the "what actually runs" filter; immediately-invoked
    // lambdas are re-seeded below.
    let mut callee_exprs: Vec<ExprId> = Vec::new();
    let mut awaits: Vec<ExprId> = Vec::new();
    let mut seen_lambda_bodies: HashSet<ExprId> = HashSet::new();
    let mut roots = vec![root];
    while let Some(root) = roots.pop() {
        for node in body.reachable_excluding_lambdas(root) {
            let BodyNode::Expr(id) = node else { continue };
            // A `spawn { … }` body is a launch, not a stored thunk, so it runs
            // during `$init` too (module docs, "Closures"). The sugar lowers it
            // to a lambda marked `LambdaKind::Spawn` and hands it to
            // `Plan.new`, so the lambda-excluding walk stops at it — re-seed it
            // here, the same way an IIFE is re-seeded below. Without this, io
            // inside a spawn in a top-level initializer passes the check and
            // dies at runtime as an opaque `InitFailed`, which is the failure
            // this module exists to replace. A plan built by hand is an
            // ordinary stored closure and gets the ordinary treatment: nothing
            // runs until it is launched.
            if let Expr::Lambda(lambda) = &body.exprs[id]
                && lambda.kind == baml_compiler2_ast::LambdaKind::Spawn
                && let Some(lambda_body) = lambda.body
                && seen_lambda_bodies.insert(lambda_body)
            {
                roots.push(lambda_body);
            }
            let callee = match &body.exprs[id] {
                Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => *callee,
                Expr::Await { .. } => {
                    awaits.push(id);
                    continue;
                }
                _ => continue,
            };
            callee_exprs.push(callee);
            // An IIFE runs now, so its body joins the evaluated path.
            if let Expr::Lambda(lambda) = &body.exprs[callee]
                && let Some(lambda_body) = lambda.body
                && seen_lambda_bodies.insert(lambda_body)
            {
                roots.push(lambda_body);
            }
        }
    }

    let mut out: Vec<EvaluatedCall<'db>> = Vec::new();
    for callee in callee_exprs {
        let resolution = inference.member_resolutions.get(&callee).or_else(|| {
            // `a.b.c()` with a VALUE root records its ladder here instead; the
            // call targets the last rung.
            inference
                .path_resolutions
                .get(&callee)
                .and_then(|path| path.segments.last())
                .and_then(|segment| segment.resolution.as_ref())
        });
        if let Some(func) = resolution.and_then(resolved_function) {
            out.push((callee, func));
        }
    }
    // Arena order is deterministic and (for a single body) close enough to
    // source order that the reported hop is the first one written.
    out.sort_by_key(|(expr, _)| *expr);
    out.dedup();
    awaits.sort();
    awaits.dedup();
    EvaluatedCalls { calls: out, awaits }
}

/// The statically-known callee behind a member resolution, when there is one.
///
/// A virtual interface call resolves to a slot, not a body — no edge (see the
/// under-approximation note in the module docs). Fields and enum variants are
/// not calls at all.
pub(crate) fn resolved_function<'db>(
    resolution: &crate::infer::MemberResolution<'db>,
) -> Option<FunctionLoc<'db>> {
    use baml_compiler2_hir::loc::DeclRef;

    use crate::infer::{MemberResolution, MethodCallee};
    let func = match resolution {
        MemberResolution::Free { func }
        | MemberResolution::Method {
            callee: MethodCallee::Inherent(func) | MethodCallee::Concrete { func, .. },
            ..
        } => *func,
        MemberResolution::Method {
            callee: MethodCallee::Virtual { .. },
            ..
        }
        | MemberResolution::Field { .. }
        | MemberResolution::Variant { .. }
        | MemberResolution::InterfaceVirtualField { .. } => return None,
    };
    match func {
        DeclRef::Source(func) => Some(func),
        // A callee of a package served from its interface: no body to walk,
        // so the analysis skips it — the same deliberate under-approximation
        // as virtual dispatch (module docs).
        DeclRef::External(_) => None,
    }
}

/// The fully-qualified name of `func` when it is an io sysop
/// (`$rust_io_function`), else `None`.
fn io_sysop_of<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    viewer: baml_base::SourceRoot,
    func: FunctionLoc<'db>,
) -> Option<Name> {
    let body = baml_compiler2_hir::body::function_body(db, func);
    match body.as_ref() {
        FunctionBody::Builtin(BuiltinKind::Io) => Some(qualified_name(db, viewer, func)),
        _ => None,
    }
}

/// `func`'s user-facing dotted name (`baml.env.get`) as seen from `viewer`:
/// a dependency package qualifies by its spelling, the viewer's own package
/// never does.
fn qualified_name<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    viewer: baml_base::SourceRoot,
    func: FunctionLoc<'db>,
) -> Name {
    let data = baml_compiler2_hir::item_data::function_data(db, func);
    let pkg = baml_compiler2_hir::file_package::file_package(db, func.file(db));
    let mut parts: Vec<&str> = Vec::new();
    let spelled;
    if pkg.root != viewer {
        // The viewer's own edge name for the package, not its canonical
        // spelling: a dependency reached under an alias is named in source by
        // that alias, and a diagnostic that says otherwise names something the
        // reader cannot find in their own file.
        let viewpoint = crate::render::Viewpoint::user_facing(db, viewer);
        spelled = match viewpoint.package_prefix(pkg.root) {
            Some(prefix) => Name::new(prefix),
            None => baml_compiler2_hir::package::spelling(db)
                .of(pkg.root)
                .clone(),
        };
        parts.push(spelled.as_str());
    }
    parts.extend(pkg.namespace_path.iter().map(Name::as_str));
    parts.push(data.name.as_str());
    Name::new(parts.join("."))
}

/// An effect `$init` cannot perform.
#[derive(Debug, Clone, PartialEq, Eq)]
enum InitEffect {
    /// An io sysop, by its fully-qualified name (`baml.env.get`).
    Io(Name),
    /// Launching a task (`spawn { … }`, `baml.spawn.__spawn`).
    Spawn,
    /// Suspending on a future (`await`, `baml.future.__await_any`).
    Await,
}

/// The effect `func` performs by being called, when it is an effectful
/// builtin; `None` for an ordinary function (whose body the walk descends
/// into instead).
fn builtin_effect_of<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    viewer: baml_base::SourceRoot,
    func: FunctionLoc<'db>,
) -> Option<InitEffect> {
    match io_sysop_of(db, viewer, func) {
        Some(sysop) => Some(InitEffect::Io(sysop)),
        None => match baml_compiler2_hir::body::function_body(db, func).as_ref() {
            FunctionBody::Builtin(BuiltinKind::Spawn) => Some(InitEffect::Spawn),
            FunctionBody::Builtin(BuiltinKind::AwaitAny) => Some(InitEffect::Await),
            _ => None,
        },
    }
}

/// Whether calling `func` transitively performs an init-unsafe effect, and
/// which one. Io wins over suspension when one call reaches both — a helper
/// that spawns a task to read the environment is reported for the read, the
/// more specific cause — and otherwise the first effect in source order.
///
/// Depth-first over the memoized [`body_evaluated_calls`] graph, on an
/// explicit stack rather than the call stack — the graph's depth is the
/// user's, not ours. `visited` carries across the whole walk (including
/// sibling hops): a body already proven clean cannot become tainted, and a
/// body already on the path is a cycle, which adds nothing either way.
fn effect_reached_from<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    viewer: baml_base::SourceRoot,
    func: FunctionLoc<'db>,
    visited: &mut HashSet<FunctionLoc<'db>>,
) -> Option<InitEffect> {
    let mut suspension = None;
    let mut stack = vec![func];
    while let Some(func) = stack.pop() {
        if !visited.insert(func) {
            continue;
        }
        match builtin_effect_of(db, viewer, func) {
            Some(io @ InitEffect::Io(_)) => return Some(io),
            Some(effect) => {
                suspension.get_or_insert(effect);
                continue;
            }
            None => {}
        }
        let evaluated = body_evaluated_calls(db, BodyOwnerId::Function(func));
        if !evaluated.awaits.is_empty() {
            suspension.get_or_insert(InitEffect::Await);
        }
        // Reversed so the first-written callee is explored first — the search
        // order, and so the reported effect, follows source order.
        for (_, callee) in evaluated.calls.iter().rev() {
            stack.push(*callee);
        }
    }
    suspension
}

/// E0163 / E0182 for one top-level declaration: its initializer reaches io,
/// `spawn`, or `await`, none of which `$init` can run.
///
/// Returns at most one diagnostic — the first offending hop in the
/// initializer — because every later one has the same cause and the same fix.
pub fn let_init_io_diagnostics<'db>(
    db: &'db dyn baml_compiler2_hir::Db,
    let_loc: LetLoc<'db>,
) -> Vec<(TextRange, TirTypeError)> {
    let evaluated = body_evaluated_calls(db, BodyOwnerId::Let(let_loc));
    let viewer = baml_compiler2_hir::file_package::file_package(db, let_loc.file(db)).root;
    if evaluated.calls.is_empty() && evaluated.awaits.is_empty() {
        return Vec::new();
    }
    let Some(source_map) = baml_compiler2_hir::body::body_source_map(db, BodyOwnerId::Let(let_loc))
    else {
        return Vec::new();
    };
    let data = baml_compiler2_hir::item_data::let_data(db, let_loc);
    let is_client = matches!(data.origin, baml_compiler2_ast::ast::LetOrigin::Client);

    // Every offending site in the initializer, as (anchor, effect, via):
    // direct `await`s, then each call hop. The first in source order wins.
    // One visited set for the whole declaration: a helper shared by two hops
    // is walked once, and the FIRST hop that reaches an effect is reported.
    let mut visited: HashSet<FunctionLoc<'db>> = HashSet::new();
    let mut sites: Vec<(ExprId, InitEffect, Option<Name>)> = evaluated
        .awaits
        .iter()
        .map(|&expr| (expr, InitEffect::Await, None))
        .collect();
    for (callee_expr, func) in &evaluated.calls {
        let direct = builtin_effect_of(db, viewer, *func);
        let effect = match &direct {
            Some(effect) => Some(effect.clone()),
            None => effect_reached_from(db, viewer, *func, &mut visited),
        };
        if let Some(effect) = effect {
            // A direct call needs no "via": the hop IS the effect.
            let via = direct.is_none().then(|| qualified_name(db, viewer, *func));
            sites.push((*callee_expr, effect, via));
        }
    }
    let Some((anchor, effect, via)) = sites.into_iter().min_by_key(|(expr, ..)| *expr) else {
        return Vec::new();
    };
    let declaration = data.name.clone();
    let error = match effect {
        InitEffect::Io(sysop) => TirTypeError::InitIoNotAllowed {
            declaration,
            is_client,
            sysop,
            via,
        },
        InitEffect::Spawn => TirTypeError::InitSuspendNotAllowed {
            declaration,
            is_client,
            operation: "spawn",
            via,
        },
        InitEffect::Await => TirTypeError::InitSuspendNotAllowed {
            declaration,
            is_client,
            operation: "await",
            via,
        },
    };
    vec![(source_map.expr_span(anchor), error)]
}
