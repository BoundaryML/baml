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
//!   left operand's facts (true facts for `&&`, false facts for `||`). Only
//!   a bare local that no closure captures carries a fact: BAML tasks
//!   interleave between statements, so a field path or an index path, which
//!   is read a second time, can change between the test and the read.
//! - An operand that may be skipped - the right operand of `&&`/`||`, a
//!   `??` fallback, the links of a `?.` chain after the first `?.` - is
//!   typed from a [`SkipPoint`] and rejoined there: its divergence is not
//!   the expression's, and a binding it assigns keeps a refinement only
//!   where the taken and the skipped path agree.
//! - Branch merges are divergence-aware: a diverged branch contributes
//!   nothing, so guard-with-early-return narrowing is the ordinary merge
//!   rule, not a special case.
//! - A loop drops the facts about every binding its condition, body or
//!   update slot assigns before typing each of them once (a `for let`
//!   collection runs once, before the loop, so it is typed in the entry
//!   flow and not dropped), and the post-loop environment is the merge of
//!   its exit edges, each with the flow where it is taken: every `break`,
//!   plus the header's miss edge (a `while` condition's false edge, a
//!   `while let` pattern's failure), which leaves that havoced flow
//!   (zero-iteration soundness, B-735 - the TS/pyright structural rule). A
//!   `for let` ends at its header too, so its exits are loop entry, the
//!   body's end and every `continue`, plus its `break`s. A jump written
//!   where the flow has already diverged is no edge. A loop with no exit
//!   edge diverges.
//! - Assignments check against the DECLARED binding type, never the
//!   overlay (B-618), then re-narrow the overlay to the assigned value.
//! - Else-side subtraction is gated on [`super::pat::PatternOutcome::
//!   consumes_matched`]: only a pattern refutable by type alone may be
//!   subtracted (B-1069).

use baml_compiler2_ast::{Expr, ExprBody, ExprId, Stmt, StmtId, traverse::BodyNode};
use baml_compiler2_hir::semantic_index::BindingId;
use baml_type::{
    interned::{InferTy, Ty},
    normalize::TypeContext,
};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{Diverges, Expectation, InferenceContext};
use crate::diagnostics::LoopHeaderSlot;

/// The narrowing overlay: a binding's flow type where a fact about it is in
/// force. A binding with no entry has its declared type.
pub(super) type Flow = FxHashMap<BindingId, Ty>;

/// One enclosing loop while its header, body and update slot are typed: the
/// flows at the `break`s and `continue`s that bind to it, all written in
/// its body.
pub(super) struct LoopFrame {
    /// The flow at each `break`: an exit edge of the loop.
    pub breaks: Vec<Flow>,
    /// The flow at each `continue`: an edge into a C-style loop's update
    /// slot, and an exit edge of a `for let`, whose header may end the
    /// loop. A `while` and a `while let` re-test their header in the
    /// havoced flow, which covers these edges, so theirs go unread.
    pub continues: Vec<Flow>,
    /// The header slot being typed, if any: a `break`/`continue` written
    /// there is rejected (rustc's E0590 rule for `while` conditions), since
    /// a loop is left from its body only.
    pub header: Option<LoopHeaderSlot>,
    /// Whether a `break`/`continue` that would have bound to this loop was
    /// rejected (in its header, or behind a lambda or template inside it).
    /// The author meant an exit: a loop left with no edge then completes
    /// rather than reporting the code after it unreachable as well.
    pub has_rejected_jump: bool,
    /// `block_defer_writes.len()` at loop entry. The defers armed in the
    /// blocks opened inside the loop run on every `break`/`continue` edge.
    defer_floor: usize,
}

/// The edge a `break` or `continue` takes.
#[derive(Clone, Copy)]
pub(super) enum LoopEdge {
    Break,
    Continue,
}

impl LoopEdge {
    pub(super) fn keyword(self) -> &'static str {
        match self {
            LoopEdge::Break => "break",
            LoopEdge::Continue => "continue",
        }
    }
}

/// The verdicts about a body's loops and jumps that the reachability pass
/// reads from inference instead of re-deriving from syntax.
#[derive(Default)]
pub(super) struct LoopVerdicts {
    /// The `while` and `while let` statements with no exit edge: a header
    /// that never misses (a statically true condition, a header that always
    /// returns or `continue`s) and no live `break` that binds to the loop.
    /// The code after one is unreachable.
    pub diverging_loops: FxHashSet<StmtId>,
    /// The `break`/`continue` statements that were rejected (E0141, E0187,
    /// E0188). A rejected jump still ends the flow as written, but the code
    /// after it is not reported unreachable on top of the rejection.
    pub rejected_jumps: FxHashSet<StmtId>,
}

/// The state where an operand that may be skipped begins - the right
/// operand of `&&`/`||`, a `??` fallback, the links of a `?.` chain after
/// the first `?.` - for [`InferenceContext::rejoin_skipped`] to resume.
pub(super) struct SkipPoint {
    flow: Flow,
    diverges: Diverges,
}

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
    /// The right operand may be skipped ([`Self::rejoin_skipped`]), and a
    /// binding it assigns no longer has the type that the left operand
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
        let skip = self.skip_point();
        self.apply_facts(match op {
            ShortCircuit::And => &left.when_true,
            ShortCircuit::Or => &left.when_false,
        });
        let (rhs_ty, right) = self.check_truthy_operand(body, rhs, true, position);
        let reassigned = self.assigned_bindings(body, rhs);
        self.rejoin_skipped(skip, &reassigned);
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
    /// inside `matched`; bool or enum member splits into its values
    /// when an arm covers some; nothing dropped leaves the scrutinee unchanged
    /// (never a fabricated narrower type), everything dropped is `never`.
    pub(super) fn subtract_narrow(&mut self, scrut: &Ty, matched: &Ty) -> Ty {
        let members: Vec<Ty> = match scrut.kind() {
            InferTy::Union(members) => members.to_vec(),
            _ => vec![scrut.clone()],
        };
        let mut kept: Vec<Ty> = Vec::new();
        let mut changed = false;
        for member in &members {
            if self.provable_subtype(member, matched) {
                changed = true;
                continue;
            }
            match member.kind() {
                InferTy::Bool => {
                    let pieces: Vec<Ty> = vec![
                        Ty::intern(InferTy::Literal(
                            baml_base::Literal::Bool(true),
                            baml_type::Freshness::Regular,
                        )),
                        Ty::intern(InferTy::Literal(
                            baml_base::Literal::Bool(false),
                            baml_type::Freshness::Regular,
                        )),
                    ];
                    let mut pieces_unproven: Vec<Ty> = Vec::new();
                    for piece in &pieces {
                        if !self.provable_subtype(piece, matched) {
                            pieces_unproven.push(piece.clone());
                        }
                    }
                    if pieces_unproven.len() == pieces.len() {
                        kept.push(member.clone());
                    } else {
                        changed = true;
                        for piece in &pieces_unproven {
                            kept.push(piece.clone());
                        }
                    }
                }
                InferTy::Enum(qtn) => {
                    let pieces: Vec<Ty> =
                        self.facts
                            .enum_variants(qtn)
                            .map_or_else(Vec::new, |variants| {
                                variants
                                    .into_iter()
                                    .map(|variant| {
                                        Ty::intern(InferTy::EnumVariant(qtn.clone(), variant))
                                    })
                                    .collect()
                            });
                    if pieces.is_empty() {
                        kept.push(member.clone());
                    } else {
                        let mut pieces_unproven: Vec<Ty> = Vec::new();
                        for piece in &pieces {
                            if !self.provable_subtype(piece, matched) {
                                pieces_unproven.push(piece.clone());
                            }
                        }
                        if pieces_unproven.len() == pieces.len() {
                            kept.push(member.clone());
                        } else {
                            changed = true;
                            for piece in &pieces_unproven {
                                kept.push(piece.clone());
                            }
                        }
                    }
                }
                _ => {
                    kept.push(member.clone());
                }
            }
        }
        if !changed {
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

    /// The state at the point from which an operand may be skipped; the
    /// operand is typed from here and [`Self::rejoin_skipped`] resumes here.
    pub(super) fn skip_point(&self) -> SkipPoint {
        SkipPoint {
            flow: self.flow.clone(),
            diverges: self.diverges(),
        }
    }

    /// Ends an operand that may not have run (`&&`/`||` right operands,
    /// `??` fallbacks, the links of a `?.` chain after the first `?.`):
    /// its divergence is not the expression's, since after `a || { return; }`
    /// execution can proceed, and the overlay is the one from the skip
    /// point. A binding the operand assigns - `reassigned`, its assigned
    /// set - keeps a refinement only where the taken and the skipped path
    /// agree, joined; an operand that diverged is not a path that reaches
    /// here, and the facts a test in the operand established do not outlive
    /// it.
    pub(super) fn rejoin_skipped(&mut self, skip: SkipPoint, reassigned: &FxHashSet<BindingId>) {
        let taken_diverged = self.diverges() == Diverges::Always;
        self.set_diverges(skip.diverges);
        let taken_flow = std::mem::replace(&mut self.flow, skip.flow);
        if taken_diverged {
            return;
        }
        for binding in reassigned {
            match (taken_flow.get(binding), self.flow.get(binding)) {
                (Some(taken), Some(skipped)) if taken == skipped => {}
                (Some(taken), Some(skipped)) => {
                    let joined = self.join(&[taken.clone(), skipped.clone()]);
                    self.flow.insert(*binding, joined);
                }
                (_, None) | (None, Some(_)) => {
                    self.flow.remove(binding);
                }
            }
        }
    }

    /// The divergence-aware merge of the live paths into one point. A path
    /// that diverged is not among them and contributes nothing, which is
    /// why early-return narrowing is the ordinary merge rule and not a
    /// special case. A binding keeps an overlay only when EVERY path
    /// narrowed it, joined: narrowed on one path only, the other path keeps
    /// the wider type, so the join is the declared type - no overlay.
    pub(super) fn merge_flows(
        &mut self,
        first: Flow,
        rest: impl IntoIterator<Item = Flow>,
    ) -> Flow {
        let mut merged = first;
        for flow in rest {
            let mut next = Flow::default();
            for (binding, ty) in merged {
                let Some(other) = flow.get(&binding) else {
                    continue;
                };
                let joined = if ty == *other {
                    ty
                } else {
                    self.join(&[ty, other.clone()])
                };
                next.insert(binding, joined);
            }
            merged = next;
        }
        merged
    }

    /// [`Self::merge_flows`] over paths that may all have diverged: `None`
    /// when nothing reaches the point, which is then unreachable.
    pub(super) fn merge_reaching(&mut self, mut flows: Vec<Flow>) -> Option<Flow> {
        let first = flows.pop()?;
        Some(self.merge_flows(first, flows))
    }

    /// Opens the frame of a loop whose header is about to be typed.
    pub(super) fn enter_loop(&mut self) {
        let defer_floor = self.block_defer_writes.len();
        self.control_frame_mut().loops.push(LoopFrame {
            breaks: Vec::new(),
            continues: Vec::new(),
            header: None,
            has_rejected_jump: false,
            defer_floor,
        });
    }

    /// Records a rejected `break`/`continue` against the loop it would have
    /// bound to: the innermost loop open in any frame, if there is one.
    pub(super) fn mark_rejected_jump(&mut self) {
        if let Some(frame) = self
            .frames
            .iter_mut()
            .rev()
            .find_map(|frame| frame.loops.last_mut())
        {
            frame.has_rejected_jump = true;
        }
    }

    /// The innermost loop of the frame, between `enter_loop` and
    /// `leave_loop`.
    fn innermost_loop_mut(&mut self) -> &mut LoopFrame {
        self.control_frame_mut()
            .loops
            .last_mut()
            .unwrap_or_else(|| unreachable!("called between `enter_loop` and `leave_loop`"))
    }

    /// Marks which header slot of the innermost loop is being typed, or
    /// that its body is.
    pub(super) fn set_loop_header(&mut self, slot: Option<LoopHeaderSlot>) {
        self.innermost_loop_mut().header = slot;
    }

    /// The header slot of the innermost loop being typed, where a
    /// `break`/`continue` has no place.
    pub(super) fn loop_header(&self) -> Option<LoopHeaderSlot> {
        self.control_frame()
            .loops
            .last()
            .and_then(|frame| frame.header)
    }

    /// Closes the innermost loop's frame and hands back its edges.
    pub(super) fn leave_loop(&mut self) -> LoopFrame {
        self.control_frame_mut()
            .loops
            .pop()
            .unwrap_or_else(|| unreachable!("`leave_loop` pairs with `enter_loop`"))
    }

    /// Takes the innermost loop's `continue` edges, for its update slot.
    pub(super) fn take_continues(&mut self) -> Vec<Flow> {
        std::mem::take(&mut self.innermost_loop_mut().continues)
    }

    /// Records the flow at a `break`/`continue` as an edge of the innermost
    /// loop, which the caller has checked is open
    /// ([`Self::loop_control_escape`]). The defers armed inside the loop so
    /// far run before the jump, and their writes end the facts about the
    /// bindings they assign. A jump the flow cannot reach - after a
    /// `return`, in the body of a header that never completes - is no edge:
    /// recording it would keep the code after the loop alive.
    pub(super) fn record_loop_edge(&mut self, edge: LoopEdge) {
        if self.diverges() == Diverges::Always {
            return;
        }
        let defer_floor = self
            .control_frame()
            .loops
            .last()
            .unwrap_or_else(|| unreachable!("the caller checked that a loop is open"))
            .defer_floor;
        let mut flow = self.flow.clone();
        for writes in &self.block_defer_writes[defer_floor..] {
            for binding in writes {
                flow.remove(binding);
            }
        }
        let frame = self.innermost_loop_mut();
        match edge {
            LoopEdge::Break => frame.breaks.push(flow),
            LoopEdge::Continue => frame.continues.push(flow),
        }
    }

    /// Every narrowable binding the subtree assigns: the havoc set of a
    /// construct that may not run, may run again or runs at another time -
    /// a skipped operand, a match arm, a `catch`, a `defer`, a `for let`
    /// body.
    pub(super) fn assigned_bindings(&self, body: &ExprBody, root: ExprId) -> FxHashSet<BindingId> {
        self.assigned_bindings_in(body, [BodyNode::Expr(root)])
    }

    /// [`Self::assigned_bindings`] over several roots - a loop's havoc set,
    /// which spans its condition, body and update slot.
    pub(super) fn assigned_bindings_in(
        &self,
        body: &ExprBody,
        roots: impl IntoIterator<Item = BodyNode>,
    ) -> FxHashSet<BindingId> {
        let mut out = FxHashSet::default();
        // The arena is a DAG (templates share `ExprId`s between their
        // segments and desugared payload), so the walk dedupes.
        let mut seen: FxHashSet<BodyNode> = FxHashSet::default();
        let mut stack: Vec<BodyNode> = roots.into_iter().collect();
        while let Some(node) = stack.pop() {
            if !seen.insert(node) {
                continue;
            }
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
}
