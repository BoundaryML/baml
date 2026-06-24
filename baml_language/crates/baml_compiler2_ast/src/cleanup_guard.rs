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

use crate::ast::{CallArg, ClassDef, Expr, FunctionBodyDef, FunctionDef, TypeExpr};

/// The reserved magic-method name for the BEP-042 finalizer.
pub const CLEANUP_METHOD: &str = "cleanup";

/// `true` if `func` is the magic `cleanup` finalizer: named `cleanup` *and*
/// having the required shape (see [`has_cleanup_shape`]). A method named
/// `cleanup` with any other shape is a *malformed* finalizer (reported by HIR as
/// `CleanupMagicMethodSignature`), not a magic method.
pub fn is_cleanup_magic_method(func: &FunctionDef) -> bool {
    func.name.as_str() == CLEANUP_METHOD && has_cleanup_shape(func)
}

/// The required shape of the magic `cleanup` method: exactly one parameter
/// `self`, no generic parameters, and an explicit `-> void` return. Shared by
/// the guard injector (which only rewrites well-shaped finalizers) and the HIR
/// signature check (which reports a method named `cleanup` that fails this).
/// The name itself is checked by [`is_cleanup_magic_method`] / the HIR pass.
pub fn has_cleanup_shape(func: &FunctionDef) -> bool {
    func.generic_params.is_empty()
        && func.params.len() == 1
        && func.params[0].name.as_str() == "self"
        && matches!(
            func.return_type.as_ref().map(|st| &st.expr),
            Some(TypeExpr::Void { .. })
        )
}

/// Wrap the body of a class's magic `cleanup` method in the run-once guard. A
/// no-op for classes without a (well-shaped) `cleanup`. Runs as a pure AST
/// transform alongside `auto_derive_json`, so the guard is type-checked by the
/// normal pipeline.
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

        // Build `if (root._cleanup_begin(self)) { <orig body> }`. Allocate into
        // the body's expr arena while keeping the source map's `expr_spans`
        // arena index-aligned (one span per allocated expr), exactly as the
        // parser and `auto_derive_json` maintain the two in lockstep. All four
        // synthesized nodes carry the method-name span.
        // `baml._cleanup_begin` — the fully-qualified public path, as seen from
        // user code (the stdlib's internal `root.` self-reference is not in
        // scope here, the same way `auto_derive_json` calls `baml.json.*`).
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
