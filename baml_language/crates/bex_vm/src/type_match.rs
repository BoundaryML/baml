//! Runtime `IsType` matching against a [`TyTemplate`], using the canonical type
//! algebra (`baml_type::normalize`) over the running program — the [`BexVm`]
//! itself is the [`baml_type::normalize::TypeContext`].
//!
//! A `match`/`is` pattern template is *complete*: `TyTemplate` has no match-any
//! holes, so given the enclosing frame's realized `type_args` it denotes
//! exactly one type. [`value_matches_template`] therefore substitutes and asks
//! the canonical algebra one covariant membership question (`is_subtype`): a
//! value belongs to `int | string` iff its concrete type is a subtype, and
//! generic argument positions are related invariantly *by the algebra itself*
//! (`int[]` is not `string[]`, `Foo<int>` is not `Foo<string>`).
//!
//! The `ClassWithTypeArgs` fast path (class-pointer identity fixes the class,
//! then each type-arg position is checked separately) uses the same machinery
//! per argument via [`class_type_arg_matches`] — invariantly, since BAML
//! generics are invariant.

use baml_type::normalize;
use bex_vm_types::{RealizedTy, Ty, TyTemplate, Value, errors::VmInternalError};

use crate::BexVm;

/// Whether `value` is a member of the type denoted by `template`, with the
/// enclosing frame's realized `type_args` resolving the template's frame
/// references. The `IsType` value matcher for `match` and `is` expressions.
///
/// A substitution failure is a broken compiler/VM invariant, surfaced as an
/// internal error rather than silently mis-answering the test: a frame
/// reference the seeded frame does not supply is a frame-layout bug, and a
/// pattern template's projection reducing opaquely breaks the registry's
/// completeness guarantee (every baked impl rule carries a binding for every
/// declared associated member — pinned or baked from its declared default).
pub(crate) fn value_matches_template(
    vm: &BexVm,
    value: Value,
    template: &TyTemplate,
    frame_type_args: &[RealizedTy],
) -> Result<bool, VmInternalError> {
    // A value with no reconstructible concrete BAML type (an opaque native
    // handle, or a compile-time definition object — see `value_concrete_ty`) is
    // a member of no structural type test. Every *data* value reconstructs
    // faithfully, including the callables: a closure, generic function, or
    // bound method materializes its stored signature templates against the
    // realized frame the value carries (a bound method's drops the applied
    // receiver), so one minted in a generic frame is as precise as any other;
    // futures reconstruct at the `Future<T, E>` their spawn site was typed at.
    //
    // The reconstruction is the singleton-precise one: a literal type holds a
    // single value, so nothing decides membership in it short of the value
    // reporting the type of exactly itself.
    let Some(value_ty) = vm.value_singleton_ty(value) else {
        return Ok(false);
    };
    // The canonical algebra operates over `Ty`; a value's realized type widens
    // into it.
    let value_ty: Ty = value_ty.into();
    // Resolve frame references into a realized type, then let the canonical
    // algebra do the work.
    let expected = template.substitute(frame_type_args, vm).map_err(|e| {
        VmInternalError::TypeSubstitution {
            message: e.to_string(),
        }
    })?;
    Ok(normalize::is_subtype(&value_ty, expected.as_ty(), vm))
}

/// Whether `actual` is *invariantly* the type denoted by `template` (resolved
/// against `frame_type_args`) — one class type-arg position of the
/// `ClassWithTypeArgs` check. BAML generics are invariant, so the relation is
/// canonical equivalence, not membership. A substitution failure is a broken
/// invariant, exactly as in [`value_matches_template`].
pub(crate) fn class_type_arg_matches<H: baml_type::Head, C: normalize::TypeContext<H>>(
    ctx: &C,
    template: &baml_type::TyTemplate<H>,
    frame_type_args: &[baml_type::RealizedTy<H>],
    actual: &baml_type::Ty<H>,
) -> Result<bool, VmInternalError> {
    let expected = template.substitute(frame_type_args, ctx).map_err(|e| {
        VmInternalError::TypeSubstitution {
            message: e.to_string(),
        }
    })?;
    Ok(normalize::equivalent(actual, expected.as_ty(), ctx))
}

#[cfg(test)]
mod tests {
    use baml_type::{
        Interface, Name, ParamTy, QualifiedTypeName, RealizedTy, RuntimeTy, TyTemplate, TypeName,
        normalize::TypeContext,
    };

    use super::class_type_arg_matches;

    /// A fail-safe, context-free [`TypeContext`]: no aliases, no interface
    /// memberships, no bounds. It exercises the matcher's *structural* algebra
    /// (invariant generic args, literal widening, union membership) — the parts
    /// that don't need program facts. Nominal facts (a class implementing an
    /// interface) are validated by the VM-backed e2e tests.
    struct EmptyCtx;
    impl TypeContext for EmptyCtx {
        /// A name-based context represents a declaration by its own name, so this
        /// is the identity — no resolution step, and never `None`.
        fn head_lookup(
            &self,
            qtn: &baml_type::QualifiedTypeName,
        ) -> Option<baml_type::QualifiedTypeName> {
            Some(qtn.clone())
        }

        fn alias_def(&self, _: &QualifiedTypeName) -> Option<baml_type::Ty> {
            None
        }
        fn implements_interface(&self, _: &baml_type::Ty, _: &Interface) -> bool {
            false
        }
        fn type_var_bound(&self, _: &ParamTy) -> Vec<Interface> {
            Vec::new()
        }
        fn interface_requires(&self, _: &Interface, _: &Interface) -> bool {
            false
        }
        fn enum_variants(&self, _: &QualifiedTypeName) -> Option<Vec<Name>> {
            None
        }
        fn associated_type_bound(&self, _: &Interface, _: Name) -> Vec<Interface> {
            // Context-free: no interface declarations, so no declared bounds.
            Vec::new()
        }
        fn project(
            &self,
            _: &baml_type::Ty,
            _: &Interface,
            _: &Name,
            _fuel: u32,
        ) -> baml_type::normalize::ProjectionStep {
            // Context-free: no impls to reduce through; projections stay opaque.
            baml_type::normalize::ProjectionStep::Opaque
        }
    }

    /// The complete-template value relation over the empty context: substitute
    /// against the frame, then covariant membership — mirroring
    /// `value_matches_template` past the VM-specific concrete-type read. The
    /// frame args are written as `RuntimeTy` for test ergonomics and narrowed
    /// to the realized frame the matcher takes (they are all concrete here).
    fn matches(template: &TyTemplate, frame: &[RuntimeTy], actual: &RuntimeTy) -> bool {
        let frame: Vec<RealizedTy> = frame
            .iter()
            .map(|t| RealizedTy::try_from(t).expect("test frame arg is realized"))
            .collect();
        let Ok(expected) = template.substitute(&frame, &EmptyCtx) else {
            return false;
        };
        baml_type::normalize::is_subtype(actual.as_ty(), expected.as_ty(), &EmptyCtx)
    }

    /// The invariant class-arg relation over the empty context
    /// (`ClassWithTypeArgs`'s per-arg check).
    fn arg_matches(template: &TyTemplate, frame: &[RuntimeTy], actual: &RuntimeTy) -> bool {
        let frame: Vec<RealizedTy> = frame
            .iter()
            .map(|t| RealizedTy::try_from(t).expect("test frame arg is realized"))
            .collect();
        class_type_arg_matches(&EmptyCtx, template, &frame, actual.as_ty())
            .expect("test templates reference only supplied frame slots")
    }

    /// A realized-leaf template from a `RealizedTy`.
    fn leaf(ty: RealizedTy) -> TyTemplate {
        TyTemplate::from(ty)
    }

    fn user_class(name: &str) -> TypeName {
        TypeName::local(Name::new(name))
    }

    #[test]
    fn list_discriminates_element_type() {
        let int_list_pat = TyTemplate::list(leaf(RealizedTy::int()));
        assert!(matches(
            &int_list_pat,
            &[],
            &RuntimeTy::list(RuntimeTy::int())
        ));
        // `int[]` must NOT match a `string[]` value — element position is invariant.
        assert!(!matches(
            &int_list_pat,
            &[],
            &RuntimeTy::list(RuntimeTy::string())
        ));
        // Nor a bare `int`.
        assert!(!matches(&int_list_pat, &[], &RuntimeTy::int()));
    }

    #[test]
    fn map_discriminates_value_type() {
        let m_int = TyTemplate::map(leaf(RealizedTy::string()), leaf(RealizedTy::int()));
        assert!(matches(
            &m_int,
            &[],
            &RuntimeTy::map(RuntimeTy::string(), RuntimeTy::int())
        ));
        assert!(!matches(
            &m_int,
            &[],
            &RuntimeTy::map(RuntimeTy::string(), RuntimeTy::string())
        ));
    }

    #[test]
    fn class_type_args_are_invariant() {
        let tn = user_class("Foo");
        let foo_int = TyTemplate::class(tn.clone(), vec![leaf(RealizedTy::int())]);
        assert!(matches(
            &foo_int,
            &[],
            &RuntimeTy::class_with_args(tn.clone(), vec![RuntimeTy::int()])
        ));
        assert!(!matches(
            &foo_int,
            &[],
            &RuntimeTy::class_with_args(tn, vec![RuntimeTy::string()])
        ));
    }

    #[test]
    fn type_arg_ref_realizes_frame_slot() {
        // `T` arm with the frame binding `T = int`: an `int` value matches, a
        // `string` value does not.
        let t = TyTemplate::TypeArgRef(0);
        assert!(matches(&t, &[RuntimeTy::int()], &RuntimeTy::int()));
        assert!(!matches(&t, &[RuntimeTy::int()], &RuntimeTy::string()));
    }

    #[test]
    fn type_arg_ref_membership_is_covariant_at_top_level() {
        // `T = int | string`: an `int` value is a *member* of the union arm.
        let t = TyTemplate::TypeArgRef(0);
        let frame = [RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()])];
        assert!(matches(&t, &frame, &RuntimeTy::int()));
        assert!(matches(&t, &frame, &RuntimeTy::string()));
        assert!(!matches(&t, &frame, &RuntimeTy::bool()));
    }

    #[test]
    fn list_of_type_arg_ref() {
        // `T[]` with `T = int`.
        let t_list = TyTemplate::list(TyTemplate::TypeArgRef(0));
        assert!(matches(
            &t_list,
            &[RuntimeTy::int()],
            &RuntimeTy::list(RuntimeTy::int())
        ));
        assert!(!matches(
            &t_list,
            &[RuntimeTy::int()],
            &RuntimeTy::list(RuntimeTy::string())
        ));
    }

    #[test]
    fn union_top_level_membership() {
        // A bare `int | string` arm: an `int` value is a member; a `bool` is not.
        let u = TyTemplate::union([leaf(RealizedTy::int()), leaf(RealizedTy::string())]);
        assert!(matches(&u, &[], &RuntimeTy::int()));
        assert!(matches(&u, &[], &RuntimeTy::string()));
        assert!(!matches(&u, &[], &RuntimeTy::bool()));
    }

    #[test]
    fn literal_widens_into_base() {
        // A value of literal type `1` is a member of the `int` arm.
        let int_arm = leaf(RealizedTy::int());
        let one = RuntimeTy::Literal(
            baml_type::Literal::Int(1),
            baml_type::Freshness::Regular,
            baml_type::TyAttr::default(),
        );
        assert!(matches(&int_arm, &[], &one));
    }

    /// The algebra half of literal membership: the relation
    /// `ConstValue::Literal` specializes must give the same answers the
    /// specialization does, or the fast path and the general path disagree.
    ///
    /// Each case below is exactly one `value_is_literal` arm: a singleton
    /// admits itself, no other value of its base type, and — the B-1073 case —
    /// nothing from a neighbouring rung of the numeric tower, which the `==`
    /// operator deliberately widens across and this relation deliberately does
    /// not.
    #[test]
    fn literal_membership_agrees_with_algebra() {
        fn lit(l: baml_type::Literal) -> RuntimeTy {
            RuntimeTy::Literal(
                l,
                baml_type::Freshness::Regular,
                baml_type::TyAttr::default(),
            )
        }
        fn lit_realized(l: baml_type::Literal) -> RealizedTy {
            RealizedTy::Literal(
                l,
                baml_type::Freshness::Regular,
                baml_type::TyAttr::default(),
            )
        }
        let one = leaf(lit_realized(baml_type::Literal::Int(1)));

        // A singleton admits exactly itself.
        assert!(matches(&one, &[], &lit(baml_type::Literal::Int(1))));
        assert!(!matches(&one, &[], &lit(baml_type::Literal::Int(2))));

        // ...and nothing from another rung of the tower, at either precision.
        assert!(!matches(&one, &[], &RuntimeTy::float()));
        assert!(!matches(
            &one,
            &[],
            &lit(baml_type::Literal::Bigint(1.into()))
        ));
        assert!(!matches(&one, &[], &lit(baml_type::Literal::Bool(true))));

        // The base type is not a member of its own singleton: `int` includes
        // values other than `1`. This is why the value matcher must reconstruct
        // the *singleton* type of a value (`value_singleton_ty`) rather than
        // its base — with the base it reports, no value inhabits any literal.
        assert!(!matches(&one, &[], &RuntimeTy::int()));
    }

    // ── Class-arg (invariant) checks — the `ClassWithTypeArgs` per-arg relation ──

    #[test]
    fn class_arg_is_invariant_not_covariant() {
        // A concrete arg is equivalence, not membership: `int` relates to
        // `int`, but NOT to a wider union it is merely a member of.
        let int_arg = leaf(RealizedTy::int());
        assert!(arg_matches(&int_arg, &[], &RuntimeTy::int()));
        assert!(!arg_matches(&int_arg, &[], &RuntimeTy::string()));
        let union_arg = TyTemplate::union([leaf(RealizedTy::int()), leaf(RealizedTy::string())]);
        assert!(!arg_matches(&union_arg, &[], &RuntimeTy::int()));
    }

    #[test]
    fn class_arg_frame_ref_resolves_and_relates_invariantly() {
        // `Foo<#0>`'s arg with `#0 = int`: the instance's stored arg must be
        // equivalent to the frame's binding.
        let t = TyTemplate::TypeArgRef(0);
        assert!(arg_matches(&t, &[RuntimeTy::int()], &RuntimeTy::int()));
        assert!(!arg_matches(&t, &[RuntimeTy::int()], &RuntimeTy::string()));
        // A widened frame binding still relates by *equivalence*: the algebra
        // absorbs subsumed members, so `int | int` ≡ `int`, but `int | string`
        // is not equivalent to `int`.
        let frame = [RuntimeTy::union([RuntimeTy::int(), RuntimeTy::string()])];
        assert!(!arg_matches(&t, &frame, &RuntimeTy::int()));
    }
}
