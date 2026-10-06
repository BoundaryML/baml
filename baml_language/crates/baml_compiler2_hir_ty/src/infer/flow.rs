//! Flow-sensitive narrowing (S10b) - the settled eager-forward design
//! from the pyright/TS + Kotlin/Roslyn/Sorbet survey, carried directly on
//! the structured AST walk (BAML has no unstructured control flow, so no
//! CFG layer is needed):
//!
//! - The environment is the `flow` overlay on `InferenceContext`
//!   (`BindingId -> Ty`), cloned at branch points, consulted before the
//!   declared/widened binding type.
//! - [`CondFacts`] carries a condition's `when_true`/`when_false` fact
//!   maps, combined with walk-time De Morgan over `&&`/`||`/`!` (B-688) -
//!   the AST still has the boolean structure, so no bind-time target
//!   threading is required. The walk that types a tested expression
//!   returns them ([`InferenceContext::infer_test`]): a test is judged
//!   once, in the flow that holds where it is evaluated.
//! - `&&` / `||` short-circuit, so their RIGHT operand is typed under the
//!   left operand's facts (true facts for `&&`, false facts for `||`), and
//!   the overlay from before the operand comes back after it. Only a bare
//!   local that no closure captures carries a fact: BAML tasks interleave
//!   between statements, so a field path or an index path, which is read a
//!   second time, can change between the test and the read.
//! - Branch merges are divergence-aware: a diverged branch contributes
//!   nothing, so guard-with-early-return narrowing is the ordinary merge
//!   rule, not a special case.
//! - Loops havoc the bindings their bodies assign, and the post-loop
//!   environment restarts from loop ENTRY (zero-iteration soundness,
//!   B-735 - the TS/pyright structural rule) plus the condition's false
//!   facts.
//! - Assignments check against the DECLARED binding type, never the
//!   overlay (B-618), then re-narrow the overlay to the assigned value.
//! - Else-side subtraction is gated on [`super::pat::PatternOutcome::
//!   consumes_matched`]: only a pattern refutable by type alone may be
//!   subtracted (B-1069).

use baml_compiler2_ast::{Expr, ExprBody, ExprId, StmtId};
use baml_compiler2_hir::semantic_index::BindingId;
use baml_type::interned::{InferTy, Ty};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{Expectation, InferenceContext};

/// A condition's narrowing consequences for each polarity.
#[derive(Default)]
pub(super) struct CondFacts {
    pub when_true: FxHashMap<BindingId, Ty>,
    pub when_false: FxHashMap<BindingId, Ty>,
}

impl CondFacts {
    pub(super) fn swapped(self) -> CondFacts {
        CondFacts {
            when_true: self.when_false,
            when_false: self.when_true,
        }
    }
}

/// Where a test is written. A test that can never hold is reported only in
/// a branch condition (`if`, `while`, a match guard): as a value it is a
/// legal test that answers `false`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TestPosition {
    Condition,
    Value,
}

/// The operators that evaluate their right operand for one outcome of the
/// left operand only.
#[derive(Clone, Copy)]
pub(super) enum ShortCircuit {
    And,
    Or,
}

impl From<ShortCircuit> for baml_compiler2_ast::BinaryOp {
    fn from(op: ShortCircuit) -> Self {
        match op {
            ShortCircuit::And => Self::And,
            ShortCircuit::Or => Self::Or,
        }
    }
}

impl InferenceContext<'_> {
    /// Infers an expression that is tested for truthiness and returns, with
    /// its type, what each outcome proves about bare locals. Leaves:
    /// `x == null` / `x != null`, `x is <pattern>` and a bare `x`;
    /// combinators: `&&`/`||`/`!` with De Morgan.
    ///
    /// The facts come from the same walk that types the expression, so each
    /// test is judged in the flow that holds where it is evaluated: after
    /// the assignments of the operands before it, and under the facts of a
    /// left operand that had to hold for it to run.
    pub(super) fn infer_test(
        &mut self,
        body: &ExprBody,
        expr: ExprId,
        position: TestPosition,
    ) -> (Ty, CondFacts) {
        use baml_compiler2_ast::{BinaryOp, UnaryOp};
        let (ty, facts) = match &body.exprs[expr] {
            Expr::Unary {
                op: UnaryOp::Not,
                expr: operand,
            } => self.infer_not(body, *operand, position),
            Expr::Binary {
                op: BinaryOp::And,
                lhs,
                rhs,
            } => self.infer_short_circuit(body, ShortCircuit::And, *lhs, *rhs, position),
            Expr::Binary {
                op: BinaryOp::Or,
                lhs,
                rhs,
            } => self.infer_short_circuit(body, ShortCircuit::Or, *lhs, *rhs, position),
            Expr::Is { scrutinee, pattern } => self.infer_is(body, *scrutinee, *pattern, position),
            Expr::Binary {
                op: op @ (BinaryOp::Eq | BinaryOp::Ne),
                lhs,
                rhs,
            } => {
                let ty = self.infer_expr(body, expr, &Expectation::None);
                let facts = self.null_test_facts(body, *lhs, *rhs).unwrap_or_default();
                let facts = if matches!(op, BinaryOp::Eq) {
                    facts
                } else {
                    facts.swapped()
                };
                return (ty, facts);
            }
            _ => {
                let ty = self.infer_expr(body, expr, &Expectation::None);
                return (ty, self.truthiness_facts(body, expr));
            }
        };
        self.result.type_of_expr.insert(expr, ty.clone());
        (ty, facts)
    }

    /// `lhs && rhs` / `lhs || rhs`.
    ///
    /// The operators short-circuit: the right operand runs only after the
    /// left one was true (`&&`) or false (`||`), so it is typed under the
    /// left operand's facts for that outcome. The facts are about bare
    /// local bindings that no closure captures ([`Self::narrowable_binding`]):
    /// such a binding belongs to this call, so nothing can change it between
    /// the test on the left and the read on the right. A field path is read a
    /// second time on the right, and another task or an alias can change it
    /// between the two reads, so it is never narrowed.
    ///
    /// After the expression the overlay is the one from before the right
    /// operand, because that operand may not have run; a binding it assigns
    /// keeps a refinement only where both paths agree. A binding that the
    /// right operand assigns no longer has the type that the left operand
    /// found, so the left operand's facts about it are dropped:
    /// `x is A && { x = b; true }` proves nothing about `x`.
    pub(super) fn infer_short_circuit(
        &mut self,
        body: &ExprBody,
        op: ShortCircuit,
        lhs: ExprId,
        rhs: ExprId,
        position: TestPosition,
    ) -> (Ty, CondFacts) {
        let (lhs_ty, mut left) = self.check_truthy_operand(body, lhs, true, position);
        let skipped_flow = self.flow.clone();
        self.apply_facts(match op {
            ShortCircuit::And => &left.when_true,
            ShortCircuit::Or => &left.when_false,
        });
        let (rhs_ty, right) = self.check_truthy_operand(body, rhs, true, position);
        let taken_flow = std::mem::replace(&mut self.flow, skipped_flow);
        let reassigned = self.assigned_bindings(body, rhs);
        for binding in &reassigned {
            match (taken_flow.get(binding), self.flow.get(binding)) {
                (Some(taken), Some(skipped)) if taken == skipped => {}
                (Some(taken), Some(skipped)) => {
                    let joined = self.join(&[taken.clone(), skipped.clone()]);
                    self.flow.insert(*binding, joined);
                }
                _ => {
                    self.flow.remove(binding);
                }
            }
        }
        left.when_true
            .retain(|binding, _| !reassigned.contains(binding));
        left.when_false
            .retain(|binding, _| !reassigned.contains(binding));
        let facts = match op {
            ShortCircuit::And => CondFacts {
                when_true: self.all_facts(left.when_true, right.when_true),
                when_false: self.any_facts(&left.when_false, &right.when_false),
            },
            ShortCircuit::Or => CondFacts {
                when_true: self.any_facts(&left.when_true, &right.when_true),
                when_false: self.all_facts(left.when_false, right.when_false),
            },
        };
        let lhs_ty = self.table.resolve_completely(&lhs_ty);
        let rhs_ty = self.table.resolve_completely(&rhs_ty);
        let ty = super::const_fold_binary(op.into(), &lhs_ty, &rhs_ty).unwrap_or_else(Ty::bool);
        (ty, facts)
    }

    /// Truthiness (B-1563): a bare narrowable value as a whole test narrows
    /// by POLARITY - the true branch drops always-falsy union members
    /// (`null`, `false`, zero/empty literals), the false branch drops
    /// always-truthy ones (instances, functions, non-falsy literals).
    /// Runtime-decided members (`string`, `int`, containers) survive both
    /// sides - the language has no "non-empty string" type to narrow to.
    fn truthiness_facts(&mut self, body: &ExprBody, expr: ExprId) -> CondFacts {
        let Some(binding) = self.narrowable_binding(body, expr) else {
            return CondFacts::default();
        };
        let scrut = self.binding_flow_ty(binding);
        let resolved = self.table.resolve_completely(&scrut);
        if resolved.has_error() || resolved.has_infer() {
            return CondFacts::default();
        }
        let mut facts = CondFacts::default();
        facts
            .when_true
            .insert(binding, Self::drop_members_by_truthiness(&resolved, false));
        facts
            .when_false
            .insert(binding, Self::drop_members_by_truthiness(&resolved, true));
        facts
    }

    /// Set-subtraction on truthiness (the `subtract_narrow` discipline):
    /// drop the members whose truthiness is statically the given
    /// polarity; survivors keep their identity, nothing dropped leaves
    /// the type unchanged, everything dropped is `never`.
    fn drop_members_by_truthiness(scrut: &Ty, drop_truthy: bool) -> Ty {
        use crate::infer::truthy::{Truthiness, truthiness};
        let members: Vec<Ty> = match scrut.kind() {
            InferTy::Union(members) => members.to_vec(),
            _ => vec![scrut.clone()],
        };
        let dropped = if drop_truthy {
            Truthiness::AlwaysTruthy
        } else {
            Truthiness::AlwaysFalsy
        };
        let kept: Vec<Ty> = members
            .iter()
            .filter(|member| truthiness(member) != dropped)
            .cloned()
            .collect();
        if kept.len() == members.len() {
            return scrut.clone();
        }
        crate::infer::syntactic_union(&kept)
    }

    /// `x == null` / `null == x` for a narrowable `x`: true implies the
    /// null part, false implies the non-null part.
    fn null_test_facts(&mut self, body: &ExprBody, lhs: ExprId, rhs: ExprId) -> Option<CondFacts> {
        let (null_side, other) = match (&body.exprs[lhs], &body.exprs[rhs]) {
            (Expr::Null, _) => (lhs, rhs),
            (_, Expr::Null) => (rhs, lhs),
            _ => return None,
        };
        let _ = null_side;
        let binding = self.narrowable_binding(body, other)?;
        let ty = self.binding_flow_ty(binding);
        let resolved = self.table.resolve_completely(&ty);
        let mut facts = CondFacts::default();
        facts.when_true.insert(binding, Ty::null());
        facts
            .when_false
            .insert(binding, self.remove_null(&resolved));
        Some(facts)
    }

    /// Conjunction of fact maps: both hold, so entries merge and a shared
    /// binding takes the tighter type.
    fn all_facts(
        &mut self,
        mut left: FxHashMap<BindingId, Ty>,
        right: FxHashMap<BindingId, Ty>,
    ) -> FxHashMap<BindingId, Ty> {
        for (binding, ty) in right {
            let entry = match left.remove(&binding) {
                Some(existing) => self.narrow_meet(&existing, &ty),
                None => ty,
            };
            left.insert(binding, entry);
        }
        left
    }

    /// Disjunction of fact maps: only facts BOTH sides establish survive,
    /// joined.
    fn any_facts(
        &mut self,
        left: &FxHashMap<BindingId, Ty>,
        right: &FxHashMap<BindingId, Ty>,
    ) -> FxHashMap<BindingId, Ty> {
        let mut out = FxHashMap::default();
        for (binding, left_ty) in left {
            if let Some(right_ty) = right.get(binding) {
                out.insert(*binding, self.join(&[left_ty.clone(), right_ty.clone()]));
            }
        }
        out
    }

    /// The tighter of two refinements of the same binding (approximate
    /// meet, same policy as pattern narrowing).
    fn narrow_meet(&self, a: &Ty, b: &Ty) -> Ty {
        if self.provable_subtype(a, b) {
            a.clone()
        } else {
            b.clone()
        }
    }

    /// A binding's current flow type: the overlay, else the declared type
    /// (the recorded binding type for locals, the signature/deduced type
    /// for parameters - mirroring `infer_path`).
    pub(super) fn binding_flow_ty(&self, binding: BindingId) -> Ty {
        if let Some(narrowed) = self.flow.get(&binding) {
            return narrowed.clone();
        }
        self.binding_declared_ty(binding)
    }

    /// The un-narrowed type a binding declares.
    pub(super) fn binding_declared_ty(&self, binding: BindingId) -> Ty {
        use baml_compiler2_hir::semantic_index::BindingKind;
        match binding.kind {
            BindingKind::Local(_) => self
                .index
                .local_binding(binding)
                .and_then(|local| self.result.type_of_pat.get(&local.bind_pattern))
                .cloned()
                .unwrap_or_else(Ty::error),
            BindingKind::Parameter(param_index) => {
                let params = if Some(binding.scope) == self.owner_scope {
                    Some(&self.param_tys)
                } else {
                    self.lambda_params.get(&binding.scope)
                };
                params
                    .and_then(|params| params.get(param_index))
                    .cloned()
                    .unwrap_or_else(Ty::error)
            }
        }
    }

    /// Set-subtraction on canonical members: drop the members provably
    /// inside `matched`; nothing dropped leaves the scrutinee unchanged
    /// (never a fabricated narrower type), everything dropped is `never`.
    pub(super) fn subtract_narrow(&mut self, scrut: &Ty, matched: &Ty) -> Ty {
        let members: Vec<Ty> = match scrut.kind() {
            InferTy::Union(members) => members.to_vec(),
            _ => vec![scrut.clone()],
        };
        let kept: Vec<Ty> = members
            .iter()
            .filter(|member| !self.provable_subtype(member, matched))
            .cloned()
            .collect();
        if kept.len() == members.len() {
            return scrut.clone();
        }
        // Filtering never REWRITES the survivors (TS's `getTypeWithFacts`
        // shape): the structural constructor keeps each member's
        // identity - freshness included - where the canonical path
        // would re-mark literals rigid.
        crate::infer::syntactic_union(&kept)
    }

    /// Applies one polarity's facts onto the flow overlay.
    pub(super) fn apply_facts(&mut self, facts: &FxHashMap<BindingId, Ty>) {
        for (binding, ty) in facts {
            self.flow.insert(*binding, ty.clone());
        }
    }

    /// The divergence-aware branch merge: a diverged branch contributes
    /// nothing (early-return narrowing IS this rule); with both branches
    /// live, a binding keeps an overlay only when both narrowed it, joined.
    pub(super) fn merge_branch_flows(
        &mut self,
        base: FxHashMap<BindingId, Ty>,
        then_flow: Option<FxHashMap<BindingId, Ty>>,
        else_flow: Option<FxHashMap<BindingId, Ty>>,
    ) {
        self.flow = match (then_flow, else_flow) {
            // Both diverged: everything after is unreachable; keep entry.
            (None, None) => base,
            (Some(live), None) | (None, Some(live)) => live,
            (Some(then_flow), Some(else_flow)) => {
                // Facts invalidated on both paths must not reappear from base.
                let mut merged = FxHashMap::default();
                let keys: FxHashSet<BindingId> =
                    then_flow.keys().chain(else_flow.keys()).copied().collect();
                for binding in keys {
                    match (then_flow.get(&binding), else_flow.get(&binding)) {
                        (Some(a), Some(b)) if a == b => {
                            merged.insert(binding, a.clone());
                        }
                        (Some(a), Some(b)) => {
                            let joined = self.join(&[a.clone(), b.clone()]);
                            merged.insert(binding, joined);
                        }
                        // Narrowed on one path only: the other path keeps
                        // the wider type, so the join is the declared type
                        // - no overlay.
                        _ => {
                            merged.remove(&binding);
                        }
                    }
                }
                merged
            }
        };
    }

    /// Every narrowable binding the subtree assigns - the loop havoc set.
    pub(super) fn assigned_bindings(&self, body: &ExprBody, root: ExprId) -> FxHashSet<BindingId> {
        use baml_compiler2_ast::{Stmt, traverse::BodyNode};
        let mut out = FxHashSet::default();
        let mut stack = vec![BodyNode::Expr(root)];
        while let Some(node) = stack.pop() {
            let mut children = Vec::new();
            match node {
                BodyNode::Expr(expr) => body.expr_children(expr, &mut children),
                BodyNode::Stmt(stmt) => {
                    if let Stmt::Assign { target, .. } | Stmt::AssignOp { target, .. } =
                        &body.stmts[stmt]
                        && let Some(binding) = self.narrowable_binding(body, *target)
                    {
                        out.insert(binding);
                    }
                    body.stmt_children(stmt, &mut children);
                }
            }
            stack.extend(children);
        }
        out
    }

    /// Whether a loop contains a `break` that binds to THAT loop.
    ///
    /// `root` is the loop body and `after` its C-style update slot. Both are
    /// searched: `for (let i = 0; 1 == 1; i = break) {}` puts a real exit edge
    /// in the update slot, so missing it would call an escapable loop
    /// divergent.
    ///
    /// TIR keeps no break-target machinery — `loop_depth` is a bare counter
    /// and the `Stmt::Break` arm only sets `Diverges::Always`, which the
    /// enclosing loop then discards — so the binding is recovered
    /// syntactically here: descend everything except the BODIES of nested
    /// loops, whose `break`s bind to the inner loop. A nested loop's
    /// condition and C-style update slot still belong to this loop, so they
    /// are still descended. Lambda bodies are already excluded by
    /// `expr_children`, which is right: a `break` cannot leave a lambda.
    /// `continue` is deliberately not counted — it re-enters the loop.
    ///
    /// A `break` under `defer` or `spawn` is counted even though both are
    /// rejected elsewhere; counting keeps the answer on the conservative
    /// side, where the loop simply does not diverge.
    pub(super) fn loop_body_breaks(body: &ExprBody, root: ExprId, after: Option<StmtId>) -> bool {
        use baml_compiler2_ast::{Stmt, traverse::BodyNode};
        // The arena is a DAG (templates share `ExprId`s between their
        // segments and desugared payload), so the walk must dedupe.
        let mut seen: FxHashSet<BodyNode> = FxHashSet::default();
        let mut stack = vec![BodyNode::Expr(root)];
        stack.extend(after.map(BodyNode::Stmt));
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
            let mut children = Vec::new();
            match node {
                BodyNode::Expr(expr) => body.expr_children(expr, &mut children),
                BodyNode::Stmt(stmt) => match &body.stmts[stmt] {
                    Stmt::Break => return true,
                    Stmt::While {
                        condition, after, ..
                    } => {
                        children.push(BodyNode::Expr(*condition));
                        children.extend(after.map(BodyNode::Stmt));
                    }
                    Stmt::WhileLet { scrutinee, .. } => {
                        children.push(BodyNode::Expr(*scrutinee));
                    }
                    Stmt::For { collection, .. } => {
                        children.push(BodyNode::Expr(*collection));
                    }
                    _ => body.stmt_children(stmt, &mut children),
                },
            }
            stack.extend(children);
        }
        false
    }
}
