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
    Freshness, TyAttr,
    interned::{InterfaceRef, Ty, TyKind},
    normalize::{TypeContext as _, normalize_interned},
};

use super::{Expectation, InferenceContext};
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
        let scrut_ty = self.infer_expr(body, scrutinee, &Expectation::None);
        let scrut_resolved = self.scrutinee_demand(&scrut_ty);
        let scrut_binding = self.narrowable_binding(body, scrutinee);
        let branch_expectation = expected.adjust_for_branches(&mut self.table);

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
            let outcome = self.lower_pattern(body, arm.pattern, &scrut_resolved);
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
                self.check_expr(body, guard, &Ty::bool());
                let guard_facts = self.condition_facts(body, guard);
                self.apply_facts(&guard_facts.when_true);
            }
            // A hard branch expectation CHECKS each arm (rustc coerces
            // every arm to the expectation): a failing arm reports at
            // ITSELF - and recovers as the expectation, so the join does
            // not re-report the same mismatch on the whole match. A
            // passing arm keeps its ACTUAL type (literal grain survives
            // the join).
            let arm_ty = match branch_expectation.only_has_type() {
                Some(expected_ty) => {
                    let expected_ty = expected_ty.clone();
                    let actual = self.check_expr(body, arm.body, &expected_ty);
                    if self.result.type_mismatches.contains_key(&arm.body) {
                        expected_ty
                    } else {
                        actual
                    }
                }
                None => self.infer_expr(body, arm.body, &branch_expectation),
            };
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
        // An unknown-FIELD in some arm's class pattern makes that arm's
        // matrix row a lie (the bad field dropped out), so usefulness
        // verdicts are noise - same suppression as an errored pattern.
        any_pattern_error |= self.pending_diags[pending_before..]
            .iter()
            .any(|pending| matches!(pending, super::PendingDiag::UnknownPatternField { .. }));

        if scrut_resolved.has_error() || scrut_resolved.has_infer() {
            return Ty::error();
        }
        let col_ty = scrut_resolved.to_plain();
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
                .map(|w| crate::exhaustiveness::render_witness_pat(self.db, w))
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
    /// result is always bool. S10b reads the outcome for narrowing.
    pub(super) fn infer_is(&mut self, body: &ExprBody, scrutinee: ExprId, pattern: PatId) -> Ty {
        let scrut_ty = self.infer_expr(body, scrutinee, &Expectation::None);
        let scrut_resolved = self.scrutinee_demand(&scrut_ty);
        // `is` is a runtime TYPE TEST: a pattern provably disjoint from
        // the scrutinee is a legal test that answers `false` (the corpus
        // pins `42 is string`), not the dead-pattern error a match arm
        // gets - probe silently.
        self.or_probe_depth += 1;
        self.lower_pattern(body, pattern, &scrut_resolved);
        self.or_probe_depth -= 1;
        Ty::bool()
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
                let def = self.lower.resolve_type_definition(&segments)?;
                let baml_compiler2_hir::contributions::Definition::Class(class_loc) = def else {
                    return None;
                };
                let short = segments.last()?;
                let qtn = self.lower.qualify_definition(def, short);
                let generic_count = baml_compiler2_ppir::item_data::class_data(self.db, class_loc)
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
                Some(Ty::intern(TyKind::Class(
                    qtn,
                    args.into(),
                    TyAttr::default(),
                )))
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
        if ty.has_infer() || ty.has_error() {
            return ty.clone();
        }
        normalize_interned(ty, &self.facts)
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
        let outcome = self.lower_pattern_inner(body, pat, scrut);
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

    fn lower_pattern_inner(&mut self, body: &ExprBody, pat: PatId, scrut: &Ty) -> PatternOutcome {
        match &body.patterns[pat] {
            Pattern::Wildcard => PatternOutcome {
                dpat: DPat::wildcard(scrut.to_plain()),
                matched_ty: scrut.clone(),
                recorded_ty: None,
                covers_type: true,
                consumes_matched: true,
            },
            Pattern::Bind { subpat, .. } => {
                let inner = match subpat {
                    Some(sub) => self.lower_pattern(body, *sub, scrut),
                    None => PatternOutcome {
                        dpat: DPat::wildcard(scrut.to_plain()),
                        matched_ty: scrut.clone(),
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
            Pattern::Unreflect(operand) => {
                self.validate_runtime_type_operand(body, *operand);
                let mut identity = self.body_owner_identity;
                for byte in pat.into_raw().into_u32().to_le_bytes() {
                    identity ^= u32::from(byte);
                    identity = identity.wrapping_mul(0x0100_0193);
                }
                let parameter = baml_type::ParamTy::new(
                    0xc000_0000 | (identity & 0x3fff_ffff),
                    baml_type::Name::new(format!("$unreflect${identity:08x}")),
                );
                let constructor = Ty::intern(TyKind::TypeVar(parameter, TyAttr::default()));
                PatternOutcome {
                    // Each runtime predicate is possible but cannot cover a
                    // static alphabet. Its statement-independent rigid
                    // singleton also keeps two source patterns distinct.
                    dpat: DPat::single(constructor.to_plain(), scrut.to_plain()),
                    matched_ty: scrut.clone(),
                    recorded_ty: None,
                    covers_type: false,
                    consumes_matched: false,
                }
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
                    .map(|&alt| self.lower_pattern(body, alt, scrut))
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
                        scrut.to_plain(),
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
        let attr = TyAttr::default;
        let data = baml_compiler2_ppir::item_data::interface_data(self.db, interface);
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
        let adopted = scrut_members(scrut)
            .into_iter()
            .find_map(|member| match member.kind() {
                TyKind::Interface(member_qtn, args, pins, _)
                    if *member_qtn == qtn
                        && (written_args.is_empty()
                            || (written_args.len() == args.len()
                                && written_args.iter().zip(args.iter()).all(|(a, b)| {
                                    baml_type::normalize::equivalent(
                                        &a.to_plain(),
                                        &b.to_plain(),
                                        &self.facts,
                                    )
                                }))) =>
                {
                    Some((args.to_vec(), pins.to_vec()))
                }
                _ => None,
            });
        let (args, pins): (Vec<Ty>, Vec<(baml_type::Name, Ty)>) = {
            let (adopted_args, adopted_pins) = adopted.unwrap_or_else(|| {
                (
                    (0..data.generic_params.len())
                        .map(|_| Ty::error())
                        .collect(),
                    Vec::new(),
                )
            });
            (
                if written_args.is_empty() {
                    adopted_args
                } else {
                    written_args
                },
                if written_pins.is_empty() {
                    adopted_pins
                } else {
                    written_pins
                },
            )
        };
        let head = Ty::intern(TyKind::Interface(
            qtn.clone(),
            args.clone().into_boxed_slice(),
            pins.clone().into_boxed_slice(),
            attr(),
        ));
        let target = InterfaceRef::new(qtn.clone(), args.into_boxed_slice(), pins);

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
            .map(|member| member.ty);
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
                    .map(|member| member.ty)
                    .unwrap_or_else(Ty::error);
                    DPat::wildcard(field_ty.to_plain())
                })
            })
            .collect();
        let head_covers = provable_subtype(scrut, &head, &self.facts, &self.canonical_cache);

        // The matrix's single-ctor STRUCT VIEW of an existential
        // (`Ctor::Interface`, rustc's non-enum struct treatment): field
        // sub-patterns decompose, so refutable field arms COMPOSE to
        // coverage (`{ active: true }` + `{ active: false }`).
        let dpat = {
            let iface_dpat = DPat::interface(head.to_plain(), fields, scrut.to_plain());
            match scrut.kind() {
                TyKind::Union(members, _) => {
                    let claimed: Vec<&Ty> = members
                        .iter()
                        .filter(|member| {
                            matches!(member.kind(), TyKind::Interface(member_qtn, _, _, _) if *member_qtn == qtn)
                        })
                        .collect();
                    match claimed.as_slice() {
                        [member] => {
                            DPat::union_member(member.to_plain(), iface_dpat, scrut.to_plain())
                        }
                        _ => iface_dpat,
                    }
                }
                _ => iface_dpat,
            }
        };
        PatternOutcome {
            dpat,
            matched_ty: if head_covers { scrut.clone() } else { head },
            recorded_ty: None,
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
        if pat_ty.has_error() || scrut.has_error() || pat_ty.has_infer() || scrut.has_infer() {
            return;
        }
        let pat_plain = pat_ty.to_plain();
        let scrut_plain = scrut.to_plain();
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
        let mismatch = oracle_no
            || (ground
                && !self.pattern_matchable(&pat_plain, &scrut_plain)
                && !self.pattern_overlaps_scrut_member(&pat_plain, &scrut_plain));
        if mismatch {
            self.pending_diags
                .push(super::PendingDiag::PatternScrutMismatch {
                    pat,
                    expected: scrut.clone(),
                    found: pat_ty.clone(),
                });
        }
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
            (P::Never { .. }, _) => true,
            (P::List(a, _) | P::EvolvingList(a, _), P::List(b, _) | P::EvolvingList(b, _)) => {
                self.pattern_matchable(a, b)
            }
            (
                P::Map {
                    key: ka, value: va, ..
                },
                P::Map {
                    key: kb, value: vb, ..
                },
            ) => self.pattern_matchable(ka, kb) && self.pattern_matchable(va, vb),
            (
                P::Interface(pat_qtn, pat_args, pat_assoc, _),
                P::Interface(scrut_qtn, scrut_args, _, _),
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
            baml_type::Ty::Union(members, _) => members
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
            let baml_type::Ty::TypeAlias(qtn, _) = &ty else {
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
                dpat: DPat::wildcard(scrut.to_plain()),
                matched_ty: pat_ty.clone(),
                recorded_ty: None,
                covers_type: false,
                consumes_matched: false,
            };
        }
        let covers = provable_subtype(scrut, pat_ty, &self.facts, &self.canonical_cache);
        if let TyKind::Union(members, _) = scrut.kind()
            && !covers
        {
            let members: Vec<Ty> = members.to_vec();
            let claimed: Vec<&Ty> = members
                .iter()
                .filter(|member| {
                    provable_subtype(member, pat_ty, &self.facts, &self.canonical_cache)
                        || provable_subtype(pat_ty, member, &self.facts, &self.canonical_cache)
                })
                .collect();
            if !claimed.is_empty() {
                let scrut_plain = scrut.to_plain();
                let alts: Vec<DPat> = claimed
                    .iter()
                    .map(|member| {
                        let inner =
                            if provable_subtype(member, pat_ty, &self.facts, &self.canonical_cache)
                            {
                                DPat::wildcard(member.to_plain())
                            } else {
                                self.dpat_for_type(pat_ty, member)
                            };
                        DPat::union_member(member.to_plain(), inner, scrut_plain.clone())
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
                dpat: DPat::single(pat_ty.to_plain(), scrut.to_plain()),
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
        if provable_subtype(scrut, pat_ty, &self.facts, &self.canonical_cache) {
            scrut.clone()
        } else {
            pat_ty.clone()
        }
    }

    /// The coverage-side lowering of a type pattern (TIR's `dpat_for_type`
    /// five regimes): singletons, finite alphabets, classes, rigid vars,
    /// and the subtype fallback. Deliberately stricter than arm validity.
    fn dpat_for_type(&self, pat_ty: &Ty, col: &Ty) -> DPat {
        let col_plain = col.to_plain();
        let pat_plain = pat_ty.to_plain();
        // The universal coverage rule first: a pattern the whole column
        // provably fits is a wildcard at this column, whatever its shape
        // (a same-union pattern must not decompose into per-member
        // singles that no longer align with UnionMember ctors).
        if provable_subtype(col, pat_ty, &self.facts, &self.canonical_cache) {
            return DPat::wildcard(col_plain);
        }
        match pat_ty.kind() {
            TyKind::Literal(..) | TyKind::EnumVariant(..) | TyKind::Null { .. } => {
                DPat::single(pat_plain, col_plain)
            }
            TyKind::Bool { .. } => DPat::or(
                [true, false]
                    .into_iter()
                    .map(|value| {
                        DPat::single(
                            baml_type::Ty::Literal(
                                baml_base::Literal::Bool(value),
                                Freshness::Regular,
                                TyAttr::default(),
                            ),
                            col_plain.clone(),
                        )
                    })
                    .collect(),
                col_plain,
            ),
            TyKind::Enum(qtn, _) => {
                let variants = self.facts.enum_variants(qtn).unwrap_or_default();
                DPat::or(
                    variants
                        .into_iter()
                        .map(|variant| {
                            DPat::single(
                                baml_type::Ty::EnumVariant(qtn.clone(), variant, TyAttr::default()),
                                col_plain.clone(),
                            )
                        })
                        .collect(),
                    col_plain,
                )
            }
            TyKind::Union(members, _) => DPat::or(
                members
                    .iter()
                    .map(|member| self.dpat_for_type(member, col))
                    .collect(),
                col_plain,
            ),
            TyKind::Class(qtn, args, _) => {
                let fields = self.class_pattern_field_types(qtn, args);
                DPat::class_inst(
                    qtn.clone(),
                    args.iter().map(Ty::to_plain).collect(),
                    fields
                        .iter()
                        .map(|field_ty| DPat::wildcard(field_ty.to_plain()))
                        .collect(),
                    pat_plain,
                )
            }
            TyKind::TypeVar(..) => {
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
                if provable_subtype(col, pat_ty, &self.facts, &self.canonical_cache) {
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
                dpat: DPat::wildcard(scrut.to_plain()),
                matched_ty: Ty::error(),
                recorded_ty: None,
                covers_type: false,
                consumes_matched: false,
            };
        }
        let definition = self.lower.resolve_type_definition(class_path);
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
            // E0003 at the pattern, fields lower against the sentinel.
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
                    });
            }
            for &(_, field_pat) in field_pats {
                self.lower_pattern(body, field_pat, &Ty::error());
            }
            return PatternOutcome {
                dpat: DPat::wildcard(scrut.to_plain()),
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
                    matches!(member.kind(), TyKind::Class(member_qtn, _, _) if *member_qtn == qtn)
                })
                .collect();
            match candidates.as_slice() {
                [only] => match only.kind() {
                    TyKind::Class(_, args, _) => args.to_vec(),
                    _ => Vec::new(),
                },
                // None or ambiguous: Error args (S17's diagnostic).
                _ => (0..generic_count).map(|_| Ty::error()).collect(),
            }
        };

        let head = crate::lower::class_ty(qtn.clone(), args.clone());
        let declared = crate::lower::class_field_types(self.db, class);
        let mut field_covers = true;
        let mut sub_dpats: Vec<Option<DPat>> = vec![None; declared.len()];
        for (name, field_pat) in field_pats {
            let index = declared.iter().position(|(field, _)| field == name);
            match index {
                Some(index) => {
                    let field_ty = crate::lower::substitute_params(&declared[index].1, &args);
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
                    DPat::wildcard(crate::lower::substitute_params(field_ty, &args).to_plain())
                })
            })
            .collect();
        let head_covers = provable_subtype(scrut, &head, &self.facts, &self.canonical_cache);

        // Union scrutinee: claim the same-class member.
        let dpat = {
            let class_dpat = DPat::class_inst(
                qtn.clone(),
                args.iter().map(Ty::to_plain).collect(),
                fields,
                head.to_plain(),
            );
            match scrut.kind() {
                TyKind::Union(members, _) => {
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
                            TyKind::Class(member_qtn, member_args, _) => {
                                *member_qtn == qtn
                                    && (args.is_empty()
                                        || (member_args.len() == args.len()
                                            && member_args.iter().zip(args.iter()).all(
                                                |(member_arg, arg)| {
                                                    baml_type::normalize::equivalent_interned(
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
                        [member] => {
                            DPat::union_member(member.to_plain(), class_dpat, scrut.to_plain())
                        }
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
        // Union scrutinee: claim the list member the pattern
        // DISCRIMINATES - the class arm's agreeing-instantiation rule
        // applied to the list constructor. A single list member claims
        // by constructor kind alone; among several, the pattern's own
        // demands (ascriptions, nested structural shapes) filter, and
        // only a UNIQUE fit claims - B-633's provable-overlap
        // conservatism: "cannot tell" keeps the member, several
        // survivors stay unclaimed.
        let claimed_union = match effective.kind() {
            TyKind::Union(members, _) => {
                let members = members.to_vec();
                let mut lists: Vec<Ty> = Vec::new();
                for member in &members {
                    if matches!(member.kind(), TyKind::List(..)) {
                        lists.push(member.clone());
                    }
                }
                let claimed = match lists.as_slice() {
                    [member] => Some(member.clone()),
                    [] => None,
                    _ => {
                        let mut fitting: Vec<Ty> = Vec::new();
                        for member in &lists {
                            if self.pattern_fits(body, pat, member) {
                                fitting.push(member.clone());
                            }
                        }
                        match fitting.as_slice() {
                            [member] => Some(member.clone()),
                            _ => None,
                        }
                    }
                };
                claimed.map(|member| (effective.clone(), member))
            }
            _ => None,
        };
        let effective = match &claimed_union {
            Some((_, member)) => member.clone(),
            None => effective,
        };
        let element = match effective.kind() {
            TyKind::List(element, _) => element.clone(),
            _ => Ty::error(),
        };
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
        let slice_dpat = DPat::slice(shape, sub_dpats, effective.to_plain());
        let dpat = match &claimed_union {
            Some((scrut_union, member)) => {
                DPat::union_member(member.to_plain(), slice_dpat, scrut_union.to_plain())
            }
            None => slice_dpat,
        };
        PatternOutcome {
            dpat,
            matched_ty: effective,
            recorded_ty: None,
            // A member-claiming pattern narrows; it never covers the
            // whole union.
            covers_type: covers && ascribed.is_none() && claimed_union.is_none(),
            consumes_matched: covers,
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
        match &body.patterns[pat] {
            Pattern::Wildcard => true,
            Pattern::Bind { subpat, .. } => {
                let Some(sub) = *subpat else {
                    return true;
                };
                if let Some(ascribed) = self.pattern_ascription_ty(body, sub) {
                    return provable_subtype(
                        &ascribed,
                        &expanded,
                        &self.facts,
                        &self.canonical_cache,
                    ) || provable_subtype(
                        &expanded,
                        &ascribed,
                        &self.facts,
                        &self.canonical_cache,
                    );
                }
                self.pattern_fits(body, sub, &expanded)
            }
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ..
            } => {
                let TyKind::List(element, _) = expanded.kind() else {
                    return false;
                };
                let element = element.clone();
                let subs: Vec<PatId> = prefix.iter().chain(suffix.iter()).copied().collect();
                let rest_pat = rest.as_ref().and_then(|rest| rest.pat);
                subs.into_iter()
                    .all(|sub| self.pattern_fits(body, sub, &element))
                    && rest_pat.is_none_or(|rest| {
                        self.pattern_fits(body, rest, &Ty::list(element.clone()))
                    })
            }
            Pattern::Class { class, .. } => match expanded.kind() {
                // A class head demands the same class; interfaces and
                // rigid vars could still adopt or implement - true.
                TyKind::Class(qtn, ..) => class.last().is_none_or(|name| name == qtn.name()),
                TyKind::Interface(..) | TyKind::TypeVar(..) => true,
                _ => false,
            },
            // Type patterns carry their own runtime test; the lowering
            // settles their claim - no discrimination here.
            Pattern::Type(_) | Pattern::Unreflect(_) => true,
            Pattern::Or(alternatives) => {
                let alternatives = alternatives.clone();
                alternatives
                    .into_iter()
                    .any(|alternative| self.pattern_fits(body, alternative, &expanded))
            }
        }
    }

    fn class_pattern_field_types(&self, qtn: &baml_type::TypeName, args: &[Ty]) -> Vec<Ty> {
        match self.facts.definition_of(qtn) {
            Some(baml_compiler2_hir::contributions::Definition::Class(class)) => {
                crate::lower::class_field_types(self.db, class)
                    .iter()
                    .map(|(_, field_ty)| crate::lower::substitute_params(field_ty, args))
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

/// A PROVABLE subtype verdict: ground on both sides and confirmed by the
/// oracle. Rigid or unresolved pairs are not provable - the conservative
/// direction for both coverage and claiming.
pub(super) fn provable_subtype(
    sub: &Ty,
    sup: &Ty,
    facts: &crate::facts::Facts<'_>,
    canonical_cache: &baml_type::normalize::InternedCanonicalCache,
) -> bool {
    if sub == sup {
        return true;
    }
    if sub.has_infer() || sup.has_infer() || sub.has_error() || sup.has_error() {
        return false;
    }
    // Rigid variables go to the oracle too: its typevar arms are already
    // conservative (`T <: T`, `T <: unknown`, `never <: T` prove; a rigid
    // against an unrelated concrete does not - which is exactly the B-633
    // rule). The corpus pinned the case this matters for: a synthetic
    // effect var IS covered by `throws unknown`.
    canonical_cache.is_subtype(sub, sup, facts)
}

/// The legal shapes of a rest sub-pattern: the wildcard, a bare
/// binding, or a bind chain whose tail is at most a type ascription
/// (`..let name: T[]`). A chain ending in a STRUCTURAL link (array,
/// class, or-pattern) has no sliced-middle semantics.
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
        TyKind::Union(members, _) => members.iter().collect(),
        _ => vec![scrut],
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
        class_qtn: &baml_type::QualifiedTypeName,
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
        let Some(Definition::Interface(iface_loc)) = facts.definition_of(iface_qtn) else {
            return None;
        };
        let class_data = baml_compiler2_ppir::item_data::class_data(db, class);
        let pkg = baml_compiler2_hir::file_package::file_package(db, class.file(db));
        let pkg_items = baml_compiler2_ppir::package_items(
            db,
            baml_compiler2_hir::package::PackageId::new(db, pkg.package.clone()),
        );
        // The class's implements block for THIS interface supplies the
        // `field as class_field` links (default: the same name).
        let block = class_data.implements.iter().find(|block| {
            crate::interfaces::resolve_ref_to_interface(
                db,
                &class_data.type_refs,
                block.target,
                pkg_items,
                &pkg.namespace_path,
            ) == Some(iface_loc)
        })?;
        let iface_data = baml_compiler2_ppir::item_data::interface_data(db, iface_loc);
        iface_data
            .fields
            .iter()
            .map(|field| {
                let class_field = block
                    .field_links
                    .iter()
                    .find(|link| link.interface_field == field.name)
                    .map(|link| link.class_field.clone())
                    .unwrap_or_else(|| field.name.clone());
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
            P::Bool { .. } => vec![
                Ctor::Single(P::Literal(
                    baml_base::Literal::Bool(true),
                    Freshness::Regular,
                    TyAttr::default(),
                )),
                Ctor::Single(P::Literal(
                    baml_base::Literal::Bool(false),
                    Freshness::Regular,
                    TyAttr::default(),
                )),
            ],
            P::Null { .. } | P::Literal(..) | P::EnumVariant(..) => vec![Ctor::Single(ty.clone())],
            P::Never { .. } => vec![],
            P::Union(members, _) => members
                .iter()
                .map(|member| Ctor::UnionMember(member.clone()))
                .collect(),
            P::Enum(qtn, _) => self
                .infer
                .facts
                .enum_variants(qtn)
                .unwrap_or_default()
                .into_iter()
                .map(|variant| {
                    Ctor::Single(P::EnumVariant(qtn.clone(), variant, TyAttr::default()))
                })
                .collect(),
            P::Class(qtn, args, _) => vec![Ctor::Class(qtn.clone(), args.clone())],
            // An existential column is a single-constructor STRUCT VIEW
            // over its declared fields (rustc's non-enum struct shape).
            P::Interface(..) => vec![Ctor::Interface(ty.clone())],
            // Slice splitting owns list columns; empty defers to it.
            P::List(..) | P::EvolvingList(..) => vec![],
            // Everything else is an infinite or open alphabet.
            _ => vec![Ctor::NonExhaustive],
        }
    }

    /// The struct-view field types of an existential column, in declared
    /// order - the same member instantiation field access uses.
    fn interface_field_types(&self, iface_ty: &baml_type::Ty) -> Vec<baml_type::Ty> {
        use baml_compiler2_hir::contributions::Definition;
        let baml_type::Ty::Interface(qtn, args, pins, _) = iface_ty else {
            return Vec::new();
        };
        let Some(Definition::Interface(interface)) = self.infer.facts.definition_of(qtn) else {
            return Vec::new();
        };
        let head = Ty::from_plain(iface_ty);
        let target = InterfaceRef::new(
            qtn.clone(),
            args.iter()
                .map(Ty::from_plain)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            pins.iter()
                .map(|(name, ty)| (name.clone(), Ty::from_plain(ty)))
                .collect(),
        );
        baml_compiler2_ppir::item_data::interface_data(self.infer.db, interface)
            .fields
            .iter()
            .map(|field| {
                crate::method_resolution::member_on_interface(
                    self.infer.db,
                    &self.infer.facts,
                    &target,
                    &head,
                    &field.name,
                    true,
                )
                .map(|member| member.ty.to_plain())
                .unwrap_or_else(|| baml_type::Ty::Error {
                    attr: TyAttr::default(),
                })
            })
            .collect()
    }

    fn class_field_types(
        &self,
        qtn: &baml_type::QualifiedTypeName,
        ty: &baml_type::Ty,
    ) -> Vec<baml_type::Ty> {
        let args: Vec<Ty> = match ty {
            baml_type::Ty::Class(_, args, _) => args
                .iter()
                .map(baml_type::interned::Ty::from_plain)
                .collect(),
            _ => Vec::new(),
        };
        self.infer
            .class_pattern_field_types(qtn, &args)
            .iter()
            .map(Ty::to_plain)
            .collect()
    }

    fn list_element_type(&self, ty: &baml_type::Ty) -> baml_type::Ty {
        match self.peel_aliases(ty.clone(), 8) {
            baml_type::Ty::List(element, _) | baml_type::Ty::EvolvingList(element, _) => *element,
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
            baml_type::Ty::TypeAlias(qtn, _) => match self.infer.facts.alias_def(qtn) {
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
