//! Unit tests for the canonical type algebra, driven by a hand-built
//! [`TypeContext`] stub that records nominal facts directly.

use std::collections::HashMap;

use super::*;
use crate::{Freshness, FunctionParamTy, Literal, Name, QualifiedTypeName, Ty, TyAttr};

// ── stub context ───────────────────────────────────────────────────────────

#[derive(Default)]
struct Ctx {
    aliases: HashMap<QualifiedTypeName, Ty>,
    /// `(concrete nominal head, interface head)` membership facts.
    impls: Vec<(QualifiedTypeName, QualifiedTypeName)>,
    /// `(primitive name, interface head)` membership facts (e.g. `int: Compare`).
    prim_impls: Vec<(&'static str, QualifiedTypeName)>,
    /// `(interface head, required interface head)` direct requirements.
    requires: Vec<(QualifiedTypeName, QualifiedTypeName)>,
    /// Conjunction (`T: A + B`) bounds per type variable.
    var_bounds: HashMap<Name, Vec<Ty>>,
    enums: HashMap<QualifiedTypeName, Vec<Name>>,
    /// Declared `extends` bounds per `(interface head, associated-type name)`.
    assoc_bounds: HashMap<(QualifiedTypeName, Name), Vec<Ty>>,
    /// `(base, interface head, member) → reduced type` projection facts, for the
    /// `project` oracle. A `Vec` (not a map) because `Ty` is not `Hash`.
    projections: Vec<(Ty, QualifiedTypeName, Name, Ty)>,
}

fn nominal_head(ty: &Ty) -> Option<QualifiedTypeName> {
    match ty {
        Ty::Class(q, ..) | Ty::Interface(q, ..) | Ty::Enum(q, _) | Ty::EnumVariant(q, ..) => {
            Some(q.clone())
        }
        _ => None,
    }
}

fn primitive_name(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Int { .. } => Some("int"),
        Ty::Bigint { .. } => Some("bigint"),
        Ty::Float { .. } => Some("float"),
        Ty::String { .. } => Some("string"),
        Ty::Bool { .. } => Some("bool"),
        _ => None,
    }
}

impl TypeContext for Ctx {
    fn alias_def(&self, name: &QualifiedTypeName) -> Option<Ty> {
        self.aliases.get(name).cloned()
    }

    fn implements_interface(&self, concrete: &Ty, interface: &Interface) -> bool {
        let iface = &interface.name;
        if let Some(c) = nominal_head(concrete) {
            return self.impls.iter().any(|(cc, ii)| *cc == c && ii == iface);
        }
        if let Some(p) = primitive_name(concrete) {
            return self
                .prim_impls
                .iter()
                .any(|(pp, ii)| *pp == p && ii == iface);
        }
        false
    }

    fn type_var_bound(&self, name: &Name) -> Vec<Interface> {
        self.var_bounds
            .get(name)
            .into_iter()
            .flatten()
            .filter_map(Ty::as_interface)
            .collect()
    }

    fn interface_requires(&self, sub: &Interface, sup: &Interface) -> bool {
        let (a, b) = (&sub.name, &sup.name);
        a == b || self.requires.iter().any(|(x, y)| x == a && y == b)
    }

    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>> {
        self.enums.get(name).cloned()
    }

    fn associated_type_bound(&self, interface: &Interface, assoc: Name) -> Vec<Interface> {
        self.assoc_bounds
            .get(&(interface.name.clone(), assoc))
            .into_iter()
            .flatten()
            .filter_map(Ty::as_interface)
            .collect()
    }

    fn project(
        &self,
        base: &Ty,
        interface: &Interface,
        member: &Name,
        _fuel: u32,
    ) -> ProjectionStep {
        self.projections
            .iter()
            .find(|(b, i, m, _)| b == base && i == &interface.name && m == member)
            .map_or(ProjectionStep::Opaque, |(_, _, _, reduced)| {
                ProjectionStep::Reduced(reduced.clone())
            })
    }
}

// ── constructors ─────────────────────────────────────────────────────────--

fn qtn(s: &str) -> QualifiedTypeName {
    QualifiedTypeName::local(Name::new(s))
}
fn class(s: &str) -> Ty {
    Ty::Class(qtn(s), vec![], TyAttr::default())
}
fn class1(s: &str, arg: Ty) -> Ty {
    Ty::Class(qtn(s), vec![arg], TyAttr::default())
}
fn iface(s: &str) -> Ty {
    Ty::Interface(qtn(s), vec![], vec![], TyAttr::default())
}
fn enum_ty(s: &str) -> Ty {
    Ty::Enum(qtn(s), TyAttr::default())
}
fn variant(e: &str, v: &str) -> Ty {
    Ty::EnumVariant(qtn(e), Name::new(v), TyAttr::default())
}
fn lit_int(n: i64) -> Ty {
    Ty::Literal(Literal::Int(n), Freshness::Regular, TyAttr::default())
}
fn union(v: Vec<Ty>) -> Ty {
    Ty::Union(v, TyAttr::default())
}
fn typevar(s: &str) -> Ty {
    Ty::TypeVar(Name::new(s), TyAttr::default())
}
fn projection(base: Ty, iface_name: &str, member: &str) -> Ty {
    Ty::AssociatedTypeProjection {
        base: Box::new(base),
        interface: Box::new(Interface::new(qtn(iface_name), vec![], vec![])),
        member: Name::new(member),
        attr: TyAttr::default(),
    }
}

// ── projection reduction ─────────────────────────────────────────────────--

#[test]
fn projection_reduces_to_its_binding() {
    // `(C as Foo).Assoc` *is* the type the oracle reduces it to — a pure type-level
    // operator, so its canonical form is the reduced type.
    let ctx = Ctx {
        projections: vec![(class("C"), qtn("Foo"), Name::new("Assoc"), Ty::string())],
        ..Ctx::default()
    };
    assert!(equivalent(
        &projection(class("C"), "Foo", "Assoc"),
        &Ty::string(),
        &ctx,
    ));
}

#[test]
fn cyclic_projection_reduction_terminates_and_stays_opaque() {
    // `(C as I).A → (C as J).B → (C as I).A → …`: fuel-bounded, so normalization
    // terminates (this test completing proves it) and the projection stays opaque —
    // never wrongly equated to a concrete type.
    let ctx = Ctx {
        projections: vec![
            (
                class("C"),
                qtn("I"),
                Name::new("A"),
                projection(class("C"), "J", "B"),
            ),
            (
                class("C"),
                qtn("J"),
                Name::new("B"),
                projection(class("C"), "I", "A"),
            ),
        ],
        ..Ctx::default()
    };
    assert!(!equivalent(
        &projection(class("C"), "I", "A"),
        &Ty::int(),
        &ctx,
    ));
}

// ── union algebra ────────────────────────────────────────────────────────--

#[test]
fn union_aci() {
    let ctx = Ctx::default();
    // Commutativity.
    assert!(equivalent(
        &union(vec![Ty::int(), Ty::string()]),
        &union(vec![Ty::string(), Ty::int()]),
        &ctx,
    ));
    // Idempotence.
    assert!(equivalent(
        &union(vec![Ty::int(), Ty::int()]),
        &Ty::int(),
        &ctx
    ));
    // Associativity / flattening.
    assert!(equivalent(
        &union(vec![Ty::int(), union(vec![Ty::string(), Ty::bool()])]),
        &union(vec![union(vec![Ty::int(), Ty::string()]), Ty::bool()]),
        &ctx,
    ));
}

#[test]
fn never_is_removed_unknown_absorbs() {
    let ctx = Ctx::default();
    assert!(equivalent(
        &union(vec![
            Ty::int(),
            Ty::Never {
                attr: TyAttr::default()
            }
        ]),
        &Ty::int(),
        &ctx,
    ));
    assert!(equivalent(
        &union(vec![
            Ty::int(),
            Ty::BuiltinUnknown {
                attr: TyAttr::default()
            }
        ]),
        &Ty::BuiltinUnknown {
            attr: TyAttr::default()
        },
        &ctx,
    ));
}

#[test]
fn literal_types_are_subtypes_of_their_base_only() {
    // A literal type is a member of its base primitive's value set — a free,
    // representation-preserving widening (TYPE_SYSTEM.md §BAML Subtyping Cases) —
    // and of any union/optional containing it. It does NOT cross representations:
    // `42 <: float` would be an int→float coercion, which is not a subtype
    // relation.
    let ctx = Ctx::default();
    assert!(is_subtype(&lit_int(42), &Ty::int(), &ctx));
    assert!(is_subtype(&lit_float("3.14"), &Ty::float(), &ctx));
    assert!(is_subtype(
        &Ty::Literal(
            Literal::String("hello".to_string()),
            Freshness::Regular,
            TyAttr::default(),
        ),
        &Ty::string(),
        &ctx
    ));
    assert!(is_subtype(
        &Ty::Literal(Literal::Bool(true), Freshness::Regular, TyAttr::default()),
        &Ty::bool(),
        &ctx
    ));
    assert!(!is_subtype(&lit_int(42), &Ty::float(), &ctx));
    // Union / optional membership.
    assert!(is_subtype(
        &lit_int(42),
        &union(vec![Ty::string(), Ty::int()]),
        &ctx
    ));
    assert!(is_subtype(&lit_int(42), &Ty::optional(Ty::int()), &ctx));
    assert!(is_subtype(&Ty::null(), &Ty::optional(Ty::string()), &ctx));
}

#[test]
fn numeric_types_do_not_widen_across_representations() {
    // Concrete types are atomic: `int` (i64) is not a subtype of `float` (f64 —
    // precision loss past 2^53) nor of `bigint` (heap representation); those
    // conversions are explicit or FFI-boundary coercions, never subtyping. The
    // same holds under an (invariant) container.
    let ctx = Ctx::default();
    assert!(!is_subtype(&Ty::int(), &Ty::float(), &ctx));
    assert!(!is_subtype(&Ty::int(), &bigint(), &ctx));
    assert!(!is_subtype(&Ty::list(Ty::int()), &Ty::list(bigint()), &ctx));
}

#[test]
fn literal_into_base_absorption() {
    let ctx = Ctx::default();
    // `int | 99 == int`.
    assert!(equivalent(
        &union(vec![lit_int(99), Ty::int()]),
        &Ty::int(),
        &ctx
    ));
    assert_eq!(
        normalize(&union(vec![lit_int(99), Ty::int()]), &ctx),
        Ty::int()
    );
    // A bare literal does not absorb into a different base.
    assert!(!equivalent(
        &union(vec![lit_int(99), Ty::string()]),
        &Ty::int(),
        &ctx
    ));
}

#[test]
fn optional_is_union_with_null() {
    let ctx = Ctx::default();
    let null = Ty::Null {
        attr: TyAttr::default(),
    };
    assert!(equivalent(
        &Ty::optional(Ty::int()),
        &union(vec![Ty::int(), null]),
        &ctx,
    ));
}

// ── enums ──────────────────────────────────────────────────────────────────

#[test]
fn enum_completeness_collapse() {
    let mut ctx = Ctx::default();
    ctx.enums
        .insert(qtn("Side"), vec![Name::new("Left"), Name::new("Right")]);

    // All variants present → the enum.
    assert!(equivalent(
        &union(vec![variant("Side", "Left"), variant("Side", "Right")]),
        &enum_ty("Side"),
        &ctx,
    ));
    // An incomplete set does NOT collapse.
    assert!(!equivalent(
        &variant("Side", "Left"),
        &enum_ty("Side"),
        &ctx
    ));
    // A bare enum absorbs its own variants.
    assert!(equivalent(
        &union(vec![variant("Side", "Left"), enum_ty("Side")]),
        &enum_ty("Side"),
        &ctx,
    ));
}

#[test]
fn unknown_enum_does_not_collapse() {
    // No `enum_variants` fact registered → fail-safe: keep the variants.
    let ctx = Ctx::default();
    assert!(!equivalent(
        &union(vec![variant("Side", "Left"), variant("Side", "Right")]),
        &enum_ty("Side"),
        &ctx,
    ));
}

// ── interface absorption ─────────────────────────────────────────────────--

#[test]
fn concrete_implements_interface() {
    let mut ctx = Ctx::default();
    ctx.impls.push((qtn("Dog"), qtn("Animal")));

    assert!(is_subtype(&class("Dog"), &iface("Animal"), &ctx));
    // `Dog | Animal == Animal`.
    assert!(equivalent(
        &union(vec![class("Dog"), iface("Animal")]),
        &iface("Animal"),
        &ctx,
    ));
    // A non-implementor is not absorbed.
    assert!(!is_subtype(&class("Rock"), &iface("Animal"), &ctx));
    assert!(!equivalent(
        &union(vec![class("Rock"), iface("Animal")]),
        &iface("Animal"),
        &ctx,
    ));
}

#[test]
fn interface_requires_absorption() {
    let mut ctx = Ctx::default();
    ctx.requires.push((qtn("Compare"), qtn("Equals")));

    assert!(is_subtype(&iface("Compare"), &iface("Equals"), &ctx));
    // `Compare | Equals == Equals`.
    assert!(equivalent(
        &union(vec![iface("Compare"), iface("Equals")]),
        &iface("Equals"),
        &ctx,
    ));
    assert!(!is_subtype(&iface("Equals"), &iface("Compare"), &ctx));
}

#[test]
fn type_var_bound_absorption() {
    let mut ctx = Ctx::default();
    ctx.var_bounds.insert(Name::new("T"), vec![iface("Animal")]);

    assert!(is_subtype(&typevar("T"), &iface("Animal"), &ctx));
    // `T | Animal == Animal` when `T: Animal`.
    assert!(equivalent(
        &union(vec![typevar("T"), iface("Animal")]),
        &iface("Animal"),
        &ctx,
    ));
    // A different type variable is not absorbed and not equivalent.
    assert!(!equivalent(&typevar("T"), &typevar("U"), &ctx));
}

#[test]
fn type_var_conjunction_bound() {
    // `T: Animal + Compare` (a Rust-style `+` conjunction), and `Compare`
    // transitively requires `Equals`.
    let mut ctx = Ctx::default();
    ctx.var_bounds
        .insert(Name::new("T"), vec![iface("Animal"), iface("Compare")]);
    ctx.requires.push((qtn("Compare"), qtn("Equals")));

    // Each conjunct is provable on its own…
    assert!(is_subtype(&typevar("T"), &iface("Animal"), &ctx));
    assert!(is_subtype(&typevar("T"), &iface("Compare"), &ctx));
    // …including transitively through a conjunct's `requires`.
    assert!(is_subtype(&typevar("T"), &iface("Equals"), &ctx));
    // An interface that no conjunct provides is not a supertype.
    assert!(!is_subtype(&typevar("T"), &iface("Serialize"), &ctx));

    // Union absorption follows the same per-conjunct rule.
    assert!(equivalent(
        &union(vec![typevar("T"), iface("Equals")]),
        &iface("Equals"),
        &ctx,
    ));
    assert!(!equivalent(
        &union(vec![typevar("T"), iface("Serialize")]),
        &iface("Serialize"),
        &ctx,
    ));
}

// ── invariant generics ───────────────────────────────────────────────────--

#[test]
fn generics_are_invariant_up_to_equivalence() {
    let ctx = Ctx::default();
    assert!(!equivalent(
        &class1("Box", Ty::int()),
        &class1("Box", Ty::string()),
        &ctx
    ));
    // Equivalent argument spellings make the containers equivalent.
    assert!(equivalent(
        &class1("Box", union(vec![lit_int(1), Ty::int()])),
        &class1("Box", Ty::int()),
        &ctx,
    ));
    assert!(!is_subtype(
        &class1("Box", Ty::int()),
        &class1("Box", Ty::string()),
        &ctx
    ));
}

// ── subtype basics ───────────────────────────────────────────────────────--

#[test]
fn subtype_basics() {
    let ctx = Ctx::default();
    let never = Ty::Never {
        attr: TyAttr::default(),
    };
    let unknown = Ty::BuiltinUnknown {
        attr: TyAttr::default(),
    };

    assert!(is_subtype(&lit_int(1), &Ty::int(), &ctx));
    assert!(is_subtype(&variant("Side", "Left"), &enum_ty("Side"), &ctx));
    assert!(is_subtype(&never, &Ty::int(), &ctx));
    assert!(is_subtype(&Ty::int(), &unknown, &ctx));
    assert!(!is_subtype(&unknown, &Ty::int(), &ctx));

    // No numeric coercion across representations.
    assert!(!is_subtype(
        &Ty::int(),
        &Ty::Bigint {
            attr: TyAttr::default()
        },
        &ctx
    ));

    // Union decomposition.
    assert!(is_subtype(
        &Ty::int(),
        &union(vec![Ty::int(), Ty::string()]),
        &ctx
    ));
    assert!(is_subtype(
        &union(vec![Ty::int(), Ty::string()]),
        &union(vec![Ty::string(), Ty::int(), Ty::bool()]),
        &ctx,
    ));
    assert!(!is_subtype(
        &union(vec![Ty::int(), Ty::string()]),
        &Ty::int(),
        &ctx
    ));
}

// ── subtyping: variance, holes, unions (flip de-risk) ─────────────────────--

#[test]
fn invariant_arg_distinguishes_top_from_recovery_hole() {
    // The exact divergence the subtyping migration relies on. Generics are
    // invariant (TYPE_SYSTEM.md §Variance), so the genuine top type `unknown`
    // (`BuiltinUnknown`) is invariant-distinct: `Box<unknown>` is NOT `Box<int>`.
    // The error-recovery sentinel (`Unknown`) is different — it stays
    // bidirectionally compatible, so a recovered `Box<Unknown>` never cascades a
    // subtype error. Keeping the two apart is what lets error recovery use the
    // recovery sentinel while `unknown` keeps its sound invariant identity.
    let ctx = Ctx::default();
    let top = Ty::BuiltinUnknown {
        attr: TyAttr::default(),
    };
    let hole = Ty::Unknown {
        attr: TyAttr::default(),
    };

    assert!(!is_subtype(
        &class1("Box", top.clone()),
        &class1("Box", Ty::int()),
        &ctx
    ));
    assert!(!is_subtype(
        &class1("Box", Ty::int()),
        &class1("Box", top),
        &ctx
    ));

    assert!(is_subtype(
        &class1("Box", hole.clone()),
        &class1("Box", Ty::int()),
        &ctx
    ));
    assert!(is_subtype(
        &class1("Box", Ty::int()),
        &class1("Box", hole),
        &ctx
    ));
}

#[test]
fn function_subtyping_is_contravariant_in_params_covariant_in_return() {
    // TYPE_SYSTEM.md §Variance: `foo: (int | string) -> bool throws never` is a
    // subtype of `(int) -> bool | float throws never` — the parameter is
    // contravariant (`int <: int | string`) and the return covariant
    // (`bool <: bool | float`).
    let ctx = Ctx::default();
    let never = Ty::Never {
        attr: TyAttr::default(),
    };
    let float = Ty::Float {
        attr: TyAttr::default(),
    };
    let foo = Ty::Function {
        params: vec![FunctionParamTy::required(
            None,
            union(vec![Ty::int(), Ty::string()]),
        )],
        ret: Box::new(Ty::bool()),
        throws: Box::new(never.clone()),
        attr: TyAttr::default(),
    };
    let expected = Ty::Function {
        params: vec![FunctionParamTy::required(None, Ty::int())],
        ret: Box::new(union(vec![Ty::bool(), float])),
        throws: Box::new(never),
        attr: TyAttr::default(),
    };
    assert!(is_subtype(&foo, &expected, &ctx));
    // The reverse fails: `int | string` (foo's param) is not <: `int`, so the
    // contravariant direction rejects it.
    assert!(!is_subtype(&expected, &foo, &ctx));
}

#[test]
fn function_throws_is_covariant() {
    // Error type is covariant (TYPE_SYSTEM.md, Subtyping Rules → Variance): a
    // function that throws `never` is a subtype of one that throws `int`, but not
    // the reverse.
    let ctx = Ctx::default();
    let mk = |throws: Ty| Ty::Function {
        params: vec![],
        ret: Box::new(Ty::string()),
        throws: Box::new(throws),
        attr: TyAttr::default(),
    };
    let never = Ty::Never {
        attr: TyAttr::default(),
    };
    assert!(is_subtype(&mk(never.clone()), &mk(Ty::int()), &ctx));
    assert!(!is_subtype(&mk(Ty::int()), &mk(never), &ctx));
}

#[test]
fn function_required_param_names_are_insignificant() {
    // Required params are positional; their names are not part of the type
    // (TYPE_SYSTEM.md, Subtyping Rules), so two functions differing only in a
    // required param's name are equivalent (a named vs unnamed one too).
    let ctx = Ctx::default();
    let mk = |name: Option<&str>| Ty::Function {
        params: vec![FunctionParamTy::required(name.map(Name::new), Ty::int())],
        ret: Box::new(Ty::string()),
        throws: Box::new(Ty::Never {
            attr: TyAttr::default(),
        }),
        attr: TyAttr::default(),
    };
    assert!(equivalent(&mk(Some("a")), &mk(Some("b")), &ctx));
    assert!(equivalent(&mk(Some("a")), &mk(None), &ctx));
}

#[test]
fn function_optional_params_follow_the_superset_rule() {
    // Optional params (BEP-033) are matched by name, and the *subtype* declares a
    // superset of them — a subset function type accepts more optional args
    // (TYPE_SYSTEM.md, Subtyping Rules). Their order is insignificant.
    let ctx = Ctx::default();
    let func = |params: Vec<FunctionParamTy>| Ty::Function {
        params,
        ret: Box::new(Ty::string()),
        throws: Box::new(Ty::Never {
            attr: TyAttr::default(),
        }),
        attr: TyAttr::default(),
    };
    let req = FunctionParamTy::required(None, Ty::string());
    let opt_max = FunctionParamTy::optional(Some(Name::new("max")), Ty::int());
    let opt_filter = FunctionParamTy::optional(Some(Name::new("filter")), Ty::string());

    let two = func(vec![req.clone(), opt_max.clone(), opt_filter.clone()]);
    let one = func(vec![req.clone(), opt_filter.clone()]);

    // More optionals <: fewer optionals; the reverse fails (missing `max`).
    assert!(is_subtype(&two, &one, &ctx));
    assert!(!is_subtype(&one, &two, &ctx));

    // Optional order does not matter: reordered forms are mutual subtypes.
    let two_reordered = func(vec![req, opt_filter, opt_max]);
    assert!(is_subtype(&two, &two_reordered, &ctx));
    assert!(is_subtype(&two_reordered, &two, &ctx));
}

#[test]
fn function_optional_and_required_params_are_incomparable() {
    // A single-required-param function and a single-optional-param function are
    // unrelated: the required-arity check (equal counts) already fails both ways.
    let ctx = Ctx::default();
    let mk = |optional: bool| Ty::Function {
        params: vec![if optional {
            FunctionParamTy::optional(Some(Name::new("value")), Ty::int())
        } else {
            FunctionParamTy::required(Some(Name::new("value")), Ty::int())
        }],
        ret: Box::new(Ty::string()),
        throws: Box::new(Ty::Never {
            attr: TyAttr::default(),
        }),
        attr: TyAttr::default(),
    };
    assert!(!is_subtype(&mk(true), &mk(false), &ctx));
    assert!(!is_subtype(&mk(false), &mk(true), &ctx));
}

#[test]
fn interface_membership_through_unions() {
    let mut ctx = Ctx::default();
    ctx.impls.push((qtn("Dog"), qtn("Animal")));
    ctx.impls.push((qtn("Cat"), qtn("Animal")));

    // A concrete implementor is a subtype of an interface wrapped in a union.
    assert!(is_subtype(
        &class("Dog"),
        &union(vec![iface("Animal"), Ty::null()]),
        &ctx,
    ));
    // A union of implementors is a subtype of the interface (left-union rule).
    assert!(is_subtype(
        &union(vec![class("Dog"), class("Cat")]),
        &iface("Animal"),
        &ctx,
    ));
    // …but `null` is not a member of the bare interface.
    assert!(!is_subtype(
        &Ty::optional(class("Dog")),
        &iface("Animal"),
        &ctx
    ));
}

#[test]
fn type_var_is_reflexive_independent_of_its_bound() {
    // A type variable is a subtype of itself, of a union containing itself, and
    // of its own optional — by identity, no bound needed. (The legacy oracle
    // special-cased this before bound expansion; the canonical algebra gets it
    // from reflexivity + the right-union rule.)
    let ctx = Ctx::default();
    assert!(is_subtype(&typevar("T"), &typevar("T"), &ctx));
    assert!(is_subtype(
        &typevar("T"),
        &union(vec![typevar("T"), typevar("U")]),
        &ctx,
    ));
    assert!(is_subtype(&typevar("T"), &Ty::optional(typevar("T")), &ctx));
    // An unbounded `T` is not a subtype of an unrelated concrete type.
    assert!(!is_subtype(&typevar("T"), &Ty::int(), &ctx));
}

#[test]
fn symbolic_associated_type_projection_subtypes_via_its_bound() {
    // `interface Iter { type Item extends Summarizable }`, with `Summarizable`
    // transitively requiring `Displayable`. A still-symbolic projection
    // `(T as Iter).Item` (the base is a type variable, so it can't be resolved
    // to a concrete type) is a subtype of its declared bound's supertypes — the
    // projection analogue of a type-var bound.
    let mut ctx = Ctx::default();
    ctx.assoc_bounds.insert(
        (qtn("Iter"), Name::new("Item")),
        vec![iface("Summarizable")],
    );
    ctx.requires.push((qtn("Summarizable"), qtn("Displayable")));

    let proj = |interface: Interface| Ty::AssociatedTypeProjection {
        base: Box::new(typevar("T")),
        interface: Box::new(interface),
        member: Name::new("Item"),
        attr: TyAttr::default(),
    };
    let iter_proj = proj(
        iface("Iter")
            .as_interface()
            .expect("iface() builds an interface existential"),
    );

    assert!(is_subtype(&iter_proj, &iface("Summarizable"), &ctx));
    // …including transitively through the bound's `requires`.
    assert!(is_subtype(&iter_proj, &iface("Displayable"), &ctx));
    // Not a subtype of an interface the bound doesn't provide.
    assert!(!is_subtype(&iter_proj, &iface("Unrelated"), &ctx));
    // Reflexivity still holds (the projection is equal to itself).
    assert!(is_subtype(&iter_proj, &iter_proj, &ctx));
}

// ── aliases & recursion ──────────────────────────────────────────────────--

#[test]
fn non_recursive_alias_expands() {
    let mut ctx = Ctx::default();
    ctx.aliases.insert(qtn("MyInt"), Ty::int());
    assert!(equivalent(
        &Ty::TypeAlias(qtn("MyInt"), TyAttr::default()),
        &Ty::int(),
        &ctx
    ));
}

#[test]
fn unresolved_alias_is_opaque() {
    let ctx = Ctx::default();
    let a = Ty::TypeAlias(qtn("Missing"), TyAttr::default());
    // Equal only to itself; never equated to a concrete type (fail-safe).
    assert!(equivalent(&a, &a, &ctx));
    assert!(!equivalent(&a, &Ty::int(), &ctx));
}

#[test]
fn recursive_alias_terminates() {
    // `Tree = int | Box<Tree>` — normalization and subtyping must not diverge.
    let mut ctx = Ctx::default();
    let tree = qtn("Tree");
    ctx.aliases.insert(
        tree.clone(),
        union(vec![
            Ty::int(),
            class1("Box", Ty::TypeAlias(tree.clone(), TyAttr::default())),
        ]),
    );
    let alias = Ty::TypeAlias(tree.clone(), TyAttr::default());

    assert!(equivalent(&alias, &alias, &ctx));
    assert!(is_subtype(&alias, &alias, &ctx));
    // The recursive alias is not equivalent to a flat unrelated type.
    assert!(!equivalent(&alias, &Ty::int(), &ctx));
}

// ── compare-style exact-type use (the motivating case) ───────────────────--

#[test]
fn catch_result_union_equals_base() {
    // `(... catch ...) < 0` widens to `int | 99` vs `int`; exact-type ordering
    // needs these to be equivalent so the comparison is accepted.
    let ctx = Ctx::default();
    assert!(equivalent(
        &union(vec![Ty::int(), lit_int(99)]),
        &Ty::int(),
        &ctx
    ));
}

// ── concrete-type disjointness (`==` always-false fold) ──────────────────--

fn bigint() -> Ty {
    Ty::Bigint {
        attr: TyAttr::default(),
    }
}

#[test]
fn disjoint_distinct_primitives() {
    let ctx = Ctx::default();
    // `5 == 5n`: an int literal and bigint never share a concrete type.
    assert!(definitely_disjoint(&lit_int(5), &bigint(), &ctx));
    assert!(definitely_disjoint(&Ty::int(), &bigint(), &ctx));
    assert!(definitely_disjoint(&Ty::int(), &Ty::string(), &ctx));
    assert!(definitely_disjoint(&lit_int(1), &Ty::string(), &ctx));
    // Distinct primitive literals are disjoint (unoverridable built-in equality).
    assert!(definitely_disjoint(&lit_int(1), &lit_int(2), &ctx));
    assert!(definitely_disjoint(&lit_str("a"), &lit_str("b"), &ctx));
    // Same family (incl. literal inside its base, equal literals) → not disjoint.
    assert!(!definitely_disjoint(&Ty::int(), &Ty::int(), &ctx));
    assert!(!definitely_disjoint(&lit_int(1), &Ty::int(), &ctx));
    assert!(!definitely_disjoint(&lit_int(1), &lit_int(1), &ctx));
    // Floats are excluded from literal disjointness (NaN / decimal aliasing),
    // but still disjoint across categories.
    assert!(!definitely_disjoint(
        &lit_float("1.5"),
        &lit_float("2.5"),
        &ctx
    ));
    assert!(definitely_disjoint(&lit_float("1.5"), &Ty::string(), &ctx));
}

#[test]
fn disjoint_invariant_generic_classes() {
    let ctx = Ctx::default();
    // Invariant: distinct instantiations are distinct runtime types.
    assert!(definitely_disjoint(
        &class1("Box", Ty::int()),
        &class1("Box", Ty::string()),
        &ctx
    ));
    assert!(!definitely_disjoint(
        &class1("Box", Ty::int()),
        &class1("Box", Ty::int()),
        &ctx
    ));
    // `unknown` is the determined top type, so `Box<unknown>` is a distinct
    // invariant instantiation from `Box<int>` → disjoint.
    let unknown = Ty::BuiltinUnknown {
        attr: TyAttr::default(),
    };
    assert!(definitely_disjoint(
        &class1("Box", unknown),
        &class1("Box", Ty::int()),
        &ctx
    ));
    // A not-yet-resolved generic argument could realize to match → not provable.
    assert!(!definitely_disjoint(
        &class1("Box", typevar("T")),
        &class1("Box", Ty::int()),
        &ctx
    ));
}

fn map_ty(key: Ty, value: Ty) -> Ty {
    Ty::Map {
        key: Box::new(key),
        value: Box::new(value),
        attr: TyAttr::default(),
    }
}

#[test]
fn containers_are_invariant_even_when_the_element_is_a_genuine_subtype() {
    // The memory-corruption example (TYPE_SYSTEM.md §Variance): `Dog <: Animal`
    // does NOT make `list<Dog> <: list<Animal>`. Element subtyping would let a
    // caller holding the `list<Animal>` view store a non-`Dog` into a `list<Dog>`,
    // so the containers are invariant — related only when the elements are mutual
    // subtypes (equivalent). This is the exact rule value-checking now enforces by
    // routing through this relation instead of the legacy element-covariant one.
    let mut ctx = Ctx::default();
    ctx.impls.push((qtn("Dog"), qtn("Animal")));
    assert!(is_subtype(&class("Dog"), &iface("Animal"), &ctx));

    assert!(!is_subtype(
        &Ty::list(class("Dog")),
        &Ty::list(iface("Animal")),
        &ctx,
    ));
    assert!(!is_subtype(
        &map_ty(Ty::string(), class("Dog")),
        &map_ty(Ty::string(), iface("Animal")),
        &ctx,
    ));
    // A generic class is invariant in its argument for the same reason.
    assert!(!is_subtype(
        &class1("Box", class("Dog")),
        &class1("Box", iface("Animal")),
        &ctx,
    ));
    // Reflexive same-instantiation still holds.
    assert!(is_subtype(
        &Ty::list(class("Dog")),
        &Ty::list(class("Dog")),
        &ctx,
    ));
}

#[test]
fn disjoint_containers_are_invariant() {
    let ctx = Ctx::default();
    // Different constructors / categories → disjoint.
    assert!(definitely_disjoint(&Ty::list(Ty::int()), &Ty::int(), &ctx));
    assert!(definitely_disjoint(
        &Ty::list(Ty::int()),
        &map_ty(Ty::string(), Ty::int()),
        &ctx
    ));
    assert!(definitely_disjoint(
        &Ty::list(Ty::int()),
        &class("Dog"),
        &ctx
    ));
    // Invariant: distinct element/value instantiations are disjoint (type args
    // are real instance data — there is no empty-container overlap).
    assert!(definitely_disjoint(
        &Ty::list(Ty::int()),
        &Ty::list(Ty::string()),
        &ctx
    ));
    assert!(definitely_disjoint(
        &map_ty(Ty::string(), Ty::int()),
        &map_ty(Ty::string(), Ty::bool()),
        &ctx
    ));
    // Same instantiation → not disjoint.
    assert!(!definitely_disjoint(
        &Ty::list(Ty::int()),
        &Ty::list(Ty::int()),
        &ctx
    ));
    // `unknown` element is determined → `list<unknown>` is disjoint from `list<int>`.
    let list_unknown = Ty::list(Ty::BuiltinUnknown {
        attr: TyAttr::default(),
    });
    assert!(definitely_disjoint(
        &list_unknown,
        &Ty::list(Ty::int()),
        &ctx
    ));
    // A not-yet-resolved generic element → not provably disjoint.
    assert!(!definitely_disjoint(
        &Ty::list(typevar("T")),
        &Ty::list(Ty::int()),
        &ctx
    ));
}

#[test]
fn disjoint_distinct_classes_and_enums() {
    let ctx = Ctx::default();
    assert!(definitely_disjoint(&class("Dog"), &class("Cat"), &ctx));
    assert!(!definitely_disjoint(&class("Dog"), &class("Dog"), &ctx));
    assert!(definitely_disjoint(&class("Dog"), &Ty::int(), &ctx));
    assert!(definitely_disjoint(&enum_ty("Foo"), &enum_ty("Bar"), &ctx));
    assert!(!definitely_disjoint(&enum_ty("Foo"), &enum_ty("Foo"), &ctx));
}

#[test]
fn same_enum_variants_are_not_disjoint() {
    // `E.A` vs `E.B` share concrete enum `E`, so the result depends on `E`'s
    // equality (a custom `Equals` could make them equal) — decided at runtime,
    // never folded.
    let ctx = Ctx::default();
    assert!(!definitely_disjoint(
        &variant("Foo", "A"),
        &variant("Foo", "B"),
        &ctx
    ));
    assert!(!definitely_disjoint(
        &variant("Foo", "A"),
        &enum_ty("Foo"),
        &ctx
    ));
    // Variants of *different* enums are disjoint.
    assert!(definitely_disjoint(
        &variant("Foo", "A"),
        &variant("Bar", "X"),
        &ctx
    ));
}

#[test]
fn non_ground_types_are_never_disjoint() {
    let ctx = Ctx::default();
    let unknown = Ty::BuiltinUnknown {
        attr: TyAttr::default(),
    };
    assert!(!definitely_disjoint(&Ty::int(), &unknown, &ctx));
    assert!(!definitely_disjoint(&Ty::int(), &typevar("T"), &ctx));
    assert!(!definitely_disjoint(&Ty::int(), &iface("Animal"), &ctx));
    // A concrete vs an interface it might implement: not provably disjoint.
    assert!(!definitely_disjoint(&class("Dog"), &iface("Animal"), &ctx));
}

#[test]
fn disjoint_unions_require_all_cross_pairs() {
    let ctx = Ctx::default();
    let int_or_string = union(vec![Ty::int(), Ty::string()]);
    assert!(definitely_disjoint(&int_or_string, &Ty::bool(), &ctx));
    assert!(!definitely_disjoint(&int_or_string, &Ty::int(), &ctx));
    assert!(definitely_disjoint(
        &int_or_string,
        &union(vec![Ty::bool(), bigint()]),
        &ctx
    ));
    assert!(!definitely_disjoint(
        &int_or_string,
        &union(vec![Ty::string(), Ty::bool()]),
        &ctx
    ));
}

#[test]
fn disjoint_null() {
    let ctx = Ctx::default();
    let null = Ty::Null {
        attr: TyAttr::default(),
    };
    assert!(definitely_disjoint(&null, &Ty::int(), &ctx));
    assert!(!definitely_disjoint(&null, &null, &ctx));
}

// ── always-equal fold (`==` provably true) ───────────────────────────────--

fn lit_str(s: &str) -> Ty {
    Ty::Literal(
        Literal::String(s.to_string()),
        Freshness::Regular,
        TyAttr::default(),
    )
}
fn lit_bool(b: bool) -> Ty {
    Ty::Literal(Literal::Bool(b), Freshness::Regular, TyAttr::default())
}
fn lit_float(s: &str) -> Ty {
    Ty::Literal(
        Literal::Float(s.to_string()),
        Freshness::Regular,
        TyAttr::default(),
    )
}

#[test]
fn equal_same_primitive_literals_and_null() {
    let ctx = Ctx::default();
    let null = Ty::Null {
        attr: TyAttr::default(),
    };
    assert!(definitely_equal(&lit_int(1), &lit_int(1), &ctx));
    assert!(definitely_equal(&lit_str("a"), &lit_str("a"), &ctx));
    assert!(definitely_equal(&lit_bool(true), &lit_bool(true), &ctx));
    assert!(definitely_equal(&null, &null, &ctx));
    // Different values of the same primitive are not always-equal.
    assert!(!definitely_equal(&lit_int(1), &lit_int(2), &ctx));
    assert!(!definitely_equal(&lit_bool(true), &lit_bool(false), &ctx));
}

#[test]
fn equal_requires_singleton_with_unoverridable_eq() {
    let ctx = Ctx::default();
    // Base primitives are not singletons — `int == int` is not always true.
    assert!(!definitely_equal(&Ty::int(), &Ty::int(), &ctx));
    // A literal vs its base is not always-equal (the base value may differ).
    assert!(!definitely_equal(&lit_int(1), &Ty::int(), &ctx));
    // Float literals are excluded (NaN-safety), even when identical.
    assert!(!definitely_equal(
        &lit_float("1.5"),
        &lit_float("1.5"),
        &ctx
    ));
    // Enum-variant singletons are excluded: a dynamic package could add an
    // `Equals` to the enum after compilation.
    assert!(!definitely_equal(
        &variant("Foo", "A"),
        &variant("Foo", "A"),
        &ctx
    ));
    // Class singletons (and distinct values generally) are not folded.
    assert!(!definitely_equal(&class("Dog"), &class("Dog"), &ctx));
}
