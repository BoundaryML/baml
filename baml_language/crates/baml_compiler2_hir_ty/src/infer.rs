//! Body type inference: `infer_body` walks one body owner's expression tree
//! with an [`InferenceContext`] over an [`unify::InferenceTable`].
//!
//! S5 scope: the walk and the table exist; every visited expression and
//! binding records the `Error` sentinel (the slice-local rule: constructs
//! the engine does not handle yet infer to `Error` silently, so fixtures
//! run end to end and the dump snapshots show walk coverage). S6 starts
//! replacing `Error` with real types, bottom-up.

pub mod unify;

use baml_compiler2_ast::{Expr, ExprBody, ExprId, PatId, traverse::BodyNode};
use baml_compiler2_hir::body::BodyOwnerId;
use baml_type::interned::Ty;
use rustc_hash::FxHashMap;

use crate::infer::unify::InferenceTable;

/// Inference side tables for one body owner, keyed by arena ids, mirroring
/// rust-analyzer's `InferenceResult`. Types are the hash-consed
/// `baml_type::interned` representation (this crate's native vocabulary);
/// they are materialized to plain `baml_type::Ty` only at consumer
/// boundaries, after resolve-all guarantees no inference variables remain.
/// Grows one map per slice; consumers must treat a missing entry as "not
/// inferred", never as an error.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct InferenceResult {
    pub type_of_expr: FxHashMap<ExprId, Ty>,
    pub type_of_binding: FxHashMap<PatId, Ty>,
}

/// Infers types for one body owner (function or top-level let), keyed by the
/// S1 `BodyOwnerId` (rust-analyzer's `DefWithBodyId` shape). Lambdas are
/// typed inside their owner's run; parameter defaults get their own
/// inference root later. Becomes a salsa query when the incremental firewall
/// work (S3) lands.
pub fn infer_body<'db>(
    db: &'db dyn baml_compiler2_ppir::Db,
    owner: BodyOwnerId<'db>,
) -> InferenceResult {
    let body = baml_compiler2_ppir::body(db, owner);
    let mut ctx = InferenceContext::new();
    if let Some(expr_body) = body.expr_body() {
        ctx.infer_expr_body(expr_body);
    }
    ctx.finish()
}

/// One inference run over one body owner: the table, the accumulating
/// result, and (from S6 on) the expectation-driven expression walk.
struct InferenceContext {
    table: InferenceTable,
    result: InferenceResult,
}

impl InferenceContext {
    fn new() -> InferenceContext {
        InferenceContext {
            table: InferenceTable::new(),
            result: InferenceResult::default(),
        }
    }

    fn infer_expr_body(&mut self, body: &ExprBody) {
        if let Some(root) = body.root_expr {
            self.infer_expr(body, root);
        }
        // Bindings: S6 types them from initializers/annotations; until then
        // every pattern in the arena records the sentinel so coverage is
        // visible and caret lookups find a (wrong) type instead of nothing.
        for (pat_id, _) in body.patterns.iter() {
            self.result.type_of_binding.insert(pat_id, Ty::error());
        }
    }

    /// The recursive walk. S5: visits every reachable node - including
    /// lambda bodies, which hang off `Expr::Lambda` rather than appearing as
    /// children - and records the `Error` sentinel for each expression.
    fn infer_expr(&mut self, body: &ExprBody, expr: ExprId) -> Ty {
        let mut children = Vec::new();
        body.expr_children(expr, &mut children);
        for node in children {
            match node {
                BodyNode::Expr(child) => {
                    self.infer_expr(body, child);
                }
                BodyNode::Stmt(stmt) => self.infer_stmt(body, stmt),
            }
        }
        // A lambda's body is deliberately not a child (it belongs to the
        // lambda value), but it IS typed by the owner's inference run.
        if let Expr::Lambda(def) = &body.exprs[expr]
            && let Some(lambda_body) = def.body
        {
            self.infer_expr(body, lambda_body);
        }

        let ty = Ty::error();
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    fn infer_stmt(&mut self, body: &ExprBody, stmt: baml_compiler2_ast::StmtId) {
        let mut children = Vec::new();
        body.stmt_children(stmt, &mut children);
        for node in children {
            match node {
                BodyNode::Expr(child) => {
                    self.infer_expr(body, child);
                }
                BodyNode::Stmt(child) => self.infer_stmt(body, child),
            }
        }
    }

    /// Substitutes solved variables out of every recorded type. The S13
    /// finalization invariant (no `Infer` reaches the result) lands with
    /// defaulting; the substitution pass exists from day one so nothing is
    /// built on tables that still contain live variables.
    fn finish(mut self) -> InferenceResult {
        let mut result = self.result;
        for ty in result
            .type_of_expr
            .values_mut()
            .chain(result.type_of_binding.values_mut())
        {
            *ty = self.table.resolve_completely(ty);
        }
        result
    }
}
