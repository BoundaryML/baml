//! Body type inference: `infer_body` walks one body owner's expression tree
//! with an [`InferenceContext`] over an [`unify::InferenceTable`].
//!
//! S9 state: bidirectional checking. `infer_expr` synthesizes with an
//! [`Expectation`] flowing down (informing shape: container elements,
//! if-branch pass-through, lambda signature deduction); `check_expr`
//! additionally emits a `Sub` constraint, discharged eagerly per the settled
//! design - invariant heads decay to `Eq`, ground pairs ask the canonical
//! oracle, var-headed pairs deposit bounds, the irreducible residue defers
//! to finish. Control-flow merge points join through
//! `canonical_union_interned` (never fabricated at variables - ruling 1);
//! `Diverges` tracks never-propagation. Value paths resolve through one
//! entry (`resolve_value_path`); lambdas deduce unwritten signature slots
//! from the expected function type and their bodies type in the owner's
//! table under the lambda's scope. Constructs the engine does not handle
//! yet still record the `Error` sentinel and upgrade slice by slice.

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
        BindingKind, ExprMetadataKey, ExprMetadataScope, FileSemanticIndex, PathResolution,
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
#[derive(Debug, Default, Clone, PartialEq)]
pub struct InferenceResult {
    pub type_of_expr: FxHashMap<ExprId, Ty>,
    pub type_of_binding: FxHashMap<PatId, Ty>,
    /// Definite check failures, keyed by the checked expression:
    /// `(expected, actual)`. Recorded always (rust-analyzer's discipline);
    /// rendered as diagnostics in S17.
    pub type_mismatches: FxHashMap<ExprId, (Ty, Ty)>,
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
    let (frame, param_tys, return_ty) = match owner {
        BodyOwnerId::Function(function) => {
            let signature = function_signature(db, function);
            (
                function_generic_frame(db, function),
                signature.params.into_iter().map(|param| param.ty).collect(),
                Some(signature.ret),
            )
        }
        BodyOwnerId::Let(_) => (Vec::new(), Vec::new(), None),
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
            lower,
            param_tys,
            type_refs,
            return_ty,
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
                let condition_diverges = self.diverges;
                let branch_expectation = expected.adjust_for_branches(&mut self.table);

                self.diverges = Diverges::Maybe;
                let then_ty = self.infer_expr(body, *then_branch, &branch_expectation);
                let then_diverges = self.diverges;

                match else_branch {
                    Some(else_expr) => {
                        self.diverges = Diverges::Maybe;
                        let else_ty = self.infer_expr(body, *else_expr, &branch_expectation);
                        let else_diverges = self.diverges;
                        self.diverges = condition_diverges.or(then_diverges.and(else_diverges));
                        // The merge point: a canonical union, never a forced
                        // equality (joins happen at generation sites).
                        self.join(&[then_ty, else_ty])
                    }
                    None => {
                        // No else: the if produces no value.
                        self.diverges = condition_diverges;
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
                        Ty::list(canonical_union_interned(&joined, &self.facts))
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
                        key: canonical_union_interned(&keys, &self.facts),
                        value: canonical_union_interned(&values, &self.facts),
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
                // The thrown type feeds the throws channel in S12.
                self.infer_expr(body, *value, &Expectation::None);
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
            // Destructuring patterns: later slices.
            _ => {
                if let Some(init) = initializer {
                    self.infer_expr(body, init, &Expectation::None);
                }
                Ty::error()
            }
        };
        if let Some(else_expr) = else_branch {
            // Ruling: the else branch must diverge; the check itself is an
            // S17 diagnostic. Its divergence does not leak past the let.
            let saved = self.diverges;
            self.infer_expr(body, else_expr, &Expectation::None);
            self.diverges = saved;
        }
        self.result.type_of_binding.insert(pattern, binding_ty);
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
        let joined = canonical_union_interned(members, &self.facts);
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
                canonical_union_interned(&widened, &self.facts)
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
        canonical_union_interned(&outputs, &self.facts)
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
                    canonical_union_interned(&non_null, &self.facts)
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
    /// (TIR's InvalidBinaryOp, S17's diagnostic). DELETE this table - and
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
        let callee_fn_ty = self.infer_callee(body, call, callee);
        let TyKind::Function { params, ret, .. } = callee_fn_ty.kind() else {
            // Not callable (or not yet typed): visit the args, sentinel out.
            for arg in args {
                self.infer_expr(body, arg.expr, &Expectation::None);
            }
            return Ty::error();
        };
        let params: Vec<Ty> = params.iter().map(|param| param.ty.clone()).collect();
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

    /// The callee's (instantiated) function type. A direct function
    /// reference is the one case that does not go through
    /// `resolve_value_path`: its instantiation reads the CALL site's
    /// turbofish, which a bare path value does not have. Everything else is
    /// whatever the expression infers to (method callees arrive with
    /// S10/S11).
    fn infer_callee(&mut self, body: &ExprBody, call: ExprId, callee: ExprId) -> Ty {
        // A path that names a function (and is not shadowed by a local)
        // is a direct call.
        if let Expr::Path(segments) = &body.exprs[callee]
            && !self.path_resolves_locally(callee)
            && let Some(baml_compiler2_hir::contributions::Definition::Function(function)) =
                self.lower.resolve_value(segments)
        {
            let signature = function_signature(self.db, function);
            let instantiation = self.instantiation_args(call, &signature.generic_params);
            let fn_ty = function_value_ty(&signature, &instantiation);
            self.result.type_of_expr.insert(callee, fn_ty.clone());
            return fn_ty;
        }
        self.infer_expr(body, callee, &Expectation::None)
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
                .map(|_| self.table.new_var_ty())
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

        let throws_ty = signature
            .as_ref()
            .and_then(|sig| sig.throws)
            .map(|type_ref| self.lower_body_annotation(type_ref))
            .unwrap_or_else(Ty::error);

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
        (0..generic_params.len())
            .map(|index| {
                explicit
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| self.table.new_var_ty())
            })
            .collect()
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

    /// Class field access. Inspection site: the receiver must resolve
    /// enough to look inside (rustc's `structurally_resolve` discipline).
    /// Methods, builtins, and non-class receivers arrive with S10/S11.
    fn field_access(&mut self, base_ty: &Ty, member: &baml_type::Name) -> Ty {
        let resolved = self.table.resolve_completely(base_ty);
        let TyKind::Class(qtn, args, _) = resolved.kind() else {
            return Ty::error();
        };
        let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
            self.facts.definition_of(qtn)
        else {
            return Ty::error();
        };
        crate::lower::class_field_types(self.db, class)
            .iter()
            .find(|(field, _)| field == member)
            .map(|(_, field_ty)| substitute_params(field_ty, args))
            .unwrap_or_else(Ty::error)
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
                BindingKind::Local(_) => self
                    .index
                    .local_binding(binding_id)
                    .and_then(|binding| self.result.type_of_binding.get(&binding.pattern))
                    .cloned()
                    .unwrap_or_else(Ty::error),
                BindingKind::Parameter(param_index) => {
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

    /// The endgame: resolve bounded variables (the ruling-1 skeleton -
    /// widen fresh lowers, lowers must AGREE, checked against the uppers;
    /// S13 adds defaulting rounds and the full policy), drain the deferred
    /// residue, then substitute solved variables out of every recorded
    /// type. The S13 finalization invariant (no `Infer` reaches the result)
    /// lands with `finalize_var`.
    fn finish(mut self) -> InferenceResult {
        self.resolve_bounded_vars();
        self.drain_deferred_subs();
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

    /// Derives solutions from accumulated bounds, iterating because one
    /// resolution can make another class's bounds ground.
    fn resolve_bounded_vars(&mut self) {
        loop {
            let mut progressed = false;
            for (var, bounds) in self.table.unsolved_bounded_vars() {
                // Bounds must be ground to decide; classes whose bounds
                // still mention other unsolved vars wait for a later round.
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
                    // No values flowed in: a single agreed upper is the
                    // answer (the general meet arrives with S13).
                    let mut uppers = uppers;
                    uppers.dedup();
                    match uppers.as_slice() {
                        [only] => only.clone(),
                        _ => continue,
                    }
                } else {
                    // Ruling 1: widen fresh literals, then all lowers must
                    // agree; disagreement is a mismatch (Error until the
                    // S17 diagnostic), and the choice is checked against
                    // every upper.
                    let mut widened: Vec<Ty> =
                        lowers.iter().map(|ty| self.widen_fresh(ty)).collect();
                    widened.dedup();
                    match widened.as_slice() {
                        [only] => {
                            let candidate = only.clone();
                            if uppers
                                .iter()
                                .all(|upper| is_subtype_interned(&candidate, upper, &self.facts))
                            {
                                candidate
                            } else {
                                Ty::error()
                            }
                        }
                        _ => Ty::error(),
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
