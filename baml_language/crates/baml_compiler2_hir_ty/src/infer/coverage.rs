//! Coverage lowering: the `DPat` rows the usefulness matrix runs over.
//!
//! Pattern typing (`pat.rs`) types every pattern against the scrutinee's
//! inference type and records, in `pattern_heads`, what each pattern
//! wrote. The rows are built here, in a separate pass, against
//! [`ColumnTy`] columns: the root is the canonical scrutinee, and every
//! nested column comes from [`ctor_sub_tys`] / [`PatCtx::enumerate_ctors`],
//! the same derivation the matrix specializes with. A row and the column
//! it is checked against therefore agree on every nested type by
//! construction. Binding types and pattern facts never come from here.

use baml_compiler2_ast::{ArrayRestPat, ExprBody, PatId, Pattern};
use baml_type::{
    DeclName, Freshness,
    interned::{InferInterface, Ty},
    normalize::{TypeContext as _, normalize_interned},
};

use super::InferenceContext;
use crate::exhaustiveness::{ColumnTy, Ctor, DPat, PatCtx, SliceShape, ctor_sub_tys};

/// What a pattern wrote, as typing resolved it: the inputs a row needs
/// beyond the pattern's syntax. Typing records one for every type pattern,
/// destructure and list pattern it lowers; a missing or mismatched entry
/// fails closed (the row covers nothing, and debug builds assert).
#[derive(Clone)]
pub(super) enum PatternHead {
    /// A type pattern's written type.
    Type(Ty),
    /// A class destructure at its written or adopted args, and the declared
    /// slot of each field it writes.
    Class {
        qtn: DeclName,
        head: Ty,
        fields: Vec<(usize, PatId)>,
    },
    /// An interface destructure's existential head, and the declared slot
    /// of each field it writes.
    Interface {
        head: Ty,
        fields: Vec<(usize, PatId)>,
    },
    /// A list pattern's written ascription.
    Array { ascription: Option<Ty> },
    /// A pattern typing reported (a destructure whose head resolves to no
    /// class or interface, a tuple that never matches its scrutinee) or one
    /// under an already-reported errored scrutinee: its row is a wildcard,
    /// so no coverage verdict stacks on that report.
    Recovery,
}

impl PatternHead {
    /// The head with every inference variable typing has solved since it
    /// was recorded substituted.
    pub(super) fn resolved(&self, table: &mut super::unify::InferenceTable) -> Self {
        match self {
            Self::Type(ty) => Self::Type(table.resolve_completely(ty)),
            Self::Class { qtn, head, fields } => Self::Class {
                qtn: qtn.clone(),
                head: table.resolve_completely(head),
                fields: fields.clone(),
            },
            Self::Interface { head, fields } => Self::Interface {
                head: table.resolve_completely(head),
                fields: fields.clone(),
            },
            Self::Array { ascription } => Self::Array {
                ascription: ascription.as_ref().map(|ty| table.resolve_completely(ty)),
            },
            Self::Recovery => Self::Recovery,
        }
    }
}

/// The `PatCtx` backing the usefulness matrix and coverage lowering, over
/// plain resolved types.
pub(super) struct HirPatCtx<'a, 'db> {
    pub(super) infer: &'a InferenceContext<'db>,
}

impl HirPatCtx<'_, '_> {
    /// The row for `pat` against column `col`.
    pub(super) fn coverage_dpat(&self, body: &ExprBody, pat: PatId, col: &ColumnTy) -> DPat {
        if matches!(
            self.infer.pattern_heads.get(&pat),
            Some(PatternHead::Recovery)
        ) {
            return DPat::wildcard(col.clone());
        }
        match &body.patterns[pat] {
            Pattern::Wildcard | Pattern::Bind { subpat: None, .. } => DPat::wildcard(col.clone()),
            Pattern::Bind {
                subpat: Some(sub), ..
            } => self.coverage_dpat(body, *sub, col),
            Pattern::Or(alts) => any_of(
                alts.iter()
                    .map(|&alt| self.coverage_dpat(body, alt, col))
                    .collect(),
                col,
                || proves_nothing(col),
            ),
            Pattern::Tuple(elements) => self.tuple_dpat(body, pat, elements, col),
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ..
            } => self.slice_dpat(body, pat, prefix, rest.as_ref(), suffix, col),
            Pattern::Type(_) => match self.infer.pattern_heads.get(&pat) {
                Some(PatternHead::Type(written)) => match self.written(written) {
                    Some(written) if baml_type::contains_error(&written) => {
                        // An erroneous written type was reported where it
                        // was written; its row does not stack a coverage
                        // verdict on that report.
                        DPat::wildcard(col.clone())
                    }
                    Some(written) => self.type_dpat(&written, col),
                    None => proves_nothing(col),
                },
                _ => missing_head(pat, "type", col),
            },
            Pattern::Class { .. } => match self.infer.pattern_heads.get(&pat) {
                Some(PatternHead::Class { qtn, head, fields }) => {
                    self.class_dpat(body, qtn, head, fields, col)
                }
                Some(PatternHead::Interface { head, fields }) => {
                    self.interface_dpat(body, head, fields, col)
                }
                _ => missing_head(pat, "destructure", col),
            },
        }
    }

    /// A written type in canonical form, so ctor payloads built from it
    /// compare equal to the members of the canonical columns; `None` while
    /// it still carries inference variables.
    fn written(&self, ty: &Ty) -> Option<baml_type::Ty> {
        let closed = baml_type::interned::ClosedTy::try_from(ty).ok()?;
        Some(self.canonicalize(&closed.to_plain()))
    }

    /// A type test (`let x: T`, a literal, an enum variant, `null`). A
    /// written union tests each of its alternatives. A union column
    /// projects onto every member the written type provably overlaps in
    /// either direction (the B-633 rule: undecidable rigid-vs-concrete
    /// pairs stay unclaimed and therefore uncovered).
    fn type_dpat(&self, written: &baml_type::Ty, col: &ColumnTy) -> DPat {
        if self.is_subtype(col.ty(), written) {
            return DPat::wildcard(col.clone());
        }
        if let baml_type::Ty::Union(alternatives) = written {
            return any_of(
                alternatives
                    .iter()
                    .map(|alternative| self.type_dpat(alternative, col))
                    .collect(),
                col,
                || proves_nothing(col),
            );
        }
        if !matches!(col.ty(), baml_type::Ty::Union(_)) {
            return self.type_dpat_in(written, col);
        }
        let claims = self.claims(col, &|member_col| {
            (self.is_subtype(member_col.ty(), written) || self.is_subtype(written, member_col.ty()))
                .then(|| self.type_dpat(written, member_col))
        });
        // No claim: possible-but-not-covering.
        any_of(claims, col, || {
            DPat::type_test(written.clone(), col.clone())
        })
    }

    /// A type test against a column that is not a union (TIR's
    /// `dpat_for_type` regimes): singletons, finite alphabets, classes,
    /// rigid variables, and the subtype fallback.
    fn type_dpat_in(&self, written: &baml_type::Ty, col: &ColumnTy) -> DPat {
        use baml_type::Ty as P;
        let singletons = |values: Vec<P>| {
            any_of(
                values
                    .into_iter()
                    .map(|value| DPat::type_test(value, col.clone()))
                    .collect(),
                col,
                || DPat::type_test(written.clone(), col.clone()),
            )
        };
        match written {
            P::Bool => singletons(
                [true, false]
                    .into_iter()
                    .map(|value| P::Literal(baml_base::Literal::Bool(value), Freshness::Regular))
                    .collect(),
            ),
            P::Enum(qtn) => singletons(
                self.infer
                    .facts
                    .enum_variants(qtn)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|variant| P::EnumVariant(qtn.clone(), variant))
                    .collect(),
            ),
            P::Class(qtn, args) => {
                let ctor = Ctor::Class(qtn.clone(), args.clone());
                let fields = ctor_sub_tys(self, &ctor, col)
                    .into_iter()
                    .map(DPat::wildcard)
                    .collect();
                DPat::class_inst(qtn.clone(), args.clone(), fields, col.clone())
            }
            // Rigid: covers only a column of the SAME variable (the B-633
            // rule); `type_dpat` already took that case as a subtype.
            _ => DPat::type_test(written.clone(), col.clone()),
        }
    }

    /// A class destructure. In a union column it claims the one member of
    /// the same class at agreeing args (so per-instantiation arms compose
    /// to coverage); a column with no such member, or several, takes the
    /// destructure directly.
    fn class_dpat(
        &self,
        body: &ExprBody,
        qtn: &DeclName,
        head: &Ty,
        fields: &[(usize, PatId)],
        col: &ColumnTy,
    ) -> DPat {
        // Args come off the canonical head as a whole: canonicalizing each
        // arg alone would unfold a recursive alias the head keeps folded.
        let Some(written) = self.written(head) else {
            return proves_nothing(col);
        };
        // Error args (a generic destructure without them, an erroneous
        // written arg) were reported by typing; such a ctor covers any args.
        let baml_type::Ty::Class(_, args) = written else {
            return fails_closed(col, format_args!("class destructure head {written:?}"));
        };
        let ctor = Ctor::Class(qtn.clone(), args.clone());
        let build = |col: &ColumnTy| {
            let sub_cols = ctor_sub_tys(self, &ctor, col);
            let rows = self.field_rows(body, fields, &sub_cols);
            DPat::class_inst(qtn.clone(), args.clone(), rows, col.clone())
        };
        let mut claims = self.claims(col, &|member_col| match member_col.ty() {
            baml_type::Ty::Class(member_qtn, member_args)
                if member_qtn == qtn
                    && (args.iter().any(baml_type::contains_error)
                        || Ctor::Class(member_qtn.clone(), member_args.clone()) == ctor) =>
            {
                Some(build(member_col))
            }
            _ => None,
        });
        if claims.len() == 1 {
            claims.pop().expect("one claim")
        } else {
            build(col)
        }
    }

    /// An interface destructure: the existential's single-ctor struct view.
    /// It claims the members of the same interface, and each implementing
    /// class member as that class, its written fields carried to the class
    /// slots that supply them.
    fn interface_dpat(
        &self,
        body: &ExprBody,
        head: &Ty,
        fields: &[(usize, PatId)],
        col: &ColumnTy,
    ) -> DPat {
        let Some(head) = self.written(head) else {
            return proves_nothing(col);
        };
        let baml_type::Ty::Interface(qtn, ..) = &head else {
            return fails_closed(col, format_args!("interface destructure head {head:?}"));
        };
        let ctor = Ctor::Interface(head.clone());
        let build = |col: &ColumnTy| {
            let sub_cols = ctor_sub_tys(self, &ctor, col);
            let rows = self.field_rows(body, fields, &sub_cols);
            DPat::interface(head.clone(), rows, col.clone())
        };
        let claims = self.claims(col, &|member_col| match member_col.ty() {
            baml_type::Ty::Interface(member_qtn, ..) if member_qtn == qtn => {
                Some(build(member_col))
            }
            baml_type::Ty::Class(class_qtn, class_args) => {
                let projection =
                    self.interface_field_projection_for_class(&head, class_qtn, class_args)?;
                let class_ctor = Ctor::Class(class_qtn.clone(), class_args.clone());
                let class_fields: Vec<(usize, PatId)> = fields
                    .iter()
                    .map(|&(slot, field_pat)| (projection[slot], field_pat))
                    .collect();
                let sub_cols = ctor_sub_tys(self, &class_ctor, member_col);
                Some(DPat::class_inst(
                    class_qtn.clone(),
                    class_args.clone(),
                    self.field_rows(body, &class_fields, &sub_cols),
                    member_col.clone(),
                ))
            }
            _ => None,
        });
        any_of(claims, col, || build(col))
    }

    /// A tuple: claims the members its shape may fit (the members of its
    /// arity whose slots its sub-patterns may match, and those that may
    /// hold a tuple of any slot types). Where it fits nothing it covers
    /// nothing.
    fn tuple_dpat(&self, body: &ExprBody, pat: PatId, elements: &[PatId], col: &ColumnTy) -> DPat {
        let ctor = Ctor::Tuple(elements.len());
        let build = |col: &ColumnTy| {
            let sub_cols = ctor_sub_tys(self, &ctor, col);
            let rows = elements
                .iter()
                .zip(&sub_cols)
                .map(|(&element, sub_col)| self.coverage_dpat(body, element, sub_col))
                .collect();
            DPat::tuple(rows, col.clone())
        };
        let claims = self.claims(col, &|member_col| {
            self.may_fit(body, pat, member_col)
                .then(|| build(member_col))
        });
        any_of(claims, col, || build(col))
    }

    /// A list pattern: claims the members that hold lists its shape may
    /// fit. A written ascription, on the list or on its rest, narrows that
    /// to the members inside every one of them, and in a member that may
    /// hold any shape tests for the ascribed type.
    fn slice_dpat(
        &self,
        body: &ExprBody,
        pat: PatId,
        prefix: &[PatId],
        rest: Option<&ArrayRestPat>,
        suffix: &[PatId],
        col: &ColumnTy,
    ) -> DPat {
        let shape = match rest {
            Some(_) => SliceShape::Variable {
                prefix: prefix.len(),
                suffix: suffix.len(),
            },
            None => SliceShape::Fixed(prefix.len() + suffix.len()),
        };
        let ctor = Ctor::Slice(shape.clone());
        let build = |col: &ColumnTy| {
            let sub_cols = ctor_sub_tys(self, &ctor, col);
            let rows = prefix
                .iter()
                .chain(suffix)
                .zip(&sub_cols)
                .map(|(&element, sub_col)| self.coverage_dpat(body, element, sub_col))
                .collect();
            DPat::slice(shape.clone(), rows, col.clone())
        };
        let ascriptions = match self.slice_ascriptions(body, pat, rest) {
            Ok(Some(ascriptions)) => ascriptions,
            Ok(None) => return proves_nothing(col),
            Err(missing) => return missing_head(missing, "list ascription", col),
        };
        let within_ascriptions = |member: &baml_type::Ty| {
            ascriptions
                .iter()
                .all(|ascription| self.is_subtype(member, ascription))
        };
        // The type an ascribed pattern tests for in a column that may hold
        // any shape: the ascription inside all the others.
        let tested = ascriptions
            .iter()
            .find(|&ascription| within_ascriptions(ascription));
        let claims = self.claims(col, &|member_col| {
            let member = member_col.ty();
            match tested {
                // A test inside an open member keeps its own type, so arms
                // testing different types stay apart; the open member never
                // lists that type, so the arm proves none of it covered.
                Some(tested) if holds_any_shape(member) => Some(DPat::union_member(
                    tested.clone(),
                    build(&self.column(tested)),
                    member_col.clone(),
                )),
                _ => {
                    let holds = if ascriptions.is_empty() {
                        matches!(member, baml_type::Ty::List(_)) || holds_any_shape(member)
                    } else {
                        matches!(member, baml_type::Ty::List(_)) && within_ascriptions(member)
                    };
                    (holds && self.may_fit(body, pat, member_col)).then(|| build(member_col))
                }
            }
        });
        if !claims.is_empty() || matches!(col.ty(), baml_type::Ty::Union(_)) {
            // A union whose lists the pattern does not claim: the slice
            // stands in the column itself, covering none of its members.
            return any_of(claims, col, || build(col));
        }
        if within_ascriptions(col.ty()) {
            build(col)
        } else {
            // A list the ascriptions do not contain: the pattern matches
            // some of its values (or none) and covers none of them.
            proves_nothing(col)
        }
    }

    /// The canonical written ascriptions of a list pattern: its own, and
    /// its rest binding's. An erroneous one was reported where it was
    /// written and constrains nothing. `Ok(None)` while one is still open;
    /// `Err` names a pattern typing recorded no head for.
    fn slice_ascriptions(
        &self,
        body: &ExprBody,
        pat: PatId,
        rest: Option<&ArrayRestPat>,
    ) -> Result<Option<Vec<baml_type::Ty>>, PatId> {
        let Some(PatternHead::Array { ascription: own }) = self.infer.pattern_heads.get(&pat)
        else {
            return Err(pat);
        };
        let rest = match rest
            .and_then(|rest| rest.pat)
            .and_then(|rest_pat| rest_ascription(body, rest_pat))
        {
            Some(type_pat) => match self.infer.pattern_heads.get(&type_pat) {
                Some(PatternHead::Type(written)) => Some(written),
                _ => return Err(type_pat),
            },
            None => None,
        };
        Ok(own
            .iter()
            .chain(rest)
            .map(|ascription| self.written(ascription))
            .filter(|ascription| {
                ascription
                    .as_ref()
                    .is_none_or(|ascription| !baml_type::contains_error(ascription))
            })
            .collect())
    }

    /// Whether `pat` may match some value of `col`: false only on a
    /// provable misfit (a written type provably disjoint from the column, a
    /// shape the column cannot hold), so "cannot tell" keeps a claim
    /// (B-633's direction). Columns whose values may be of several shapes
    /// are never a provable misfit as a whole.
    fn may_fit(&self, body: &ExprBody, pat: PatId, col: &ColumnTy) -> bool {
        let ty = col.ty();
        if matches!(ty, baml_type::Ty::Union(_)) || holds_any_shape(ty) {
            return true;
        }
        let head = self.infer.pattern_heads.get(&pat);
        if matches!(head, Some(PatternHead::Recovery)) {
            return true;
        }
        match &body.patterns[pat] {
            Pattern::Wildcard | Pattern::Bind { subpat: None, .. } => true,
            Pattern::Bind {
                subpat: Some(sub), ..
            } => self.may_fit(body, *sub, col),
            Pattern::Or(alts) => alts.iter().any(|&alt| self.may_fit(body, alt, col)),
            Pattern::Tuple(elements) => match ty {
                baml_type::Ty::Tuple(slots) if slots.len() == elements.len() => elements
                    .iter()
                    .zip(ctor_sub_tys(self, &Ctor::Tuple(elements.len()), col))
                    .all(|(&element, slot)| self.may_fit(body, element, &slot)),
                _ => false,
            },
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ..
            } => {
                let baml_type::Ty::List(_) = ty else {
                    return false;
                };
                let disjoint = |written: &Ty| {
                    self.written(written).is_some_and(|written| {
                        self.infer
                            .provably_disjoint(&Ty::from_plain(ty), &Ty::from_plain(&written))
                    })
                };
                if let Some(PatternHead::Array {
                    ascription: Some(ascription),
                }) = head
                    && disjoint(ascription)
                {
                    return false;
                }
                let element = ctor_sub_tys(self, &Ctor::Slice(SliceShape::Fixed(1)), col)
                    .pop()
                    .expect("a one-element slice has one sub-column");
                prefix
                    .iter()
                    .chain(suffix)
                    .all(|&element_pat| self.may_fit(body, element_pat, &element))
                    && rest
                        .as_ref()
                        .and_then(|rest| rest.pat)
                        .is_none_or(|rest_pat| self.may_fit(body, rest_pat, col))
            }
            Pattern::Type(_) | Pattern::Class { .. } => match head {
                Some(PatternHead::Type(written)) => self.written(written).is_none_or(|written| {
                    !self
                        .infer
                        .provably_disjoint(&Ty::from_plain(ty), &Ty::from_plain(&written))
                }),
                Some(PatternHead::Class { qtn, .. }) => {
                    matches!(ty, baml_type::Ty::Class(col_qtn, _) if col_qtn == qtn)
                }
                Some(
                    PatternHead::Interface { .. }
                    | PatternHead::Array { .. }
                    | PatternHead::Recovery,
                )
                | None => true,
            },
        }
    }

    /// The rows a structural pattern claims in `col`: in a union column one
    /// `UnionMember` branch per member `claim` accepts, each built against
    /// that member's own column (a member whose column is itself a union, a
    /// recursive alias the canonical form keeps folded, is searched in
    /// turn); in any other column, the claim of the column itself.
    fn claims(&self, col: &ColumnTy, claim: &dyn Fn(&ColumnTy) -> Option<DPat>) -> Vec<DPat> {
        if !matches!(col.ty(), baml_type::Ty::Union(_)) {
            return claim(col).into_iter().collect();
        }
        let mut branches = Vec::new();
        for ctor in self.enumerate_ctors(col) {
            let Ctor::UnionMember(member) = &ctor else {
                continue;
            };
            let [member_col] = <[ColumnTy; 1]>::try_from(ctor_sub_tys(self, &ctor, col))
                .expect("a union member has one sub-column");
            let mut nested = self.claims(&member_col, claim);
            let inner = match nested.len() {
                0 => continue,
                1 => nested.pop().expect("one claim"),
                _ => DPat::or(nested, member_col.clone()),
            };
            branches.push(DPat::union_member(member.clone(), inner, col.clone()));
        }
        branches
    }

    /// One row per declared field: the written sub-pattern against its
    /// field column, a wildcard where none is written.
    fn field_rows(
        &self,
        body: &ExprBody,
        fields: &[(usize, PatId)],
        sub_cols: &[ColumnTy],
    ) -> Vec<DPat> {
        sub_cols
            .iter()
            .enumerate()
            .map(|(slot, sub_col)| {
                match fields
                    .iter()
                    .find(|(written_slot, _)| *written_slot == slot)
                {
                    Some(&(_, field_pat)) => self.coverage_dpat(body, field_pat, sub_col),
                    None => DPat::wildcard(sub_col.clone()),
                }
            })
            .collect()
    }
}

/// The row matching any of `alts`: an `or` of two or more, the one
/// alternative itself, and `empty` for none.
fn any_of(mut alts: Vec<DPat>, col: &ColumnTy, empty: impl FnOnce() -> DPat) -> DPat {
    match alts.len() {
        0 => empty(),
        1 => alts.pop().expect("one alternative"),
        _ => DPat::or(alts, col.clone()),
    }
}

/// A row that covers no value of `col`: what a pattern contributes when
/// nothing proves which values it matches.
fn proves_nothing(col: &ColumnTy) -> DPat {
    DPat::type_test(baml_type::Ty::Never, col.clone())
}

/// The row for a pattern whose recorded head is missing or of the wrong
/// kind, which typing never leaves: it covers nothing, and debug builds
/// fail.
fn fails_closed(col: &ColumnTy, what: std::fmt::Arguments<'_>) -> DPat {
    debug_assert!(false, "coverage head invariant broken: {what}");
    proves_nothing(col)
}

/// [`fails_closed`] for a pattern typing recorded no fitting head for.
fn missing_head(pat: PatId, kind: &str, col: &ColumnTy) -> DPat {
    fails_closed(col, format_args!("no {kind} head recorded for {pat:?}"))
}

/// The type pattern a rest binding's chain ends in (`..let rest: T`).
fn rest_ascription(body: &ExprBody, pat: PatId) -> Option<PatId> {
    match &body.patterns[pat] {
        Pattern::Type(_) => Some(pat),
        Pattern::Bind {
            subpat: Some(sub), ..
        } => rest_ascription(body, *sub),
        _ => None,
    }
}

impl PatCtx for HirPatCtx<'_, '_> {
    /// The canonical algebra's normal form (`matrix_scrut`'s): aliases
    /// expanded, unions flat and ordered, a recursive alias at the root
    /// unfolded once. Errored types pass through (the normalizer takes
    /// error-free input); an open type is the Error sentinel, on which
    /// exhaustiveness withholds verdicts.
    fn canonicalize(&self, ty: &baml_type::Ty) -> baml_type::Ty {
        if baml_type::contains_error(ty) {
            return ty.clone();
        }
        match baml_type::interned::ClosedTy::try_from(&Ty::from_plain(ty)) {
            Ok(closed) => normalize_interned(&closed, &self.infer.facts).to_plain(),
            Err(baml_type::interned::OpenTy) => baml_type::Ty::Error,
        }
    }

    fn interface_field_projection_for_class(
        &self,
        iface_ty: &baml_type::Ty,
        class_qtn: &baml_type::DeclName,
        class_type_args: &[baml_type::Ty],
    ) -> Option<Vec<usize>> {
        use baml_compiler2_hir::contributions::Definition;
        let baml_type::Ty::Interface(iface_qtn, iface_args, iface_assoc) = iface_ty else {
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
        // The impl by which THIS realized class type implements THIS realized
        // interface, selected like every other impl goal, so a class that
        // implements one interface at several argument lists (`Foo<int>` and
        // `Foo<string>`) gets the links of the matching block, not the first
        // block of that name.
        let concrete = crate::impls::try_interned_ty(&baml_type::Ty::Class(
            class_qtn.clone(),
            class_type_args.into(),
        ))?;
        let goal = baml_type::interned::InferInterface::from_constraint(
            &baml_type::Interface::new(iface_qtn.clone(), iface_args.clone(), iface_assoc.clone()),
        );
        let resolved = crate::impls::resolve_impl(db, &concrete, &goal)?;
        // Field links are source data (an interface with fields is implemented
        // in-body, so the block is in this compilation).
        let crate::impls::ResolvedImplFacts::Source { block, .. } = resolved.facts else {
            return None;
        };
        let block = baml_compiler2_hir::item_data::impl_block_data(db, block);
        let class_data = baml_compiler2_hir::item_data::class_data(db, class);
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

    fn enumerate_ctors(&self, col: &ColumnTy) -> Vec<Ctor> {
        use baml_type::Ty as P;
        let ty = col.ty();
        match ty {
            P::Bool => vec![
                Ctor::Type(P::Literal(
                    baml_base::Literal::Bool(true),
                    Freshness::Regular,
                )),
                Ctor::Type(P::Literal(
                    baml_base::Literal::Bool(false),
                    Freshness::Regular,
                )),
            ],
            P::Null | P::Literal(..) | P::EnumVariant(..) => vec![Ctor::Type(ty.clone())],
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
                .map(|variant| Ctor::Type(P::EnumVariant(qtn.clone(), variant)))
                .collect(),
            P::Class(qtn, args) => vec![Ctor::Class(qtn.clone(), args.clone())],
            // An existential column is a single-constructor STRUCT VIEW
            // over its declared fields (rustc's non-enum struct shape).
            P::Interface(..) => vec![Ctor::Interface(ty.clone())],
            // Slice splitting owns list columns; empty defers to it.
            P::List(..) => vec![],
            P::Tuple(elements) => vec![Ctor::Tuple(elements.len())],
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
                .map_or(baml_type::Ty::Error, |member| plain_or_error(&member.ty))
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
            .map(plain_or_error)
            .collect()
    }

    fn is_subtype(&self, sub: &baml_type::Ty, sup: &baml_type::Ty) -> bool {
        // Error is compatible with everything in the subtype relation; here
        // it must prove nothing (exhaustiveness withholds verdicts on it).
        !baml_type::contains_error(sub)
            && !baml_type::contains_error(sup)
            && baml_type::normalize::is_subtype(sub, sup, &self.infer.facts)
    }

    fn list_element_type(&self, ty: &baml_type::Ty) -> baml_type::Ty {
        match ty {
            baml_type::Ty::List(element) => (**element).clone(),
            other => other.clone(),
        }
    }
}

/// A field type as a plain type: instantiating a closed column's head is
/// closed, and an open escape takes the Error sentinel, on which
/// exhaustiveness withholds verdicts. Canonicalization is
/// [`ctor_sub_tys`]'s, applied once to every sub-column.
fn plain_or_error(ty: &Ty) -> baml_type::Ty {
    baml_type::interned::ClosedTy::try_from(ty)
        .map_or(baml_type::Ty::Error, |closed| closed.to_plain())
}

/// Types that may hold a value of any shape (a list, a tuple of any
/// slots): no ctor of theirs can be listed.
fn holds_any_shape(ty: &baml_type::Ty) -> bool {
    matches!(
        ty,
        baml_type::Ty::Unknown
            | baml_type::Ty::Interface(..)
            | baml_type::Ty::TypeVar(_)
            | baml_type::Ty::AssociatedTypeProjection { .. }
            | baml_type::Ty::Error
    )
}
