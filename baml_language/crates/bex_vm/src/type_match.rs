//! Runtime `IsType` matching against a [`TyTemplate`], using the canonical type
//! algebra (`baml_type::normalize`) over the running program — the [`BexVm`]
//! itself is the [`baml_type::normalize::TypeContext`].
//!
//! This is the value-directed successor to the tag/pointer `IsType` fast paths
//! and the type-directed `guard_template_matches`: given a VM value, a template
//! (which may carry frame references and `Wildcard` holes), and the enclosing
//! frame's realized `type_args`, it answers "is this value a member of the type
//! the template denotes".
//!
//! # Relation
//!
//! The leaf relation is the canonical `baml_type::normalize` algebra — invariant
//! generic arguments, interface-membership- and alias-aware — never the
//! deprecated context-free [`RuntimeTy::is_subtype_of`] fork. It is applied with
//! a [`Variance`]:
//!
//! - **top level** (a value against the arm type) is *covariant* membership
//!   (`is_subtype`): a value belongs to `int | string` iff its concrete type is
//!   a subtype;
//! - **generic-argument positions** (a list element, a class type-arg) are
//!   *invariant* (`equivalent`): `int[]` is not `string[]`, and `Foo<int>` is
//!   not `Foo<string>`.
//!
//! `Wildcard` holes match anything at their position, which is why a hole-free
//! subtree short-circuits to a single canonical relation while a subtree
//! containing a hole is walked structurally down to it. The walk descends only
//! into positions whose variance is covariant (top level) or invariant (generic
//! arguments); function types — whose parameters are contravariant and whose
//! subtyping is arity-sensitive — are never hand-walked, so a holey function arm
//! fails closed while a hole-free one is left to the canonical algebra.
//!
//! Live: the emitter routes element-discriminating containers, unions, and
//! frame refs through `ConstValue::Type` to [`value_matches_template`], and the
//! `ClassWithTypeArgs` `IsType` check relates its per-arg positions through
//! [`class_type_arg_matches`] — both over this same canonical algebra.

use baml_type::{RealizedTy, Ty, TyTemplate, normalize};
use bex_vm_types::Value;

use crate::BexVm;

/// The variance at a template position the structural walk visits. Only two
/// arise here:
///
/// - **`Covariant`** — top-level value membership (`is_subtype`): a value
///   belongs to an arm iff its concrete type is a subtype.
/// - **`Invariant`** — a generic-argument position (a list element, a class
///   type-arg): BAML generics are invariant, so `Foo<A>` relates to `Foo<B>`
///   only when `A` and `B` are equivalent.
///
/// There is deliberately no `Contravariant`. Contravariance arises for function
/// *parameters*, but this walk never descends into a function type (see the
/// `Function` arm of [`template_relates`]) — so no contravariant, nor any
/// variance-*composing*, relation is ever needed. Invariance moreover *absorbs*
/// any incoming variance (invariant ∘ anything = invariant), which is why the
/// generic-argument positions can pass a fixed `Invariant` without composing it
/// with the variance they were reached under. A hole-free function subtree is
/// handed whole to the canonical algebra, which applies parameter contravariance
/// (and the rest of function subtyping) itself.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Variance {
    Covariant,
    Invariant,
}

/// Whether `value` is a member of the type denoted by `template`, with the
/// enclosing frame's realized `type_args` resolving the template's frame
/// references. The `IsType` value matcher for `match` and `is` expressions.
pub(crate) fn value_matches_template(
    vm: &BexVm,
    value: Value,
    template: &TyTemplate,
    frame_type_args: &[RealizedTy],
) -> bool {
    // A value with no concrete BAML type (function/closure/future/…) is a member
    // of no structural type test.
    let Some(value_ty) = vm.value_concrete_ty(value) else {
        return false;
    };
    // The canonical algebra operates over `Ty`; a value's concrete type widens
    // into it (a shallow structural conversion — `ConcreteRealizedTy` is not a
    // deep, transmute-compatible family member with a borrowed upcast).
    let value_ty: Ty = value_ty.into();
    template_relates(
        vm,
        template,
        frame_type_args,
        &value_ty,
        Variance::Covariant,
    )
}

/// Whether `actual` relates to the type denoted by `template` (resolved against
/// `frame_type_args`) under `variance`.
///
/// A hole-free subtree is compared as a whole by the canonical relation
/// (`is_subtype` for covariant, `equivalent` for invariant). A subtree that
/// contains a `Wildcard` is walked structurally so the hole can match anything
/// at its own position while the rest is still related canonically.
///
/// The `ClassWithTypeArgs` `IsType` check calls this directly with
/// [`Variance::Invariant`] for each class type-arg (`self` as the `TypeContext`),
/// which is why it is `pub(crate)`.
pub(crate) fn template_relates<C: normalize::TypeContext>(
    ctx: &C,
    template: &TyTemplate,
    frame_type_args: &[RealizedTy],
    actual: &Ty,
    variance: Variance,
) -> bool {
    if !template.contains_wildcard() {
        // Resolve frame references (and reduce any projection) into a realized
        // type, then let the canonical algebra do the work. A template that does
        // not realize against the frame's realized args matches nothing.
        let Ok(expected) = template.substitute(frame_type_args, ctx) else {
            return false;
        };
        return relate(ctx, actual, expected.as_ty(), variance);
    }

    // Holey subtree: match the actual's shape structurally, recursing into
    // generic-argument positions invariantly, until each `Wildcard` is reached.
    match template {
        TyTemplate::Wildcard => true,
        TyTemplate::List(inner, _) => match actual {
            Ty::List(actual_inner, _) => template_relates(
                ctx,
                inner,
                frame_type_args,
                actual_inner,
                Variance::Invariant,
            ),
            _ => false,
        },
        TyTemplate::Map { key, value, .. } => match actual {
            Ty::Map {
                key: actual_key,
                value: actual_value,
                ..
            } => {
                template_relates(ctx, key, frame_type_args, actual_key, Variance::Invariant)
                    && template_relates(
                        ctx,
                        value,
                        frame_type_args,
                        actual_value,
                        Variance::Invariant,
                    )
            }
            _ => false,
        },
        TyTemplate::Class(name, args, _) => match actual {
            Ty::Class(actual_name, actual_args, _) => {
                name == actual_name
                    && args.len() == actual_args.len()
                    && args.iter().zip(actual_args).all(|(arg, actual_arg)| {
                        template_relates(ctx, arg, frame_type_args, actual_arg, Variance::Invariant)
                    })
            }
            _ => false,
        },
        TyTemplate::Future(value, error, _) => match actual {
            Ty::Future(actual_value, actual_error, _) => {
                template_relates(
                    ctx,
                    value,
                    frame_type_args,
                    actual_value,
                    Variance::Invariant,
                ) && template_relates(
                    ctx,
                    error,
                    frame_type_args,
                    actual_error,
                    Variance::Invariant,
                )
            }
            _ => false,
        },
        // A union at the top level matches if any member matches (covariant); as
        // an invariant argument it must match member-for-member, but a
        // value's concrete type is never itself a union at an argument position
        // that a hole could distinguish, so the any-member rule is sufficient
        // here and stays conservative.
        TyTemplate::Union(parts, _) => parts
            .iter()
            .any(|part| template_relates(ctx, part, frame_type_args, actual, variance)),
        // A function type's parameters are *contravariant*, and its subtyping is
        // subtle: required params match positionally, optional params match by
        // name, and return/throws are covariant (see `normalize`'s
        // `list_subtype`). Rather than hand-roll that — plus the variance
        // composition contravariant positions would force — for the rare holey
        // function arm, fail closed: a hole inside a function type matches
        // nothing. Hole-free function arms never reach here; they take the fast
        // path into the canonical algebra, which applies all of the above.
        TyTemplate::Function { .. } => false,
        // Interface and projection templates with holes have no structural
        // runtime membership check yet (Unit-5 work); a hole there matches
        // nothing until then.
        TyTemplate::Interface(..) | TyTemplate::AssociatedTypeProjection { .. } => false,
        // Frame references and realized leaves contain no `Wildcard`, so the
        // hole-free fast path above already handled them; they cannot be the top
        // of a holey subtree.
        _ => unreachable!("hole-free template handled by the fast path"),
    }
}

/// Apply the canonical relation at `variance`: covariant positions use
/// membership (`is_subtype`), invariant positions use equality (`equivalent`).
fn relate<C: normalize::TypeContext>(
    ctx: &C,
    actual: &Ty,
    expected: &Ty,
    variance: Variance,
) -> bool {
    match variance {
        Variance::Covariant => normalize::is_subtype(actual, expected, ctx),
        Variance::Invariant => normalize::equivalent(actual, expected, ctx),
    }
}

#[cfg(test)]
mod tests {
    use baml_type::{
        Interface, Name, QualifiedTypeName, RealizedTy, RuntimeTy, TyTemplate, TypeName,
        normalize::TypeContext,
    };

    use super::{Variance, template_relates};

    /// A fail-safe, context-free [`TypeContext`]: no aliases, no interface
    /// memberships, no bounds. It exercises the matcher's *structural* algebra
    /// (invariant generic args, literal widening, union membership, `Wildcard`
    /// holes) — the parts that don't need program facts. Nominal facts (a class
    /// implementing an interface) are validated by the VM-backed e2e tests.
    struct EmptyCtx;
    impl TypeContext for EmptyCtx {
        fn alias_def(&self, _: &QualifiedTypeName) -> Option<baml_type::Ty> {
            None
        }
        fn implements_interface(&self, _: &baml_type::Ty, _: &Interface) -> bool {
            false
        }
        fn type_var_bound(&self, _: &Name) -> Vec<Interface> {
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

    /// `template_relates` over the empty context, covariant at the top level.
    /// The frame args are written as `RuntimeTy` for test ergonomics and narrowed
    /// to the realized frame the matcher takes (they are all concrete here).
    fn matches(template: &TyTemplate, frame: &[RuntimeTy], actual: &RuntimeTy) -> bool {
        let frame: Vec<RealizedTy> = frame
            .iter()
            .map(|t| RealizedTy::try_from(t).expect("test frame arg is realized"))
            .collect();
        template_relates(
            &EmptyCtx,
            template,
            &frame,
            actual.as_ty(),
            Variance::Covariant,
        )
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
    fn wildcard_matches_anything() {
        let w = TyTemplate::Wildcard;
        assert!(matches(&w, &[], &RuntimeTy::int()));
        assert!(matches(&w, &[], &RuntimeTy::list(RuntimeTy::string())));
    }

    #[test]
    fn class_with_wildcard_arg() {
        // `Foo<_>` matches every `Foo` instantiation, but not another class.
        let foo = user_class("Foo");
        let bar = user_class("Bar");
        let foo_any = TyTemplate::class(foo.clone(), vec![TyTemplate::Wildcard]);
        assert!(matches(
            &foo_any,
            &[],
            &RuntimeTy::class_with_args(foo.clone(), vec![RuntimeTy::int()])
        ));
        assert!(matches(
            &foo_any,
            &[],
            &RuntimeTy::class_with_args(foo, vec![RuntimeTy::string()])
        ));
        assert!(!matches(
            &foo_any,
            &[],
            &RuntimeTy::class_with_args(bar, vec![RuntimeTy::int()])
        ));
    }

    #[test]
    fn map_with_wildcard_value() {
        // `map<string, _>` matches any value type but requires a string key.
        let m = TyTemplate::map(leaf(RealizedTy::string()), TyTemplate::Wildcard);
        assert!(matches(
            &m,
            &[],
            &RuntimeTy::map(RuntimeTy::string(), RuntimeTy::int())
        ));
        assert!(matches(
            &m,
            &[],
            &RuntimeTy::map(RuntimeTy::string(), RuntimeTy::list(RuntimeTy::bool()))
        ));
        assert!(!matches(
            &m,
            &[],
            &RuntimeTy::map(RuntimeTy::int(), RuntimeTy::int())
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
}
