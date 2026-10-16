//! Pattern typing and match exhaustiveness - the rust-analyzer
//! `infer/pat.rs` analog (S10a), with the per-shape lowering rules
//! transcribed from TIR's pattern machinery (the designated reference
//! implementation) and two fixes applied at the rewrite:
//!
//! - B-919: class patterns ADOPT generic args from the scrutinee when none
//!   are written (porting the adoption TIR only implemented for interface
//!   heads).
//! - B-633: claiming a union member requires PROVABLE overlap - a rigid
//!   `T` pattern no longer claims a `null` member outright (TIR's
//!   `atoms_overlap` blanket-true for type variables), so
//!   `match (xs.pop()) { let x: T => x }` is correctly non-exhaustive.
//!
//! The matrix side is the lifted rustc `rustc_pattern_analysis` port in
//! `crate::exhaustiveness`, running over plain `baml_type::Ty` (ground
//! resolved types; interned converts at the boundary). A non-exhaustive
//! match types as the Error sentinel - the E0062-class diagnostic lands
//! with S17.
//!
//! `PatternOutcome::covers_type` records whether the pattern matches by
//! type alone (no literal, field, or length constraints) - the gate the
//! S10b else-branch subtraction needs (B-1069).

use baml_compiler2_ast::{ExprBody, ExprId, MatchArmId, PatId, Pattern};
use baml_type::{
    Freshness,
    interned::{InferInterface, InferTy, Ty},
    normalize::{TypeContext as _, normalize_interned},
};

use super::{
    Expectation, InferenceContext,
    flow::{CondFacts, TestPosition},
};
use crate::exhaustiveness::{Ctor, DPat, PatCtx, compute_match_usefulness};

/// One lowered pattern: the matrix row piece, the refined scrutinee type
/// the arm body sees, and two type-only verdicts:
///
/// - `covers_type`: the pattern matches EVERY scrutinee value
///   (irrefutability - the S17 refutable-let check).
/// - `consumes_matched`: the pattern matches every value of its OWN
///   `matched_ty` (refutable by type alone - no literal, field, or length
///   constraint). The gate for else-side subtraction and match residual
///   accumulation (B-1069): a `Foo { x: 1 }` failing says nothing
///   type-shaped about the scrutinee.
pub(super) struct PatternOutcome {
    pub dpat: DPat,
    pub matched_ty: Ty,
    /// The WRITTEN form to record when it differs from `matched_ty` -
    /// ruling 3 (bindings record what the user wrote; rustc's
    /// `user_provided_types` discipline): a type ascription's recorded
    /// type is the declared spelling, while `matched_ty` stays the
    /// refined working form that drives narrowing and destructuring.
    pub recorded_ty: Option<Ty>,
    pub covers_type: bool,
    pub consumes_matched: bool,
}

impl<'db> InferenceContext<'db> {
    /// Match typing: lower every arm's pattern against the scrutinee,
    /// narrow the scrutinee binding per arm, type the bodies against the
    /// branch expectation, then run the usefulness matrix over the
    /// unguarded arms (guarded arms contribute nothing to exhaustiveness -
    /// `TYPE_SYSTEM.md`'s guard rule). Non-exhaustive -> Error sentinel.
    pub(super) fn infer_match(
        &mut self,
        body: &ExprBody,
        match_expr: ExprId,
        scrutinee: ExprId,
        arms: &[MatchArmId],
        expected: &Expectation,
    ) -> Ty {
        let written_scrutinee = self
            .type_refs
            .match_scrutinee_types
            .get(&match_expr)
            .copied();
        let scrut_ty = match written_scrutinee {
            Some(type_ref) => {
                let annotation = self.lower_body_annotation(type_ref);
                self.check_expr(body, scrutinee, &annotation);
                // A match annotation declares the matrix's full input type.
                // Keep it instead of narrowing back to the scrutinee's current
                // concrete value so later arms remain reachable.
                annotation
            }
            None => self.infer_expr(body, scrutinee, &Expectation::None),
        };
        let scrut_resolved = self.scrutinee_demand(&scrut_ty);
        // An arm that writes a type or a shape is typed against the
        // scrutinee's structure here - a typed arm narrows, a structural one
        // destructures, and each catch-all after them is the scrutinee minus
        // what they took - and judged for usefulness, so the type must be
        // known at that point: a structural demand, like a member access,
        // refused here whatever a variable later solves to (rustc resolves
        // the scrutinee's structure where a pattern asks for it). Arms that
        // only bind (`_`, `let rest`, a chain of them) ask for nothing: they
        // take the scrutinee as it is, open or not, and cover it. The report
        // owns the open classes (no "cannot infer" of their own, nor an arm
        // pattern's), and a class an earlier error already owns is that
        // error's cascade.
        let needs_structure = arms
            .iter()
            .any(|&arm| !binds_without_a_type(body, body.match_arms[arm].pattern));
        let refused_open = needs_structure
            && !scrut_resolved.has_error()
            && baml_type::interned::ClosedTy::try_from(&scrut_resolved).is_err();
        if refused_open {
            let roots = self.table.open_roots_in(&scrut_resolved);
            let owed = roots.iter().any(|&root| !self.table.is_tainted(root));
            for root in roots {
                self.table.taint_root(root);
            }
            if owed {
                let full_type = self.plain_finalized(&scrut_resolved);
                self.pending_diags
                    .push(super::PendingDiag::ScrutineeTypeMustBeKnown {
                        expr: scrutinee,
                        full_type,
                    });
            }
        }
        let scrut_binding = self.narrowable_binding(body, scrutinee);
        let branch_expectation = expected.adjust_for_branches(&mut self.table);

        // Arm execution can change outer locals. Do not restore an entry
        // fact after an arm (or its deferred cleanup) invalidates it.
        for &arm in arms {
            let arm = &body.match_arms[arm];
            for root in arm.guard.into_iter().chain(std::iter::once(arm.body)) {
                for binding in self.assigned_bindings(body, root) {
                    self.flow.remove(&binding);
                }
            }
        }

        let entry_diverges = self.diverges;
        let mut arm_tys = Vec::new();
        let mut matrix_arms: Vec<DPat> = Vec::new();
        let mut matrix_arm_bodies: Vec<ExprId> = Vec::new();
        let mut any_pattern_error = false;
        let mut all_diverge = super::Diverges::Always;
        // The residual accumulator (B-774): each unguarded arm that is
        // refutable by type alone subtracts what it matched, so a later
        // catch-all arm sees the complement - the catch-residual
        // mechanism generalized to match.
        let mut residual = scrut_resolved.clone();
        let pending_before = self.pending_diags.len();
        for &arm_id in arms {
            let arm = &body.match_arms[arm_id];
            // The arm's own catch-all bindings read the same way as the
            // scrutinee does in the arm: `null => .., let rest => ..` binds
            // `rest` to the non-null remainder. The matrix row stays a
            // wildcard over the whole column.
            let outcome =
                self.lower_pattern_reaching(body, arm.pattern, &scrut_resolved, &residual);
            any_pattern_error |= outcome.matched_ty.has_error();
            // A pattern irrefutable against the full scrutinee is really
            // matching the residual; typed arms take their own refinement.
            let narrow_ty = if outcome.covers_type {
                residual.clone()
            } else {
                outcome.matched_ty.clone()
            };

            let saved_flow = self.flow.clone();
            if let Some(binding) = scrut_binding {
                self.flow.insert(binding, narrow_ty);
            }
            self.diverges = super::Diverges::Maybe;
            if let Some(guard) = arm.guard {
                let (_, guard_facts) = self.check_condition(body, guard);
                self.apply_facts(&guard_facts.when_true);
            }
            let arm_ty = self.infer_branch(body, arm.body, &branch_expectation);
            all_diverge = all_diverge.and(self.diverges);
            self.flow = saved_flow;
            arm_tys.push(arm_ty);
            if arm.guard.is_none() {
                if outcome.consumes_matched {
                    residual = self.subtract_narrow(&residual, &outcome.matched_ty);
                }
                matrix_arms.push(outcome.dpat);
                matrix_arm_bodies.push(arm.body);
            }
        }
        self.diverges = entry_diverges.or(all_diverge);
        let parts = arms
            .iter()
            .zip(&arm_tys)
            .map(|(&arm, ty)| (ty.clone(), self.take_units(body.match_arms[arm].body)))
            .collect();
        let units = self.join_units(parts, expected);
        // An unknown-FIELD in some arm's class pattern makes that arm's
        // matrix row a lie (the bad field dropped out), so usefulness
        // verdicts are noise - same suppression as an errored pattern.
        any_pattern_error |= self.pending_diags[pending_before..]
            .iter()
            .any(|pending| matches!(pending, super::PendingDiag::UnknownPatternField { .. }));

        if scrut_resolved.has_error() {
            return Ty::error();
        }
        let Ok(scrut_closed) = baml_type::interned::ClosedTy::try_from(&scrut_resolved) else {
            if refused_open {
                return Ty::error();
            }
            debug_assert!(
                !needs_structure,
                "an open scrutinee an arm needs the structure of was refused before its arms"
            );
            // Catch-all arms over a scrutinee still being inferred: the
            // first unguarded one takes every value, so the match is
            // exhaustive and every unguarded arm after it is unreachable.
            // With every arm guarded, a value whose guards all fail reaches
            // no arm, and nothing names a narrower witness than `_`.
            let Some((_, later)) = matrix_arm_bodies.split_first() else {
                self.pending_diags
                    .push(super::PendingDiag::NonExhaustiveMatch {
                        expr: match_expr,
                        scrutinee: scrut_resolved.clone(),
                        missing: vec!["_".to_string()],
                    });
                self.result.non_exhaustive_matches.insert(match_expr);
                return Ty::error();
            };
            for &arm_body in later {
                self.pending_diags.push(super::PendingDiag::UnreachableArm {
                    expr: arm_body,
                    warning: false,
                });
            }
            if !units.is_empty() {
                self.unjudged_units.insert(match_expr, units);
            }
            return self.join(&arm_tys);
        };
        let col_ty = scrut_closed.to_plain();
        let ctx = HirPatCtx { infer: self };
        let report = compute_match_usefulness(&ctx, &matrix_arms, col_ty);
        // An errored arm pattern makes the reachability verdicts noise
        // (TIR's pattern_had_error suppression).
        for arm in &report.unreachable_arms {
            if any_pattern_error {
                break;
            }
            if let Some(&arm_body) = matrix_arm_bodies.get(arm.0) {
                self.pending_diags.push(super::PendingDiag::UnreachableArm {
                    expr: arm_body,
                    warning: false,
                });
            }
        }
        if !report.missing.is_empty() {
            // Non-exhaustive: the match can fall through, which the type
            // system rejects; E0062 carries the witnesses.
            let missing: Vec<String> = report
                .missing
                .iter()
                .map(|w| crate::exhaustiveness::render_witness_pat(self.db, &self.viewpoint(), w))
                .collect();
            self.pending_diags
                .push(super::PendingDiag::NonExhaustiveMatch {
                    expr: match_expr,
                    scrutinee: scrut_resolved.clone(),
                    missing,
                });
            self.result.non_exhaustive_matches.insert(match_expr);
            return Ty::error();
        }
        if !units.is_empty() {
            self.unjudged_units.insert(match_expr, units);
        }
        self.join(&arm_tys)
    }

    /// The scrutinee demand every pattern site shares (the
    /// `infer_match` rule applied uniformly): resolve, force occurring
    /// vars - exhaustiveness and narrowing cannot defer, so the
    /// one-pass walk commits here - then matrix-normalize.
    pub(super) fn scrutinee_demand(&mut self, ty: &Ty) -> Ty {
        let resolved = self.table.resolve_completely(ty);
        let resolved = if resolved.has_infer() {
            self.force_occurring_vars(&resolved)
        } else {
            resolved
        };
        self.matrix_scrut(&resolved)
    }

    /// `expr is pattern`: the pattern lowers against the operand (no
    /// subtype gate - a never-matching test is legal and just false); the
    /// result is always bool. A narrowable operand also yields the facts
    /// of the test: the matched type when it holds and, for a pattern that
    /// is refutable by type alone, the rest when it does not.
    pub(super) fn infer_is(
        &mut self,
        body: &ExprBody,
        scrutinee: ExprId,
        pattern: PatId,
        position: TestPosition,
    ) -> (Ty, CondFacts) {
        let scrut_ty = self.infer_expr(body, scrutinee, &Expectation::None);
        let binding = self.narrowable_binding(body, scrutinee);
        let scrut = match binding {
            Some(binding) => self.binding_flow_ty(binding),
            None => scrut_ty,
        };
        let scrut = self.scrutinee_demand(&scrut);
        // `is` is a runtime TYPE TEST: a pattern provably disjoint from
        // the scrutinee is a legal test that answers `false` (the corpus
        // pins `42 is string`), not the dead-pattern error a match arm
        // gets - probe silently. A branch condition on a narrowable local
        // is the exception: there the test is dead code, and it reports.
        let silent = binding.is_none() || position == TestPosition::Value;
        if silent {
            self.or_probe_depth += 1;
        }
        let outcome = self.lower_pattern(body, pattern, &scrut);
        if silent {
            self.or_probe_depth -= 1;
        }
        let mut facts = CondFacts::default();
        if let Some(binding) = binding {
            // Subtraction only when the pattern is refutable by type
            // alone (B-1069): a field- or length-constrained pattern
            // failing tells us nothing type-shaped about the scrutinee.
            if outcome.consumes_matched {
                let complement = self.subtract_narrow(&scrut, &outcome.matched_ty);
                facts.when_false.insert(binding, complement);
            }
            facts.when_true.insert(binding, outcome.matched_ty);
        }
        (Ty::bool(), facts)
    }

    /// Destructuring `let`: the initializer synthesizes (widened at the
    /// binding site), then the pattern lowers against it. Refutability
    /// enforcement (a refutable pattern in `let` is an error) is S17's
    /// diagnostic; the types are recorded either way.
    pub(super) fn infer_let_destructure(
        &mut self,
        body: &ExprBody,
        pattern: PatId,
        initializer: Option<ExprId>,
        has_else: bool,
    ) {
        let init_ty = match initializer {
            Some(init) => {
                // The pattern's informative type INFORMS the initializer
                // (TIR's `pattern_expected_ty`): a class destructure over a
                // generic call solves the callee's return var from the
                // pattern's class. Inform-only - the pattern walk below is
                // the check.
                let informed = self.pattern_informative_ty(body, pattern);
                let expectation = match informed.clone() {
                    Some(informed) => Expectation::has_type(informed),
                    // `let _ = …` binds nothing: nobody reads the value.
                    None if matches!(body.patterns[pattern], Pattern::Wildcard) => {
                        Expectation::Discarded
                    }
                    None => Expectation::None,
                };
                let ty = self.infer_expr(body, init, &expectation);
                if let Some(informed) = informed
                    && ty.has_infer()
                {
                    // Deposit the demand as a bound so a generic callee's
                    // return var solves from the pattern; inform-only (a
                    // failure is the pattern walk's to report).
                    let _ = self.sub(&ty, &informed);
                }
                self.widen_fresh(&ty)
            }
            None => Ty::error(),
        };
        let resolved = self.scrutinee_demand(&init_ty);
        let outcome = self.lower_pattern(body, pattern, &resolved);
        // A refutable pattern in an irrefutable position has nowhere to
        // go when it fails (E0111) - unless the `else` branch IS that
        // somewhere (`let Ok(v) = f() else { return }` is the refutable
        // let's legal form). Error-typed scrutinees are cascades.
        if !has_else
            && !outcome.covers_type
            && !outcome.matched_ty.has_error()
            && !resolved.has_error()
            && !resolved.has_infer()
        {
            self.pending_diags.push(super::PendingDiag::RefutableLet {
                pat: pattern,
                context: crate::diagnostics::IrrefutableContextKind::Let,
            });
        }
    }

    /// The informative type a pattern DEMANDS of its scrutinee, when one
    /// exists (TIR's `pattern_expected_ty`, minimally): a class pattern's
    /// head (written args honored, unwritten as fresh vars), through
    /// binds; an Or-pattern's UNIQUE informative branch.
    fn pattern_informative_ty(&mut self, body: &ExprBody, pat: PatId) -> Option<Ty> {
        match &body.patterns[pat] {
            Pattern::Bind {
                subpat: Some(sub), ..
            } => {
                let sub = *sub;
                self.pattern_informative_ty(body, sub)
            }
            Pattern::Class { class, .. } => {
                let class = class.clone();
                let segments: Vec<baml_type::Name> = class
                    .iter()
                    .map(|s| baml_type::Name::new(s.as_str()))
                    .collect();
                let head = self.lower.path_head(&segments);
                if self.scoped_type_param_heads(head, &segments) {
                    return None;
                }
                let def = self.lower.resolve_type_definition(head, &segments)?;
                let baml_compiler2_hir::contributions::Definition::Class(class_loc) = def else {
                    return None;
                };
                let short = segments.last()?;
                let qtn = self.lower.qualify_definition(def, short);
                let generic_count = baml_compiler2_hir::item_data::class_data(self.db, class_loc)
                    .generic_params
                    .len();
                let written = self.type_refs.pattern_class_args.get(&pat).cloned();
                let args: Vec<Ty> = match written {
                    Some(written) => written
                        .iter()
                        .map(|&type_ref| self.lower_body_annotation(type_ref))
                        .collect(),
                    None => (0..generic_count)
                        .map(|_| self.table.new_var_ty())
                        .collect(),
                };
                Some(Ty::intern(InferTy::Class(qtn, args.into())))
            }
            Pattern::Or(alts) => {
                let alts = alts.clone();
                let mut informative = None;
                for alt in alts {
                    if let Some(ty) = self.pattern_informative_ty(body, alt) {
                        if informative.is_some() {
                            return None;
                        }
                        informative = Some(ty);
                    }
                }
                informative
            }
            _ => None,
        }
    }

    /// The canonical scrutinee for pattern analysis (TIR's
    /// `matrix_normalize_scrut`): aliases expanded, unions canonical - so
    /// a `Role = "user" | "assistant"` alias scrutinee projects onto its
    /// members. Var/error scrutinees pass through (the oracle requires
    /// var-free input; those matches sentinel out anyway).
    pub(super) fn matrix_scrut(&self, ty: &Ty) -> Ty {
        if ty.has_error() {
            return ty.clone();
        }
        match baml_type::interned::ClosedTy::try_from(ty) {
            Ok(closed) => normalize_interned(&closed, &self.facts).into_ty(),
            // Open scrutinees pass through (the oracle takes closed input;
            // those matches sentinel out anyway).
            Err(baml_type::interned::OpenTy) => ty.clone(),
        }
    }

    /// The scrutinee's binding, when it is a bare local - the only
    /// narrowable reference shape in S10a (member chains are a documented
    /// later extension).
    pub(super) fn narrowable_binding(
        &self,
        body: &ExprBody,
        expr: ExprId,
    ) -> Option<baml_compiler2_hir::semantic_index::BindingId> {
        let baml_compiler2_ast::Expr::Path(segments) = &body.exprs[expr] else {
            return None;
        };
        if segments.len() != 1 {
            return None;
        }
        let key = self.metadata_key(expr)?;
        match self.index.path_resolution(key) {
            Some(baml_compiler2_hir::semantic_index::PathResolution::Local(binding)) => {
                // Captured bindings are never narrowed (the lambda could
                // observe a different value) - TIR's rule, kept.
                let captured = self
                    .index
                    .scope_bindings
                    .get(binding.scope.index() as usize)
                    .is_some_and(|bindings| bindings.captured_bindings.contains(&binding));
                (!captured).then_some(binding)
            }
            _ => None,
        }
    }

    // -- The per-shape lowering walk ------------------------------------------

    /// Lowers one pattern against a resolved scrutinee type, recording
    /// binding types along the way. The TIR per-shape table, transcribed.
    pub(super) fn lower_pattern(
        &mut self,
        body: &ExprBody,
        pat: PatId,
        scrut: &Ty,
    ) -> PatternOutcome {
        self.lower_pattern_reaching(body, pat, scrut, scrut)
    }

    /// [`Self::lower_pattern`] for a pattern that only part of the scrutinee
    /// can reach: `reaching` is the scrutinee minus the members that earlier
    /// `match` arms consumed. A pattern that writes no type of its own (`_`,
    /// a bare `let name`, a chain of bare bindings) takes whatever reaches
    /// it, so `reaching` is its type. Every other pattern is typed by what
    /// it writes, against `scrut`.
    pub(super) fn lower_pattern_reaching(
        &mut self,
        body: &ExprBody,
        pat: PatId,
        scrut: &Ty,
        reaching: &Ty,
    ) -> PatternOutcome {
        debug_assert!(
            reaching == scrut || {
                let members = scrut_members(scrut);
                scrut_members(reaching).into_iter().all(|member| {
                    matches!(member.kind(), InferTy::Never) || members.contains(&member)
                })
            },
            "`reaching` must be made of members of the scrutinee",
        );
        let outcome = self.lower_pattern_inner(body, pat, scrut, reaching);
        // Every pattern node records its type (TIR's pattern_types
        // single-write-point discipline); ascriptions record the WRITTEN
        // form (ruling 3) while narrowing keeps the refined one.
        let recorded = outcome
            .recorded_ty
            .clone()
            .unwrap_or_else(|| outcome.matched_ty.clone());
        self.result.type_of_pat.insert(pat, recorded);
        outcome
    }

    fn lower_pattern_inner(
        &mut self,
        body: &ExprBody,
        pat: PatId,
        scrut: &Ty,
        reaching: &Ty,
    ) -> PatternOutcome {
        let mut written_refs = Vec::new();
        written_refs.extend(self.type_refs.pattern_types.get(&pat).copied());
        written_refs.extend(self.type_refs.array_ascriptions.get(&pat).copied());
        written_refs.extend(
            self.type_refs
                .pattern_class_args
                .get(&pat)
                .into_iter()
                .flatten()
                .copied(),
        );
        written_refs.extend(
            self.type_refs
                .pattern_assoc_bindings
                .get(&pat)
                .into_iter()
                .flatten()
                .map(|(_, type_ref)| *type_ref),
        );
        match &body.patterns[pat] {
            // A pattern that writes no type matches what reaches it. Its
            // matrix row is still a wildcard over the whole column.
            Pattern::Wildcard => PatternOutcome {
                dpat: DPat::wildcard(dpat_ty(scrut)),
                matched_ty: reaching.clone(),
                recorded_ty: None,
                covers_type: true,
                consumes_matched: true,
            },
            Pattern::Bind { subpat, .. } => {
                let inner = match subpat {
                    Some(sub) => self.lower_pattern_reaching(body, *sub, scrut, reaching),
                    None => PatternOutcome {
                        dpat: DPat::wildcard(dpat_ty(scrut)),
                        matched_ty: reaching.clone(),
                        recorded_ty: None,
                        covers_type: true,
                        consumes_matched: true,
                    },
                };
                // Chain semantics: every bind in a chain takes the
                // rightmost type - the WRITTEN form when an ascription
                // supplied one (ruling 3), else the refined one.
                let recorded = inner
                    .recorded_ty
                    .clone()
                    .unwrap_or_else(|| inner.matched_ty.clone());
                self.result.type_of_pat.insert(pat, recorded);
                inner
            }
            Pattern::Type(_) => {
                let pat_ty = self
                    .type_refs
                    .pattern_types
                    .get(&pat)
                    .copied()
                    .map(|type_ref| self.lower_body_annotation(type_ref))
                    .unwrap_or_else(Ty::error);
                self.type_pattern_outcome(pat, scrut, &pat_ty)
            }
            Pattern::Class { class, fields, .. } => {
                let class = class.clone();
                let fields: Vec<(baml_type::Name, PatId)> = fields
                    .iter()
                    .map(|field| (field.field.clone(), field.pat))
                    .collect();
                self.lower_class_pattern(body, pat, &class, &fields, scrut)
            }
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ..
            } => {
                let prefix = prefix.clone();
                let rest = rest.clone();
                let suffix = suffix.clone();
                self.lower_array_pattern(body, pat, &prefix, rest.as_ref(), &suffix, scrut)
            }
            Pattern::Or(alts) => {
                let alts = alts.clone();
                self.or_probe_depth += 1;
                let outcomes: Vec<PatternOutcome> = alts
                    .iter()
                    // Every alternative is tried on the value that reached
                    // the whole pattern.
                    .map(|&alt| self.lower_pattern_reaching(body, alt, scrut, reaching))
                    .collect();
                self.or_probe_depth -= 1;
                // The dead-pattern rule at the CHAIN level: one alt that
                // can't match is just never taken, but a chain where no
                // alternative overlaps the scrutinee is dead outright.
                let whole: Vec<Ty> = outcomes
                    .iter()
                    .map(|outcome| {
                        outcome
                            .recorded_ty
                            .clone()
                            .unwrap_or_else(|| outcome.matched_ty.clone())
                    })
                    .collect();
                let whole = self.union_of(&whole);
                self.check_pattern_type_overlap(pat, scrut, &whole);
                // Alternatives bind the SAME names (HIR enforces); their
                // NARROW types must agree - a name bound `int` in one alt
                // and `string` in another has no one type at the join.
                {
                    let mut first: rustc_hash::FxHashMap<baml_type::Name, Ty> =
                        rustc_hash::FxHashMap::default();
                    for &alt in &alts {
                        let mut binds = Vec::new();
                        collect_bind_types(self, body, alt, &mut binds);
                        for (name, ty) in binds {
                            if ty.has_error() || ty.has_infer() {
                                continue;
                            }
                            match first.get(&name) {
                                None => {
                                    first.insert(name, ty);
                                }
                                Some(existing) if *existing == ty => {}
                                Some(existing) => {
                                    self.pending_diags.push(
                                        super::PendingDiag::OrBindingConflict {
                                            pat,
                                            name: name.clone(),
                                            first: existing.clone(),
                                            other: ty.clone(),
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
                let matched = self.join(
                    &outcomes
                        .iter()
                        .map(|outcome| outcome.matched_ty.clone())
                        .collect::<Vec<_>>(),
                );
                PatternOutcome {
                    covers_type: outcomes.iter().any(|outcome| outcome.covers_type),
                    consumes_matched: outcomes.iter().all(|outcome| outcome.consumes_matched),
                    dpat: DPat::or(
                        outcomes.into_iter().map(|outcome| outcome.dpat).collect(),
                        dpat_ty(scrut),
                    ),
                    matched_ty: matched,
                    recorded_ty: None,
                }
            }
        }
    }

    /// A destructure pattern spelled with an INTERFACE name
    /// (`MdidAnimal { name }` over an existential scrutinee): the
    /// interface's declared FIELDS destructure like a class's, each
    /// typed through the same member instantiation field access uses
    /// (`member_on_interface`), the head being the existential view at
    /// the written args - else the args a same-interface scrutinee
    /// member carries.
    fn lower_interface_pattern(
        &mut self,
        body: &ExprBody,
        pat: PatId,
        interface: baml_compiler2_hir::loc::InterfaceLoc<'db>,
        path: &[baml_type::Name],
        field_pats: &[(baml_type::Name, PatId)],
        scrut: &Ty,
    ) -> PatternOutcome {
        let data = baml_compiler2_hir::item_data::interface_data(self.db, interface);
        let short = path.last().expect("type paths are never empty");
        let qtn = self.lower.qualify_definition(
            baml_compiler2_hir::contributions::Definition::Interface(interface),
            short,
        );

        let written_args: Vec<Ty> = self
            .type_refs
            .pattern_class_args
            .get(&pat)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|&type_ref| self.lower_body_annotation(type_ref))
            .collect();
        let written_pins: Vec<(baml_type::Name, Ty)> = self
            .type_refs
            .pattern_assoc_bindings
            .get(&pat)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|(name, type_ref)| (name.clone(), self.lower_body_annotation(*type_ref)))
            .collect();
        // Written positions win; anything unwritten ADOPTS from an
        // arg-compatible same-interface scrutinee member (the class road's
        // adoption rule) - so `Source { value }` destructures whichever
        // realization the scrutinee carries, while a written pin
        // (`Source<Item = string> { value }`) constrains the head and
        // types its fields.
        let existential = scrut_members(scrut)
            .into_iter()
            .find_map(|member| match member.kind() {
                InferTy::Interface(member_qtn, args, pins)
                    if *member_qtn == qtn
                        && (written_args.is_empty()
                            || (written_args.len() == args.len()
                                // The fail-closed pair: a written arg still
                                // carrying a variable (an annotation hole)
                                // adopts nothing rather than materializing
                                // mid-inference.
                                && written_args
                                    .iter()
                                    .zip(args.iter())
                                    .all(|(a, b)| self.cached_equivalent(a, b)))) =>
                {
                    Some(super::obligations::InterfaceInstantiation {
                        args: args.to_vec(),
                        pins: pins.to_vec(),
                    })
                }
                _ => None,
            });
        // Else the instantiation the scrutinee's implementors provide: the
        // one at which every member that implements the interface does so (a
        // class's impl, a type parameter's bound, another interface that
        // requires it). Members that implement it at different arguments
        // leave them undetermined.
        let adopted = existential.or_else(|| {
            if !written_args.is_empty() || scrut.has_error() {
                return None;
            }
            let associated: Vec<baml_type::Name> = data
                .associated_types
                .iter()
                .map(|assoc| assoc.name.clone())
                .collect();
            let mut provided: Option<super::obligations::InterfaceInstantiation> = None;
            for member in scrut_members(scrut) {
                let Some(instantiation) = self.implemented_instantiation(
                    member,
                    &qtn,
                    data.generic_params.len(),
                    &associated,
                ) else {
                    continue;
                };
                match &provided {
                    Some(seen) if *seen != instantiation => return None,
                    Some(_) => {}
                    None => provided = Some(instantiation),
                }
            }
            provided
        });
        let super::obligations::InterfaceInstantiation { args, pins } = {
            // Nothing written and nothing to adopt: the interface's generics
            // are undetermined, and the pattern says so (E0002 at the
            // pattern) unless the scrutinee's own report already covers it:
            // its error, or the open classes a `match` refused.
            if adopted.is_none()
                && written_args.is_empty()
                && !scrut.has_error()
                && !self.open_classes_owned(scrut)
            {
                for param in &data.generic_params {
                    self.pending_diags
                        .push(super::PendingDiag::UninferredPatternTypeParam {
                            pat,
                            name: param.name.clone(),
                        });
                }
            }
            let adopted = adopted.unwrap_or_else(|| super::obligations::InterfaceInstantiation {
                args: (0..data.generic_params.len())
                    .map(|_| Ty::error())
                    .collect(),
                pins: Vec::new(),
            });
            super::obligations::InterfaceInstantiation {
                args: if written_args.is_empty() {
                    adopted.args
                } else {
                    written_args
                },
                pins: if written_pins.is_empty() {
                    adopted.pins
                } else {
                    written_pins
                },
            }
        };
        // The fields are read through this head, so every declared
        // associated type is pinned on it: one the written or adopted
        // instantiation leaves out is undetermined - reported, unless the
        // scrutinee's own report covers it - and takes the sentinel, so a
        // field typed by it (`value: Self.Item`) is the sentinel rather than
        // a projection nothing can reduce.
        let mut pins = pins;
        for assoc in &data.associated_types {
            if pins.iter().any(|(name, _)| *name == assoc.name) {
                continue;
            }
            if !scrut.has_error() && !self.open_classes_owned(scrut) {
                self.pending_diags
                    .push(super::PendingDiag::UninferredPatternAssociatedType {
                        pat,
                        name: assoc.name.clone(),
                    });
            }
            pins.push((assoc.name.clone(), Ty::error()));
        }
        let head = Ty::intern(InferTy::Interface(
            qtn.clone(),
            args.clone().into_boxed_slice(),
            pins.clone().into_boxed_slice(),
        ));
        let target = InferInterface::new(
            qtn.clone(),
            args.into_boxed_slice(),
            pins.into_boxed_slice(),
        );

        let declared: Vec<baml_type::Name> =
            data.fields.iter().map(|field| field.name.clone()).collect();
        let mut field_covers = true;
        let mut sub_dpats: Vec<Option<DPat>> = vec![None; declared.len()];
        for (name, field_pat) in field_pats {
            let field_ty = crate::method_resolution::member_on_interface(
                self.db,
                &self.facts,
                &target,
                &head,
                name,
                true,
            )
            .filter(|member| !member.is_method)
            .map(|member| self.reduced_for_pattern(&member.ty));
            match field_ty {
                Some(field_ty) => {
                    let index = declared.iter().position(|field| field == name);
                    let outcome = self.lower_pattern(body, *field_pat, &field_ty);
                    field_covers &= outcome.covers_type;
                    if let Some(index) = index {
                        sub_dpats[index] = Some(outcome.dpat);
                    }
                }
                None => {
                    // Unknown field: S17's diagnostic; sub-bindings still
                    // record (as Error).
                    self.lower_pattern(body, *field_pat, &Ty::error());
                    field_covers = false;
                }
            }
        }
        let fields: Vec<DPat> = declared
            .iter()
            .zip(sub_dpats)
            .map(|(name, sub)| {
                sub.unwrap_or_else(|| {
                    let field_ty = crate::method_resolution::member_on_interface(
                        self.db,
                        &self.facts,
                        &target,
                        &head,
                        name,
                        true,
                    )
                    .map(|member| self.reduced_for_pattern(&member.ty))
                    .unwrap_or_else(Ty::error);
                    DPat::wildcard(dpat_ty(&field_ty))
                })
            })
            .collect();
        let head_covers = self.provable_subtype(scrut, &head);

        // The matrix's single-ctor STRUCT VIEW of an existential
        // (`Ctor::Interface`, rustc's non-enum struct treatment): field
        // sub-patterns decompose, so refutable field arms COMPOSE to
        // coverage (`{ active: true }` + `{ active: false }`).
        let dpat = {
            let iface_dpat = DPat::interface(dpat_ty(&head), fields, dpat_ty(scrut));
            match scrut.kind() {
                InferTy::Union(members) => {
                    let claimed: Vec<&Ty> = members
                        .iter()
                        .filter(|member| {
                            matches!(member.kind(), InferTy::Interface(member_qtn, _, _) if *member_qtn == qtn)
                        })
                        .collect();
                    match claimed.as_slice() {
                        [member] => DPat::union_member(dpat_ty(member), iface_dpat, dpat_ty(scrut)),
                        _ => iface_dpat,
                    }
                }
                _ => iface_dpat,
            }
        };
        PatternOutcome {
            dpat,
            // A head the scrutinee provably inhabits narrows nothing; the
            // pattern still records the interface it names, which its test
            // and field views dispatch through whatever the scrutinee is (a
            // class whose field links rename, a type parameter).
            matched_ty: if head_covers {
                scrut.clone()
            } else {
                head.clone()
            },
            recorded_ty: Some(head),
            covers_type: head_covers && field_covers,
            consumes_matched: field_covers,
        }
    }

    /// A type pattern (`let x: T`, literals, enum variants, `null`)
    /// against the scrutinee. Union scrutinees project onto claimed
    /// members; claiming requires PROVABLE overlap in either direction
    /// (the B-633 rule - undecidable rigid-vs-concrete pairs stay
    /// unclaimed and therefore uncovered).
    fn type_pattern_outcome(&mut self, pat: PatId, scrut: &Ty, pat_ty: &Ty) -> PatternOutcome {
        let mut outcome = self.type_pattern_outcome_inner(scrut, pat_ty);
        // The recorded form is the WRITTEN annotation whenever it is not
        // error-carrying (error ascriptions already record the written
        // shape through matched_ty).
        if !pat_ty.has_error() {
            outcome.recorded_ty = Some(pat_ty.clone());
        }
        if !outcome.covers_type {
            self.check_pattern_type_overlap(pat, scrut, pat_ty);
        }
        outcome
    }

    /// The dead-pattern rule: a pattern TYPE provably sharing no value
    /// with its scrutinee reports like any concrete mismatch. Ground
    /// pairs the oracle decides by unification; rigid/projection pairs
    /// ask the reachability oracle, whose `No` is trusted only when it
    /// can see every variable (the `all_typevars_within` obligation).
    fn check_pattern_type_overlap(&mut self, pat: PatId, scrut: &Ty, pat_ty: &Ty) {
        if self.or_probe_depth > 0 || self.rest_reject_depth > 0 {
            return;
        }
        if self.provably_disjoint(scrut, pat_ty) {
            self.pending_diags
                .push(super::PendingDiag::PatternScrutMismatch {
                    pat,
                    expected: scrut.clone(),
                    found: pat_ty.clone(),
                });
        }
    }

    /// Whether a pattern of type `pat_ty` provably shares no value with a
    /// scrutinee of type `scrut`: the one judgement the dead-arm report and
    /// a union claim's candidate test read. An erroneous or still-open pair
    /// is undecidable here, so never provably disjoint.
    fn provably_disjoint(&self, scrut: &Ty, pat_ty: &Ty) -> bool {
        if pat_ty.has_error() || scrut.has_error() {
            return false;
        }
        let (Ok(pat_closed), Ok(scrut_closed)) = (
            baml_type::interned::ClosedTy::try_from(pat_ty),
            baml_type::interned::ClosedTy::try_from(scrut),
        ) else {
            return false;
        };
        let pat_plain = pat_closed.to_plain();
        let scrut_plain = scrut_closed.to_plain();
        // The overlap oracle's `No` rejects on every shape (invariant
        // container elements, disjoint concretes), trusted only when it
        // can see every variable.
        let vars = self.lower.generic_params();
        let oracle_no = baml_type::unify::all_typevars_within(&pat_plain, vars)
            && baml_type::unify::all_typevars_within(&scrut_plain, vars)
            && self.pattern_overlap_verdict(&pat_plain, &scrut_plain)
                == baml_type::unify::Overlap::No;
        // Ground pairs ADDITIONALLY take TIR's bidirectional-subtyping
        // strictness, against the whole scrutinee or any union member:
        // the oracle alone answers Yes for any existential pair, but two
        // same-interface existentials with differing pins share no value
        // (pins are invariant).
        let ground = !pat_ty.has_typevar()
            && !scrut.has_typevar()
            && !pat_ty.has_projection()
            && !scrut.has_projection();
        oracle_no
            || (ground
                && !self.pattern_matchable(&pat_plain, &scrut_plain)
                && !self.pattern_overlaps_scrut_member(&pat_plain, &scrut_plain))
    }

    /// Whether an arm with natural type `pat` is *plausible* against a
    /// scrutinee of type `scrut` - TIR's arm-validity over-approximation.
    /// Container element positions recurse laxly (a structural pattern's
    /// element type embeds its sub-patterns); a bare interface head places
    /// no pin constraint (it destructures any realization); non-container
    /// pairs use bidirectional subtyping (either direction is a possible
    /// match - `Dog` matches an `Animal` scrutinee, and vice-versa).
    fn pattern_matchable(&self, pat: &baml_type::Ty, scrut: &baml_type::Ty) -> bool {
        use baml_type::Ty as P;
        let pat = self.expand_alias_chain(pat);
        let scrut = self.expand_alias_chain(scrut);
        match (&pat, &scrut) {
            (P::Never, _) => true,
            (P::List(a), P::List(b)) => self.pattern_matchable(a, b),
            (
                P::Map {
                    key: ka, value: va, ..
                },
                P::Map {
                    key: kb, value: vb, ..
                },
            ) => self.pattern_matchable(ka, kb) && self.pattern_matchable(va, vb),
            (
                P::Interface(pat_qtn, pat_args, pat_assoc),
                P::Interface(scrut_qtn, scrut_args, _),
            ) if pat_qtn == scrut_qtn
                && pat_assoc.is_empty()
                && (pat_args.is_empty()
                    || (pat_args.len() == scrut_args.len()
                        && pat_args.iter().zip(scrut_args.iter()).all(|(a, b)| {
                            baml_type::normalize::equivalent(a, b, &self.facts)
                        }))) =>
            {
                true
            }
            _ => {
                baml_type::normalize::is_subtype(&pat, &scrut, &self.facts)
                    || baml_type::normalize::is_subtype(&scrut, &pat, &self.facts)
            }
        }
    }

    /// A match arm is valid if its pattern overlaps ANY member of a
    /// union/optional scrutinee - the arm matches that member's values
    /// even when other members don't.
    fn pattern_overlaps_scrut_member(&self, pat: &baml_type::Ty, scrut: &baml_type::Ty) -> bool {
        let members = self.flatten_union_members(scrut);
        members.len() > 1
            && members
                .iter()
                .any(|member| self.pattern_matchable(pat, member))
    }

    fn flatten_union_members(&self, ty: &baml_type::Ty) -> Vec<baml_type::Ty> {
        match self.expand_alias_chain(ty) {
            baml_type::Ty::Union(members) => members
                .iter()
                .flat_map(|member| self.flatten_union_members(member))
                .collect(),
            other => vec![other],
        }
    }

    /// Follow a top-level alias chain to its target (bounded; a cyclic or
    /// unknown alias stays as written).
    fn expand_alias_chain(&self, ty: &baml_type::Ty) -> baml_type::Ty {
        use baml_type::normalize::TypeContext as _;
        let mut ty = ty.clone();
        for _ in 0..64 {
            let baml_type::Ty::TypeAlias(qtn) = &ty else {
                return ty;
            };
            match self.facts.alias_def(qtn) {
                Some(next) => ty = next,
                None => return ty,
            }
        }
        ty
    }

    fn type_pattern_outcome_inner(&mut self, scrut: &Ty, pat_ty: &Ty) -> PatternOutcome {
        if pat_ty.has_error() {
            // Fail-safe for coverage, but the error stays LOCAL: the
            // written type is the matched type (`map<!error, int>`
            // keeps its shape - the replace-with-error discipline,
            // never poison-to-top).
            return PatternOutcome {
                dpat: DPat::wildcard(dpat_ty(scrut)),
                matched_ty: pat_ty.clone(),
                recorded_ty: None,
                covers_type: false,
                consumes_matched: false,
            };
        }
        let covers = self.provable_subtype(scrut, pat_ty);
        if let InferTy::Union(members) = scrut.kind()
            && !covers
        {
            let members: Vec<Ty> = members.to_vec();
            let claimed: Vec<&Ty> = members
                .iter()
                .filter(|member| {
                    self.provable_subtype(member, pat_ty) || self.provable_subtype(pat_ty, member)
                })
                .collect();
            if !claimed.is_empty() {
                let scrut_plain = dpat_ty(scrut);
                let alts: Vec<DPat> = claimed
                    .iter()
                    .map(|member| {
                        let inner = if self.provable_subtype(member, pat_ty) {
                            DPat::wildcard(dpat_ty(member))
                        } else {
                            self.dpat_for_type(pat_ty, member)
                        };
                        DPat::union_member(dpat_ty(member), inner, scrut_plain.clone())
                    })
                    .collect();
                let dpat = if alts.len() == 1 {
                    alts.into_iter().next().expect("checked len")
                } else {
                    DPat::or(alts, scrut_plain)
                };
                let matched =
                    self.union_of(&claimed.iter().map(|&ty| ty.clone()).collect::<Vec<_>>());
                return PatternOutcome {
                    dpat,
                    matched_ty: self.narrow_to(&matched, pat_ty),
                    recorded_ty: None,
                    covers_type: false,
                    consumes_matched: true,
                };
            }
            // Nothing provably claimed: possible-but-not-covering.
            return PatternOutcome {
                dpat: DPat::single(dpat_ty(pat_ty), dpat_ty(scrut)),
                matched_ty: pat_ty.clone(),
                recorded_ty: None,
                covers_type: false,
                consumes_matched: true,
            };
        }
        PatternOutcome {
            dpat: self.dpat_for_type(pat_ty, scrut),
            matched_ty: if covers {
                scrut.clone()
            } else {
                self.narrow_to(scrut, pat_ty)
            },
            recorded_ty: None,
            covers_type: covers,
            consumes_matched: true,
        }
    }

    /// The refined scrutinee type after a successful type test - the meet,
    /// approximated: a scrutinee already inside the pattern stays; a
    /// pattern inside the scrutinee narrows to the pattern; otherwise the
    /// pattern type stands (rigid pairs keep the written type - TIR's
    /// `intersect_pattern_flow_types` policy).
    fn narrow_to(&self, scrut: &Ty, pat_ty: &Ty) -> Ty {
        if self.provable_subtype(scrut, pat_ty) {
            scrut.clone()
        } else {
            pat_ty.clone()
        }
    }

    /// The coverage-side lowering of a type pattern (TIR's `dpat_for_type`
    /// five regimes): singletons, finite alphabets, classes, rigid vars,
    /// and the subtype fallback. Deliberately stricter than arm validity.
    fn dpat_for_type(&self, pat_ty: &Ty, col: &Ty) -> DPat {
        let col_plain = dpat_ty(col);
        let pat_plain = dpat_ty(pat_ty);
        // The universal coverage rule first: a pattern the whole column
        // provably fits is a wildcard at this column, whatever its shape
        // (a same-union pattern must not decompose into per-member
        // singles that no longer align with UnionMember ctors).
        if self.provable_subtype(col, pat_ty) {
            return DPat::wildcard(col_plain);
        }
        match pat_ty.kind() {
            InferTy::Literal(..) | InferTy::EnumVariant(..) | InferTy::Null => {
                DPat::single(pat_plain, col_plain)
            }
            InferTy::Bool => DPat::or(
                [true, false]
                    .into_iter()
                    .map(|value| {
                        DPat::single(
                            baml_type::Ty::Literal(
                                baml_base::Literal::Bool(value),
                                Freshness::Regular,
                            ),
                            col_plain.clone(),
                        )
                    })
                    .collect(),
                col_plain,
            ),
            InferTy::Enum(qtn) => {
                let variants = self.facts.enum_variants(qtn).unwrap_or_default();
                DPat::or(
                    variants
                        .into_iter()
                        .map(|variant| {
                            DPat::single(
                                baml_type::Ty::EnumVariant(qtn.clone(), variant),
                                col_plain.clone(),
                            )
                        })
                        .collect(),
                    col_plain,
                )
            }
            InferTy::Union(members) => DPat::or(
                members
                    .iter()
                    .map(|member| self.dpat_for_type(member, col))
                    .collect(),
                col_plain,
            ),
            InferTy::Class(qtn, args) => {
                let fields = self.class_pattern_field_types(qtn, args);
                DPat::class_inst(
                    qtn.clone(),
                    args.iter().map(dpat_ty).collect(),
                    fields
                        .iter()
                        .map(|field_ty| DPat::wildcard(dpat_ty(field_ty)))
                        .collect(),
                    pat_plain,
                )
            }
            InferTy::TypeVar(..) => {
                // Rigid: covers only a column of the SAME variable;
                // otherwise possible-but-not-covering (never a blanket
                // claim - the B-633 rule).
                if pat_ty == col {
                    DPat::wildcard(col_plain)
                } else {
                    DPat::single(pat_plain, col_plain)
                }
            }
            _ => {
                if self.provable_subtype(col, pat_ty) {
                    DPat::wildcard(col_plain)
                } else {
                    DPat::single(pat_plain, col_plain)
                }
            }
        }
    }

    /// Class destructure. Generic args: written turbofish wins; otherwise
    /// ADOPTED from the scrutinee when exactly one same-class member
    /// determines them (B-919 - the adoption TIR implemented only for
    /// interface heads); a generic class with neither is Error (S17's
    /// must-specify diagnostic).
    fn lower_class_pattern(
        &mut self,
        body: &ExprBody,
        pat: PatId,
        class_path: &[baml_type::Name],
        field_pats: &[(baml_type::Name, PatId)],
        scrut: &Ty,
    ) -> PatternOutcome {
        // Under an ERRORED scrutinee the whole destructure is tainted:
        // fields lower against the sentinel (their bindings type error,
        // so uses do not cascade) and nothing reports.
        if scrut.has_error() {
            for &(_, field_pat) in field_pats {
                self.lower_pattern(body, field_pat, &Ty::error());
            }
            return PatternOutcome {
                dpat: DPat::wildcard(dpat_ty(scrut)),
                matched_ty: Ty::error(),
                recorded_ty: None,
                covers_type: false,
                consumes_matched: false,
            };
        }
        // A type parameter or scoped `type` binding at the head shadows
        // every item (as for a type pattern): no class is named.
        let head = self.lower.path_head(class_path);
        let definition = (!self.scoped_type_param_heads(head, class_path))
            .then(|| self.lower.resolve_type_definition(head, class_path))
            .flatten();
        // Destructure spelled with an INTERFACE name: the existential's
        // declared FIELDS destructure like a class's.
        if let Some(baml_compiler2_hir::contributions::Definition::Interface(interface)) =
            definition
        {
            return self
                .lower_interface_pattern(body, pat, interface, class_path, field_pats, scrut);
        }
        let Some(baml_compiler2_hir::contributions::Definition::Class(class)) = definition else {
            // The destructured name resolves nowhere (or to a non-class):
            // E0002 at the pattern, as for a type pattern; fields lower
            // against the sentinel.
            if definition.is_none() {
                self.pending_diags
                    .push(super::PendingDiag::UnresolvedPatternName {
                        pat,
                        name: baml_type::Name::new(
                            class_path
                                .iter()
                                .map(baml_type::Name::as_str)
                                .collect::<Vec<_>>()
                                .join("."),
                        ),
                        suggestions: self.lower.type_suggestions(class_path),
                    });
            }
            for &(_, field_pat) in field_pats {
                self.lower_pattern(body, field_pat, &Ty::error());
            }
            return PatternOutcome {
                dpat: DPat::wildcard(dpat_ty(scrut)),
                matched_ty: Ty::error(),
                recorded_ty: None,
                covers_type: false,
                consumes_matched: false,
            };
        };
        let qtn = crate::lower::class_qualified_name(self.db, class);
        let generic_count = crate::lower::class_generic_frame(self.db, class).len();
        // A generic class destructure must WRITE its type arguments (the
        // ratified rule; inference from the scrutinee is not offered).
        if generic_count > 0 && !self.type_refs.pattern_class_args.contains_key(&pat) {
            self.pending_diags
                .push(super::PendingDiag::GenericDestructureNoArgs {
                    pat,
                    class_name: qtn.name().clone(),
                });
        }

        let written: Vec<Ty> = self
            .type_refs
            .pattern_class_args
            .get(&pat)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|&type_ref| self.lower_body_annotation(type_ref))
            .collect();
        let args: Vec<Ty> = if !written.is_empty() || generic_count == 0 {
            written
        } else {
            // Adoption: same-class scrutinee members that pin the args.
            let candidates: Vec<&Ty> = scrut_members(scrut)
                .into_iter()
                .filter(|member| {
                    matches!(member.kind(), InferTy::Class(member_qtn, _) if *member_qtn == qtn)
                })
                .collect();
            match candidates.as_slice() {
                [only] => match only.kind() {
                    InferTy::Class(_, args) => args.to_vec(),
                    _ => Vec::new(),
                },
                // None or ambiguous: Error args (S17's diagnostic).
                _ => (0..generic_count).map(|_| Ty::error()).collect(),
            }
        };

        let head = crate::lower::class_ty(self.lang(), qtn.clone(), args.clone());
        let declared = if self.is_opaque_trace_type(&qtn) {
            Vec::new()
        } else {
            crate::lower::class_field_types(self.db, class)
        };
        let mut field_covers = true;
        let mut sub_dpats: Vec<Option<DPat>> = vec![None; declared.len()];
        for (name, field_pat) in field_pats {
            let index = declared.iter().position(|(field, _)| field == name);
            match index {
                Some(index) => {
                    let field_ty = self
                        .pattern_field_ty(&crate::impls::interned_ty(&declared[index].1), &args);
                    let outcome = self.lower_pattern(body, *field_pat, &field_ty);
                    field_covers &= outcome.covers_type;
                    sub_dpats[index] = Some(outcome.dpat);
                }
                None => {
                    // Unknown field (E0007); sub-bindings still record
                    // (as Error).
                    self.pending_diags
                        .push(super::PendingDiag::UnknownPatternField {
                            pat,
                            class_name: qtn.clone(),
                            field_name: name.clone(),
                            declared: declared.iter().map(|(field, _)| field.clone()).collect(),
                        });
                    self.lower_pattern(body, *field_pat, &Ty::error());
                }
            }
        }
        let fields: Vec<DPat> = declared
            .iter()
            .zip(sub_dpats)
            .map(|((_, field_ty), sub)| {
                sub.unwrap_or_else(|| {
                    DPat::wildcard(dpat_ty(
                        &self.pattern_field_ty(&crate::impls::interned_ty(field_ty), &args),
                    ))
                })
            })
            .collect();
        let head_covers = self.provable_subtype(scrut, &head);

        // Union scrutinee: claim the same-class member.
        let dpat = {
            let class_dpat = DPat::class_inst(
                qtn.clone(),
                args.iter().map(dpat_ty).collect(),
                fields,
                dpat_ty(&head),
            );
            match scrut.kind() {
                InferTy::Union(members) => {
                    // Same class AND agreeing instantiation: against
                    // `Box<int> | Box<string>`, `Box<int> { .. }` claims
                    // exactly the `Box<int>` member, so
                    // per-instantiation arms compose to full coverage -
                    // the member attribution bare type patterns already
                    // get. Without written/adopted args the name alone
                    // decides (a multi-instantiation scrutinee then
                    // stays unclaimed, as before).
                    let claimed: Vec<&Ty> = members
                        .iter()
                        .filter(|member| match member.kind() {
                            InferTy::Class(member_qtn, member_args) => {
                                *member_qtn == qtn
                                    && (args.is_empty()
                                        || (member_args.len() == args.len()
                                            && member_args.iter().zip(args.iter()).all(
                                                |(member_arg, arg)| {
                                                    crate::impls::eq_admitted(
                                                        member_arg,
                                                        arg,
                                                        &self.facts,
                                                    )
                                                },
                                            )))
                            }
                            _ => false,
                        })
                        .collect();
                    match claimed.as_slice() {
                        [member] => DPat::union_member(dpat_ty(member), class_dpat, dpat_ty(scrut)),
                        _ => class_dpat,
                    }
                }
                _ => class_dpat,
            }
        };
        PatternOutcome {
            dpat,
            matched_ty: if head_covers { scrut.clone() } else { head },
            recorded_ty: None,
            covers_type: head_covers && field_covers,
            // The head is fully consumed only when every field sub-pattern
            // is irrefutable for its field type.
            consumes_matched: field_covers,
        }
    }

    /// Array destructure: elements lower against the scrutinee's element
    /// type (ascription intersects when written); a rest binding takes the
    /// list type. A length-constrained shape never covers by type alone.
    fn lower_array_pattern(
        &mut self,
        body: &ExprBody,
        pat: PatId,
        prefix: &[PatId],
        rest: Option<&baml_compiler2_ast::ArrayRestPat>,
        suffix: &[PatId],
        scrut: &Ty,
    ) -> PatternOutcome {
        let ascribed = self
            .type_refs
            .array_ascriptions
            .get(&pat)
            .copied()
            .map(|type_ref| self.lower_body_annotation(type_ref));
        let effective = match &ascribed {
            Some(ascribed) if !ascribed.has_error() => {
                // A written ascription that provably shares no value with
                // the scrutinee (a non-list, or a disjoint list) is the
                // same dead-pattern mismatch a type pattern gets.
                self.check_pattern_type_overlap(pat, scrut, ascribed);
                self.narrow_to(scrut, ascribed)
            }
            _ => scrut.clone(),
        };
        // The element extraction below is a STRUCTURE demand - r-a's
        // `infer_slice_pat` structurally resolves the expected type
        // before the Array/Slice match, so an ascription naming a weak
        // alias (or a reducible projection, or a bounded var) answers
        // as its structure.
        let effective = self.structurally_resolve(&effective);
        // The lists the pattern can match are the list-holding members of
        // the scrutinee: a list type holds lists of its own element, and a
        // member whose values may be lists of ANY element (`unknown`, an
        // interface existential, a type parameter) holds lists no one list
        // type describes - generics are invariant, so an `int[]` is never an
        // `unknown[]`. Among several holders the pattern claims the ones its
        // own demands (ascriptions, nested structural shapes) fit, B-633's
        // provable-overlap conservatism: "cannot tell" keeps the member. A
        // pattern no holder fits is dead, and its sub-patterns report that
        // against every holder's elements.
        let scrut_structure = self.structurally_resolve(scrut);
        let erroneous = effective.has_error();
        let mut holders: Vec<(Ty, ListHolding)> = Vec::new();
        let mut scrutinee_is_lists = !erroneous;
        for member in scrut_members(&effective) {
            scrutinee_is_lists &= self.collect_list_holders(member, &mut Vec::new(), &mut holders);
        }
        let holder_count = holders.len();
        let claimed: Vec<(Ty, ListHolding)> = if holders.len() > 1 {
            let mut fitting = Vec::new();
            for (member, holding) in &holders {
                let fits = match holding {
                    ListHolding::Elements(_) => self.pattern_fits(body, pat, member),
                    ListHolding::AnyElements => true,
                };
                if fits {
                    fitting.push((member.clone(), holding.clone()));
                }
            }
            if fitting.is_empty() { holders } else { fitting }
        } else {
            holders
        };
        let element = if erroneous {
            // The scrutinee's own error was reported; its elements are
            // that error too, never a type of their own.
            Ty::error()
        } else if claimed.is_empty() {
            // A scrutinee that holds no list: the pattern is dead. It is
            // reported where a type pattern's dead arm is - not from a
            // silent probe (`is` written as a value, an or-alternative, a
            // rejected rest), and not against `never`, which no arm matches
            // - and a written ascription made the report above. Reported,
            // the elements are the sentinel; unreported, they are what a
            // dead binding holds, nothing.
            let reported = ascribed.is_none()
                && self.or_probe_depth == 0
                && self.rest_reject_depth == 0
                && !matches!(effective.kind(), InferTy::Never);
            if reported {
                self.pending_diags
                    .push(super::PendingDiag::ListPatternNeverMatches {
                        pat,
                        scrutinee: scrut.clone(),
                    });
                Ty::error()
            } else {
                Ty::never()
            }
        } else if claimed
            .iter()
            .any(|(_, holding)| matches!(holding, ListHolding::AnyElements))
        {
            Ty::intern(InferTy::Unknown)
        } else {
            let elements: Vec<Ty> = claimed
                .iter()
                .filter_map(|(_, holding)| match holding {
                    ListHolding::Elements(element) => Some(element.clone()),
                    ListHolding::AnyElements => None,
                })
                .collect();
            self.union_of(&elements)
        };
        // What the pattern matches - the claimed holders - is what its test
        // decides at run time: membership in that union where every claimed
        // holder is a list type, the list head alone where one holds lists of
        // any element.
        let matched_ty = if claimed.is_empty() {
            effective.clone()
        } else {
            let members: Vec<Ty> = claimed.iter().map(|(member, _)| member.clone()).collect();
            self.union_of(&members)
        };
        let claims_only_lists = !claimed.is_empty()
            && claimed
                .iter()
                .all(|(_, holding)| matches!(holding, ListHolding::Elements(_)));
        let mut sub_dpats = Vec::new();
        for &sub in prefix {
            sub_dpats.push(self.lower_pattern(body, sub, &element).dpat);
        }
        if let Some(rest) = rest
            && let Some(rest_pat) = rest.pat
        {
            // The rest slot carries a BINDING (`..let name`, optionally
            // ascribed: the chain may END in a type ascription) or the
            // wildcard (`.._` ignores the middle). Structural sub-patterns
            // have no sliced-middle semantics (E0001's rest-sub-pattern
            // rule) - including a bind CHAIN whose tail is structural.
            let shape_ok = rest_pattern_shape_ok(body, rest_pat);
            if !shape_ok && self.rest_reject_depth == 0 {
                self.pending_diags
                    .push(super::PendingDiag::RestNotBinding { pat: rest_pat });
            }
            // The rest binds the sliced middle: a list of the element.
            // A rejected subtree still lowers so its bindings record
            // (no unresolved-name cascades), but reports inside it are
            // suppressed - one rejection at the outermost link.
            let rest_ty = Ty::list(element.clone());
            if shape_ok {
                self.lower_pattern(body, rest_pat, &rest_ty);
            } else {
                self.rest_reject_depth += 1;
                self.lower_pattern(body, rest_pat, &rest_ty);
                self.rest_reject_depth -= 1;
            }
        }
        for &sub in suffix {
            sub_dpats.push(self.lower_pattern(body, sub, &element).dpat);
        }
        let shape = match rest {
            Some(_) => crate::exhaustiveness::SliceShape::Variable {
                prefix: prefix.len(),
                suffix: suffix.len(),
            },
            None => crate::exhaustiveness::SliceShape::Fixed(prefix.len() + suffix.len()),
        };
        let covers = matches!(
            shape,
            crate::exhaustiveness::SliceShape::Variable {
                prefix: 0,
                suffix: 0
            }
        );
        // A union column takes the claimed members as its branches, one
        // `UnionMember` each; any other column (or a claim that is not made of
        // the column's own members, after an ascription narrowed it) takes
        // the slice directly.
        let slice_dpat = |ty: &Ty| DPat::slice(shape.clone(), sub_dpats.clone(), dpat_ty(ty));
        let dpat = match scrut_structure.kind() {
            InferTy::Union(members)
                if !claimed.is_empty()
                    && claimed.iter().all(|(member, _)| members.contains(member)) =>
            {
                let mut branches: Vec<DPat> = claimed
                    .iter()
                    .map(|(member, _)| {
                        DPat::union_member(dpat_ty(member), slice_dpat(member), dpat_ty(scrut))
                    })
                    .collect();
                if branches.len() == 1 {
                    branches.remove(0)
                } else {
                    DPat::or(branches, dpat_ty(scrut))
                }
            }
            _ => slice_dpat(&matched_ty),
        };
        PatternOutcome {
            dpat,
            matched_ty,
            recorded_ty: None,
            // `[..]` accepts every value of a type made of lists only, and
            // every value of the claimed list types; a member that may hold
            // something other than a list (`unknown`, a type parameter) is
            // never wholly accepted.
            covers_type: covers
                && ascribed.is_none()
                && scrutinee_is_lists
                && claimed.len() == holder_count,
            consumes_matched: covers && claims_only_lists,
        }
    }

    /// Appends the list-holding members `ty` unfolds to, through unions and
    /// recursive aliases; returns whether every member is a list type. An
    /// alias met again on its own unfolding is an unguarded recursive member,
    /// which contributes nothing to its type (`TYPE_SYSTEM.md`, Productivity).
    fn collect_list_holders(
        &self,
        ty: &Ty,
        unfolding: &mut Vec<Ty>,
        holders: &mut Vec<(Ty, ListHolding)>,
    ) -> bool {
        if let InferTy::TypeAlias(_) = ty.kind() {
            if unfolding.contains(ty) {
                return true;
            }
            let expanded = self.expand_alias_ty(ty);
            if expanded != *ty {
                unfolding.push(ty.clone());
                let only_lists = self.collect_list_holders(&expanded, unfolding, holders);
                unfolding.pop();
                return only_lists;
            }
        }
        if let InferTy::Union(members) = ty.kind() {
            // Every member is visited: a non-list member must not hide the
            // lists after it.
            let mut only_lists = true;
            for member in members {
                only_lists &= self.collect_list_holders(member, unfolding, holders);
            }
            return only_lists;
        }
        match list_holding(ty) {
            Some(holding) => {
                let is_list = matches!(holding, ListHolding::Elements(_));
                if !holders.iter().any(|(member, _)| member == ty) {
                    holders.push((ty.clone(), holding));
                }
                is_list
            }
            None => false,
        }
    }

    /// Whether `pat` can fit a value of `ty`: false only on a PROVABLE
    /// misfit (an ascription with no overlap either way, a structural
    /// demand the type's kind cannot meet). The candidate-matching half
    /// of the union claim - `match_pattern`'s boolean-matcher shape,
    /// separate from the committing lowering - conservative in B-633's
    /// direction: "cannot tell" answers true, and the caller claims
    /// only on a unique fit.
    fn pattern_fits(&mut self, body: &ExprBody, pat: PatId, ty: &Ty) -> bool {
        let expanded = self.expand_alias_ty(ty);
        // A type whose values may be of any shape - or of any of several -
        // is never a provable misfit as a whole.
        match expanded.kind() {
            InferTy::Unknown
            | InferTy::TypeVar(_)
            | InferTy::AssociatedTypeProjection { .. }
            | InferTy::InferVar { .. }
            | InferTy::Error => return true,
            InferTy::Union(members) => {
                let members = members.to_vec();
                return members
                    .iter()
                    .any(|member| self.pattern_fits(body, pat, member));
            }
            _ => {}
        }
        match &body.patterns[pat] {
            Pattern::Wildcard => true,
            Pattern::Bind { subpat, .. } => {
                let Some(sub) = *subpat else {
                    return true;
                };
                if let Some(ascribed) = self.pattern_ascription_ty(body, sub) {
                    return !self.provably_disjoint(&expanded, &ascribed);
                }
                self.pattern_fits(body, sub, &expanded)
            }
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ..
            } => {
                let element = match list_holding(&expanded) {
                    Some(ListHolding::Elements(element)) => element,
                    // Lists of any element may be among its values: cannot
                    // tell.
                    Some(ListHolding::AnyElements) => return true,
                    None => return false,
                };
                if let Some(type_ref) = self.type_refs.array_ascriptions.get(&pat).copied() {
                    let ascribed = self.lower_body_annotation(type_ref);
                    if self.provably_disjoint(&expanded, &ascribed) {
                        return false;
                    }
                }
                let subs: Vec<PatId> = prefix.iter().chain(suffix.iter()).copied().collect();
                let rest_pat = rest.as_ref().and_then(|rest| rest.pat);
                subs.into_iter()
                    .all(|sub| self.pattern_fits(body, sub, &element))
                    && rest_pat.is_none_or(|rest| {
                        self.pattern_fits(body, rest, &Ty::list(element.clone()))
                    })
            }
            Pattern::Class { class, .. } => {
                let class = class.clone();
                self.class_head_fits(&class, &expanded)
            }
            // Type patterns carry their own runtime test; the lowering
            // settles their claim - no discrimination here.
            Pattern::Type(_) => true,
            Pattern::Or(alternatives) => {
                let alternatives = alternatives.clone();
                alternatives
                    .into_iter()
                    .any(|alternative| self.pattern_fits(body, alternative, &expanded))
            }
        }
    }

    /// Whether a destructure spelled with `class_path` can fit a value of
    /// `ty` (neither a union nor a type that may hold any shape): a class
    /// head demands that class by identity; an interface head demands an
    /// implementor, which an existential may be and any other type is
    /// unless no implementation of the interface is visible from it. A
    /// head that resolves to nothing fits - the lowering reports it.
    fn class_head_fits(&mut self, class_path: &[baml_type::Name], ty: &Ty) -> bool {
        use baml_compiler2_hir::contributions::Definition;
        let head = self.lower.path_head(class_path);
        let definition = (!self.scoped_type_param_heads(head, class_path))
            .then(|| self.lower.resolve_type_definition(head, class_path))
            .flatten();
        match definition {
            Some(Definition::Class(class)) => match ty.kind() {
                InferTy::Class(qtn, ..) => {
                    *qtn == crate::lower::class_qualified_name(self.db, class)
                }
                _ => false,
            },
            Some(Definition::Interface(interface)) => {
                if matches!(ty.kind(), InferTy::Interface(..)) {
                    return true;
                }
                let short = class_path.last().expect("type paths are never empty");
                let qtn = self
                    .lower
                    .qualify_definition(Definition::Interface(interface), short);
                crate::impls::structural_interface(self.db, &qtn).is_some()
                    || !crate::impls::impl_candidates(self.db, ty, &qtn).is_empty()
            }
            Some(_) | None => true,
        }
    }

    fn class_pattern_field_types(&self, qtn: &baml_type::DeclName, args: &[Ty]) -> Vec<Ty> {
        if let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
            self.facts.definition_of(qtn)
        {
            return crate::lower::class_field_types(self.db, class)
                .iter()
                .map(|(_, field_ty)| {
                    self.pattern_field_ty(&crate::impls::interned_ty(field_ty), args)
                })
                .collect();
        }
        // A served package's class: its row's fields, in declaration order.
        crate::extern_loc::mounted_class_loc(self.db, qtn)
            .map(|class| {
                crate::extern_loc::extern_class_row(self.db, class)
                    .body
                    .fields()
                    .iter()
                    .map(|(_, field_ty, _)| self.pattern_field_ty(&Ty::from_plain(field_ty), args))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A declared field's type at a pattern's class arguments, as the
    /// sub-pattern judges it: overlap and coverage need the type the field
    /// holds, so a projection the arguments make ground is reduced
    /// (`T.Out` at `T = IntOut` IS `int`).
    fn pattern_field_ty(&self, declared: &Ty, args: &[Ty]) -> Ty {
        self.reduced_for_pattern(&crate::lower::substitute_params(declared, args))
    }

    /// `ty` with its ground projections reduced; one still waiting on an
    /// inference variable stays, as it does in every relation.
    fn reduced_for_pattern(&self, ty: &Ty) -> Ty {
        match baml_type::interned::ClosedTy::try_from(ty) {
            Ok(closed) if closed.has_projection() => self
                .reduce_projections(&closed, super::PROJECTION_FINALIZE_FUEL)
                .into_ty(),
            Ok(_) | Err(baml_type::interned::OpenTy) => ty.clone(),
        }
    }
}

/// The legal shapes of a rest sub-pattern: the wildcard, a bare
/// binding, or a bind chain whose tail is at most a type ascription
/// (`..let name: T[]`). A chain ending in a STRUCTURAL link (array,
/// class, or-pattern) has no sliced-middle semantics.
/// Materialize a (possibly in-flight) type as a `DPat` column type.
///
/// Pattern lowering runs mid-inference, so a scrutinee (or a field/member
/// instantiated at its arguments) can still carry unsolved variables or
/// holes — which the finalized plain `Ty` cannot represent. An open column
/// poisons as `Ty::Error`, the same suppression `check_match` applies before
/// usefulness (`has_infer()` gate) and the vocabulary the exhaustiveness
/// machinery already keys on (`contains_error`): verdicts over an
/// open column are noise. Coarser than the retired lossy materialization
/// (which kept the closed spine around per-node holes), in the conservative
/// direction — fewer verdicts, never wrong ones. Recorded pattern FACTS are
/// unaffected: `type_of_pat` stores the interned type, resolved at finalize.
fn dpat_ty(ty: &Ty) -> baml_type::Ty {
    match baml_type::interned::ClosedTy::try_from(ty) {
        Ok(closed) => closed.to_plain(),
        // SUPPRESSION disposition: an open column takes the Error sentinel
        // because exhaustiveness exclusively WITHHOLDS verdicts on Error —
        // it never derives from it — so this is conservative, not laundered
        // compatibility. (An explicit unjudgeable `DPat` ctor could replace
        // the sentinel reuse; today recovery-Error and open-Error want the
        // same suppression.)
        Err(baml_type::interned::OpenTy) => baml_type::Ty::Error,
    }
}

/// Whether `pat` binds without writing a type or a shape: the wildcard, a
/// bare binding, a chain of bare bindings, or an or-pattern of these. Such a
/// pattern asks nothing of its scrutinee's structure and takes every value
/// that reaches it (`TYPE_SYSTEM.md`, Pattern Matching: "a pattern that
/// writes no type of its own").
pub(super) fn binds_without_a_type(body: &ExprBody, pat: PatId) -> bool {
    match &body.patterns[pat] {
        Pattern::Wildcard => true,
        Pattern::Bind { subpat: None, .. } => true,
        Pattern::Bind {
            subpat: Some(sub), ..
        } => binds_without_a_type(body, *sub),
        Pattern::Or(alternatives) => alternatives
            .iter()
            .all(|&alternative| binds_without_a_type(body, alternative)),
        Pattern::Type(_) | Pattern::Class { .. } | Pattern::Array { .. } => false,
    }
}

/// Whether `pat` takes every value that reaches it: a pattern that
/// [`binds_without_a_type`], or an or-pattern one of whose alternatives
/// does (`Panic | _`).
pub(super) fn takes_every_value(body: &ExprBody, pat: PatId) -> bool {
    match &body.patterns[pat] {
        Pattern::Wildcard => true,
        Pattern::Bind { subpat: None, .. } => true,
        Pattern::Bind {
            subpat: Some(sub), ..
        } => takes_every_value(body, *sub),
        Pattern::Or(alternatives) => alternatives
            .iter()
            .any(|&alternative| takes_every_value(body, alternative)),
        Pattern::Type(_) | Pattern::Class { .. } | Pattern::Array { .. } => false,
    }
}

fn rest_pattern_shape_ok(body: &ExprBody, pat: PatId) -> bool {
    match &body.patterns[pat] {
        Pattern::Wildcard => true,
        Pattern::Bind { subpat: None, .. } => true,
        Pattern::Bind {
            subpat: Some(sub), ..
        } => matches!(body.patterns[*sub], Pattern::Type(_)) || rest_pattern_shape_ok(body, *sub),
        _ => false,
    }
}

/// A scrutinee's members: union members, or the type itself.
fn scrut_members(scrut: &Ty) -> Vec<&Ty> {
    match scrut.kind() {
        InferTy::Union(members) => members.iter().collect(),
        _ => vec![scrut],
    }
}

/// The lists a list pattern can match among one type's values.
#[derive(Clone)]
enum ListHolding {
    /// Lists of exactly this element type.
    Elements(Ty),
    /// Lists of any element type, among other values: no one list type
    /// describes them, and their elements are only known to be `unknown`.
    AnyElements,
}

/// What a list pattern can match among the values of `ty`, a type that is
/// neither a union nor an alias; `None` when no value of it is a list.
fn list_holding(ty: &Ty) -> Option<ListHolding> {
    match ty.kind() {
        InferTy::List(element) => Some(ListHolding::Elements(element.clone())),
        // Any value at all, an implementor of an interface, whatever a type
        // parameter or projection is instantiated with, or a type that is
        // still being inferred: each may be a list of any element.
        InferTy::Unknown
        | InferTy::Interface(..)
        | InferTy::TypeVar(_)
        | InferTy::AssociatedTypeProjection { .. }
        | InferTy::InferVar { .. }
        | InferTy::Error => Some(ListHolding::AnyElements),
        // An alias that did not unfold: its shape is not known here.
        InferTy::TypeAlias(_) => Some(ListHolding::AnyElements),
        // A union an alias unfolded to: a caller that needs its members
        // unfolds it first; one that does not cannot tell.
        InferTy::Union(_) => Some(ListHolding::AnyElements),
        InferTy::Int
        | InferTy::Bigint
        | InferTy::Float
        | InferTy::String
        | InferTy::Bool
        | InferTy::Null
        | InferTy::Uint8Array
        | InferTy::Media(_)
        | InferTy::Literal(..)
        | InferTy::Class(..)
        | InferTy::Enum(_)
        | InferTy::EnumVariant(..)
        | InferTy::Map { .. }
        | InferTy::Function { .. }
        | InferTy::Future(..)
        | InferTy::RustType
        | InferTy::Type
        | InferTy::Resource
        | InferTy::PromptAst
        | InferTy::Never => None,
    }
}

/// The `PatCtx` impl backing the usefulness matrix, over plain resolved
/// types (TIR's impl transcribed; interface projections join with the I
/// cluster).
struct HirPatCtx<'a, 'db> {
    infer: &'a InferenceContext<'db>,
}

impl PatCtx for HirPatCtx<'_, '_> {
    fn interface_field_projection_for_class(
        &self,
        iface_ty: &baml_type::Ty,
        class_qtn: &baml_type::DeclName,
        _class_type_args: &[baml_type::Ty],
    ) -> Option<Vec<usize>> {
        use baml_compiler2_hir::contributions::Definition;
        let baml_type::Ty::Interface(iface_qtn, ..) = iface_ty else {
            return None;
        };
        let db = self.infer.db;
        let facts = &self.infer.facts;
        let Some(Definition::Class(class)) = facts.definition_of(class_qtn) else {
            return None;
        };
        // The interface's declared field list, whichever lane declares it.
        let iface_fields: Vec<baml_type::Name> = match facts.definition_of(iface_qtn) {
            Some(Definition::Interface(iface_loc)) => {
                baml_compiler2_hir::item_data::interface_data(db, iface_loc)
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect()
            }
            _ => {
                let interface = crate::extern_loc::mounted_interface_loc(db, iface_qtn)?;
                crate::extern_loc::extern_interface_row(db, interface)
                    .fields
                    .iter()
                    .map(|(name, ..)| name.clone())
                    .collect()
            }
        };
        let class_data = baml_compiler2_hir::item_data::class_data(db, class);
        let pkg = baml_compiler2_hir::file_package::file_package(db, class.file(db));
        let pkg_items = baml_compiler2_hir::package::package_items(db, pkg.root);
        // The class's block for that interface, matched by HEAD: the block's
        // written target lowers to a name, which is the same whether the
        // interface is declared in source or served from its rows.
        let block = class_data.implements.iter().find(|block| {
            crate::interfaces::resolve_ref_to_interface_name(
                db,
                &class_data.type_refs,
                block.target,
                pkg_items,
                &pkg.namespace_path,
            )
            .as_ref()
                == Some(iface_qtn)
        })?;
        iface_fields
            .iter()
            .map(|field| {
                let class_field = block
                    .field_links
                    .iter()
                    .find(|link| link.interface_field == *field)
                    .map(|link| link.class_field.clone())
                    .unwrap_or_else(|| field.clone());
                class_data
                    .fields
                    .iter()
                    .position(|candidate| candidate.name == class_field)
            })
            .collect()
    }

    fn enumerate_ctors(&self, ty: &baml_type::Ty) -> Vec<Ctor> {
        use baml_type::Ty as P;
        let ty = self.peel_aliases(ty.clone(), 8);
        match &ty {
            P::Bool => vec![
                Ctor::Single(P::Literal(
                    baml_base::Literal::Bool(true),
                    Freshness::Regular,
                )),
                Ctor::Single(P::Literal(
                    baml_base::Literal::Bool(false),
                    Freshness::Regular,
                )),
            ],
            P::Null | P::Literal(..) | P::EnumVariant(..) => vec![Ctor::Single(ty.clone())],
            P::Never => vec![],
            P::Union(members) => members
                .iter()
                .map(|member| Ctor::UnionMember(member.clone()))
                .collect(),
            P::Enum(qtn) => self
                .infer
                .facts
                .enum_variants(qtn)
                .unwrap_or_default()
                .into_iter()
                .map(|variant| Ctor::Single(P::EnumVariant(qtn.clone(), variant)))
                .collect(),
            P::Class(qtn, args) => vec![Ctor::Class(qtn.clone(), args.clone())],
            // An existential column is a single-constructor STRUCT VIEW
            // over its declared fields (rustc's non-enum struct shape).
            P::Interface(..) => vec![Ctor::Interface(ty.clone())],
            // Slice splitting owns list columns; empty defers to it.
            P::List(..) => vec![],
            // Everything else is an infinite or open alphabet.
            _ => vec![Ctor::NonExhaustive],
        }
    }

    /// The struct-view field types of an existential column, in declared
    /// order - the same member instantiation field access uses.
    fn interface_field_types(&self, iface_ty: &baml_type::Ty) -> Vec<baml_type::Ty> {
        use baml_compiler2_hir::contributions::Definition;
        let baml_type::Ty::Interface(qtn, args, pins) = iface_ty else {
            return Vec::new();
        };
        // The interface's declared field list, whichever lane declares it;
        // each field's type comes from the same member lookup either way.
        let field_names: Vec<baml_type::Name> = match self.infer.facts.definition_of(qtn) {
            Some(Definition::Interface(interface)) => {
                baml_compiler2_hir::item_data::interface_data(self.infer.db, interface)
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect()
            }
            _ => match crate::extern_loc::mounted_interface_loc(self.infer.db, qtn) {
                Some(interface) => {
                    crate::extern_loc::extern_interface_row(self.infer.db, interface)
                        .fields
                        .iter()
                        .map(|(name, ..)| name.clone())
                        .collect()
                }
                None => return Vec::new(),
            },
        };
        let head = Ty::from_plain(iface_ty);
        let target = InferInterface::new(
            qtn.clone(),
            args.iter()
                .map(Ty::from_plain)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            pins.iter()
                .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                .collect(),
        );
        field_names
            .iter()
            .map(|field| {
                crate::method_resolution::member_on_interface(
                    self.infer.db,
                    &self.infer.facts,
                    &target,
                    &head,
                    field,
                    true,
                )
                // Instantiation of a closed scrutinee head is closed; an
                // open escape poisons the column, the file's suppression
                // discipline (`dpat_ty`).
                .and_then(|member| {
                    baml_type::interned::ClosedTy::try_from(&member.ty)
                        .ok()
                        .map(|closed| closed.to_plain())
                })
                .unwrap_or(baml_type::Ty::Error)
            })
            .collect()
    }

    fn class_field_types(
        &self,
        qtn: &baml_type::DeclName,
        ty: &baml_type::Ty,
    ) -> Vec<baml_type::Ty> {
        let args: Vec<Ty> = match ty {
            baml_type::Ty::Class(_, args) => args
                .iter()
                .map(baml_type::interned::Ty::from_plain)
                .collect(),
            _ => Vec::new(),
        };
        self.infer
            .class_pattern_field_types(qtn, &args)
            .iter()
            // Substitution of closed args into declaration types is
            // closed; an open escape poisons the column (`dpat_ty`).
            .map(|ty| {
                baml_type::interned::ClosedTy::try_from(ty)
                    .map(|closed| closed.to_plain())
                    .unwrap_or_else(|_| baml_type::Ty::Error)
            })
            .collect()
    }

    fn list_element_type(&self, ty: &baml_type::Ty) -> baml_type::Ty {
        match self.peel_aliases(ty.clone(), 8) {
            baml_type::Ty::List(element) => *element,
            other => other,
        }
    }
}

impl HirPatCtx<'_, '_> {
    fn peel_aliases(&self, ty: baml_type::Ty, fuel: u32) -> baml_type::Ty {
        if fuel == 0 {
            return ty;
        }
        match &ty {
            baml_type::Ty::TypeAlias(qtn) => match self.infer.facts.alias_def(qtn) {
                Some(target) => self.peel_aliases(target, fuel - 1),
                None => ty,
            },
            _ => ty,
        }
    }
}

/// Every `(name, recorded type)` bind under `pat` (or-alternative
/// conflict detection walks each alternative's binds).
fn collect_bind_types(
    ctx: &super::InferenceContext<'_>,
    body: &ExprBody,
    pat: PatId,
    out: &mut Vec<(baml_type::Name, Ty)>,
) {
    match &body.patterns[pat] {
        Pattern::Bind { name, subpat } => {
            if let Some(ty) = ctx.result.type_of_pat.get(&pat) {
                out.push((name.clone(), ty.clone()));
            }
            if let Some(sub) = subpat {
                collect_bind_types(ctx, body, *sub, out);
            }
        }
        Pattern::Class { fields, .. } => {
            for field in fields {
                collect_bind_types(ctx, body, field.pat, out);
            }
        }
        Pattern::Array {
            prefix,
            rest,
            suffix,
            ..
        } => {
            for &p in prefix.iter().chain(suffix.iter()) {
                collect_bind_types(ctx, body, p, out);
            }
            if let Some(rest) = rest
                && let Some(rest_pat) = rest.pat
            {
                collect_bind_types(ctx, body, rest_pat, out);
            }
        }
        Pattern::Or(alts) => {
            for &alt in alts {
                collect_bind_types(ctx, body, alt, out);
            }
        }
        _ => {}
    }
}
