//! Body type inference: `infer_body` walks one body owner's expression tree
//! with an [`InferenceContext`] over an [`unify::InferenceTable`].
//!
//! S9 state: bidirectional checking. `infer_expr` synthesizes with an
//! [`Expectation`] flowing down (informing shape: container elements,
//! if-branch pass-through, lambda signature deduction); `check_expr`
//! additionally emits a `Sub` constraint, discharged eagerly per the settled
//! design - invariant heads decay to `Eq`, ground pairs ask the canonical
//! oracle, var-headed pairs deposit bounds, the irreducible residue defers
//! to finish. Control-flow merge points join through `union_of` - the
//! canonical union when members are var-free, syntactic until resolution
//! otherwise (never fabricated at variables - ruling 1);
//! `Diverges` tracks never-propagation. Value paths resolve through one
//! entry (`resolve_value_path`); lambdas deduce unwritten signature slots
//! from the expected function type and their bodies type in the owner's
//! table under the lambda's scope. Constructs the engine does not handle
//! yet still record the `Error` sentinel and upgrade slice by slice.

pub(crate) mod flow;
pub(crate) mod pat;
pub mod unify;

use std::sync::Arc;

use baml_compiler2_ast::{
    Expr, ExprBody, ExprId, PatId, Pattern, Stmt, StmtId, traverse::BodyNode,
};
use baml_compiler2_hir::{
    body::BodyOwnerId,
    body_type_refs::BodyTypeRefs,
    scope::FileScopeId,
    semantic_index::{
        BindingId, BindingKind, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex,
        PathResolution,
    },
};
use baml_type::{
    Freshness, Literal, TyAttr,
    interned::{Ty, TyKind},
    normalize::{canonical_union_interned, is_subtype_interned},
};
use rustc_hash::FxHashMap;

use crate::{
    facts::Facts,
    infer::unify::InferenceTable,
    lower::{
        LowerCtx, function_generic_frame, function_signature, lower_ctx_for_file, substitute_params,
    },
};

/// Inference side tables for one body owner, keyed by arena ids, mirroring
/// rust-analyzer's `InferenceResult`. Types are the hash-consed
/// `baml_type::interned` representation (this crate's native vocabulary);
/// they are materialized to plain `baml_type::Ty` only at consumer
/// boundaries, after resolve-all guarantees no inference variables remain.
/// Grows one map per slice; consumers must treat a missing entry as "not
/// inferred", never as an error.
#[derive(Debug, Clone, PartialEq)]
pub struct InferenceResult {
    pub type_of_expr: FxHashMap<ExprId, Ty>,
    pub type_of_binding: FxHashMap<PatId, Ty>,
    /// The owner's effect: the declared clause when written, else the
    /// canonical union of the body's throw sites and callee throws
    /// (`never` when nothing throws) - S12.
    pub throws: Ty,
    /// Definite check failures, keyed by the checked expression:
    /// `(expected, actual)`. Recorded always (rust-analyzer's discipline);
    /// rendered as diagnostics in S17.
    pub type_mismatches: FxHashMap<ExprId, (Ty, Ty)>,
    /// Match expressions whose unguarded arms do not cover the scrutinee.
    /// The expression types as Error; S17 renders E0062 with witnesses.
    pub non_exhaustive_matches: rustc_hash::FxHashSet<ExprId>,
}

impl Default for InferenceResult {
    fn default() -> InferenceResult {
        InferenceResult {
            type_of_expr: FxHashMap::default(),
            type_of_binding: FxHashMap::default(),
            throws: Ty::never(),
            type_mismatches: FxHashMap::default(),
            non_exhaustive_matches: rustc_hash::FxHashSet::default(),
        }
    }
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
    // signature gives parameter references their types and the body its
    // return expectation.
    let (frame, param_tys, return_ty, declared_throws) = match owner {
        BodyOwnerId::Function(function) => {
            let signature = function_signature(db, function);
            (
                function_generic_frame(db, function),
                signature.params.into_iter().map(|param| param.ty).collect(),
                Some(signature.ret),
                signature.throws_declared.then_some(signature.throws),
            )
        }
        BodyOwnerId::Let(_) => (Vec::new(), Vec::new(), None, None),
    };
    let lower = lower_ctx_for_file(db, owner.file(db)).with_frame(frame);
    let type_refs = baml_compiler2_ppir::body_type_refs(db, owner);
    let source_map = baml_compiler2_ppir::body_source_map(db, owner);
    let mut ctx = InferenceContext::new(
        db,
        index,
        owner_scope,
        lower,
        param_tys,
        return_ty,
        type_refs,
        source_map,
    );
    ctx.declared_throws = declared_throws;
    if let Some(expr_body) = body.expr_body() {
        ctx.infer_expr_body(expr_body);
    }
    ctx.finish()
}

/// Whether execution can proceed past the current point. `Maybe & Maybe`
/// branch-combines to `Maybe`; a `return`/`throw` sets `Always`, and a block
/// whose statements always diverge types as `never`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Diverges {
    Maybe,
    Always,
}

impl Diverges {
    /// Sequential combination: diverged stays diverged.
    fn or(self, other: Diverges) -> Diverges {
        if self == Diverges::Always || other == Diverges::Always {
            Diverges::Always
        } else {
            Diverges::Maybe
        }
    }

    /// Branch combination: all branches must diverge.
    fn and(self, other: Diverges) -> Diverges {
        if self == Diverges::Always && other == Diverges::Always {
            Diverges::Always
        } else {
            Diverges::Maybe
        }
    }
}

/// Contextual type information flowing DOWN the walk - the "check" half of
/// bidirectional inference. Not a bare `Option<Ty>`: the methods encode when
/// context may CONSTRAIN (emit `Sub`) versus merely INFORM (shape container
/// literals, branch pass-through).
#[derive(Debug, Clone)]
enum Expectation {
    None,
    HasType(Ty),
}

impl Expectation {
    /// The `Error` sentinel is never propagated as context. Top-level only
    /// (rust-analyzer's `Expectation::has_type` discipline): a nested
    /// sentinel - e.g. the `throws Error` placeholder inside a function
    /// type until S12 - must not discard the useful structure around it.
    fn has_type(ty: Ty) -> Expectation {
        if matches!(ty.kind(), TyKind::Error { .. }) {
            Expectation::None
        } else {
            Expectation::HasType(ty)
        }
    }

    fn only_has_type(&self) -> Option<&Ty> {
        match self {
            Expectation::HasType(ty) => Some(ty),
            Expectation::None => None,
        }
    }

    /// For if/match arms: drop the expectation when it resolves to a bare
    /// unsolved variable, so the first arm cannot over-constrain the rest -
    /// the arms JOIN at the merge point instead.
    fn adjust_for_branches(&self, table: &mut InferenceTable) -> Expectation {
        match self {
            Expectation::HasType(ty) => {
                let resolved = table.shallow_resolve(ty);
                if matches!(resolved.kind(), TyKind::Infer { .. }) {
                    Expectation::None
                } else {
                    Expectation::HasType(resolved)
                }
            }
            Expectation::None => Expectation::None,
        }
    }
}

/// One inference run over one body owner: the table, the accumulating
/// result, and the bidirectional expression walk.
struct InferenceContext<'db> {
    db: &'db dyn baml_compiler2_ppir::Db,
    facts: Facts<'db>,
    index: &'db FileSemanticIndex<'db>,
    /// The owner body's scope: the key half mapping this body's `ExprId`s
    /// into the semantic index's per-file tables, and the guard that keeps
    /// parameter lookups from crossing into lambda scopes.
    owner_scope: Option<FileScopeId>,
    /// The metadata scope expression lookups key under RIGHT NOW. Equal to
    /// `owner_scope` except while walking a lambda body: the semantic index
    /// keys a lambda body's expressions under the LAMBDA's scope even though
    /// they share the owner's arena (`builder.rs::walk_lambda_expr`), so the
    /// walk must swap this when it descends into one.
    current_scope: Option<FileScopeId>,
    /// Expression spans, for locating the `ScopeKind::Lambda` scope that a
    /// `Expr::Lambda` node opened (scopes are keyed by source range).
    source_map: Option<baml_compiler2_ast::AstSourceMap>,
    /// Parameter types for each lambda scope this run has walked, deduced by
    /// `infer_lambda`; the lambda-scope analog of `param_tys`.
    lambda_params: FxHashMap<FileScopeId, Vec<Ty>>,
    /// The flow-narrowing overlay: refined types for bindings, consulted
    /// before `type_of_binding`. S10a populates it per match arm only;
    /// the S10b condition/branch machinery grows it into the full
    /// environment (design: eager-forward on the structured walk).
    flow: FxHashMap<BindingId, Ty>,
    /// Lowering for body-position type annotations, carrying the owner's
    /// generic frame.
    lower: LowerCtx<'db>,
    /// The owner's parameter types, from its lowered signature, indexed by
    /// declaration position.
    param_tys: Vec<Ty>,
    /// Every type annotation written in this body, pre-lowered to span-free
    /// `TypeRef`s (the rust-analyzer bodies-own-their-type-refs shape).
    type_refs: Arc<BodyTypeRefs>,
    /// The owner's declared return type, the body root's expectation.
    return_ty: Option<Ty>,
    /// The owner's DECLARED throws clause, when written: the contract
    /// every throw site and callee effect is checked against. `None`
    /// means the effect is inferred instead (from the channel below).
    declared_throws: Option<Ty>,
    /// The effect-channel stack: contributions from `throw` sites and
    /// callee throws accumulate into the top. The bottom entry is the
    /// owner's channel; lambdas and `catch` bases push their own.
    throws_channels: Vec<Vec<Ty>>,
    table: InferenceTable,
    /// Operator impls, built lazily on the first dispatching operator
    /// (becomes a salsa query with S3's incremental firewall).
    operators: Option<crate::ops::OperatorRegistry>,
    /// The irreducible `Sub` residue: pairs that were neither ground nor
    /// var-headed nor decomposable when emitted; re-examined at finish once
    /// resolution has run. Generalizes into the obligation worklist at I4.
    deferred_subs: Vec<(Ty, Ty)>,
    diverges: Diverges,
    result: InferenceResult,
}

impl<'db> InferenceContext<'db> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        db: &'db dyn baml_compiler2_ppir::Db,
        index: &'db FileSemanticIndex<'db>,
        owner_scope: Option<FileScopeId>,
        lower: LowerCtx<'db>,
        param_tys: Vec<Ty>,
        return_ty: Option<Ty>,
        type_refs: Arc<BodyTypeRefs>,
        source_map: Option<baml_compiler2_ast::AstSourceMap>,
    ) -> InferenceContext<'db> {
        InferenceContext {
            db,
            facts: Facts::new(db),
            index,
            owner_scope,
            current_scope: owner_scope,
            source_map,
            lambda_params: FxHashMap::default(),
            flow: FxHashMap::default(),
            lower,
            param_tys,
            type_refs,
            return_ty,
            declared_throws: None,
            throws_channels: vec![Vec::new()],
            table: InferenceTable::new(),
            operators: None,
            deferred_subs: Vec::new(),
            diverges: Diverges::Maybe,
            result: InferenceResult::default(),
        }
    }

    fn infer_expr_body(&mut self, body: &ExprBody) {
        if let Some(root) = body.root_expr {
            match self.return_ty.clone() {
                Some(return_ty) if !return_ty.has_error() => {
                    self.check_expr(body, root, &return_ty);
                }
                _ => {
                    self.infer_expr(body, root, &Expectation::None);
                }
            }
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

    /// Checking mode: infer with the expectation, then constrain -
    /// `Sub(actual, expected)`, discharged eagerly. Definite failures are
    /// recorded against the checked expression, never dropped.
    fn check_expr(&mut self, body: &ExprBody, expr: ExprId, expected: &Ty) -> Ty {
        let ty = self.infer_expr(body, expr, &Expectation::has_type(expected.clone()));
        if !self.sub(&ty, expected) {
            self.result
                .type_mismatches
                .insert(expr, (expected.clone(), ty.clone()));
        }
        ty
    }

    fn infer_expr(&mut self, body: &ExprBody, expr: ExprId, expected: &Expectation) -> Ty {
        let ty = match &body.exprs[expr] {
            Expr::Literal(lit) => Ty::intern(TyKind::Literal(
                lit.clone(),
                Freshness::Fresh,
                TyAttr::default(),
            )),
            Expr::Null => Ty::null(),
            Expr::Path(segments) => self.resolve_value_path(expr, segments),
            Expr::Block { stmts, tail_expr } => {
                let entry_diverges = self.diverges;
                for stmt in stmts {
                    self.infer_stmt(body, *stmt);
                }
                match tail_expr {
                    Some(tail) => self.infer_expr(body, *tail, expected),
                    // A tail-less block that always diverged is never;
                    // otherwise it is void.
                    None if self.diverges == Diverges::Always
                        && entry_diverges == Diverges::Maybe =>
                    {
                        Ty::never()
                    }
                    None => Ty::void(),
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expr(body, *condition, &Ty::bool());
                let facts = self.condition_facts(body, *condition);
                let condition_diverges = self.diverges;
                let branch_expectation = expected.adjust_for_branches(&mut self.table);
                let base_flow = self.flow.clone();

                self.apply_facts(&facts.when_true);
                self.diverges = Diverges::Maybe;
                let then_ty = self.infer_expr(body, *then_branch, &branch_expectation);
                let then_diverges = self.diverges;
                let then_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                let then_flow = (then_diverges == Diverges::Maybe).then_some(then_flow);

                // The else path (written or implicit fall-through) carries
                // the condition's false facts; the divergence-aware merge
                // makes guard-with-early-return narrowing the ordinary
                // rule (B-688), no special case.
                self.apply_facts(&facts.when_false);
                match else_branch {
                    Some(else_expr) => {
                        self.diverges = Diverges::Maybe;
                        let else_ty = self.infer_expr(body, *else_expr, &branch_expectation);
                        let else_diverges = self.diverges;
                        let else_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                        let else_flow = (else_diverges == Diverges::Maybe).then_some(else_flow);
                        self.diverges = condition_diverges.or(then_diverges.and(else_diverges));
                        self.merge_branch_flows(base_flow, then_flow, else_flow);
                        // The merge point: a canonical union, never a forced
                        // equality (joins happen at generation sites).
                        self.join(&[then_ty, else_ty])
                    }
                    None => {
                        let else_flow = std::mem::replace(&mut self.flow, base_flow.clone());
                        self.diverges = condition_diverges;
                        self.merge_branch_flows(base_flow, then_flow, Some(else_flow));
                        // No else: the if produces no value.
                        Ty::void()
                    }
                }
            }
            Expr::Array { elements } => {
                // With an expected element type, elements are CHECKED against
                // it; otherwise they synthesize and JOIN (fresh literals
                // widening at the join, per ruling 1's generation-site rule).
                let expected_element = expected.only_has_type().and_then(|ty| {
                    match self.table.shallow_resolve(ty).kind() {
                        TyKind::List(element, _) => Some(element.clone()),
                        _ => None,
                    }
                });
                match expected_element {
                    Some(element_ty) => {
                        for element in elements {
                            self.check_expr(body, *element, &element_ty);
                        }
                        Ty::list(element_ty)
                    }
                    None if elements.is_empty() => {
                        // `[]`: a list over a fresh element variable - the
                        // honest replacement for the EvolvingList sentinel.
                        Ty::list(self.table.new_var_ty())
                    }
                    None => {
                        let joined: Vec<Ty> = elements
                            .iter()
                            .map(|element| {
                                let ty = self.infer_expr(body, *element, &Expectation::None);
                                self.widen_fresh(&ty)
                            })
                            .collect();
                        Ty::list(self.union_of(&joined))
                    }
                }
            }
            Expr::Map { entries } => {
                if entries.is_empty() {
                    Ty::intern(TyKind::Map {
                        key: self.table.new_var_ty(),
                        value: self.table.new_var_ty(),
                        attr: TyAttr::default(),
                    })
                } else {
                    let (keys, values): (Vec<Ty>, Vec<Ty>) = entries
                        .iter()
                        .map(|(key, value)| {
                            let key_ty = self.infer_expr(body, *key, &Expectation::None);
                            let value_ty = self.infer_expr(body, *value, &Expectation::None);
                            (self.widen_fresh(&key_ty), self.widen_fresh(&value_ty))
                        })
                        .unzip();
                    Ty::intern(TyKind::Map {
                        key: self.union_of(&keys),
                        value: self.union_of(&values),
                        attr: TyAttr::default(),
                    })
                }
            }
            Expr::Return { value } => {
                if let Some(value) = value {
                    match self.return_ty.clone() {
                        Some(return_ty) if !return_ty.has_error() => {
                            self.check_expr(body, *value, &return_ty);
                        }
                        _ => {
                            self.infer_expr(body, *value, &Expectation::None);
                        }
                    }
                }
                self.diverges = Diverges::Always;
                Ty::never()
            }
            Expr::Throw { value } => {
                let thrown = self.infer_expr(body, *value, &Expectation::None);
                self.record_throw(*value, &thrown);
                self.diverges = Diverges::Always;
                Ty::never()
            }
            Expr::Binary { op, lhs, rhs } => self.infer_binary(body, *op, *lhs, *rhs),
            Expr::Unary { op, expr: operand } => self.infer_unary(body, *op, *operand),
            Expr::Call { callee, args, .. } => self.infer_call(body, expr, *callee, args),
            Expr::Object {
                type_name,
                fields,
                spreads,
                ..
            } => self.infer_object(body, expr, type_name, fields, spreads),
            Expr::MemberAccess { base, member } => {
                let base_ty = self.infer_expr(body, *base, &Expectation::None);
                self.field_access(&base_ty, member)
            }
            Expr::Lambda(def) => self.infer_lambda(body, expr, def, expected),
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let arms = arms.clone();
                self.infer_match(body, expr, *scrutinee, &arms, expected)
            }
            Expr::Is { scrutinee, pattern } => self.infer_is(body, *scrutinee, *pattern),
            Expr::Catch { base, clauses } => {
                let clauses = clauses.clone();
                self.infer_catch(body, *base, &clauses, expected)
            }
            // Not yet implemented: visit children generically, record the
            // sentinel.
            _ => {
                let mut children = Vec::new();
                body.expr_children(expr, &mut children);
                for node in children {
                    match node {
                        BodyNode::Expr(child) => {
                            self.infer_expr(body, child, &Expectation::None);
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
                self.infer_expr(body, *expr, &Expectation::None);
            }
            Stmt::Let {
                pattern,
                initializer,
                else_branch,
                ..
            } => {
                self.infer_let(body, *pattern, *initializer, *else_branch);
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    match self.return_ty.clone() {
                        Some(return_ty) if !return_ty.has_error() => {
                            self.check_expr(body, *value, &return_ty);
                        }
                        _ => {
                            self.infer_expr(body, *value, &Expectation::None);
                        }
                    }
                }
                self.diverges = Diverges::Always;
            }
            Stmt::Throw { value } => {
                let thrown = self.infer_expr(body, *value, &Expectation::None);
                self.record_throw(*value, &thrown);
                self.diverges = Diverges::Always;
            }
            // Loop-local terminators: the path past them is dead; the loop
            // discipline restores divergence and flow at the loop boundary.
            Stmt::Break | Stmt::Continue => {
                self.diverges = Diverges::Always;
            }
            Stmt::Assign { target, value } => {
                self.infer_assign(body, *target, *value, None);
            }
            Stmt::AssignOp { target, op, value } => {
                self.infer_assign(body, *target, *value, Some(*op));
            }
            Stmt::While {
                condition,
                body: loop_body,
                after,
                ..
            } => {
                // Loop discipline (the no-fixpoint recipe): havoc the
                // bindings the body assigns, run the body once under the
                // condition's true facts, and the POST-loop environment is
                // loop entry plus the false facts - a zero-iteration loop
                // keeps no body narrowing (B-735).
                for binding in self.assigned_bindings(body, *loop_body) {
                    self.flow.remove(&binding);
                }
                self.check_expr(body, *condition, &Ty::bool());
                let facts = self.condition_facts(body, *condition);
                let entry_flow = self.flow.clone();
                self.apply_facts(&facts.when_true);
                let saved = self.diverges;
                self.infer_expr(body, *loop_body, &Expectation::None);
                if let Some(after) = after {
                    self.infer_stmt(body, *after);
                }
                self.diverges = saved;
                self.flow = entry_flow;
                self.apply_facts(&facts.when_false);
            }
            Stmt::WhileLet {
                pattern,
                scrutinee,
                body: loop_body,
            } => {
                for binding in self.assigned_bindings(body, *loop_body) {
                    self.flow.remove(&binding);
                }
                let scrut_ty = self.infer_expr(body, *scrutinee, &Expectation::None);
                let resolved = self.table.resolve_completely(&scrut_ty);
                let scrut = self.matrix_scrut(&resolved);
                let outcome = self.lower_pattern(body, *pattern, &scrut);
                let entry_flow = self.flow.clone();
                if let Some(binding) = self.narrowable_binding(body, *scrutinee) {
                    self.flow.insert(binding, outcome.matched_ty);
                }
                let saved = self.diverges;
                self.infer_expr(body, *loop_body, &Expectation::None);
                self.diverges = saved;
                self.flow = entry_flow;
            }
            Stmt::For {
                binding,
                collection,
                body: loop_body,
            } => {
                for havoced in self.assigned_bindings(body, *loop_body) {
                    self.flow.remove(&havoced);
                }
                let collection_ty = self.infer_expr(body, *collection, &Expectation::None);
                let resolved = self.table.resolve_completely(&collection_ty);
                let collection_ty = self.matrix_scrut(&resolved);
                // List elements directly; the Iterator protocol (assoc
                // `Item` through interfaces) joins with the I cluster.
                let element = match collection_ty.kind() {
                    TyKind::List(element, _) => element.clone(),
                    _ => Ty::error(),
                };
                self.lower_pattern(body, *binding, &element);
                let entry_flow = self.flow.clone();
                let saved = self.diverges;
                self.infer_expr(body, *loop_body, &Expectation::None);
                self.diverges = saved;
                self.flow = entry_flow;
            }
            _ => {
                let mut children = Vec::new();
                body.stmt_children(stmt, &mut children);
                for node in children {
                    match node {
                        BodyNode::Expr(child) => {
                            self.infer_expr(body, child, &Expectation::None);
                        }
                        BodyNode::Stmt(child) => self.infer_stmt(body, child),
                    }
                }
            }
        }
    }

    /// The `let` rule: with an annotation, the initializer is CHECKED
    /// against it (`_` holes as fresh vars, filled by the resulting bounds);
    /// without one, the initializer synthesizes and fresh literals widen at
    /// the binding site.
    fn infer_let(
        &mut self,
        body: &ExprBody,
        pattern: PatId,
        initializer: Option<ExprId>,
        else_branch: Option<ExprId>,
    ) {
        let binding_ty = match &body.patterns[pattern] {
            Pattern::Bind { subpat, .. } => {
                let annotation = subpat.and_then(|sub| {
                    matches!(body.patterns[sub], Pattern::Type(_))
                        .then(|| self.type_refs.pattern_types.get(&sub).copied())
                        .flatten()
                });
                match annotation {
                    Some(type_ref) => {
                        let annotation_ty = self.lower_body_annotation(type_ref);
                        if let Some(init) = initializer {
                            self.check_expr(body, init, &annotation_ty);
                        }
                        annotation_ty
                    }
                    None => match initializer {
                        Some(init) => {
                            let init_ty = self.infer_expr(body, init, &Expectation::None);
                            self.widen_fresh(&init_ty)
                        }
                        None => Ty::error(),
                    },
                }
            }
            // Destructures: the pattern walk records each binding's type
            // itself; the let-level entry keeps the initializer type for
            // the pattern node (refutability is S17's diagnostic).
            _ => {
                self.infer_let_destructure(body, pattern, initializer);
                return self.finish_let_else(body, else_branch);
            }
        };
        self.finish_let_else(body, else_branch);
        self.result.type_of_binding.insert(pattern, binding_ty);
    }

    /// Assignment typing. The value checks against the DECLARED binding
    /// type - never the narrowed overlay (B-618: a narrowed local may be
    /// re-assigned anything its declaration admits) - then the overlay
    /// re-narrows to the assigned value when it provably fits (the
    /// narrow-on-assign rule), else clears to declared.
    fn infer_assign(
        &mut self,
        body: &ExprBody,
        target: ExprId,
        value: ExprId,
        op: Option<baml_compiler2_ast::AssignOp>,
    ) {
        let binding = self.narrowable_binding(body, target);
        let declared = binding.map(|binding| self.binding_declared_ty(binding));
        let assigned = match op {
            None => match &declared {
                Some(declared) if !declared.has_error() => {
                    self.check_expr(body, value, declared)
                }
                _ => self.infer_expr(body, value, &Expectation::None),
            },
            Some(op) => {
                // Compound assignment: `target op value` through the same
                // operator machinery, the result checked against declared.
                use baml_compiler2_ast::AssignOp;
                let lhs = binding
                    .map(|binding| self.binding_flow_ty(binding))
                    .unwrap_or_else(|| self.infer_expr(body, target, &Expectation::None));
                let rhs = self.infer_expr(body, value, &Expectation::None);
                let result = match op {
                    AssignOp::Add => self.dispatch_operator("Add", &lhs, Some(&rhs)),
                    AssignOp::Sub => self.dispatch_operator("Subtract", &lhs, Some(&rhs)),
                    AssignOp::Mul => self.dispatch_operator("Multiply", &lhs, Some(&rhs)),
                    AssignOp::Div => self.dispatch_operator("Divide", &lhs, Some(&rhs)),
                    AssignOp::Mod => self.dispatch_operator("Remainder", &lhs, Some(&rhs)),
                    AssignOp::BitAnd
                    | AssignOp::BitOr
                    | AssignOp::BitXor
                    | AssignOp::Shl
                    | AssignOp::Shr => self.bitwise_hack_table(&lhs, &rhs),
                };
                if let Some(declared) = &declared
                    && !declared.has_error()
                    && !self.sub(&result, declared)
                {
                    self.result
                        .type_mismatches
                        .insert(value, (declared.clone(), result.clone()));
                }
                result
            }
        };
        if let Some(declared) = &declared {
            self.result.type_of_expr.insert(target, declared.clone());
        }
        if let Some(binding) = binding {
            let resolved = self.table.resolve_completely(&assigned);
            let narrowed = self.widen_fresh(&resolved);
            let fits = match &declared {
                Some(declared) if !declared.has_error() => {
                    crate::infer::pat::provable_subtype(&narrowed, declared, &self.facts)
                }
                _ => false,
            };
            if fits {
                self.flow.insert(binding, narrowed);
            } else {
                self.flow.remove(&binding);
            }
        }
    }

    /// The `let ... else` tail, shared by the bind and destructure paths.
    /// Ruling: the else branch must diverge; the check itself is an S17
    /// diagnostic. Its divergence does not leak past the let.
    fn finish_let_else(&mut self, body: &ExprBody, else_branch: Option<ExprId>) {
        if let Some(else_expr) = else_branch {
            let saved = self.diverges;
            self.infer_expr(body, else_expr, &Expectation::None);
            self.diverges = saved;
        }
    }

    /// `Sub(actual, expected)` - eager discharge per the settled design:
    /// invariant same-heads decay to `Eq`; function types relate contra/co;
    /// var-headed pairs deposit bounds; ground pairs ask the canonical
    /// oracle; the irreducible residue defers to finish. Returns `false` on
    /// a DEFINITE mismatch (callers record it); undecided is `true`.
    fn sub(&mut self, actual: &Ty, expected: &Ty) -> bool {
        let actual = self.table.shallow_resolve(actual);
        let expected = self.table.shallow_resolve(expected);
        if actual == expected || actual.has_error() || expected.has_error() {
            return true;
        }
        match (actual.kind(), expected.kind()) {
            // A variable flowing into a context: upper bound. A value
            // flowing into a variable: lower bound. (Var-var records on
            // both sides; resolution sees through whichever solves first.)
            (TyKind::Infer { var: Some(var), .. }, _) => {
                self.table.add_upper_bound(*var, expected.clone());
                if let TyKind::Infer {
                    var: Some(other), ..
                } = expected.kind()
                {
                    self.table.add_lower_bound(*other, actual.clone());
                }
                true
            }
            (_, TyKind::Infer { var: Some(var), .. }) => {
                self.table.add_lower_bound(*var, actual.clone());
                true
            }
            // Invariant constructors: Sub decays to Eq of the pieces.
            (TyKind::Class(a_name, a_args, _), TyKind::Class(b_name, b_args, _))
                if a_name == b_name && a_args.len() == b_args.len() =>
            {
                let pairs: Vec<(Ty, Ty)> =
                    a_args.iter().cloned().zip(b_args.iter().cloned()).collect();
                let mut ok = true;
                for (a, b) in pairs {
                    ok &= self.table.unify(&a, &b).is_ok();
                }
                ok
            }
            (TyKind::List(a, _), TyKind::List(b, _)) => {
                let (a, b) = (a.clone(), b.clone());
                self.table.unify(&a, &b).is_ok()
            }
            (
                TyKind::Map {
                    key: ak, value: av, ..
                },
                TyKind::Map {
                    key: bk, value: bv, ..
                },
            ) => {
                let (ak, av, bk, bv) = (ak.clone(), av.clone(), bk.clone(), bv.clone());
                let key_ok = self.table.unify(&ak, &bk).is_ok();
                let value_ok = self.table.unify(&av, &bv).is_ok();
                key_ok && value_ok
            }
            (TyKind::Future(av, ae, _), TyKind::Future(bv, be, _)) => {
                let (av, ae, bv, be) = (av.clone(), ae.clone(), bv.clone(), be.clone());
                let value_ok = self.table.unify(&av, &bv).is_ok();
                let error_ok = self.table.unify(&ae, &be).is_ok();
                value_ok && error_ok
            }
            // Function types: contravariant params, covariant ret/throws.
            (
                TyKind::Function {
                    params: a_params,
                    ret: a_ret,
                    throws: a_throws,
                    ..
                },
                TyKind::Function {
                    params: b_params,
                    ret: b_ret,
                    throws: b_throws,
                    ..
                },
            ) if a_params.len() == b_params.len() => {
                let param_pairs: Vec<(Ty, Ty)> = a_params
                    .iter()
                    .zip(b_params.iter())
                    .map(|(a, b)| (b.ty.clone(), a.ty.clone()))
                    .collect();
                let rets = (a_ret.clone(), b_ret.clone());
                let throws = (a_throws.clone(), b_throws.clone());
                let mut ok = true;
                for (b, a) in param_pairs {
                    ok &= self.sub(&b, &a);
                }
                ok &= self.sub(&rets.0, &rets.1);
                ok &= self.sub(&throws.0, &throws.1);
                ok
            }
            _ => {
                // Ground on both sides: one oracle verdict. Otherwise the
                // pair is the deferred residue.
                let actual = self.table.resolve_completely(&actual);
                let expected = self.table.resolve_completely(&expected);
                if !actual.has_infer() && !expected.has_infer() {
                    is_subtype_interned(&actual, &expected, &self.facts)
                } else {
                    self.deferred_subs.push((actual, expected));
                    true
                }
            }
        }
    }

    /// A union of members that may still contain inference variables. The
    /// canonical algebra consults the semantic oracle and REQUIRES
    /// var-free input (the normalizer's invariant), so a var-containing
    /// join stays syntactic until resolution - the S13 finalize pass
    /// re-canonicalizes once every variable is solved or ruled an error.
    fn union_of(&mut self, members: &[Ty]) -> Ty {
        if members.iter().any(Ty::has_infer) {
            return Ty::union(members.iter().cloned());
        }
        canonical_union_interned(members, &self.facts)
    }

    /// The control-flow join: a canonical union that PRESERVES literal
    /// freshness across the round-trip (the canonical algebra erases
    /// freshness as identity-irrelevant, but widening at the eventual
    /// binding site still needs it: `if c { 1 } else { 2 }` is the fresh
    /// `1 | 2`, widening to `int` at a binding - while `true | false`
    /// collapses to `bool` here, where freshness no longer matters).
    fn join(&mut self, members: &[Ty]) -> Ty {
        let fresh: Vec<Literal> = members
            .iter()
            .filter_map(|member| match member.kind() {
                TyKind::Literal(lit, Freshness::Fresh, _) => Some(lit.clone()),
                _ => None,
            })
            .collect();
        let joined = self.union_of(members);
        if fresh.is_empty() {
            return joined;
        }
        let remark = |ty: &Ty| -> Ty {
            match ty.kind() {
                TyKind::Literal(lit, Freshness::Regular, attr) if fresh.contains(lit) => {
                    Ty::intern(TyKind::Literal(lit.clone(), Freshness::Fresh, attr.clone()))
                }
                _ => ty.clone(),
            }
        };
        match joined.kind() {
            TyKind::Union(joined_members, attr) => Ty::intern(TyKind::Union(
                joined_members.iter().map(remark).collect(),
                attr.clone(),
            )),
            _ => remark(&joined),
        }
    }

    /// Fresh literals widen to their base primitive at binding sites and
    /// joins; a union of fresh literals widens member-wise and
    /// re-canonicalizes (`1 | 2` at a binding is `int`).
    fn widen_fresh(&mut self, ty: &Ty) -> Ty {
        match ty.kind() {
            TyKind::Literal(_, Freshness::Fresh, _) => widen_fresh_literal(ty),
            TyKind::Union(members, _)
                if members.iter().any(|member| {
                    matches!(member.kind(), TyKind::Literal(_, Freshness::Fresh, _))
                }) =>
            {
                let widened: Vec<Ty> = members.iter().map(widen_fresh_literal).collect();
                self.union_of(&widened)
            }
            _ => ty.clone(),
        }
    }

    /// Binary operator typing. Dispatching operators (arithmetic, ordered
    /// comparison) go through the interfaces - decision 3B, matching TIR's
    /// arithmetic arm; the structural ones (`&&`/`||` short-circuit
    /// control flow, `==`/`!=` structural equality over `Concrete`, `??`
    /// null-algebra) are type algebra, not dispatch. Operand-validity
    /// diagnostics are S17's; the Compare obligation on ordered
    /// comparisons lands with I4.
    fn infer_binary(
        &mut self,
        body: &ExprBody,
        op: baml_compiler2_ast::BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    ) -> Ty {
        use baml_compiler2_ast::BinaryOp;
        match op {
            BinaryOp::And | BinaryOp::Or => {
                self.check_expr(body, lhs, &Ty::bool());
                self.check_expr(body, rhs, &Ty::bool());
                Ty::bool()
            }
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => {
                self.infer_expr(body, lhs, &Expectation::None);
                self.infer_expr(body, rhs, &Expectation::None);
                Ty::bool()
            }
            BinaryOp::NullCoalesce => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                // B-1135: the unwrapped lhs INFORMS the rhs, so `xs ?? []`
                // adopts the element type instead of leaving a hole. It
                // does not CONSTRAIN it - `v ?? "fallback"` is a join, not
                // a mismatch - which is exactly Expectation's inform/
                // constrain split (same as if-branches).
                let inner = self.remove_null(&lhs_ty);
                let rhs_ty =
                    self.infer_expr(body, rhs, &Expectation::has_type(inner.clone()));
                self.null_coalesce(inner, &rhs_ty)
            }
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                let rhs_ty = self.infer_expr(body, rhs, &Expectation::None);
                let interface = match op {
                    BinaryOp::Add => "Add",
                    BinaryOp::Sub => "Subtract",
                    BinaryOp::Mul => "Multiply",
                    BinaryOp::Div => "Divide",
                    BinaryOp::Mod => "Remainder",
                    _ => unreachable!("outer match arm"),
                };
                self.dispatch_operator(interface, &lhs_ty, Some(&rhs_ty))
            }
            BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
            | BinaryOp::Shl
            | BinaryOp::Shr => {
                let lhs_ty = self.infer_expr(body, lhs, &Expectation::None);
                let rhs_ty = self.infer_expr(body, rhs, &Expectation::None);
                self.bitwise_hack_table(&lhs_ty, &rhs_ty)
            }
        }
    }

    fn infer_unary(
        &mut self,
        body: &ExprBody,
        op: baml_compiler2_ast::UnaryOp,
        operand: ExprId,
    ) -> Ty {
        match op {
            baml_compiler2_ast::UnaryOp::Not => {
                self.check_expr(body, operand, &Ty::bool());
                Ty::bool()
            }
            baml_compiler2_ast::UnaryOp::Neg => {
                let ty = self.infer_expr(body, operand, &Expectation::None);
                self.dispatch_operator("Negate", &ty, None)
            }
        }
    }

    /// The dispatch: every (lhs alternative, rhs alternative) pair of the
    /// operands' union members must have an impl of
    /// `baml.ops.<interface>`; the result is the join of the Outputs.
    /// Literals widen to their bases for lookup (folding literal
    /// arithmetic is a later refinement). `never` propagates
    /// (unreachable-operand rule); Error/unknown operands suppress to the
    /// sentinel; inference variables in operand position defer to the
    /// obligation machinery (I4) and sentinel until then.
    fn dispatch_operator(&mut self, interface: &str, lhs: &Ty, rhs: Option<&Ty>) -> Ty {
        let lhs = self.table.resolve_completely(lhs);
        let rhs = rhs.map(|ty| self.table.resolve_completely(ty));
        if matches!(lhs.kind(), TyKind::Never { .. })
            || rhs
                .as_ref()
                .is_some_and(|ty| matches!(ty.kind(), TyKind::Never { .. }))
        {
            return Ty::never();
        }
        let undispatchable = |ty: &Ty| {
            ty.has_error() || ty.has_infer() || matches!(ty.kind(), TyKind::Unknown { .. })
        };
        if undispatchable(&lhs) || rhs.as_ref().is_some_and(undispatchable) {
            return Ty::error();
        }
        if self.operators.is_none() {
            self.operators = Some(crate::ops::OperatorRegistry::build(self.db));
        }
        let registry = self.operators.as_ref().expect("just built");
        let mut outputs = Vec::new();
        for lhs_member in operand_members(&lhs) {
            match &rhs {
                Some(rhs) => {
                    for rhs_member in operand_members(rhs) {
                        match registry.output(interface, &lhs_member, Some(&rhs_member)) {
                            Some(output) => outputs.push(output),
                            None => return Ty::error(),
                        }
                    }
                }
                None => match registry.output(interface, &lhs_member, None) {
                    Some(output) => outputs.push(output),
                    None => return Ty::error(),
                },
            }
        }
        self.union_of(&outputs)
    }

    /// The non-null part of a type: `Null` drops from unions (an all-null
    /// type leaves `never`).
    fn remove_null(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        match resolved.kind() {
            TyKind::Null { .. } => Ty::never(),
            TyKind::Union(members, _) => {
                let non_null: Vec<Ty> = members
                    .iter()
                    .filter(|member| !matches!(member.kind(), TyKind::Null { .. }))
                    .cloned()
                    .collect();
                if non_null.is_empty() {
                    Ty::never()
                } else {
                    self.union_of(&non_null)
                }
            }
            _ => resolved,
        }
    }

    /// `a ?? b`: given the already-unwrapped lhs, the canonical-unwrap fast
    /// paths (TIR's rule) - `rhs <: inner` keeps the unwrapped lhs, `inner
    /// <: rhs` keeps rhs - else the freshness-preserving join.
    fn null_coalesce(&mut self, inner: Ty, rhs: &Ty) -> Ty {
        let rhs = self.table.resolve_completely(rhs);
        let ground =
            |ty: &Ty| !ty.has_infer() && !ty.has_error() && !matches!(ty.kind(), TyKind::Never { .. });
        if ground(&inner) && ground(&rhs) {
            if is_subtype_interned(&rhs, &inner, &self.facts) {
                return inner;
            }
            if is_subtype_interned(&inner, &rhs, &self.facts) {
                return rhs;
            }
        }
        self.join(&[inner, rhs])
    }

    /// HACK: bitwise operators do NOT go through interfaces yet. The
    /// ruling (README decision 3B) says every dispatching operator must,
    /// but the stdlib has no `baml.ops` bitwise interfaces to dispatch
    /// through (`ns_ops` holds only `math.baml` and `comparison.baml`), so
    /// this table mirrors TIR's `infer_bitwise` exactly: int&int is int,
    /// any int/bigint mix is bigint, everything else is the Error sentinel
    /// (TIR's `InvalidBinaryOp`, S17's diagnostic). DELETE this table - and
    /// route these five operators through `dispatch_operator` - when the
    /// bitwise interfaces land in the stdlib and TIR switches off its own
    /// table.
    fn bitwise_hack_table(&mut self, lhs: &Ty, rhs: &Ty) -> Ty {
        #[derive(Clone, Copy, PartialEq)]
        enum Base {
            Int,
            Bigint,
        }
        fn base_of(ty: &Ty) -> Option<Base> {
            match ty.kind() {
                TyKind::Int { .. } | TyKind::Literal(Literal::Int(_), _, _) => Some(Base::Int),
                TyKind::Bigint { .. } | TyKind::Literal(Literal::Bigint(_), _, _) => {
                    Some(Base::Bigint)
                }
                TyKind::Union(members, _) => {
                    let mut result: Option<Base> = None;
                    for member in members {
                        let base = base_of(member)?;
                        result = Some(match result {
                            None => base,
                            Some(seen) if seen == base => seen,
                            // An int|bigint union widens to bigint,
                            // matching the mixed-operand rule below.
                            Some(_) => Base::Bigint,
                        });
                    }
                    result
                }
                _ => None,
            }
        }
        let lhs = self.table.resolve_completely(lhs);
        let rhs = self.table.resolve_completely(rhs);
        match (base_of(&lhs), base_of(&rhs)) {
            (Some(Base::Int), Some(Base::Int)) => Ty::int(),
            (Some(_), Some(_)) => Ty::intern(TyKind::Bigint {
                attr: TyAttr::default(),
            }),
            _ => Ty::error(),
        }
    }

    /// Call typing: direct calls to resolved functions instantiate the
    /// signature (explicit turbofish args, else fresh variables per generic
    /// param - the equality regime resolves them from argument bounds);
    /// calls through function-typed VALUES check against the value's type.
    /// Two argument passes: non-lambda args first, so lambda signatures can
    /// be deduced from already-resolved param types (S9).
    fn infer_call(
        &mut self,
        body: &ExprBody,
        call: ExprId,
        callee: ExprId,
        args: &[baml_compiler2_ast::CallArg],
    ) -> Ty {
        let (callee_fn_ty, bound_receiver) = self.infer_callee(body, call, callee);
        let TyKind::Function {
            params,
            ret,
            throws,
            ..
        } = callee_fn_ty.kind()
        else {
            // Not callable (or not yet typed): visit the args, sentinel out.
            for arg in args {
                self.infer_expr(body, arg.expr, &Expectation::None);
            }
            return Ty::error();
        };
        let callee_throws = throws.clone();
        self.record_throw(callee, &callee_throws);
        // A bound method call: the receiver already fills the `self` slot,
        // so written arguments match against the remaining parameters.
        let params: Vec<Ty> = params
            .iter()
            .skip(usize::from(bound_receiver))
            .map(|param| param.ty.clone())
            .collect();
        let ret = ret.clone();
        let is_lambda_arg =
            |arg: &baml_compiler2_ast::CallArg| matches!(body.exprs[arg.expr], Expr::Lambda(_));
        for pass in 0..2 {
            for (index, arg) in args.iter().enumerate() {
                if (pass == 0) == is_lambda_arg(arg) {
                    continue;
                }
                match params.get(index) {
                    Some(param_ty) => {
                        let param_ty = param_ty.clone();
                        self.check_expr(body, arg.expr, &param_ty);
                    }
                    None => {
                        // Extra argument: the arity diagnostic is S17's.
                        self.infer_expr(body, arg.expr, &Expectation::None);
                    }
                }
            }
        }
        ret
    }

    /// The callee's (instantiated) function type, plus whether its first
    /// parameter is already bound to a receiver (`xs.push(1)` checks `1`
    /// against `item`, not `self`). Direct function references and
    /// type-qualified method paths instantiate here because their
    /// instantiation reads the CALL site's turbofish, which a bare value
    /// does not have; bound method callees resolve through
    /// `method_resolution`; everything else is whatever the expression
    /// infers to.
    fn infer_callee(&mut self, body: &ExprBody, call: ExprId, callee: ExprId) -> (Ty, bool) {
        if let Expr::Path(segments) = &body.exprs[callee]
            && !self.path_resolves_locally(callee)
        {
            // A path that names a function is a direct call.
            if let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
                self.lower.resolve_value(segments)
            {
                let signature = function_signature(self.db, function);
                let instantiation = self.instantiation_args(call, &signature.generic_params);
                let fn_ty = function_value_ty(&signature, &instantiation);
                self.result.type_of_expr.insert(callee, fn_ty.clone());
                return (fn_ty, false);
            }
            // A type-qualified method path (`Array.filled(3, 0)`,
            // `baml.Array.generate(...)`): statics call directly, and the
            // UFCS spelling of an instance method takes the receiver as
            // its written first argument - either way the declared
            // parameter list matches the written arguments (no bound
            // receiver). Class generics have no receiver to pin them, so
            // they instantiate fresh alongside the method's own.
            if segments.len() >= 2
                && let Some(baml_compiler2_hir::contributions::Definition::Class(class)) = self
                    .lower
                    .resolve_type_definition(&segments[..segments.len() - 1])
            {
                let member = segments.last().expect("checked len");
                let method = baml_compiler2_ppir::item_data::class_data(self.db, class)
                    .methods
                    .iter()
                    .copied()
                    .find(|&method| {
                        baml_compiler2_ppir::item_data::function_data(self.db, method).name
                            == *member
                    });
                if let Some(method) = method {
                    let signature = function_signature(self.db, method);
                    let class_count = crate::lower::class_generic_frame(self.db, class).len();
                    let mut instantiation: Vec<Ty> = (0..class_count)
                        .map(|_| self.table.new_var_ty())
                        .collect();
                    let own_params = signature.generic_params[class_count..].to_vec();
                    instantiation.extend(self.instantiation_args(call, &own_params));
                    let fn_ty = function_value_ty(&signature, &instantiation);
                    self.result.type_of_expr.insert(callee, fn_ty.clone());
                    return (fn_ty, false);
                }
            }
        }
        // A bound method callee, in either AST spelling: `expr.name(..)`
        // parses as MemberAccess, `local.name(..)` as a multi-segment Path
        // (the AST cannot split paths before name resolution).
        match &body.exprs[callee] {
            Expr::MemberAccess { base, member } => {
                let member = member.clone();
                let receiver = self.infer_expr(body, *base, &Expectation::None);
                let (ty, bound) = self.member_callee(call, &receiver, &member);
                self.result.type_of_expr.insert(callee, ty.clone());
                return (ty, bound);
            }
            Expr::Path(segments) if segments.len() >= 2 && self.path_resolves_locally(callee) => {
                let segments = segments.clone();
                let root = self.infer_path(callee);
                let receiver = segments[1..segments.len() - 1]
                    .iter()
                    .fold(root, |ty, segment| self.field_access(&ty, segment));
                let member = segments.last().expect("checked len");
                let (ty, bound) = self.member_callee(call, &receiver, member);
                self.result.type_of_expr.insert(callee, ty.clone());
                return (ty, bound);
            }
            _ => {}
        }
        (self.infer_expr(body, callee, &Expectation::None), false)
    }

    /// `receiver.member` in callee position: a method (instantiated - the
    /// receiver pins the class generics, the call site's turbofish or
    /// fresh variables fill the method's own; bound iff it takes `self`),
    /// or a field holding a function value.
    fn member_callee(&mut self, call: ExprId, receiver: &Ty, member: &baml_type::Name) -> (Ty, bool) {
        let resolved = self.table.resolve_completely(receiver);
        let candidate =
            crate::method_resolution::lookup_method(self.db, &self.facts, &resolved, member);
        let Some(candidate) = candidate else {
            return (self.field_access(&resolved, member), false);
        };
        let signature = function_signature(self.db, candidate.method);
        let class_count = candidate.class_args.len();
        let own_params = signature.generic_params[class_count..].to_vec();
        let mut instantiation = candidate.class_args;
        instantiation.extend(self.instantiation_args(call, &own_params));
        let fn_ty = function_value_ty(&signature, &instantiation);
        let bound = signature
            .params
            .first()
            .is_some_and(|param| param.name.as_str() == "self");
        (fn_ty, bound)
    }

    /// The one home for value-position path typing (rust-analyzer's
    /// `infer/path.rs` shape): a local/parameter root followed by field
    /// accesses, or a package-level FUNCTION as a first-class value (`let c:
    /// (x: int) -> int throws never = inc;`), instantiated with fresh
    /// variables per generic param - only a call site's turbofish can spell
    /// arguments explicitly, and the expectation's bounds resolve them here.
    /// Constants and enum variants join as later slices land.
    fn resolve_value_path(&mut self, expr: ExprId, segments: &[baml_type::Name]) -> Ty {
        if self.path_resolves_locally(expr) {
            // The root resolves through the semantic index; the remaining
            // segments are member accesses (the AST cannot split `b.v` into
            // base+member before name resolution).
            let root_ty = self.infer_path(expr);
            return segments
                .iter()
                .skip(1)
                .fold(root_ty, |ty, segment| self.field_access(&ty, segment));
        }
        if let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
            self.lower.resolve_value(segments)
        {
            let signature = function_signature(self.db, function);
            let instantiation: Vec<Ty> = signature
                .generic_params
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            return function_value_ty(&signature, &instantiation);
        }
        Ty::error()
    }

    /// Lambda typing (rust-analyzer's `deduce_closure_signature` shape).
    /// Written signature slots win; unannotated slots fill from the expected
    /// function type flowing down. An unannotated parameter with no
    /// expectation has no source of truth: the Error sentinel (TIR's
    /// `CannotInferLambdaParamType`; the diagnostic is S17's). An omitted
    /// `throws` stays the honest Error sentinel until S12 infers effects.
    fn infer_lambda(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        def: &baml_compiler2_ast::LambdaDef,
        expected: &Expectation,
    ) -> Ty {
        let signature = self.type_refs.lambda_signatures.get(&expr).cloned();
        let expected_fn = expected
            .only_has_type()
            .map(|ty| self.table.shallow_resolve(ty))
            .and_then(|ty| match ty.kind() {
                TyKind::Function { params, ret, .. } => Some((params.clone(), ret.clone())),
                _ => None,
            });

        let param_tys: Vec<Ty> = def
            .params
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let annotated = signature
                    .as_ref()
                    .and_then(|sig| sig.params.get(index).copied().flatten());
                match annotated {
                    Some(type_ref) => self.lower_body_annotation(type_ref),
                    None => expected_fn
                        .as_ref()
                        .and_then(|(params, _)| params.get(index))
                        .map(|param| param.ty.clone())
                        .unwrap_or_else(Ty::error),
                }
            })
            .collect();

        let annotated_ret = signature
            .as_ref()
            .and_then(|sig| sig.return_type)
            .map(|type_ref| self.lower_body_annotation(type_ref));
        let ret_expectation =
            annotated_ret.or_else(|| expected_fn.as_ref().map(|(_, ret)| ret.clone()));

        let written_throws = signature
            .as_ref()
            .and_then(|sig| sig.throws)
            .map(|type_ref| self.lower_body_annotation(type_ref));

        // The scope the lambda opened, by source range (restricted to the
        // enclosing scope's subtree to disambiguate synthetic companions
        // sharing a span). Registering the deduced params there is what
        // makes the body's parameter references resolve.
        let lambda_scope = self.source_map.as_ref().and_then(|map| {
            let span = map.expr_span(expr);
            match self.current_scope {
                Some(scope) => self.index.lambda_scope_for_within(scope, span),
                None => self.index.lambda_scope_for(span),
            }
        });
        if let Some(scope) = lambda_scope {
            self.lambda_params.insert(scope, param_tys.clone());
        }

        // The body types in the owner's run and table, but under the
        // lambda's metadata scope (the semantic index keys its expressions
        // there), and its divergence is the lambda's, not the owner's.
        // The lambda's OWN effect channel: contributions inside the body
        // belong to the lambda, not the enclosing function - defining a
        // throwing lambda throws nothing; calling it does.
        self.throws_channels.push(Vec::new());
        let ret_ty = match def.body {
            Some(lambda_body) => {
                let saved_scope = self.current_scope;
                if lambda_scope.is_some() {
                    self.current_scope = lambda_scope;
                }
                let saved_diverges = std::mem::replace(&mut self.diverges, Diverges::Maybe);
                let ret_ty = match &ret_expectation {
                    Some(ret) if !ret.has_error() => {
                        self.check_expr(body, lambda_body, ret);
                        ret.clone()
                    }
                    _ => {
                        let body_ty = self.infer_expr(body, lambda_body, &Expectation::None);
                        self.widen_fresh(&body_ty)
                    }
                };
                self.diverges = saved_diverges;
                self.current_scope = saved_scope;
                ret_ty
            }
            None => ret_expectation.unwrap_or_else(Ty::error),
        };
        let channel = self.throws_channels.pop().expect("pushed above");
        let throws_ty = written_throws.unwrap_or_else(|| {
            if channel.is_empty() {
                Ty::never()
            } else {
                self.union_of(&channel)
            }
        });

        let params: Box<[baml_type::interned::FunctionParam]> = def
            .params
            .iter()
            .zip(&param_tys)
            .map(|(param, ty)| baml_type::interned::FunctionParam {
                name: Some(param.name.clone()),
                ty: ty.clone(),
                mode: if param.default.is_some() {
                    baml_type::FunctionParamMode::Optional
                } else {
                    baml_type::FunctionParamMode::Required
                },
            })
            .collect();
        Ty::intern(TyKind::Function {
            params,
            ret: ret_ty,
            throws: throws_ty,
            attr: TyAttr::default(),
        })
    }

    /// The instantiation vector for a generic item at a use site: explicit
    /// turbofish args (with `_` holes as fresh vars) where written, fresh
    /// variables everywhere else.
    fn instantiation_args(
        &mut self,
        site: ExprId,
        generic_params: &[baml_type::ParamTy],
    ) -> Vec<Ty> {
        let explicit: Vec<Ty> = self
            .type_refs
            .expr_type_args
            .get(&site)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|&type_ref| {
                let lowered = self.lower.lower_type_ref(&self.type_refs.store, type_ref);
                self.instantiate_holes(&lowered)
            })
            .collect();
        generic_params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                explicit
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| self.fresh_generic_arg(param))
            })
            .collect()
    }

    /// A fresh variable for one generic param at a use site: synthetic
    /// effect params get EFFECT variables (unconstrained defaults to
    /// `never`, not Error - S12's defaulting rule).
    fn fresh_generic_arg(&mut self, param: &baml_type::ParamTy) -> Ty {
        if baml_type::is_synthetic_effect_param(param.name()) {
            self.table.new_effect_var_ty()
        } else {
            self.table.new_var_ty()
        }
    }

    /// Object-constructor typing: resolve the class, instantiate its
    /// generics (explicit args or fresh vars - `Box<_> { .. }` holes are
    /// vars too), check each written field against its substituted type.
    fn infer_object(
        &mut self,
        body: &ExprBody,
        object: ExprId,
        type_name: &baml_base::TypePath,
        fields: &[(baml_type::Name, ExprId)],
        spreads: &[baml_compiler2_ast::SpreadField],
    ) -> Ty {
        let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
            self.lower.resolve_type_definition(&type_name.0)
        else {
            for (_, value) in fields {
                self.infer_expr(body, *value, &Expectation::None);
            }
            return Ty::error();
        };
        let db = self.db;
        let generic_count = baml_compiler2_ppir::item_data::class_data(db, class)
            .generic_params
            .len();
        let generic_names: Vec<baml_type::ParamTy> = crate::lower::class_generic_frame(db, class);
        let instantiation = self.instantiation_args(object, &generic_names);
        let mut instantiation = instantiation;
        instantiation.truncate(generic_count);
        while instantiation.len() < generic_count {
            instantiation.push(self.table.new_var_ty());
        }
        let field_types = crate::lower::class_field_types(db, class);
        for (name, value) in fields {
            match field_types.iter().find(|(field, _)| field == name) {
                Some((_, field_ty)) => {
                    let field_ty = substitute_params(field_ty, &instantiation);
                    self.check_expr(body, *value, &field_ty);
                }
                None => {
                    // Unknown field: S17's diagnostic.
                    self.infer_expr(body, *value, &Expectation::None);
                }
            }
        }
        for spread in spreads {
            self.infer_expr(body, spread.expr, &Expectation::None);
        }
        let short = type_name.0.last().expect("type paths are never empty");
        Ty::intern(TyKind::Class(
            self.lower.qualify_definition(
                baml_compiler2_hir::contributions::Definition::Class(class),
                short,
            ),
            instantiation.into(),
            TyAttr::default(),
        ))
    }

    /// Member access in value position. Inspection site: the receiver must
    /// resolve enough to look inside (rustc's `structurally_resolve`
    /// discipline). Class fields first; then methods on any receiver kind,
    /// as full signatures (self included) with the receiver pinning the
    /// class generics and fresh variables for the method's own - value
    /// position has no turbofish.
    fn field_access(&mut self, base_ty: &Ty, member: &baml_type::Name) -> Ty {
        let resolved = self.table.resolve_completely(base_ty);
        if let TyKind::Class(qtn, args, _) = resolved.kind()
            && let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
                self.facts.definition_of(qtn)
            && let Some((_, field_ty)) = crate::lower::class_field_types(self.db, class)
                .iter()
                .find(|(field, _)| field == member)
        {
            return substitute_params(field_ty, args);
        }
        if let Some(candidate) =
            crate::method_resolution::lookup_method(self.db, &self.facts, &resolved, member)
        {
            let signature = function_signature(self.db, candidate.method);
            let mut instantiation = candidate.class_args;
            let own: Vec<Ty> = signature.generic_params[instantiation.len()..]
                .iter()
                .map(|param| self.fresh_generic_arg(param))
                .collect();
            instantiation.extend(own);
            return function_value_ty(&signature, &instantiation);
        }
        Ty::error()
    }

    /// Whether a path expression names a local binding or parameter (which
    /// shadows any package-level name at a call site). Keyed under
    /// `current_scope`: a lambda body's expressions live in the semantic
    /// index under the LAMBDA's scope, not the owner's.
    fn path_resolves_locally(&self, expr: ExprId) -> bool {
        self.current_scope.is_some_and(|scope| {
            let key = ExprMetadataKey::new(ExprMetadataScope::Body(scope), expr);
            matches!(
                self.index.path_resolution(key),
                Some(PathResolution::Local(_))
            )
        })
    }

    /// Resolves a path expression to a local binding or a parameter through
    /// the semantic index. Owner parameters come from the lowered signature;
    /// lambda parameters from the signatures `infer_lambda` deduced.
    /// Non-local names go through `resolve_value_path`.
    fn infer_path(&mut self, expr: ExprId) -> Ty {
        let Some(scope) = self.current_scope else {
            return Ty::error();
        };
        let key = ExprMetadataKey::new(ExprMetadataScope::Body(scope), expr);
        match self.index.path_resolution(key) {
            Some(PathResolution::Local(binding_id)) => match binding_id.kind {
                BindingKind::Local(_) => {
                    // The flow overlay wins over the declared/widened
                    // binding type (narrowed within a match arm).
                    if let Some(narrowed) = self.flow.get(&binding_id) {
                        return narrowed.clone();
                    }
                    self.index
                        .local_binding(binding_id)
                        .and_then(|binding| self.result.type_of_binding.get(&binding.bind_pattern))
                        .cloned()
                        .unwrap_or_else(Ty::error)
                }
                BindingKind::Parameter(param_index) => {
                    if let Some(narrowed) = self.flow.get(&binding_id) {
                        return narrowed.clone();
                    }
                    let params = if Some(binding_id.scope) == self.owner_scope {
                        Some(&self.param_tys)
                    } else {
                        self.lambda_params.get(&binding_id.scope)
                    };
                    params
                        .and_then(|params| params.get(param_index))
                        .cloned()
                        .unwrap_or_else(Ty::error)
                }
            },
            Some(PathResolution::Unknown) | None => Ty::error(),
        }
    }

    /// One body-position annotation, lowered and hole-instantiated - the
    /// single entry for every type written inside a body (let ascriptions,
    /// lambda signature slots, turbofish go through `instantiation_args`).
    fn lower_body_annotation(&mut self, type_ref: baml_compiler2_hir::type_ref::TypeRefId) -> Ty {
        let lowered = self.lower.lower_type_ref(&self.type_refs.store, type_ref);
        self.instantiate_holes(&lowered)
    }

    /// The `process_user_written_ty` funnel (rust-analyzer's discipline):
    /// lowering is pure and emits var-less hole nodes for `_`; the inference
    /// side instantiates each hole as a fresh table variable, filled from
    /// context.
    fn instantiate_holes(&mut self, ty: &Ty) -> Ty {
        if !ty.has_infer() {
            return ty.clone();
        }
        if matches!(ty.kind(), TyKind::Infer { var: None, .. }) {
            return self.table.new_var_ty();
        }
        Ty::intern(
            ty.kind()
                .map_children(|child| self.instantiate_holes(child)),
        )
    }

    /// `base catch (e) { arms }` / `catch_all`: narrowing on the ERROR
    /// channel. The base's effect contributions collect into their own
    /// channel; the clause binding takes that union; arms subtract what
    /// they provably handle (the pattern set-subtraction machinery); the
    /// residual propagates to the enclosing channel. The result joins the
    /// base value with the arm values. Exhaustiveness of `catch_all` is
    /// S17's diagnostic.
    fn infer_catch(
        &mut self,
        body: &ExprBody,
        base: ExprId,
        clauses: &[baml_compiler2_ast::CatchClause],
        expected: &Expectation,
    ) -> Ty {
        let branch_expectation = expected.adjust_for_branches(&mut self.table);
        self.throws_channels.push(Vec::new());
        let base_ty = self.infer_expr(body, base, &branch_expectation);
        let channel = self.throws_channels.pop().expect("pushed above");
        let caught: Vec<Ty> = channel
            .iter()
            .map(|ty| self.finalize_incoming_effect(ty))
            .collect();
        let caught = if caught.is_empty() {
            Ty::never()
        } else {
            self.union_of(&caught)
        };
        let caught = {
            let resolved = self.table.resolve_completely(&caught);
            self.matrix_scrut(&resolved)
        };

        let mut arm_tys = vec![base_ty];
        let mut residual = caught.clone();
        for clause in clauses {
            self.result
                .type_of_binding
                .insert(clause.binding, caught.clone());
            if let Some(stack) = clause.stack_trace_binding {
                self.result.type_of_binding.insert(stack, Ty::string());
            }
            for &arm_id in &clause.arms {
                let arm = &body.catch_arms[arm_id];
                let outcome = self.lower_pattern(body, arm.pattern, &caught);
                let consumes = outcome.consumes_matched;
                let matched = outcome.matched_ty.clone();
                self.diverges = Diverges::Maybe;
                let arm_ty = self.infer_expr(body, arm.body, &branch_expectation);
                arm_tys.push(arm_ty);
                if consumes {
                    residual = self.subtract_narrow(&residual, &matched);
                }
            }
        }
        if !matches!(residual.kind(), TyKind::Never { .. }) {
            self.record_throw(base, &residual);
        }
        self.join(&arm_tys)
    }

    /// A caught effect contribution, resolved for the error channel: still
    /// live variables resolve where possible (an unconstrained effect is
    /// `never` here too).
    fn finalize_incoming_effect(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        if resolved.has_infer() {
            // Effect vars inside the base that never got constrained: the
            // conservative read for catching purposes is Error-free
            // emptiness - drop to never; real obligations arrive with I4.
            return Ty::never();
        }
        resolved
    }

    /// One effect contribution: a thrown value or a callee's throws,
    /// accumulated into the current channel and, when the owner DECLARED
    /// its clause, checked against that contract - including when the
    /// clause mentions rigid type vars (the check defers through bounds
    /// rather than being skipped: B-1082's rule).
    fn record_throw(&mut self, at: ExprId, ty: &Ty) {
        if matches!(ty.kind(), TyKind::Never { .. }) || ty.has_error() {
            return;
        }
        // Thrown literals KEEP their literal types (no widening): catch
        // arms match on literal error codes, and the canonical union at
        // the channel is the generation site.
        let contribution = ty.clone();
        if let Some(declared) = self.declared_throws.clone()
            && !declared.has_error()
            && self.throws_channels.len() == 1
            && !self.sub(&contribution, &declared)
        {
            self.result
                .type_mismatches
                .insert(at, (declared, contribution.clone()));
        }
        self.throws_channels
            .last_mut()
            .expect("channel stack never empty")
            .push(contribution);
    }

    /// The endgame (S13 finalize): resolve bounded variables to fixpoint,
    /// drain the deferred residue, then FINALIZE every recorded type -
    /// substitute solutions, replace each surviving variable or hole with
    /// the Error sentinel LOCALLY (rust-analyzer's replace-with-error
    /// discipline, never poison-to-top; rulings 2/3 - the diagnostics land
    /// with S17), and re-canonicalize the unions that `union_of` left
    /// syntactic while variables were live. The invariant afterward: no
    /// `Infer` reaches the result.
    fn finish(mut self) -> InferenceResult {
        self.resolve_bounded_vars();
        self.drain_deferred_subs();
        // BAML's only defaulting rule: an unconstrained EFFECT is `never`
        // (a value variable erases to Error instead - ruling 2).
        self.table.default_unsolved_effects_to_never();
        let throws = match self.declared_throws.clone() {
            Some(declared) => declared,
            None => {
                let contributions = self.throws_channels[0].clone();
                let resolved: Vec<Ty> = contributions
                    .iter()
                    .map(|ty| self.finalize_ty(ty))
                    .filter(|ty| !ty.has_error())
                    .collect();
                if resolved.is_empty() {
                    Ty::never()
                } else {
                    self.union_of(&resolved)
                }
            }
        };
        let mut result = std::mem::take(&mut self.result);
        result.throws = throws;
        for ty in result
            .type_of_expr
            .values_mut()
            .chain(result.type_of_binding.values_mut())
        {
            *ty = self.finalize_ty(ty);
        }
        for (expected, actual) in result.type_mismatches.values_mut() {
            *expected = self.finalize_ty(expected);
            *actual = self.finalize_ty(actual);
        }
        result
    }

    /// One recorded type, finalized: solved variables substituted,
    /// survivors erased to the local Error sentinel, unions
    /// re-canonicalized (skipped for error-carrying types - the canonical
    /// algebra is Error-tolerant and would collapse them arbitrarily).
    fn finalize_ty(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        let erased = erase_infer(&resolved);
        if erased.has_error() {
            return erased;
        }
        self.canonicalize_unions(&erased)
    }

    /// Rebuilds `ty` with every union node in canonical form, bottom-up.
    /// Idempotent on already-canonical types; repairs the syntactic unions
    /// `union_of` built while a member still carried a variable.
    ///
    /// Presentation order: `null` moves LAST - the optional convention
    /// (`T?` reads `T | null`, the spec's own notation). The shared
    /// canonical sort is an internal detail load-bearing for the TIR-era
    /// tier snapshots, so the convention applies at this crate's result
    /// boundary; it folds into the shared algebra at cutover (S16).
    fn canonicalize_unions(&self, ty: &Ty) -> Ty {
        match ty.kind() {
            TyKind::Union(members, _) => {
                let members: Vec<Ty> = members
                    .iter()
                    .map(|member| self.canonicalize_unions(member))
                    .collect();
                let joined = canonical_union_interned(&members, &self.facts);
                match joined.kind() {
                    TyKind::Union(members, attr) => {
                        let (mut ordered, nulls): (Vec<Ty>, Vec<Ty>) = members
                            .iter()
                            .cloned()
                            .partition(|member| !matches!(member.kind(), TyKind::Null { .. }));
                        ordered.extend(nulls);
                        Ty::intern(TyKind::Union(ordered.into(), attr.clone()))
                    }
                    _ => joined,
                }
            }
            _ => Ty::intern(ty.kind().map_children(|child| self.canonicalize_unions(child))),
        }
    }

    /// Derives solutions from accumulated bounds, iterating because one
    /// resolution can make another class's bounds ground.
    fn resolve_bounded_vars(&mut self) {
        loop {
            let mut progressed = false;
            for (var, bounds) in self.table.unsolved_bounded_vars() {
                // Bounds must be ground to decide; classes whose bounds
                // still mention other unsolved vars wait for a later round
                // (a cycle that never grounds is erased at finalize).
                let lowers: Vec<Ty> = bounds
                    .lowers
                    .iter()
                    .map(|ty| self.table.resolve_completely(ty))
                    .collect();
                let uppers: Vec<Ty> = bounds
                    .uppers
                    .iter()
                    .map(|ty| self.table.resolve_completely(ty))
                    .collect();
                if lowers.iter().chain(uppers.iter()).any(Ty::has_infer) {
                    continue;
                }
                let solution = if lowers.is_empty() {
                    // No values flowed in: the MINIMUM upper is the meet
                    // when one exists (BAML has no intersections, so
                    // incomparable uppers have no representable meet -
                    // unresolved, erased at finalize).
                    let minimum = uppers.iter().find(|candidate| {
                        uppers
                            .iter()
                            .all(|upper| is_subtype_interned(candidate, upper, &self.facts))
                    });
                    match minimum {
                        Some(minimum) => minimum.clone(),
                        None => continue,
                    }
                } else {
                    // Ruling 1: widen fresh literals, then all lowers must
                    // AGREE (equality, not adjacency-dedup); disagreement
                    // is a mismatch (Error until the S17 diagnostic), and
                    // the choice is checked against every upper.
                    let widened: Vec<Ty> =
                        lowers.iter().map(|ty| self.widen_fresh(ty)).collect();
                    let first = widened.first().expect("non-empty lowers").clone();
                    if widened.iter().all(|lower| *lower == first) {
                        if uppers
                            .iter()
                            .all(|upper| is_subtype_interned(&first, upper, &self.facts))
                        {
                            first
                        } else {
                            Ty::error()
                        }
                    } else {
                        Ty::error()
                    }
                };
                self.table.solve(var, solution);
                progressed = true;
            }
            if !progressed {
                break;
            }
        }
    }

    /// Re-examines the deferred `Sub` residue now that resolution has run;
    /// still-undecidable pairs are conservatively dropped (they become
    /// diagnostics with S17 and obligations with I4).
    fn drain_deferred_subs(&mut self) {
        let deferred = std::mem::take(&mut self.deferred_subs);
        for (actual, expected) in deferred {
            let actual = self.table.resolve_completely(&actual);
            let expected = self.table.resolve_completely(&expected);
            if !actual.has_infer() && !expected.has_infer() {
                let _ = is_subtype_interned(&actual, &expected, &self.facts);
            }
        }
    }
}

/// A resolved function as a first-class value: its signature instantiated
/// into an interned function type. Shared by direct calls (turbofish-aware
/// instantiation) and value-position references (fresh-var instantiation).
fn function_value_ty(signature: &crate::lower::FunctionSignature, instantiation: &[Ty]) -> Ty {
    let params: Box<[baml_type::interned::FunctionParam]> = signature
        .params
        .iter()
        .map(|param| baml_type::interned::FunctionParam {
            name: Some(param.name.clone()),
            ty: substitute_params(&param.ty, instantiation),
            mode: if param.has_default {
                baml_type::FunctionParamMode::Optional
            } else {
                baml_type::FunctionParamMode::Required
            },
        })
        .collect();
    Ty::intern(TyKind::Function {
        params,
        ret: substitute_params(&signature.ret, instantiation),
        throws: substitute_params(&signature.throws, instantiation),
        attr: TyAttr::default(),
    })
}

/// Replaces every `Infer` node (unsolved variable or hole) with the Error
/// sentinel, in place - the finalize half of rulings 2/3.
fn erase_infer(ty: &Ty) -> Ty {
    if !ty.has_infer() {
        return ty.clone();
    }
    if matches!(ty.kind(), TyKind::Infer { .. }) {
        return Ty::error();
    }
    Ty::intern(ty.kind().map_children(erase_infer))
}

/// A fresh literal widens to its base primitive at binding sites (the spec's
/// TypeScript-style widening); everything else passes through. Top-level
/// only - container-element widening arrives with the join machinery.
fn widen_fresh_literal(ty: &Ty) -> Ty {
    match ty.kind() {
        TyKind::Literal(literal, Freshness::Fresh, attr) => {
            Ty::intern(literal_base(literal, attr.clone()))
        }
        _ => ty.clone(),
    }
}

/// The base primitive a literal type belongs to.
fn literal_base(literal: &Literal, attr: TyAttr) -> TyKind {
    match literal {
        Literal::Int(_) => TyKind::Int { attr },
        Literal::Bigint(_) => TyKind::Bigint { attr },
        Literal::Float(_) => TyKind::Float { attr },
        Literal::String(_) => TyKind::String { attr },
        Literal::Bool(_) => TyKind::Bool { attr },
    }
}

/// An operand's union alternatives for operator dispatch, literals widened
/// to their bases regardless of freshness (dispatch is by base type; every
/// alternative must support the operator).
fn operand_members(ty: &Ty) -> Vec<Ty> {
    fn widen(ty: &Ty) -> Ty {
        match ty.kind() {
            TyKind::Literal(literal, _, attr) => Ty::intern(literal_base(literal, attr.clone())),
            _ => ty.clone(),
        }
    }
    match ty.kind() {
        TyKind::Union(members, _) => members.iter().map(widen).collect(),
        _ => vec![widen(ty)],
    }
}
