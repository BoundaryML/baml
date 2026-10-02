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
//! { if (!baml._cleanup_begin(self)) { return; } { <body> } }
//! ```
//!
//! `baml._cleanup_begin` atomically test-and-sets the receiver instance's
//! per-instance latch (see `bex_vm_types::CleanupLatch`) and returns `true` only
//! for the first invocation, so every later call returns before the body.
//! Because the guard lives in the callee, it applies uniformly to every call
//! path with no call-site special-casing.
//!
//! The guard is an early return rather than an `if` around the body so that
//! the user's body stays the function's value: it is checked against the unit
//! return type exactly as any other body is, and a diagnostic about it speaks
//! of the code the user wrote, not of a branch they did not.

use baml_base::Name;

use crate::ast::{CallArg, ClassDef, Expr, FunctionBodyDef, FunctionDef, Stmt, UnaryOp};

/// The reserved magic-method name for the BEP-042 finalizer.
pub const CLEANUP_METHOD: &str = "cleanup";

/// Whether `func` reads as a class's finalizer: it is named `cleanup` and
/// takes `self` alone, the receiver the run-once guard latches on. Whether it
/// *is* the finalizer is decided on its checked signature
/// (`baml_compiler2_hir_ty::cleanup`), where an alias of `void` is the unit
/// return it must have. A method guarded here that turns out malformed is an
/// error (E0144), so its guard never runs.
fn reads_as_finalizer(func: &FunctionDef) -> bool {
    func.name.as_str() == CLEANUP_METHOD
        && matches!(func.params.as_slice(), [receiver] if receiver.name.as_str() == "self")
}

/// Wrap the body of a class's magic `cleanup` method in the run-once guard. A
/// no-op for classes without one. Runs as a pure AST transform during CST
/// lowering, so the guard is type-checked by the normal pipeline.
pub fn maybe_inject_cleanup_guard(class: &mut ClassDef) {
    for method in &mut class.methods {
        if !reads_as_finalizer(method) {
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

        // Build `{ if (!baml._cleanup_begin(self)) { return; } <orig body> }`.
        // Allocate into the body's arenas while keeping the source map's span
        // arenas index-aligned (one span per allocated node), exactly as the
        // parser maintains them in lockstep. Every synthesized node carries
        // the method-name span and is marked as compiler-generated.
        let expr = |body: &mut crate::ast::ExprBody,
                    source_map: &mut crate::ast::AstSourceMap,
                    expr: Expr| {
            let id = body.exprs.alloc(expr);
            source_map.expr_spans.alloc(span);
            source_map.synthetic_exprs.insert(id);
            id
        };
        let stmt = |body: &mut crate::ast::ExprBody,
                    source_map: &mut crate::ast::AstSourceMap,
                    stmt: Stmt| {
            let id = body.stmts.alloc(stmt);
            source_map.stmt_spans.alloc(span);
            source_map.synthetic_stmts.insert(id);
            id
        };

        // `baml._cleanup_begin` — the fully-qualified public path, as seen from
        // user code (the stdlib's internal `root.` self-reference is not in
        // scope here; user code reaches stdlib functions via `baml.*`).
        let callee = expr(
            body,
            source_map,
            Expr::Path(vec![Name::new("baml"), Name::new("_cleanup_begin")]),
        );
        let self_arg = expr(body, source_map, Expr::Path(vec![Name::new("self")]));
        let first_call = expr(
            body,
            source_map,
            Expr::Call {
                callee,
                type_args: vec![],
                args: vec![CallArg::positional(self_arg)],
            },
        );
        let already_ran = expr(
            body,
            source_map,
            Expr::Unary {
                op: UnaryOp::Not,
                expr: first_call,
            },
        );
        let bail = stmt(body, source_map, Stmt::Return(None));
        let bail_block = expr(
            body,
            source_map,
            Expr::Block {
                stmts: vec![bail],
                tail_expr: None,
            },
        );
        let guard = expr(
            body,
            source_map,
            Expr::If {
                condition: already_ran,
                then_branch: bail_block,
                else_branch: None,
            },
        );
        let guard_stmt = stmt(body, source_map, Stmt::Expr(guard));
        let guarded = expr(
            body,
            source_map,
            Expr::Block {
                stmts: vec![guard_stmt],
                tail_expr: Some(orig_root),
            },
        );

        body.root_expr = Some(guarded);
    }
}
