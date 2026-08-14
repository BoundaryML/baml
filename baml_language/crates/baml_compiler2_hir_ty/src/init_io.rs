//! Check-time detection of io reachable from `$init` (E0158).
//!
//! # What `$init` is
//!
//! Every **top-level binding with an initializer** is evaluated once, eagerly,
//! by the synthesized `$init` chainer that `baml_compiler2_emit` appends to the
//! program tail (see `is_synth_init_name` there). Today the only surface that
//! produces one is `client Foo = <expr>;`, which `lower_client_value_def`
//! lowers to a [`LetOrigin::Client`][baml_compiler2_ast::ast::LetOrigin]
//! top-level let — but the analysis is keyed on the *item* (every
//! [`LetLoc`]), not on the `client` keyword, so any future top-level binding
//! form is covered the day it lands.
//!
//! `$init` runs when the engine is *created*, before any host request, on a
//! path that cannot suspend: the VM has no scheduler turn to yield into yet.
//!
//! So an io sysop reached from a top-level initializer does not fail with a
//! BAML error the user can catch — it kills engine construction with an
//! opaque `InitFailed("$init yielded unexpectedly: SysOp BamlEnvGet")`. That
//! is a check-time-detectable mistake, and this module is the check.
//!
//! # The analysis
//!
//! A body is **io-tainted** when its evaluated path either calls an io sysop
//! directly (a `$rust_io_function` builtin — [`BuiltinKind::Io`]) or calls a
//! function that is itself tainted. Two memoized layers:
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

/// Wrapper so the tracked query can return by reference under salsa's
/// comparison-based `Update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluatedCalls<'db>(pub Vec<EvaluatedCall<'db>>);

// Safety: comparison-based replacement for salsa early cutoff, matching
// `FileThrowFacts` in `throw_facts.rs`.
#[allow(unsafe_code)]
unsafe impl salsa::Update for EvaluatedCalls<'_> {
    unsafe fn maybe_update(old_pointer: *mut Self, new_value: Self) -> bool {
        // SAFETY: pointer is salsa-owned and valid for replacement.
        #[allow(unsafe_code)]
        let old = unsafe { &*old_pointer };
        if old == &new_value {
            false
        } else {
            #[allow(unsafe_code)]
            unsafe {
                std::ptr::drop_in_place(old_pointer);
                std::ptr::write(old_pointer, new_value);
            }
            true
        }
    }
}

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
    db: &'db dyn baml_compiler2_ppir::Db,
    owner: BodyOwnerId<'db>,
) -> &'db [EvaluatedCall<'db>] {
    match owner {
        BodyOwnerId::Function(function) => &function_evaluated_calls(db, function).0,
        BodyOwnerId::Let(let_binding) => &let_evaluated_calls(db, let_binding).0,
        // Parameter defaults are evaluated per CALL, not at `$init`.
        BodyOwnerId::ParameterDefaults(_) => &[],
    }
}

#[salsa::tracked(returns(ref))]
fn function_evaluated_calls<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    function: FunctionLoc<'db>,
) -> EvaluatedCalls<'db> {
    evaluated_calls_impl(db, BodyOwnerId::Function(function))
}

#[salsa::tracked(returns(ref))]
fn let_evaluated_calls<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    let_binding: LetLoc<'db>,
) -> EvaluatedCalls<'db> {
    evaluated_calls_impl(db, BodyOwnerId::Let(let_binding))
}

fn evaluated_calls_impl<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    owner: BodyOwnerId<'db>,
) -> EvaluatedCalls<'db> {
    let owner_body = baml_compiler2_ppir::body(db, owner);
    let body: &ExprBody = match &owner_body {
        OwnerBody::Function(function) => match function.as_ref() {
            FunctionBody::Expr(body) => body,
            // A builtin has no BAML body: its io-ness is a property of the
            // declaration, read by `io_sysop_of` — not an edge.
            FunctionBody::Builtin(_) | FunctionBody::Missing => return EvaluatedCalls(Vec::new()),
        },
        OwnerBody::Let(let_body) => match let_body.as_ref() {
            LetBody::Expr(body) => body,
            LetBody::Missing => return EvaluatedCalls(Vec::new()),
        },
        // Parameter defaults are evaluated per CALL, not at `$init`.
        OwnerBody::ParameterDefaults(_) => return EvaluatedCalls(Vec::new()),
    };
    let Some(root) = body.root_expr else {
        return EvaluatedCalls(Vec::new());
    };

    let inference = crate::infer::infer_body(db, owner);

    // Callee expressions on the evaluated path. Walking from the root with
    // lambdas excluded is the "what actually runs" filter; immediately-invoked
    // lambdas are re-seeded below.
    let mut callee_exprs: Vec<ExprId> = Vec::new();
    let mut seen_lambda_bodies: HashSet<ExprId> = HashSet::new();
    let mut roots = vec![root];
    while let Some(root) = roots.pop() {
        for node in body.reachable_excluding_lambdas(root) {
            let BodyNode::Expr(id) = node else { continue };
            let callee = match &body.exprs[id] {
                Expr::Call { callee, .. } | Expr::OptionalCall { callee, .. } => *callee,
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
    EvaluatedCalls(out)
}

/// The statically-known callee behind a member resolution, when there is one.
///
/// A virtual interface call resolves to a slot, not a body — no edge (see the
/// under-approximation note in the module docs). Fields and enum variants are
/// not calls at all.
fn resolved_function<'db>(
    resolution: &crate::infer::MemberResolution<'db>,
) -> Option<FunctionLoc<'db>> {
    use crate::infer::MemberResolution;
    match resolution {
        MemberResolution::Free { func }
        | MemberResolution::BoundMethod { func, .. }
        | MemberResolution::UnboundMethod { func, .. }
        | MemberResolution::InterfaceConcreteMethod { func, .. } => Some(*func),
        MemberResolution::Field { .. }
        | MemberResolution::Variant { .. }
        | MemberResolution::InterfaceVirtualMethod { .. }
        | MemberResolution::InterfaceVirtualField { .. } => None,
    }
}

/// The fully-qualified name of `func` when it is an io sysop
/// (`$rust_io_function`), else `None`.
fn io_sysop_of<'db>(db: &'db dyn baml_compiler2_ppir::Db, func: FunctionLoc<'db>) -> Option<Name> {
    let body = baml_compiler2_hir::body::function_body(db, func);
    match body.as_ref() {
        FunctionBody::Builtin(BuiltinKind::Io) => Some(qualified_name(db, func)),
        _ => None,
    }
}

/// `func`'s user-facing dotted name (`baml.env.get`), rendered through the
/// type system's own qualifier so the package half follows the one rule
/// (dependency packages qualify; the user's own package never does).
fn qualified_name<'db>(db: &'db dyn baml_compiler2_ppir::Db, func: FunctionLoc<'db>) -> Name {
    let data = baml_compiler2_ppir::item_data::function_data(db, func);
    let pkg = baml_compiler2_hir::file_package::file_package(db, func.file(db));
    Name::new(
        baml_type::QualifiedTypeName::new(pkg.package, pkg.namespace_path, data.name.clone())
            .render_user_facing(),
    )
}

/// Whether `func` transitively reaches an io sysop, and which one.
///
/// Depth-first over the memoized [`body_evaluated_calls`] graph, on an
/// explicit stack rather than the call stack — the graph's depth is the
/// user's, not ours. `visited` carries across the whole walk (including
/// sibling hops): a body already proven clean cannot become tainted, and a
/// body already on the path is a cycle, which adds nothing either way.
fn io_sysop_reached_from<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    func: FunctionLoc<'db>,
    visited: &mut HashSet<FunctionLoc<'db>>,
) -> Option<Name> {
    let mut stack = vec![func];
    while let Some(func) = stack.pop() {
        if !visited.insert(func) {
            continue;
        }
        if let Some(sysop) = io_sysop_of(db, func) {
            return Some(sysop);
        }
        // Reversed so the first-written callee is explored first — the search
        // order, and so the reported sysop, follows source order.
        for (_, callee) in body_evaluated_calls(db, BodyOwnerId::Function(func))
            .iter()
            .rev()
        {
            stack.push(*callee);
        }
    }
    None
}

/// E0158 for one top-level declaration: its initializer reaches an io sysop,
/// which `$init` cannot run.
///
/// Returns at most one diagnostic — the first offending hop in the
/// initializer — because every later one has the same cause and the same fix.
pub fn let_init_io_diagnostics<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    let_loc: LetLoc<'db>,
) -> Vec<(TextRange, TirTypeError)> {
    let calls = body_evaluated_calls(db, BodyOwnerId::Let(let_loc));
    if calls.is_empty() {
        return Vec::new();
    }

    // One visited set for the whole declaration: a helper shared by two hops
    // is walked once, and the FIRST hop that reaches io is the one reported.
    let mut visited: HashSet<FunctionLoc<'db>> = HashSet::new();
    for (callee_expr, func) in calls {
        let direct = io_sysop_of(db, *func);
        let sysop = match &direct {
            Some(sysop) => Some(sysop.clone()),
            None => io_sysop_reached_from(db, *func, &mut visited),
        };
        let Some(sysop) = sysop else { continue };

        let Some(source_map) = baml_compiler2_ppir::body_source_map(db, BodyOwnerId::Let(let_loc))
        else {
            return Vec::new();
        };
        let data = baml_compiler2_ppir::item_data::let_data(db, let_loc);
        return vec![(
            source_map.expr_span(*callee_expr),
            TirTypeError::InitIoNotAllowed {
                declaration: data.name.clone(),
                is_client: matches!(data.origin, baml_compiler2_ast::ast::LetOrigin::Client),
                sysop,
                // A direct call needs no "via": the hop IS the sysop.
                via: direct.is_none().then(|| qualified_name(db, *func)),
            },
        )];
    }
    Vec::new()
}
