//! BEP-042 `cleanup` magic-method recognition and run-once guard injection.
//!
//! `cleanup` is a **magic method** recognized by name (like `to_json`), not an
//! interface. If a user class defines `function cleanup(self) -> void { ... }`,
//! that method is its finalizer: it runs at most once per instance, whether it
//! is invoked explicitly, via `defer`, or (Commit 2) by the GC.
//!
//! The "at most once" guarantee is implemented as a compile-time guard wrapped
//! around the user body — keeping the VM's hot dispatch loop untouched. This
//! pass rewrites a recognized `cleanup` body
//!
//! ```ignore
//! { <body> }
//! ```
//!
//! into
//!
//! ```ignore
//! if (baml._cleanup_begin(self)) { <body> }
//! ```
//!
//! `baml._cleanup_begin` atomically test-and-sets the receiver instance's
//! per-instance latch (see `bex_vm_types::CleanupLatch`) and returns `true` only
//! for the first invocation, so every later call skips the body. Because the
//! guard lives in the callee, it applies uniformly to every call path with no
//! call-site special-casing.

use baml_base::Name;

use crate::ast::{CallArg, ClassDef, Expr, FunctionBodyDef, FunctionDef, TypeExpr, TypeExprKind};

/// The reserved magic-method name for the BEP-042 finalizer.
pub const CLEANUP_METHOD: &str = "cleanup";

/// `true` if a `throws` clause is absent **or** is explicitly `throws never` —
/// both mean the function provably cannot fail. The reserved `cleanup` shape
/// requires this (a finalizer must not propagate errors), and `throws never` is
/// the language-blessed spelling of "cannot fail" (the stdlib uses it widely,
/// including on `_cleanup_begin` itself), so it must be accepted, not rejected.
///
/// Takes `Option<&TypeExpr>` so the AST guard/HIR check and the emit-side
/// `has_cleanup` detection (which holds an HIR `Function`, whose `throws` is the
/// same `ast::TypeExpr`) share one source of truth.
pub fn throws_is_effectively_none(throws: Option<&TypeExpr>) -> bool {
    match throws {
        None => true,
        Some(st) => matches!(st.kind, TypeExprKind::Never { .. }),
    }
}

/// `true` if `func` is the magic `cleanup` finalizer: named `cleanup` *and*
/// having the required shape (see [`has_cleanup_shape`]). A method named
/// `cleanup` with any other shape is a *malformed* finalizer (reported by HIR as
/// `CleanupMagicMethodSignature`), not a magic method.
pub fn is_cleanup_magic_method(func: &FunctionDef) -> bool {
    func.name.as_str() == CLEANUP_METHOD && has_cleanup_shape(func)
}

/// The required shape of the magic `cleanup` method: exactly one parameter
/// `self` with no default value, no generic parameters, an explicit `-> void`
/// return, and no `throws` clause. Shared by the guard injector (which only
/// rewrites well-shaped finalizers) and the HIR signature check (which reports a
/// method named `cleanup` that fails this). The name itself is checked by
/// [`is_cleanup_magic_method`] / the HIR pass.
///
/// A propagating `throws` is disallowed because a finalizer must not have an
/// error contract: on the GC path the error has no caller and is swallowed, so a
/// declared `throws Foo` would be silently dropped. A `cleanup` that needs to
/// handle errors must do so internally (`... catch { ... }`). An explicit
/// `throws never` is fine — it provably has no error to drop (see
/// [`throws_is_effectively_none`]).
///
/// A default on `self` (e.g. `cleanup(self = null)`) is rejected too: method
/// parameters accept defaults during lowering (`defaults_allowed`), so without
/// this guard a defaulted `self` would slip through as the reserved shape. The
/// finalizer is only ever invoked with the receiver, so a default is meaningless
/// and not part of the blessed signature.
pub fn has_cleanup_shape(func: &FunctionDef) -> bool {
    func.generic_params.is_empty()
        && func.params.len() == 1
        && func.params[0].name.as_str() == "self"
        && func.params[0].default.is_none()
        && throws_is_effectively_none(func.throws.as_ref())
        && matches!(
            func.return_type.as_ref().map(|st| &st.kind),
            Some(TypeExprKind::Void { .. })
        )
}

/// Wrap the body of a class's magic `cleanup` method in the run-once guard. A
/// no-op for classes without a (well-shaped) `cleanup`. Runs as a pure AST
/// transform during CST lowering, so the guard is type-checked by the normal
/// pipeline.
pub fn maybe_inject_cleanup_guard(class: &mut ClassDef) {
    for method in &mut class.methods {
        if !is_cleanup_magic_method(method) {
            continue;
        }
        let span = method.name_span;
        // A `$rust_function` `cleanup` (Builtin body) has no expression body to
        // guard, and an empty body has no root; neither is expected for a user
        // finalizer, so skip rather than fabricate a body.
        let Some(FunctionBodyDef::Expr(body, source_map)) = method.body.as_mut() else {
            continue;
        };
        let Some(orig_root) = body.root_expr else {
            continue;
        };

        // Build `if (baml._cleanup_begin(self)) { <orig body> }`. Allocate into
        // the body's expr arena while keeping the source map's `expr_spans`
        // arena index-aligned (one span per allocated expr), exactly as the
        // parser maintains the two in lockstep. All four synthesized nodes carry
        // the method-name span.
        // `baml._cleanup_begin` — the fully-qualified public path, as seen from
        // user code (the stdlib's internal `root.` self-reference is not in
        // scope here; user code reaches stdlib functions via `baml.*`).
        let callee = body.exprs.alloc(Expr::Path(vec![
            Name::new("baml"),
            Name::new("_cleanup_begin"),
        ]));
        source_map.expr_spans.alloc(span);

        let self_arg = body.exprs.alloc(Expr::Path(vec![Name::new("self")]));
        source_map.expr_spans.alloc(span);

        let condition = body.exprs.alloc(Expr::Call {
            callee,
            type_args: vec![],
            args: vec![CallArg::positional(self_arg)],
        });
        source_map.expr_spans.alloc(span);

        let guarded = body.exprs.alloc(Expr::If {
            condition,
            then_branch: orig_root,
            else_branch: None,
        });
        source_map.expr_spans.alloc(span);

        body.root_expr = Some(guarded);
    }
}
