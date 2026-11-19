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
/// beyond the pattern's syntax. A pattern with no entry (a destructure
/// that failed to resolve, or one under an errored scrutinee) lowers to a
/// wildcard row, so its own diagnostic is not stacked with a coverage one.
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
}

/// The `PatCtx` backing the usefulness matrix and coverage lowering, over
/// plain resolved types.
pub(super) struct HirPatCtx<'a, 'db> {
    pub(super) infer: &'a InferenceContext<'db>,
}

impl HirPatCtx<'_, '_> {
    /// The row for `pat` against column `col`.
    pub(super) fn coverage_dpat(&self, body: &ExprBody, pat: PatId, col: &ColumnTy) -> DPat {
        match &body.patterns[pat] {
            Pattern::Wildcard | Pattern::Bind { subpat: None, .. } => DPat::wildcard(col.clone()),
            Pattern::Bind {
                subpat: Some(sub), ..
            } => self.coverage_dpat(body, *sub, col),
            Pattern::Or(alts) => DPat::or(
                alts.iter()
                    .map(|&alt| self.coverage_dpat(body, alt, col))
                    .collect(),
                col.clone(),
            ),
            Pattern::Tuple(elements) => self.tuple_dpat(body, elements, col),
            Pattern::Array {
                prefix,
                rest,
                suffix,
                ..
            } => {
                let ascription = match self.infer.pattern_heads.get(&pat) {
                    Some(PatternHead::Array { ascription }) => ascription.as_ref(),
                    _ => None,
                };
                self.slice_dpat(body, prefix, rest.as_ref(), suffix, ascription, col)
            }
            Pattern::Type(_) | Pattern::Class { .. } => match self.infer.pattern_heads.get(&pat) {
                Some(PatternHead::Type(written)) => match self.written(written) {
                    Some(written) if !baml_type::contains_error(&written) => {
                        self.type_dpat(&written, col)
                    }
                    // An erroneous written type stays local to its pattern:
                    // the row covers nothing it cannot prove.
                    _ => DPat::wildcard(col.clone()),
                },
                Some(PatternHead::Class { qtn, head, fields }) => {
                    self.class_dpat(body, qtn, head, fields, col)
                }
                Some(PatternHead::Interface { head, fields }) => {
                    self.interface_dpat(body, head, fields, col)
                }
                Some(PatternHead::Array { .. }) | None => DPat::wildcard(col.clone()),
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

    /// A type test (`let x: T`, a literal, an enum variant, `null`). A union
    /// column projects onto every member the written type provably overlaps
    /// in either direction (the B-633 rule: undecidable rigid-vs-concrete
    /// pairs stay unclaimed and therefore uncovered).
    fn type_dpat(&self, written: &baml_type::Ty, col: &ColumnTy) -> DPat {
        if self.is_subtype(col.ty(), written) {
            return DPat::wildcard(col.clone());
        }
        if !matches!(col.ty(), baml_type::Ty::Union(_)) {
            return self.type_dpat_in(written, col);
        }
        let mut branches = Vec::new();
        for ctor in self.enumerate_ctors(col) {
            let Ctor::UnionMember(member) = &ctor else {
                continue;
            };
            let [member_col] = <[ColumnTy; 1]>::try_from(ctor_sub_tys(self, &ctor, col))
                .expect("a union member has one sub-column");
            if self.is_subtype(member_col.ty(), written)
                || self.is_subtype(written, member_col.ty())
            {
                branches.push(DPat::union_member(
                    member.clone(),
                    self.type_dpat(written, &member_col),
                    col.clone(),
                ));
            }
        }
        match branches.len() {
            // Possible-but-not-covering.
            0 => DPat::type_test(written.clone(), col.clone()),
            1 => branches.pop().expect("one branch"),
            _ => DPat::or(branches, col.clone()),
        }
    }

    /// A type test against a column that is not a union (TIR's
    /// `dpat_for_type` regimes): singletons, finite alphabets, classes,
    /// rigid variables, and the subtype fallback.
    fn type_dpat_in(&self, written: &baml_type::Ty, col: &ColumnTy) -> DPat {
        use baml_type::Ty as P;
        match written {
            P::Bool => DPat::or(
                [true, false]
                    .into_iter()
                    .map(|value| {
                        DPat::type_test(
                            P::Literal(baml_base::Literal::Bool(value), Freshness::Regular),
                            col.clone(),
                        )
                    })
                    .collect(),
                col.clone(),
            ),
            P::Enum(qtn) => DPat::or(
                self.infer
                    .facts
                    .enum_variants(qtn)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|variant| {
                        DPat::type_test(P::EnumVariant(qtn.clone(), variant), col.clone())
                    })
                    .collect(),
                col.clone(),
            ),
            P::Union(members) => DPat::or(
                members
                    .iter()
                    .map(|member| self.type_dpat(member, col))
                    .collect(),
                col.clone(),
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
        let args: Box<[baml_type::Ty]> = match self.written(head) {
            Some(baml_type::Ty::Class(_, args)) => args,
            _ => match head.kind() {
                baml_type::interned::InferTy::Class(_, args) => {
                    vec![baml_type::Ty::Error; args.len()].into_boxed_slice()
                }
                _ => Box::new([]),
            },
        };
        let ctor = Ctor::Class(qtn.clone(), args.clone());
        let build = |col: &ColumnTy| {
            let sub_cols = ctor_sub_tys(self, &ctor, col);
            let rows = self.field_rows(body, fields, &sub_cols);
            DPat::class_inst(qtn.clone(), args.clone(), rows, col.clone())
        };
        let applies = |member: &baml_type::Ty| match member {
            baml_type::Ty::Class(member_qtn, member_args) => {
                member_qtn == qtn
                    && (args.iter().any(baml_type::contains_error)
                        || Ctor::Class(member_qtn.clone(), member_args.clone()) == ctor)
            }
            _ => false,
        };
        let mut branches = self.member_branches(col, &applies, &build);
        if branches.len() == 1 {
            branches.pop().expect("one branch")
        } else {
            build(col)
        }
    }

    /// An interface destructure: the existential's single-ctor struct view.
    /// In a union column it claims the one member of the same interface.
    fn interface_dpat(
        &self,
        body: &ExprBody,
        head: &Ty,
        fields: &[(usize, PatId)],
        col: &ColumnTy,
    ) -> DPat {
        let Some(head) = self.written(head) else {
            return DPat::wildcard(col.clone());
        };
        let baml_type::Ty::Interface(qtn, ..) = &head else {
            return DPat::wildcard(col.clone());
        };
        let ctor = Ctor::Interface(head.clone());
        let build = |col: &ColumnTy| {
            let sub_cols = ctor_sub_tys(self, &ctor, col);
            let rows = self.field_rows(body, fields, &sub_cols);
            DPat::interface(head.clone(), rows, col.clone())
        };
        let applies = |member: &baml_type::Ty| matches!(member, baml_type::Ty::Interface(member_qtn, ..) if member_qtn == qtn);
        let mut branches = self.member_branches(col, &applies, &build);
        if branches.len() == 1 {
            branches.pop().expect("one branch")
        } else {
            build(col)
        }
    }

    /// A tuple: claims the members of its arity, and those that may hold a
    /// tuple of any slot types (`unknown`, a type parameter, an
    /// existential).
    fn tuple_dpat(&self, body: &ExprBody, elements: &[PatId], col: &ColumnTy) -> DPat {
        let arity = elements.len();
        let ctor = Ctor::Tuple(arity);
        let build = |col: &ColumnTy| {
            let sub_cols = ctor_sub_tys(self, &ctor, col);
            let rows = elements
                .iter()
                .zip(&sub_cols)
                .map(|(&element, sub_col)| self.coverage_dpat(body, element, sub_col))
                .collect();
            DPat::tuple(rows, col.clone())
        };
        let applies = |member: &baml_type::Ty| match member {
            baml_type::Ty::Tuple(slots) => slots.len() == arity,
            other => holds_any_shape(other),
        };
        if !matches!(col.ty(), baml_type::Ty::Union(_)) {
            return if applies(col.ty()) {
                build(col)
            } else {
                // A dead tuple pattern: typing reported it.
                DPat::wildcard(col.clone())
            };
        }
        let mut branches = self.member_branches(col, &applies, &build);
        match branches.len() {
            0 => DPat::wildcard(col.clone()),
            1 => branches.pop().expect("one branch"),
            _ => DPat::or(branches, col.clone()),
        }
    }

    /// A list pattern: claims the members that hold lists (a written
    /// ascription narrows that to the list types inside it).
    fn slice_dpat(
        &self,
        body: &ExprBody,
        prefix: &[PatId],
        rest: Option<&ArrayRestPat>,
        suffix: &[PatId],
        ascription: Option<&Ty>,
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
        let ascription = ascription.and_then(|ascription| self.written(ascription));
        let applies = |member: &baml_type::Ty| match &ascription {
            Some(ascription) => {
                matches!(member, baml_type::Ty::List(_)) && self.is_subtype(member, ascription)
            }
            None => matches!(member, baml_type::Ty::List(_)) || holds_any_shape(member),
        };
        let mut branches = self.member_branches(col, &applies, &build);
        match branches.len() {
            // Not a union, or a union whose lists the pattern does not
            // claim: the slice stands in the column itself.
            0 => build(col),
            1 => branches.pop().expect("one branch"),
            _ => DPat::or(branches, col.clone()),
        }
    }

    /// The `UnionMember` branches of a union column for a structural
    /// pattern: one per member it `applies` to, each built against that
    /// member's own column. A member whose column is itself a union (a
    /// recursive alias the canonical form keeps folded) is searched in turn.
    /// Empty for a column that is not a union.
    fn member_branches(
        &self,
        col: &ColumnTy,
        applies: &dyn Fn(&baml_type::Ty) -> bool,
        build: &dyn Fn(&ColumnTy) -> DPat,
    ) -> Vec<DPat> {
        if !matches!(col.ty(), baml_type::Ty::Union(_)) {
            return Vec::new();
        }
        let mut branches = Vec::new();
        for ctor in self.enumerate_ctors(col) {
            let Ctor::UnionMember(member) = &ctor else {
                continue;
            };
            let [member_col] = <[ColumnTy; 1]>::try_from(ctor_sub_tys(self, &ctor, col))
                .expect("a union member has one sub-column");
            let inner = if matches!(member_col.ty(), baml_type::Ty::Union(_)) {
                let mut nested = self.member_branches(&member_col, applies, build);
                match nested.len() {
                    0 => continue,
                    1 => nested.pop().expect("one branch"),
                    _ => DPat::or(nested, member_col.clone()),
                }
            } else if applies(member_col.ty()) {
                build(&member_col)
            } else {
                continue;
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
