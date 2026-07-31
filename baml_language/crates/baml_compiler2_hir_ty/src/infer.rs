//! Body type inference: `infer_body` walks one body owner's expression tree
//! with an [`InferenceContext`] over an [`unify::InferenceTable`].
//!
//! S6 scope: literals (with freshness), `let` bindings (annotations lowered
//! minimally, `_` holes as fresh variables, fresh literals widening at the
//! binding site), blocks, `return`, and local path resolution through the
//! semantic index. Everything else records the `Error` sentinel (the
//! slice-local rule: constructs the engine does not handle yet infer to
//! `Error` silently), and constructs are upgraded slice by slice.

pub mod unify;

use baml_compiler2_ast::{
    Expr, ExprBody, ExprId, PatId, Pattern, Stmt, StmtId, traverse::BodyNode,
};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    scope::FileScopeId,
    semantic_index::{
        BindingKind, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex, PathResolution,
    },
};
use baml_type::{
    Freshness, Literal, TyAttr,
    interned::{Ty, TyKind},
};
use rustc_hash::FxHashMap;

use crate::{
    infer::unify::InferenceTable,
    lower::{LowerCtx, function_generic_frame, function_signature, lower_ctx_for_file},
};

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
    let index = baml_compiler2_ppir::file_semantic_index(db, owner.file(db));
    let owner_scope = baml_compiler2_ppir::body_scope(db, owner).map(|s| s.file_scope_id(db));
    // The owner's generic frame makes `T` in body annotations resolve; the
    // signature gives parameter references their types.
    let (frame, param_tys) = match owner {
        BodyOwnerId::Function(function) => {
            let signature = function_signature(db, function);
            (
                function_generic_frame(db, function),
                signature.params.into_iter().map(|param| param.ty).collect(),
            )
        }
        BodyOwnerId::Let(_) => (Vec::new(), Vec::new()),
    };
    let lower = lower_ctx_for_file(db, owner.file(db)).with_frame(frame);
    let mut ctx = InferenceContext::new(index, owner_scope, lower, param_tys);
    if let Some(expr_body) = body.expr_body() {
        ctx.infer_expr_body(expr_body);
    }
    ctx.finish()
}

/// One inference run over one body owner: the table, the accumulating
/// result, and the expectation-free expression walk (expectations arrive
/// with `Sub` constraints in S7).
struct InferenceContext<'db> {
    index: &'db FileSemanticIndex<'db>,
    /// The owner body's scope: the key half mapping this body's `ExprId`s
    /// into the semantic index's per-file tables, and the guard that keeps
    /// parameter lookups from crossing into lambda scopes.
    owner_scope: Option<FileScopeId>,
    /// Lowering for body-position type annotations, carrying the owner's
    /// generic frame.
    lower: LowerCtx<'db>,
    /// The owner's parameter types, from its lowered signature, indexed by
    /// declaration position.
    param_tys: Vec<Ty>,
    table: InferenceTable,
    result: InferenceResult,
}

impl<'db> InferenceContext<'db> {
    fn new(
        index: &'db FileSemanticIndex<'db>,
        owner_scope: Option<FileScopeId>,
        lower: LowerCtx<'db>,
        param_tys: Vec<Ty>,
    ) -> InferenceContext<'db> {
        InferenceContext {
            index,
            owner_scope,
            lower,
            param_tys,
            table: InferenceTable::new(),
            result: InferenceResult::default(),
        }
    }

    fn infer_expr_body(&mut self, body: &ExprBody) {
        if let Some(root) = body.root_expr {
            self.infer_expr(body, root);
        }
        // Patterns the walk did not type (destructures, match arms - later
        // slices) record the sentinel so coverage stays visible.
        for (pat_id, _) in body.patterns.iter() {
            self.result
                .type_of_binding
                .entry(pat_id)
                .or_insert_with(Ty::error);
        }
    }

    fn infer_expr(&mut self, body: &ExprBody, expr: ExprId) -> Ty {
        let ty = match &body.exprs[expr] {
            Expr::Literal(lit) => Ty::intern(TyKind::Literal(
                lit.clone(),
                Freshness::Fresh,
                TyAttr::default(),
            )),
            Expr::Null => Ty::null(),
            Expr::Path(_) => self.infer_path(expr),
            Expr::Block { stmts, tail_expr } => {
                for stmt in stmts {
                    self.infer_stmt(body, *stmt);
                }
                match tail_expr {
                    Some(tail) => self.infer_expr(body, *tail),
                    None => Ty::void(),
                }
            }
            Expr::Return { value } => {
                // The signature check against the returned value arrives
                // with declaration lowering (S4/S8).
                if let Some(value) = value {
                    self.infer_expr(body, *value);
                }
                Ty::never()
            }
            Expr::Lambda(def) => {
                // A lambda's body is not a traversal child but IS typed by
                // the owner's run; the lambda's own type arrives in S9.
                if let Some(lambda_body) = def.body {
                    self.infer_expr(body, lambda_body);
                }
                Ty::error()
            }
            // Not yet implemented: visit children generically, record the
            // sentinel.
            _ => {
                let mut children = Vec::new();
                body.expr_children(expr, &mut children);
                for node in children {
                    match node {
                        BodyNode::Expr(child) => {
                            self.infer_expr(body, child);
                        }
                        BodyNode::Stmt(child) => self.infer_stmt(body, child),
                    }
                }
                Ty::error()
            }
        };
        self.result.type_of_expr.insert(expr, ty.clone());
        ty
    }

    fn infer_stmt(&mut self, body: &ExprBody, stmt: StmtId) {
        match &body.stmts[stmt] {
            Stmt::Expr(expr) => {
                self.infer_expr(body, *expr);
            }
            Stmt::Let {
                pattern,
                initializer,
                else_branch,
                ..
            } => {
                let init_ty = initializer.map(|init| self.infer_expr(body, init));
                if let Some(else_expr) = else_branch {
                    // Must-diverge checking arrives with S7's Diverges.
                    self.infer_expr(body, *else_expr);
                }
                self.infer_let_pattern(body, *pattern, init_ty.as_ref());
            }
            _ => {
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
        }
    }

    /// Types a `let` binding pattern: annotation if present (with `_` holes
    /// as fresh vars, filled from the initializer), else the initializer's
    /// type with fresh literals widened at the binding site.
    fn infer_let_pattern(&mut self, body: &ExprBody, pattern: PatId, init_ty: Option<&Ty>) {
        let binding_ty = match &body.patterns[pattern] {
            Pattern::Bind { subpat, .. } => {
                let annotation = subpat.and_then(|sub| match &body.patterns[sub] {
                    Pattern::Type(type_expr) => Some(type_expr),
                    _ => None,
                });
                match annotation {
                    Some(type_expr) => {
                        let lower = &self.lower;
                        let table = &mut self.table;
                        let annotation_ty =
                            lower.lower_type_expr(type_expr, &mut || table.new_var_ty());
                        if let Some(init_ty) = init_ty {
                            // Eq against the widened initializer is the S6
                            // interim check; Sub constraints (S7) replace it
                            // and mismatches become diagnostics (S17).
                            let _ = self
                                .table
                                .unify(&annotation_ty, &widen_fresh_literal(init_ty));
                        }
                        annotation_ty
                    }
                    None => init_ty.map(widen_fresh_literal).unwrap_or_else(Ty::error),
                }
            }
            // Destructuring patterns: later slices.
            _ => Ty::error(),
        };
        self.result.type_of_binding.insert(pattern, binding_ty);
    }

    /// Resolves a path expression to a local binding or an owner parameter
    /// through the semantic index. Non-local names (functions, constants)
    /// resolve in later slices.
    fn infer_path(&mut self, expr: ExprId) -> Ty {
        let Some(owner_scope) = self.owner_scope else {
            return Ty::error();
        };
        let key = ExprMetadataKey::new(ExprMetadataScope::Body(owner_scope), expr);
        match self.index.path_resolution(key) {
            Some(PathResolution::Local(binding_id)) => match binding_id.kind {
                BindingKind::Local(_) => self
                    .index
                    .local_binding(binding_id)
                    .and_then(|binding| self.result.type_of_binding.get(&binding.pattern))
                    .cloned()
                    .unwrap_or_else(Ty::error),
                // Owner parameters only: a lambda scope's parameters are
                // typed by the S9 expectation machinery, not the owner's
                // signature.
                BindingKind::Parameter(param_index) if binding_id.scope == owner_scope => self
                    .param_tys
                    .get(param_index)
                    .cloned()
                    .unwrap_or_else(Ty::error),
                BindingKind::Parameter(_) => Ty::error(),
            },
            Some(PathResolution::Unknown) | None => Ty::error(),
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

/// A fresh literal widens to its base primitive at binding sites (the spec's
/// TypeScript-style widening); everything else passes through. Top-level
/// only - container-element widening arrives with the join machinery.
fn widen_fresh_literal(ty: &Ty) -> Ty {
    let TyKind::Literal(literal, Freshness::Fresh, attr) = ty.kind() else {
        return ty.clone();
    };
    let attr = attr.clone();
    Ty::intern(match literal {
        Literal::Int(_) => TyKind::Int { attr },
        Literal::Bigint(_) => TyKind::Bigint { attr },
        Literal::Float(_) => TyKind::Float { attr },
        Literal::String(_) => TyKind::String { attr },
        Literal::Bool(_) => TyKind::Bool { attr },
    })
}
