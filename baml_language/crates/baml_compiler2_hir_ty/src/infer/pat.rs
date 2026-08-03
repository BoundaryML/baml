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
    interned::{Ty, TyKind},
    normalize::{TypeContext as _, is_subtype_interned, normalize_interned},
};

use super::{Expectation, InferenceContext};
use crate::exhaustiveness::{
    Ctor, DPat, PatCtx, compute_match_usefulness,
};

/// One lowered pattern: the matrix row piece, the refined scrutinee type
/// the arm body sees, and whether the match is decided by type alone.
pub(super) struct PatternOutcome {
    pub dpat: DPat,
    pub matched_ty: Ty,
    pub covers_type: bool,
}

impl InferenceContext<'_> {
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
        let resolved = self.table.resolve_completely(&scrut_ty);
        let scrut_resolved = self.matrix_scrut(&resolved);
        let scrut_binding = self.narrowable_binding(body, scrutinee);
        let branch_expectation = expected.adjust_for_branches(&mut self.table);

        let entry_diverges = self.diverges;
        let mut arm_tys = Vec::new();
        let mut matrix_arms: Vec<DPat> = Vec::new();
        let mut all_diverge = super::Diverges::Always;
        for &arm_id in arms {
            let arm = &body.match_arms[arm_id];
            let outcome = self.lower_pattern(body, arm.pattern, &scrut_resolved);

            // Per-arm scrutinee narrowing: the arm body sees the refined
            // type. Saved/restored - the minimal flow overlay S10b grows.
            let saved = scrut_binding
                .map(|binding| (binding, self.flow.insert(binding, outcome.matched_ty.clone())));
            self.diverges = super::Diverges::Maybe;
            if let Some(guard) = arm.guard {
                self.check_expr(body, guard, &Ty::bool());
            }
            let arm_ty = self.infer_expr(body, arm.body, &branch_expectation);
            all_diverge = all_diverge.and(self.diverges);
            if let Some((binding, previous)) = saved {
                match previous {
                    Some(previous) => {
                        self.flow.insert(binding, previous);
                    }
                    None => {
                        self.flow.remove(&binding);
                    }
                }
            }
            arm_tys.push(arm_ty);
            if arm.guard.is_none() {
                matrix_arms.push(outcome.dpat);
            }
        }
        self.diverges = entry_diverges.or(all_diverge);

        if scrut_resolved.has_error() || scrut_resolved.has_infer() {
            return Ty::error();
        }
        let col_ty = scrut_resolved.to_plain();
        let ctx = HirPatCtx { infer: self };
        let report = compute_match_usefulness(&ctx, &matrix_arms, col_ty);
        if !report.missing.is_empty() {
            // Non-exhaustive: the match can fall through, which the type
            // system rejects. S17 renders the witnesses as E0062.
            self.result.non_exhaustive_matches.insert(match_expr);
            return Ty::error();
        }
        self.join(&arm_tys)
    }

    /// `expr is pattern`: the pattern lowers against the operand (no
    /// subtype gate - a never-matching test is legal and just false); the
    /// result is always bool. S10b reads the outcome for narrowing.
    pub(super) fn infer_is(&mut self, body: &ExprBody, scrutinee: ExprId, pattern: PatId) -> Ty {
        let scrut_ty = self.infer_expr(body, scrutinee, &Expectation::None);
        let resolved = self.table.resolve_completely(&scrut_ty);
        let scrut_resolved = self.matrix_scrut(&resolved);
        self.lower_pattern(body, pattern, &scrut_resolved);
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
    ) {
        let init_ty = match initializer {
            Some(init) => {
                let ty = self.infer_expr(body, init, &Expectation::None);
                self.widen_fresh(&ty)
            }
            None => Ty::error(),
        };
        let resolved = self.table.resolve_completely(&init_ty);
        let resolved = self.matrix_scrut(&resolved);
        self.lower_pattern(body, pattern, &resolved);
    }

    /// The canonical scrutinee for pattern analysis (TIR's
    /// `matrix_normalize_scrut`): aliases expanded, unions canonical - so
    /// a `Role = "user" | "assistant"` alias scrutinee projects onto its
    /// members. Var/error scrutinees pass through (the oracle requires
    /// var-free input; those matches sentinel out anyway).
    fn matrix_scrut(&self, ty: &Ty) -> Ty {
        if ty.has_infer() || ty.has_error() {
            return ty.clone();
        }
        normalize_interned(ty, &self.facts)
    }

    /// The scrutinee's binding, when it is a bare local - the only
    /// narrowable reference shape in S10a (member chains are a documented
    /// later extension).
    fn narrowable_binding(
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
        let scope = self.current_scope?;
        let key = baml_compiler2_hir::semantic_index::ExprMetadataKey::new(
            baml_compiler2_hir::semantic_index::ExprMetadataScope::Body(scope),
            expr,
        );
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
        match &body.patterns[pat] {
            Pattern::Wildcard => PatternOutcome {
                dpat: DPat::wildcard(scrut.to_plain()),
                matched_ty: scrut.clone(),
                covers_type: true,
            },
            Pattern::Bind { subpat, .. } => {
                let inner = match subpat {
                    Some(sub) => self.lower_pattern(body, *sub, scrut),
                    None => PatternOutcome {
                        dpat: DPat::wildcard(scrut.to_plain()),
                        matched_ty: scrut.clone(),
                        covers_type: true,
                    },
                };
                // Chain semantics: every bind in a chain takes the
                // rightmost (most refined) type.
                self.result
                    .type_of_binding
                    .insert(pat, inner.matched_ty.clone());
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
                self.type_pattern_outcome(scrut, &pat_ty)
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
                let outcomes: Vec<PatternOutcome> = alts
                    .iter()
                    .map(|&alt| self.lower_pattern(body, alt, scrut))
                    .collect();
                let matched = self.join(
                    &outcomes
                        .iter()
                        .map(|outcome| outcome.matched_ty.clone())
                        .collect::<Vec<_>>(),
                );
                PatternOutcome {
                    covers_type: outcomes.iter().any(|outcome| outcome.covers_type),
                    dpat: DPat::or(
                        outcomes.into_iter().map(|outcome| outcome.dpat).collect(),
                        scrut.to_plain(),
                    ),
                    matched_ty: matched,
                }
            }
        }
    }

    /// A type pattern (`let x: T`, literals, enum variants, `null`)
    /// against the scrutinee. Union scrutinees project onto claimed
    /// members; claiming requires PROVABLE overlap in either direction
    /// (the B-633 rule - undecidable rigid-vs-concrete pairs stay
    /// unclaimed and therefore uncovered).
    fn type_pattern_outcome(&mut self, scrut: &Ty, pat_ty: &Ty) -> PatternOutcome {
        if pat_ty.has_error() {
            return PatternOutcome {
                dpat: DPat::wildcard(scrut.to_plain()),
                matched_ty: Ty::error(),
                covers_type: false,
            };
        }
        let covers = provable_subtype(scrut, pat_ty, &self.facts);
        if let TyKind::Union(members, _) = scrut.kind()
            && !covers
        {
            let members: Vec<Ty> = members.to_vec();
            let claimed: Vec<&Ty> = members
                .iter()
                .filter(|member| {
                    provable_subtype(member, pat_ty, &self.facts)
                        || provable_subtype(pat_ty, member, &self.facts)
                })
                .collect();
            if !claimed.is_empty() {
                let scrut_plain = scrut.to_plain();
                let alts: Vec<DPat> = claimed
                    .iter()
                    .map(|member| {
                        let inner = if provable_subtype(member, pat_ty, &self.facts) {
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
                    covers_type: false,
                };
            }
            // Nothing provably claimed: possible-but-not-covering.
            return PatternOutcome {
                dpat: DPat::single(pat_ty.to_plain(), scrut.to_plain()),
                matched_ty: pat_ty.clone(),
                covers_type: false,
            };
        }
        PatternOutcome {
            dpat: self.dpat_for_type(pat_ty, scrut),
            matched_ty: if covers {
                scrut.clone()
            } else {
                self.narrow_to(scrut, pat_ty)
            },
            covers_type: covers,
        }
    }

    /// The refined scrutinee type after a successful type test - the meet,
    /// approximated: a scrutinee already inside the pattern stays; a
    /// pattern inside the scrutinee narrows to the pattern; otherwise the
    /// pattern type stands (rigid pairs keep the written type - TIR's
    /// `intersect_pattern_flow_types` policy).
    fn narrow_to(&self, scrut: &Ty, pat_ty: &Ty) -> Ty {
        if provable_subtype(scrut, pat_ty, &self.facts) {
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
        if provable_subtype(col, pat_ty, &self.facts) {
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
                                baml_type::Ty::EnumVariant(
                                    qtn.clone(),
                                    variant,
                                    TyAttr::default(),
                                ),
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
                if provable_subtype(col, pat_ty, &self.facts) {
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
        let Some(baml_compiler2_hir::contributions::Definition::Class(class)) =
            self.lower.resolve_type_definition(class_path)
        else {
            for &(_, field_pat) in field_pats {
                self.lower_pattern(body, field_pat, &Ty::error());
            }
            return PatternOutcome {
                dpat: DPat::wildcard(scrut.to_plain()),
                matched_ty: Ty::error(),
                covers_type: false,
            };
        };
        let qtn = crate::lower::class_qualified_name(self.db, class);
        let generic_count = crate::lower::class_generic_frame(self.db, class).len();

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
                    let field_ty =
                        crate::lower::substitute_params(&declared[index].1, &args);
                    let outcome = self.lower_pattern(body, *field_pat, &field_ty);
                    field_covers &= outcome.covers_type;
                    sub_dpats[index] = Some(outcome.dpat);
                }
                None => {
                    // Unknown field: S17's diagnostic; sub-bindings still
                    // record (as Error).
                    self.lower_pattern(body, *field_pat, &Ty::error());
                }
            }
        }
        let fields: Vec<DPat> = declared
            .iter()
            .zip(sub_dpats)
            .map(|((_, field_ty), sub)| {
                sub.unwrap_or_else(|| {
                    DPat::wildcard(
                        crate::lower::substitute_params(field_ty, &args).to_plain(),
                    )
                })
            })
            .collect();
        let head_covers = provable_subtype(scrut, &head, &self.facts);

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
                    let claimed: Vec<&Ty> = members
                        .iter()
                        .filter(|member| {
                            matches!(member.kind(), TyKind::Class(member_qtn, _, _) if *member_qtn == qtn)
                        })
                        .collect();
                    match claimed.as_slice() {
                        [member] => DPat::union_member(
                            member.to_plain(),
                            class_dpat,
                            scrut.to_plain(),
                        ),
                        _ => class_dpat,
                    }
                }
                _ => class_dpat,
            }
        };
        PatternOutcome {
            dpat,
            matched_ty: if head_covers { scrut.clone() } else { head },
            covers_type: head_covers && field_covers,
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
            Some(ascribed) if !ascribed.has_error() => self.narrow_to(scrut, ascribed),
            _ => scrut.clone(),
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
            // The rest binds the sliced middle: a list of the element.
            let rest_ty = Ty::list(element.clone());
            self.lower_pattern(body, rest_pat, &rest_ty);
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
        PatternOutcome {
            dpat: DPat::slice(shape, sub_dpats, effective.to_plain()),
            matched_ty: effective,
            covers_type: covers && ascribed.is_none(),
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
fn provable_subtype(sub: &Ty, sup: &Ty, facts: &crate::facts::Facts<'_>) -> bool {
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
    is_subtype_interned(sub, sup, facts)
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
            // Slice splitting owns list columns; empty defers to it.
            P::List(..) | P::EvolvingList(..) => vec![],
            // Everything else is an infinite or open alphabet.
            _ => vec![Ctor::NonExhaustive],
        }
    }

    fn class_field_types(
        &self,
        qtn: &baml_type::QualifiedTypeName,
        ty: &baml_type::Ty,
    ) -> Vec<baml_type::Ty> {
        let args: Vec<Ty> = match ty {
            baml_type::Ty::Class(_, args, _) => {
                args.iter().map(baml_type::interned::Ty::from_plain).collect()
            }
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
