// Copied from crates/baml_type/src/normalize/tests.rs: the tests of the algebra over realized
// types. A change to either file must be made to both.

//! Unit tests for the canonical type algebra, driven by a hand-built
//! [`TypeContext`] stub that records nominal facts directly.

use std::{cell::RefCell, collections::HashMap};

use super::*;
use crate::{
    layout::{TAG_ENUM_VARIANT, VARIANT_INDEX},
    ty::{CowSlice, Freshness, data::ObjectHeader},
};

/// The types under test, by the original's name.
type Ty = RealizedTy;

/// What a head is in this stub: the address of an object made for a name.
type Head = NonNull<u8>;

// ── stub context ───────────────────────────────────────────────────────────

#[derive(Default)]
struct Ctx {
    /// A recursive alias is defined in its type object, which this writes.
    aliases: Aliases,
    /// `(concrete nominal head, interface head)` membership facts.
    impls: Vec<(Head, Head)>,
    /// `(primitive name, interface head)` membership facts (e.g. `int: Compare`).
    prim_impls: Vec<(&'static str, Head)>,
    /// `(interface head, required interface head)` direct requirements.
    requires: Vec<(Head, Head)>,
    enums: HashMap<Head, Vec<NonNull<EnumVariantObject>>>,
}

#[derive(Default)]
struct Aliases;

impl Aliases {
    #[expect(
        clippy::unused_self,
        reason = "the original's tests insert into the context's alias map; here the type object is the map"
    )]
    fn insert(&mut self, name: Head, def: Ty) {
        // SAFETY: every head of this stub is a type object made by `qtn`.
        unsafe { (*name.cast::<TypeObject>().as_ptr()).ty = def }
    }
}

/// The enum of a variant object.
fn variant_enum(v: NonNull<EnumVariantObject>) -> Head {
    // SAFETY: a variant object holds its enum at `VARIANT_ENUM`.
    unsafe { *v.as_ptr().cast::<Head>().byte_add(VARIANT_ENUM) }
}

fn nominal_head(ty: &Ty) -> Option<Head> {
    match ty {
        Ty::Class(q, ..) | Ty::Enum(q) => Some(q.cast()),
        Ty::Interface(q, ..) => Some(q.cast()),
        Ty::EnumVariant(v) => Some(variant_enum(*v)),
        _ => None,
    }
}

fn primitive_name(ty: &Ty) -> Option<&'static str> {
    match ty {
        Ty::Int => Some("int"),
        Ty::Bigint => Some("bigint"),
        Ty::Float => Some("float"),
        Ty::String => Some("string"),
        Ty::Bool => Some("bool"),
        _ => None,
    }
}

impl TypeContext for Ctx {
    /// The stub's `reflect` declarations are the objects made for their names.
    fn well_known(&self, head: WellKnownHead) -> Option<NonNull<InterfaceObject>> {
        Some(
            match head {
                WellKnownHead::AnyFunction => qtn("reflect.AnyFunction"),
                WellKnownHead::AnyClass => qtn("reflect.AnyClass"),
            }
            .cast(),
        )
    }

    fn implements_interface(&self, concrete: &Ty, interface: &InterfaceBound<Ty>) -> bool {
        let iface = interface.interface.cast();
        if let Some(c) = nominal_head(concrete) {
            return self.impls.iter().any(|(cc, ii)| *cc == c && *ii == iface);
        }
        if let Some(p) = primitive_name(concrete) {
            return self
                .prim_impls
                .iter()
                .any(|(pp, ii)| *pp == p && *ii == iface);
        }
        false
    }

    fn interface_requires(&self, sub: &InterfaceBound<Ty>, sup: &InterfaceBound<Ty>) -> bool {
        let (a, b) = (sub.interface.cast(), sup.interface.cast());
        a == b || self.requires.iter().any(|(x, y)| *x == a && *y == b)
    }

    fn enum_variants(
        &self,
        name: &NonNull<ClassObject>,
    ) -> Option<Vec<NonNull<EnumVariantObject>>> {
        self.enums.get(&name.cast()).cloned()
    }

    fn project(
        &self,
        _base: &Ty,
        _interface: &InterfaceBound<Ty>,
        _member: AssocIndex,
        _fuel: u32,
    ) -> ProjectionStep {
        ProjectionStep::Opaque
    }
}

// ── constructors ─────────────────────────────────────────────────────────--

thread_local! {
    /// The object made for each name on this thread, and for each variant.
    static HEADS: RefCell<HashMap<String, Head>> = RefCell::new(HashMap::new());
    static VARIANTS: RefCell<HashMap<(String, String), NonNull<EnumVariantObject>>> =
        RefCell::new(HashMap::new());
}

/// The object a name stands for: a type object, which serves as a class, an
/// enum, an interface or an alias. Never freed.
fn qtn(s: &str) -> Head {
    HEADS.with(|heads| {
        *heads.borrow_mut().entry(s.to_owned()).or_insert_with(|| {
            let object = Box::new(TypeObject {
                header: ObjectHeader(0),
                ty: Ty::Never,
            });
            NonNull::from(Box::leak(object)).cast()
        })
    })
}
/// The object of variant `v` of enum `e`. Never freed.
fn variant_head(e: &str, v: &str) -> NonNull<EnumVariantObject> {
    VARIANTS.with(|variants| {
        *variants
            .borrow_mut()
            .entry((e.to_owned(), v.to_owned()))
            .or_insert_with(|| {
                let mut object = Box::new([0usize; 4]);
                object[0] = TAG_ENUM_VARIANT;
                object[VARIANT_ENUM / 8] = qtn(e).as_ptr() as usize;
                object[VARIANT_INDEX / 8] = 0;
                NonNull::from(Box::leak(object)).cast()
            })
    })
}
fn variants(e: &str, vs: &[&str]) -> Vec<NonNull<EnumVariantObject>> {
    vs.iter().map(|v| variant_head(e, v)).collect()
}
fn name(s: &str) -> Text {
    s.as_bytes().to_vec().into()
}
fn class(s: &str) -> Ty {
    Ty::Class(qtn(s).cast(), CowSlice::from(vec![]))
}
fn class1(s: &str, arg: Ty) -> Ty {
    Ty::Class(qtn(s).cast(), CowSlice::from(vec![arg]))
}
fn iface(s: &str) -> Ty {
    Ty::Interface(
        qtn(s).cast(),
        CowSlice::from(vec![]),
        CowSlice::from(vec![]),
    )
}
fn enum_ty(s: &str) -> Ty {
    Ty::Enum(qtn(s).cast())
}
fn variant(e: &str, v: &str) -> Ty {
    Ty::EnumVariant(variant_head(e, v))
}
fn lit_int(n: i64) -> Ty {
    Ty::Literal(Literal::Int(n), Freshness::Regular)
}
fn union(v: Vec<Ty>) -> Ty {
    Ty::Union(v.into())
}
fn list(t: Ty) -> Ty {
    Ty::List(CowBox::new(t))
}
fn optional(t: Ty) -> Ty {
    union(vec![t, Ty::Null])
}
fn alias(s: &str) -> Ty {
    Ty::TypeAlias(qtn(s).cast())
}
fn required(name: Option<Text>, ty: Ty) -> FunctionParamTy<Ty> {
    FunctionParamTy {
        name,
        ty,
        mode: FunctionParamMode::Required,
    }
}
fn optional_param(name: Option<Text>, ty: Ty) -> FunctionParamTy<Ty> {
    FunctionParamTy {
        name,
        ty,
        mode: FunctionParamMode::Optional,
    }
}

// ── union algebra ────────────────────────────────────────────────────────--

#[test]
fn union_aci() {
    let ctx = Ctx::default();
    // Commutativity.
    assert!(ctx.equivalent(
        &union(vec![Ty::Int, Ty::String]),
        &union(vec![Ty::String, Ty::Int])
    ));
    // Idempotence.
    assert!(ctx.equivalent(&union(vec![Ty::Int, Ty::Int]), &Ty::Int));
    // Associativity / flattening.
    assert!(ctx.equivalent(
        &union(vec![Ty::Int, union(vec![Ty::String, Ty::Bool])]),
        &union(vec![union(vec![Ty::Int, Ty::String]), Ty::Bool])
    ));
}

#[test]
fn never_is_removed_unknown_absorbs() {
    let ctx = Ctx::default();
    assert!(ctx.equivalent(&union(vec![Ty::Int, Ty::Never]), &Ty::Int));
    assert!(ctx.equivalent(&union(vec![Ty::Int, Ty::Unknown]), &Ty::Unknown));
}

#[test]
fn literal_types_are_subtypes_of_their_base_only() {
    // A literal type is a member of its base primitive's value set — a free,
    // representation-preserving widening (TYPE_SYSTEM.md §BAML Subtyping Cases) —
    // and of any union/optional containing it. It does NOT cross representations:
    // `42 <: float` would be an int→float coercion, which is not a subtype
    // relation.
    let ctx = Ctx::default();
    assert!(ctx.is_subtype(&lit_int(42), &Ty::Int));
    assert!(ctx.is_subtype(&lit_float("3.14"), &Ty::Float));
    assert!(ctx.is_subtype(
        &Ty::Literal(Literal::String(name("hello")), Freshness::Regular),
        &Ty::String
    ));
    assert!(ctx.is_subtype(
        &Ty::Literal(Literal::Bool(true), Freshness::Regular),
        &Ty::Bool
    ));
    assert!(!ctx.is_subtype(&lit_int(42), &Ty::Float));
    // Union / optional membership.
    assert!(ctx.is_subtype(&lit_int(42), &union(vec![Ty::String, Ty::Int])));
    assert!(ctx.is_subtype(&lit_int(42), &optional(Ty::Int)));
    assert!(ctx.is_subtype(&Ty::Null, &optional(Ty::String)));
}

#[test]
fn numeric_types_do_not_widen_across_representations() {
    // Concrete types are atomic: `int` (i64) is not a subtype of `float` (f64 —
    // precision loss past 2^53) nor of `bigint` (heap representation); those
    // conversions are explicit or FFI-boundary coercions, never subtyping. The
    // same holds under an (invariant) container.
    let ctx = Ctx::default();
    assert!(!ctx.is_subtype(&Ty::Int, &Ty::Float));
    assert!(!ctx.is_subtype(&Ty::Int, &bigint()));
    assert!(!ctx.is_subtype(&list(Ty::Int), &list(bigint())));
}

#[test]
fn literal_into_base_absorption() {
    let ctx = Ctx::default();
    // `int | 99 == int`.
    assert!(ctx.equivalent(&union(vec![lit_int(99), Ty::Int]), &Ty::Int));
    assert_eq!(ctx.normalize(&union(vec![lit_int(99), Ty::Int])), Ty::Int);
    // A bare literal does not absorb into a different base.
    assert!(!ctx.equivalent(&union(vec![lit_int(99), Ty::String]), &Ty::Int));
}

#[test]
fn optional_is_union_with_null() {
    let ctx = Ctx::default();
    let null = Ty::Null;
    assert!(ctx.equivalent(&optional(Ty::Int), &union(vec![Ty::Int, null])));
}

// ── enums ──────────────────────────────────────────────────────────────────

#[test]
fn enum_completeness_collapse() {
    let mut ctx = Ctx::default();
    ctx.enums
        .insert(qtn("Side"), variants("Side", &["Left", "Right"]));

    // All variants present → the enum.
    assert!(ctx.equivalent(
        &union(vec![variant("Side", "Left"), variant("Side", "Right")]),
        &enum_ty("Side")
    ));
    // An incomplete set does NOT collapse.
    assert!(!ctx.equivalent(&variant("Side", "Left"), &enum_ty("Side")));
    // A bare enum absorbs its own variants.
    assert!(ctx.equivalent(
        &union(vec![variant("Side", "Left"), enum_ty("Side")]),
        &enum_ty("Side")
    ));
}

#[test]
fn unknown_enum_does_not_collapse() {
    // No `enum_variants` fact registered → fail-safe: keep the variants.
    let ctx = Ctx::default();
    assert!(!ctx.equivalent(
        &union(vec![variant("Side", "Left"), variant("Side", "Right")]),
        &enum_ty("Side")
    ));
}

// ── interface absorption ─────────────────────────────────────────────────--

#[test]
fn concrete_implements_interface() {
    let mut ctx = Ctx::default();
    ctx.impls.push((qtn("Dog"), qtn("Animal")));

    assert!(ctx.is_subtype(&class("Dog"), &iface("Animal")));
    // `Dog | Animal == Animal`.
    assert!(ctx.equivalent(
        &union(vec![class("Dog"), iface("Animal")]),
        &iface("Animal")
    ));
    // A non-implementor is not absorbed.
    assert!(!ctx.is_subtype(&class("Rock"), &iface("Animal")));
    assert!(!ctx.equivalent(
        &union(vec![class("Rock"), iface("Animal")]),
        &iface("Animal")
    ));
}

#[test]
fn interface_requires_absorption() {
    let mut ctx = Ctx::default();
    ctx.requires.push((qtn("Compare"), qtn("Equals")));

    assert!(ctx.is_subtype(&iface("Compare"), &iface("Equals")));
    // `Compare | Equals == Equals`.
    assert!(ctx.equivalent(
        &union(vec![iface("Compare"), iface("Equals")]),
        &iface("Equals")
    ));
    assert!(!ctx.is_subtype(&iface("Equals"), &iface("Compare")));
}

// ── invariant generics ───────────────────────────────────────────────────--

#[test]
fn generics_are_invariant_up_to_equivalence() {
    let ctx = Ctx::default();
    assert!(!ctx.equivalent(&class1("Box", Ty::Int), &class1("Box", Ty::String)));
    // Equivalent argument spellings make the containers equivalent.
    assert!(ctx.equivalent(
        &class1("Box", union(vec![lit_int(1), Ty::Int])),
        &class1("Box", Ty::Int)
    ));
    assert!(!ctx.is_subtype(&class1("Box", Ty::Int), &class1("Box", Ty::String)));
}

// ── subtype basics ───────────────────────────────────────────────────────--

#[test]
fn subtype_basics() {
    let ctx = Ctx::default();
    let never = Ty::Never;
    let unknown = Ty::Unknown;

    assert!(ctx.is_subtype(&lit_int(1), &Ty::Int));
    assert!(ctx.is_subtype(&variant("Side", "Left"), &enum_ty("Side")));
    assert!(ctx.is_subtype(&never, &Ty::Int));
    assert!(ctx.is_subtype(&Ty::Int, &unknown));
    assert!(!ctx.is_subtype(&unknown, &Ty::Int));

    // No numeric coercion across representations.
    assert!(!ctx.is_subtype(&Ty::Int, &Ty::Bigint));

    // Union decomposition.
    assert!(ctx.is_subtype(&Ty::Int, &union(vec![Ty::Int, Ty::String])));
    assert!(ctx.is_subtype(
        &union(vec![Ty::Int, Ty::String]),
        &union(vec![Ty::String, Ty::Int, Ty::Bool])
    ));
    assert!(!ctx.is_subtype(&union(vec![Ty::Int, Ty::String]), &Ty::Int));
}

// ── subtyping: variance, holes, unions (flip de-risk) ─────────────────────--

#[test]
fn invariant_arg_distinguishes_top_from_recovery_sentinel() {
    // The exact divergence the subtyping migration relies on. Generics are
    // invariant (TYPE_SYSTEM.md §Variance), so the genuine top type `unknown`
    // (`Unknown`) is invariant-distinct: `Box<unknown>` is NOT `Box<int>`.
    // (The original's half on the error-recovery sentinel is not here: a
    // realized type has no sentinel.)
    let ctx = Ctx::default();
    let top = Ty::Unknown;

    assert!(!ctx.is_subtype(&class1("Box", top.clone()), &class1("Box", Ty::Int)));
    assert!(!ctx.is_subtype(&class1("Box", Ty::Int), &class1("Box", top)));
}

#[test]
fn function_subtyping_is_contravariant_in_params_covariant_in_return() {
    // TYPE_SYSTEM.md §Variance: `foo: (int | string) -> bool throws never` is a
    // subtype of `(int) -> bool | float throws never` — the parameter is
    // contravariant (`int <: int | string`) and the return covariant
    // (`bool <: bool | float`).
    let ctx = Ctx::default();
    let never = Ty::Never;
    let float = Ty::Float;
    let foo = Ty::Function {
        params: CowSlice::from(vec![required(None, union(vec![Ty::Int, Ty::String]))]),
        ret: CowBox::new(Ty::Bool),
        throws: CowBox::new(never.clone()),
    };
    let expected = Ty::Function {
        params: CowSlice::from(vec![required(None, Ty::Int)]),
        ret: CowBox::new(union(vec![Ty::Bool, float])),
        throws: CowBox::new(never),
    };
    assert!(ctx.is_subtype(&foo, &expected));
    // The reverse fails: `int | string` (foo's param) is not <: `int`, so the
    // contravariant direction rejects it.
    assert!(!ctx.is_subtype(&expected, &foo));
}

#[test]
fn function_throws_is_covariant() {
    // Error type is covariant (TYPE_SYSTEM.md, Subtyping Rules → Variance): a
    // function that throws `never` is a subtype of one that throws `int`, but not
    // the reverse.
    let ctx = Ctx::default();
    let mk = |throws: Ty| Ty::Function {
        params: CowSlice::from(vec![]),
        ret: CowBox::new(Ty::String),
        throws: CowBox::new(throws),
    };
    let never = Ty::Never;
    assert!(ctx.is_subtype(&mk(never.clone()), &mk(Ty::Int)));
    assert!(!ctx.is_subtype(&mk(Ty::Int), &mk(never)));
}

#[test]
fn function_required_param_names_are_insignificant() {
    // Required params are positional; their names are not part of the type
    // (TYPE_SYSTEM.md, Subtyping Rules), so two functions differing only in a
    // required param's name are equivalent (a named vs unnamed one too).
    let ctx = Ctx::default();
    let mk = |param: Option<&str>| Ty::Function {
        params: CowSlice::from(vec![required(param.map(name), Ty::Int)]),
        ret: CowBox::new(Ty::String),
        throws: CowBox::new(Ty::Never),
    };
    assert!(ctx.equivalent(&mk(Some("a")), &mk(Some("b"))));
    assert!(ctx.equivalent(&mk(Some("a")), &mk(None)));
}

#[test]
fn function_optional_params_follow_the_superset_rule() {
    // Optional params (BEP-033) are matched by name, and the *subtype* declares a
    // superset of them — a subset function type accepts more optional args
    // (TYPE_SYSTEM.md, Subtyping Rules). Their order is insignificant.
    let ctx = Ctx::default();
    let func = |params: Vec<FunctionParamTy<Ty>>| Ty::Function {
        params: params.into(),
        ret: CowBox::new(Ty::String),
        throws: CowBox::new(Ty::Never),
    };
    let req = required(None, Ty::String);
    let opt_max = optional_param(Some(name("max")), Ty::Int);
    let opt_filter = optional_param(Some(name("filter")), Ty::String);

    let two = func(vec![req.clone(), opt_max.clone(), opt_filter.clone()]);
    let one = func(vec![req.clone(), opt_filter.clone()]);

    // More optionals <: fewer optionals; the reverse fails (missing `max`).
    assert!(ctx.is_subtype(&two, &one));
    assert!(!ctx.is_subtype(&one, &two));

    // Optional order does not matter: reordered forms are mutual subtypes.
    let two_reordered = func(vec![req, opt_filter, opt_max]);
    assert!(ctx.is_subtype(&two, &two_reordered));
    assert!(ctx.is_subtype(&two_reordered, &two));
}

#[test]
fn function_optional_and_required_params_are_incomparable() {
    // A single-required-param function and a single-optional-param function are
    // unrelated: the required-arity check (equal counts) already fails both ways.
    let ctx = Ctx::default();
    let mk = |optional: bool| Ty::Function {
        params: CowSlice::from(vec![if optional {
            optional_param(Some(name("value")), Ty::Int)
        } else {
            required(Some(name("value")), Ty::Int)
        }]),
        ret: CowBox::new(Ty::String),
        throws: CowBox::new(Ty::Never),
    };
    assert!(!ctx.is_subtype(&mk(true), &mk(false)));
    assert!(!ctx.is_subtype(&mk(false), &mk(true)));
}

#[test]
fn interface_membership_through_unions() {
    let mut ctx = Ctx::default();
    ctx.impls.push((qtn("Dog"), qtn("Animal")));
    ctx.impls.push((qtn("Cat"), qtn("Animal")));

    // A concrete implementor is a subtype of an interface wrapped in a union.
    assert!(ctx.is_subtype(&class("Dog"), &union(vec![iface("Animal"), Ty::Null])));
    // A union of implementors is a subtype of the interface (left-union rule).
    assert!(ctx.is_subtype(&union(vec![class("Dog"), class("Cat")]), &iface("Animal")));
    // …but `null` is not a member of the bare interface.
    assert!(!ctx.is_subtype(&optional(class("Dog")), &iface("Animal")));
}

// ── aliases & recursion ──────────────────────────────────────────────────--

#[test]
fn non_recursive_alias_expands() {
    let mut ctx = Ctx::default();
    ctx.aliases.insert(qtn("MyInt"), Ty::Int);
    assert!(ctx.equivalent(&Ty::TypeAlias(qtn("MyInt").cast()), &Ty::Int));
}

#[test]
fn recursive_alias_terminates() {
    // `Tree = int | Box<Tree>` — normalization and subtyping must not diverge.
    let mut ctx = Ctx::default();
    let tree = qtn("Tree");
    ctx.aliases.insert(
        tree,
        union(vec![Ty::Int, class1("Box", Ty::TypeAlias(tree.cast()))]),
    );
    let alias = Ty::TypeAlias(tree.cast());

    assert!(ctx.equivalent(&alias, &alias));
    assert!(ctx.is_subtype(&alias, &alias));
    // The recursive alias is not equivalent to a flat unrelated type.
    assert!(!ctx.equivalent(&alias, &Ty::Int));
}

// ── bool completeness ────────────────────────────────────────────────────--

#[test]
fn complete_bool_literals_collapse_to_bool() {
    // `(true | false) == bool` (TYPE_SYSTEM.md §Subtyping Cases) — the bool
    // analogue of enum completeness, context-free.
    let ctx = Ctx::default();
    let both = union(vec![lit_bool(true), lit_bool(false)]);
    assert!(ctx.equivalent(&both, &Ty::Bool));
    assert!(ctx.is_subtype(&Ty::Bool, &both));
    assert!(ctx.is_subtype(&both, &Ty::Bool));
    assert!(ctx.equivalent(
        &union(vec![lit_bool(true), lit_bool(false), Ty::Int]),
        &union(vec![Ty::Bool, Ty::Int])
    ));
    // A single literal does not collapse (absorption into a present base still
    // applies).
    assert!(!ctx.equivalent(&lit_bool(true), &Ty::Bool));
    assert!(ctx.equivalent(&union(vec![Ty::Bool, lit_bool(true)]), &Ty::Bool));
    // Through recursion: the collapse applies inside μ-bodies too.
    let mut ctx = Ctx::default();
    ctx.aliases.insert(
        qtn("A"),
        union(vec![lit_bool(true), lit_bool(false), list(alias("A"))]),
    );
    ctx.aliases
        .insert(qtn("B"), union(vec![Ty::Bool, list(alias("B"))]));
    assert!(ctx.equivalent(&alias("A"), &alias("B")));
}

// ── equirecursive canonical forms ────────────────────────────────────────--

/// `type A = int | A[]` registered under the given name.
fn int_list_alias(ctx: &mut Ctx, name: &str) {
    ctx.aliases
        .insert(qtn(name), union(vec![Ty::Int, list(alias(name))]));
}

#[test]
fn alpha_equivalent_recursive_aliases() {
    // Recursive aliases are equirecursive: the alias name carries no identity,
    // so two aliases with the same definition denote the same type.
    let mut ctx = Ctx::default();
    int_list_alias(&mut ctx, "A");
    int_list_alias(&mut ctx, "B");
    assert!(ctx.equivalent(&alias("A"), &alias("B")));
    assert!(ctx.is_subtype(&alias("A"), &alias("B")));
    assert!(ctx.is_subtype(&alias("B"), &alias("A")));

    ctx.aliases.insert(qtn("R"), class1("Box", alias("R")));
    ctx.aliases.insert(qtn("S"), class1("Box", alias("S")));
    assert!(ctx.equivalent(&alias("R"), &alias("S")));
}

#[test]
fn recursive_alias_spelling_invariance() {
    // Every finite unfolding of a recursive alias spells the same regular tree.
    let mut ctx = Ctx::default();
    int_list_alias(&mut ctx, "A");
    let body = union(vec![Ty::Int, list(alias("A"))]);
    let twice = union(vec![Ty::Int, list(body.clone())]);
    assert!(ctx.equivalent(&alias("A"), &body));
    assert!(ctx.equivalent(&alias("A"), &twice));
    assert!(ctx.equivalent(&list(alias("A")), &list(body.clone())));
    assert!(ctx.equivalent(&class1("Box", alias("A")), &class1("Box", body)));
}

#[test]
fn mutually_recursive_aliases_are_equivalent() {
    // `type A = int | B[]` and `type B = int | A[]` unfold to one tree.
    let mut ctx = Ctx::default();
    ctx.aliases
        .insert(qtn("A"), union(vec![Ty::Int, list(alias("B"))]));
    ctx.aliases
        .insert(qtn("B"), union(vec![Ty::Int, list(alias("A"))]));
    assert!(ctx.equivalent(&alias("A"), &alias("B")));
    assert!(ctx.equivalent(&alias("A"), &union(vec![Ty::Int, list(alias("A"))])));
}

#[test]
fn unguarded_members_are_vacuous() {
    // Productivity: an unguarded recursive union member admits only vacuous
    // circular derivations, so it contributes nothing — `type A = A | A[]` is
    // `type B = B[]`, and a fully unguarded `type C = C` is uninhabited.
    let mut ctx = Ctx::default();
    ctx.aliases
        .insert(qtn("A"), union(vec![alias("A"), list(alias("A"))]));
    ctx.aliases.insert(qtn("B"), list(alias("B")));
    ctx.aliases.insert(qtn("C"), alias("C"));
    assert!(ctx.equivalent(&alias("A"), &alias("B")));
    assert!(ctx.equivalent(&alias("C"), &Ty::Never));
    // The soundness regression: before ε-closure, the assumption-set probe
    // fired on `string ≤ A` through the unguarded member without crossing a
    // constructor, wrongly answering `true`.
    assert!(!ctx.is_subtype(&Ty::String, &alias("A")));
    assert!(!ctx.is_subtype(&class("Dog"), &alias("A")));
    // The guarded content is still there.
    assert!(ctx.is_subtype(&list(alias("A")), &alias("A")));
}

#[test]
fn normalize_is_idempotent_on_recursive_aliases() {
    let mut ctx = Ctx::default();
    int_list_alias(&mut ctx, "A");
    ctx.aliases
        .insert(qtn("M"), union(vec![Ty::Int, list(alias("B"))]));
    ctx.aliases
        .insert(qtn("B"), union(vec![Ty::Int, list(alias("M"))]));
    ctx.aliases.insert(qtn("R"), class1("Box", alias("R")));
    let corpus = [
        alias("A"),
        list(alias("A")),
        class1("Box", alias("A")),
        alias("M"),
        alias("R"),
        union(vec![Ty::String, alias("A")]),
    ];
    for t in corpus {
        let once = ctx.normalize(&t);
        let twice = ctx.normalize(&once);
        assert_eq!(once, twice, "normalize must be idempotent for {t:?}");
        assert!(
            ctx.equivalent(&t, &once),
            "normalize must preserve identity for {t:?}"
        );
    }
}

#[test]
fn normalize_folds_nested_recursion_and_unfolds_the_root() {
    let mut ctx = Ctx::default();
    int_list_alias(&mut ctx, "A");
    // Root-unfold-once: a recursive alias at the root exposes its head
    // constructor, with the recursive occurrence folded back to the name.
    assert_eq!(
        ctx.normalize(&alias("A")),
        union(vec![Ty::Int, list(alias("A"))])
    );
    // Nested recursion stays folded: `A[]` renders as `A[]`, not the unfolding.
    assert_eq!(ctx.normalize(&list(alias("A"))), list(alias("A")));
    assert_eq!(
        ctx.normalize(&class1("Box", alias("A"))),
        class1("Box", alias("A"))
    );
    // The inline spelling converges to the same rendering as the alias.
    assert_eq!(
        ctx.normalize(&union(vec![Ty::Int, list(alias("A"))])),
        union(vec![Ty::Int, list(alias("A"))])
    );
    // An unguarded member vanishes from the rendering too.
    ctx.aliases
        .insert(qtn("U"), union(vec![alias("U"), list(alias("U"))]));
    assert_eq!(ctx.normalize(&alias("U")), list(alias("U")));
}

#[test]
fn absorption_completes_across_recursion() {
    // `type A = I | Box<A>` with `Box implements I`: the recursive member is
    // absorbed into the interface, and the μ-binder disappears with it.
    let mut ctx = Ctx::default();
    ctx.impls.push((qtn("Box"), qtn("I")));
    ctx.aliases
        .insert(qtn("A"), union(vec![iface("I"), class1("Box", alias("A"))]));
    assert!(ctx.equivalent(&alias("A"), &iface("I")));
    assert_eq!(ctx.normalize(&alias("A")), iface("I"));
}

// ── compare-style exact-type use (the motivating case) ───────────────────--

#[test]
fn catch_result_union_equals_base() {
    // `(... catch ...) < 0` widens to `int | 99` vs `int`; exact-type ordering
    // needs these to be equivalent so the comparison is accepted.
    let ctx = Ctx::default();
    assert!(ctx.equivalent(&union(vec![Ty::Int, lit_int(99)]), &Ty::Int));
}

// ── concrete-type disjointness (`==` always-false fold) ──────────────────--

fn bigint() -> Ty {
    Ty::Bigint
}

fn map_ty(key: Ty, value: Ty) -> Ty {
    Ty::Map {
        key: CowBox::new(key),
        value: CowBox::new(value),
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
    assert!(ctx.is_subtype(&class("Dog"), &iface("Animal")));

    assert!(!ctx.is_subtype(&list(class("Dog")), &list(iface("Animal"))));
    assert!(!ctx.is_subtype(
        &map_ty(Ty::String, class("Dog")),
        &map_ty(Ty::String, iface("Animal"))
    ));
    // A generic class is invariant in its argument for the same reason.
    assert!(!ctx.is_subtype(
        &class1("Box", class("Dog")),
        &class1("Box", iface("Animal"))
    ));
    // Reflexive same-instantiation still holds.
    assert!(ctx.is_subtype(&list(class("Dog")), &list(class("Dog"))));
}

// ── always-equal fold (`==` provably true) ───────────────────────────────--

fn lit_str(s: &str) -> Ty {
    Ty::Literal(Literal::String(name(s)), Freshness::Regular)
}
fn lit_bool(b: bool) -> Ty {
    Ty::Literal(Literal::Bool(b), Freshness::Regular)
}
fn lit_float(s: &str) -> Ty {
    Ty::Literal(Literal::Float(name(s)), Freshness::Regular)
}

// ── reflect.AnyClass ──────────────────────────────────────────────────────────

fn any_class() -> Ty {
    Ty::Interface(
        qtn("reflect.AnyClass").cast(),
        CowSlice::from(vec![]),
        CowSlice::from(vec![]),
    )
}

#[test]
fn any_class_membership_is_derived_for_classes_only() {
    let ctx = Ctx::default();
    let target = any_class();
    assert!(ctx.is_subtype(&class("Record"), &target));
    assert!(!ctx.is_subtype(&Ty::Int, &target));
    assert!(!ctx.is_subtype(&Ty::String, &target));
    assert!(!ctx.is_subtype(&Ty::List(CowBox::new(Ty::Int)), &target));
    assert!(!ctx.is_subtype(
        &Ty::Map {
            key: CowBox::new(Ty::String),
            value: CowBox::new(Ty::Int),
        },
        &target
    ));
}

// ── BEP-062: reflect.AnyFunction ──────────────────────────────────────────────

/// `reflect.AnyFunction<...pins>` with the given associated-type pins. An empty
/// list models a pre-default-fill existential (lowering normally fills
/// `Returns`/`Throws` with `unknown`).
fn any_function(pins: Vec<(&str, Ty)>) -> Ty {
    Ty::Interface(
        qtn("reflect.AnyFunction").cast(),
        CowSlice::from(vec![]),
        pins.into_iter()
            .map(|(n, t)| AssocBinding {
                member: match n {
                    "Returns" => ANY_FUNCTION_RETURNS,
                    "Throws" => ANY_FUNCTION_THROWS,
                    other => unreachable!("`AnyFunction` declares no `{other}`"),
                },
                ty: t,
            })
            .collect(),
    )
}

fn simple_fn(ret: Ty, throws: Ty) -> Ty {
    Ty::Function {
        params: CowSlice::from(vec![required(None, Ty::Int)]),
        ret: CowBox::new(ret),
        throws: CowBox::new(throws),
    }
}

#[test]
fn every_function_implements_bare_any_function() {
    // BEP-062: conformance is compiler-derived with no impl in the context.
    // Both the default-filled existential (`Returns`/`Throws` pinned to
    // `unknown`) and a pin-free one accept any function shape.
    let ctx = Ctx::default();
    let never = Ty::Never;
    let filled = any_function(vec![("Returns", Ty::Unknown), ("Throws", Ty::Unknown)]);
    assert!(ctx.is_subtype(&simple_fn(Ty::String, never.clone()), &filled));
    assert!(ctx.is_subtype(&simple_fn(class("Doc"), class("HttpError")), &filled));
    assert!(ctx.is_subtype(&simple_fn(Ty::String, never), &any_function(vec![])));
    // Non-function values do not conform through the derived rule; they fall
    // to the ordinary nominal fact, which this context does not claim.
    assert!(!ctx.is_subtype(&Ty::Int, &filled));
    assert!(!ctx.is_subtype(&class("Doc"), &filled));
}

#[test]
fn any_function_pins_check_the_functions_channels_covariantly() {
    let ctx = Ctx::default();
    let never = Ty::Never;
    let ret_pin = any_function(vec![
        ("Returns", union(vec![Ty::Int, Ty::String])),
        ("Throws", Ty::Unknown),
    ]);
    // `int <: int | string` fits the `Returns` pin; `bool` does not.
    assert!(ctx.is_subtype(&simple_fn(Ty::Int, never.clone()), &ret_pin));
    assert!(!ctx.is_subtype(&simple_fn(Ty::Bool, never.clone()), &ret_pin));
    // A non-throwing function (`throws never`) fits every `Throws` pin; a
    // mismatched error class does not.
    let throws_pin = any_function(vec![
        ("Returns", Ty::Unknown),
        ("Throws", class("ToolError")),
    ]);
    assert!(ctx.is_subtype(&simple_fn(Ty::Int, never), &throws_pin));
    assert!(ctx.is_subtype(&simple_fn(Ty::Int, class("ToolError")), &throws_pin));
    assert!(!ctx.is_subtype(&simple_fn(Ty::Int, class("IoError")), &throws_pin));
}

#[test]
fn any_function_existentials_are_covariant_in_their_pins() {
    // Unlike every other interface binding (invariant), `AnyFunction`'s pins
    // only describe outputs of the held function, so the existentials
    // themselves relate covariantly: `AnyFunction<Returns = int>` fits where
    // `AnyFunction<Returns = int | string>` is expected.
    let ctx = Ctx::default();
    let narrow = any_function(vec![("Returns", Ty::Int), ("Throws", Ty::Never)]);
    let wide = any_function(vec![
        ("Returns", union(vec![Ty::Int, Ty::String])),
        ("Throws", class("ToolError")),
    ]);
    let bare = any_function(vec![("Returns", Ty::Unknown), ("Throws", Ty::Unknown)]);
    assert!(ctx.is_subtype(&narrow, &wide));
    assert!(ctx.is_subtype(&narrow, &bare));
    assert!(ctx.is_subtype(&wide, &bare));
    assert!(!ctx.is_subtype(&wide, &narrow));
    assert!(!ctx.is_subtype(&bare, &narrow));
    // The carve-out is AnyFunction-specific: an unrelated same-shape interface
    // still goes through the invariant `interface_requires` fact, which this
    // context does not claim.
    let other = Ty::Interface(
        qtn("Callable").cast(),
        CowSlice::from(vec![]),
        CowSlice::from(vec![AssocBinding {
            member: ANY_FUNCTION_RETURNS,
            ty: Ty::Int,
        }]),
    );
    assert!(!ctx.is_subtype(&other, &any_function(vec![])));
}

// ── head-category refutation ─────────────────────────────────────────────--

/// `type Json = null | bool | int | float | string | Json[] | map<string, Json>`
/// registered under the given name: the recursive alias whose canonical form
/// is expensive to build.
fn json_alias(ctx: &mut Ctx, name: &str) {
    ctx.aliases.insert(
        qtn(name),
        union(vec![
            Ty::Null,
            Ty::Bool,
            Ty::Int,
            Ty::Float,
            Ty::String,
            list(alias(name)),
            map_ty(Ty::String, alias(name)),
        ]),
    );
}

/// One type of every `Ty` variant, plus the facts that make the context-driven
/// rules fire: `Box` and `int` implement `Displayable`, and `Json` is recursive.
fn kind_corpus() -> (Ctx, Vec<Ty>) {
    let mut ctx = Ctx::default();
    json_alias(&mut ctx, "Json");
    ctx.enums.insert(qtn("Side"), variants("Side", &["L", "R"]));
    ctx.impls.push((qtn("Box"), qtn("Displayable")));
    ctx.prim_impls.push(("int", qtn("Displayable")));
    let corpus = vec![
        Ty::Int,
        bigint(),
        Ty::Float,
        Ty::String,
        Ty::Bool,
        Ty::Null,
        Ty::Uint8Array,
        Ty::Media(MediaKind::Image),
        Ty::Media(MediaKind::Generic),
        Ty::RustType,
        Ty::Type,
        Ty::Resource,
        Ty::PromptAst,
        lit_int(1),
        lit_float("1.5"),
        lit_str("user"),
        lit_bool(true),
        class("Box"),
        class1("Gen", Ty::Int),
        enum_ty("Side"),
        variant("Side", "L"),
        list(Ty::Int),
        list(alias("Json")),
        map_ty(Ty::String, alias("Json")),
        map_ty(Ty::String, Ty::Int),
        Ty::Function {
            params: CowSlice::from(vec![required(None, Ty::Int)]),
            ret: CowBox::new(Ty::Bool),
            throws: CowBox::new(Ty::Never),
        },
        Ty::Future(CowBox::new(Ty::Int), CowBox::new(Ty::Never)),
        union(vec![Ty::Int, Ty::String]),
        optional(Ty::String),
        alias("Json"),
        iface("Displayable"),
        Ty::Unknown,
        Ty::Never,
    ];
    (ctx, corpus)
}

/// The head-category refutation is only a shortcut: over every pair of
/// variants, `is_subtype` and `equivalent` give the verdict the canonical
/// relation gives.
#[test]
fn subtype_and_equivalence_agree_with_the_canonical_relation_for_every_kind_pair() {
    let (ctx, corpus) = kind_corpus();
    for sub in &corpus {
        for sup in &corpus {
            let canonical_sub = NormalTy::canonical(sub, &ctx);
            let canonical_sup = NormalTy::canonical(sup, &ctx);
            assert_eq!(
                ctx.is_subtype(sub, sup),
                canonical_sub.is_subtype_of(&canonical_sup, &ctx, &mut HashSet::new()),
                "subtype verdict diverged for {sub:?} <: {sup:?}"
            );
            assert_eq!(
                ctx.equivalent(sub, sup),
                canonical_sub == canonical_sup,
                "equivalence verdict diverged for {sub:?} == {sup:?}"
            );
        }
    }
}

/// Pairs whose top-level kinds no rule relates, the ones a runtime `match` on
/// a `json` value asks about for every node: none of them may canonicalize
/// (and so expand) the recursive alias.
fn cross_category_pairs() -> Vec<(Ty, Ty)> {
    let json_map = map_ty(Ty::String, alias("Json"));
    let json_list = list(alias("Json"));
    vec![
        (lit_str("user"), json_map.clone()),
        (lit_str("user"), json_list.clone()),
        (json_list.clone(), json_map.clone()),
        (json_map.clone(), json_list.clone()),
        (Ty::Int, json_map.clone()),
        (lit_int(1), Ty::Float),
        (Ty::Null, json_list),
        (class("Box"), json_map.clone()),
        (enum_ty("Side"), json_map),
    ]
}

#[test]
fn cross_category_pairs_are_refuted_without_canonicalizing() {
    let mut ctx = Ctx::default();
    json_alias(&mut ctx, "Json");
    for (sub, sup) in cross_category_pairs() {
        assert!(!ctx.is_subtype(&sub, &sup), "{sub:?} <: {sup:?}");
        assert!(!ctx.equivalent(&sub, &sup), "{sub:?} == {sup:?}");
    }
}

/// Kinds a canonical form can rewrite, or that a context-driven rule relates
/// across kinds, are never refuted by their head alone.
#[test]
fn rewritable_and_context_driven_kinds_are_not_refuted() {
    let (ctx, _) = kind_corpus();
    let json_map = map_ty(Ty::String, alias("Json"));
    assert!(ctx.is_subtype(&lit_str("user"), &alias("Json")));
    assert!(ctx.is_subtype(&json_map, &alias("Json")));
    assert!(ctx.is_subtype(&Ty::Int, &iface("Displayable")));
    assert!(ctx.is_subtype(&class("Box"), &iface("Displayable")));
    assert!(ctx.is_subtype(&Ty::Never, &json_map));
    assert!(ctx.is_subtype(&json_map, &Ty::Unknown));
    assert!(ctx.is_subtype(&lit_str("user"), &optional(Ty::String)));
}

// ── review regressions: renderer totality, unguarded members, qualifiers ──--
//
// Reproducers from the pre-merge adversarial review: each hung, panicked, or
// over-equated before the corresponding fix.
#[test]
fn nested_union_member_recursion_renders_totally() {
    // type A = string | (A | int)[]
    let mut ctx = Ctx::default();
    ctx.aliases.insert(
        qtn("A"),
        union(vec![Ty::String, list(union(vec![alias("A"), Ty::Int]))]),
    );
    let n = ctx.normalize(&alias("A"));
    assert!(ctx.equivalent(&alias("A"), &n));
    assert_eq!(ctx.normalize(&n), n, "idempotent");
    assert!(!ctx.equivalent(&alias("A"), &Ty::Int));
}

#[test]
fn unguarded_mu_member_is_not_vacuously_absorbing() {
    // type A = A | A[]  (== mu X. X[])
    let mut ctx = Ctx::default();
    ctx.aliases
        .insert(qtn("A"), union(vec![alias("A"), list(alias("A"))]));
    // string | A must NOT be equivalent to A.
    assert!(
        !ctx.equivalent(&union(vec![Ty::String, alias("A")]), &alias("A")),
        "string was unsoundly absorbed into the non-contractive mu"
    );
}

#[test]
fn map_value_union_recursion_renders_totally() {
    // type J = int | map<string, J | null>
    let mut ctx = Ctx::default();
    ctx.aliases.insert(
        qtn("J"),
        union(vec![
            Ty::Int,
            map_ty(Ty::String, union(vec![alias("J"), Ty::Null])),
        ]),
    );
    let n = ctx.normalize(&alias("J"));
    assert!(ctx.equivalent(&alias("J"), &n));
    assert_eq!(ctx.normalize(&n), n, "idempotent");
    assert!(!ctx.equivalent(&alias("J"), &Ty::Int));
}

// ── review regressions: unguarded members, disjointness, rendering ───────--

#[test]
fn unguarded_mu_member_does_not_absorb_siblings() {
    // `type A = A | A[]` is non-contractive until ε-closure; before the fix the
    // bottom-up absorption's assumption probe proved `string <: A` vacuously
    // (a self-supporting derivation that never crosses a constructor) and
    // silently dropped the sibling.
    let mut ctx = Ctx::default();
    ctx.aliases
        .insert(qtn("A"), union(vec![alias("A"), list(alias("A"))]));
    let u = union(vec![Ty::String, alias("A")]);
    assert!(ctx.is_subtype(&Ty::String, &u));
    assert!(!ctx.equivalent(&u, &alias("A")));
    assert!(ctx.equivalent(&u, &union(vec![Ty::String, list(alias("A"))])));
}

#[test]
fn union_nested_recursion_renders_via_subset_cover() {
    // `type A = int | (A | null)[]` — the recursion variable sits inside a
    // nested union under the list, so ε-closure splices the named union through
    // and the surviving cycle is nameless. The renderer must cut it with the
    // subset cover (`A | null`), not panic or loop.
    let mut ctx = Ctx::default();
    let null = || Ty::Null;
    ctx.aliases.insert(
        qtn("A"),
        union(vec![Ty::Int, list(union(vec![alias("A"), null()]))]),
    );
    let a = alias("A");
    assert!(ctx.equivalent(&a, &a));
    let rendered = ctx.normalize(&a);
    assert!(ctx.equivalent(&a, &rendered));
    assert_eq!(ctx.normalize(&rendered), rendered, "idempotent");
}

#[test]
fn single_variant_enum_union_is_idempotent() {
    // Collapsing a one-variant enum would split one value set into two
    // canonical spellings (the union spelling collapses, the bare variant
    // cannot); the collapse now requires at least two variants.
    let mut ctx = Ctx::default();
    ctx.enums.insert(qtn("E"), variants("E", &["A"]));
    let v = variant("E", "A");
    let vv = union(vec![v.clone(), v.clone()]);
    assert!(ctx.equivalent(&vv, &v));
    assert!(ctx.is_subtype(&vv, &v));
    assert!(ctx.is_subtype(&v, &vv));
    // The two-variant collapse is unaffected.
    ctx.enums.insert(qtn("F"), variants("F", &["X", "Y"]));
    assert!(ctx.equivalent(
        &union(vec![variant("F", "X"), variant("F", "Y")]),
        &enum_ty("F")
    ));
}

#[test]
fn tuples_are_invariant_elementwise() {
    let ctx = Ctx::default();
    let tuple = |elements: Vec<Ty>| Ty::Tuple(elements.into());
    let narrow = tuple(vec![Ty::Int, Ty::String]);
    let wide = tuple(vec![optional(Ty::Int), Ty::String]);
    assert!(!ctx.is_subtype(&narrow, &wide));
    assert!(!ctx.is_subtype(&wide, &narrow));
    let literal = tuple(vec![lit_int(1), Ty::String]);
    assert!(!ctx.is_subtype(&literal, &narrow));
    let spelled = tuple(vec![union(vec![lit_int(5), Ty::Int]), Ty::String]);
    assert!(ctx.equivalent(&spelled, &narrow));
    assert!(ctx.is_subtype(&spelled, &narrow));
    assert!(ctx.is_subtype(&narrow, &union(vec![narrow.clone(), Ty::Int])));
    assert!(!ctx.is_subtype(&narrow, &union(vec![wide, Ty::Int])));
}

#[test]
fn distinct_tuple_arms_do_not_absorb() {
    let ctx = Ctx::default();
    let tuple = |elements: Vec<Ty>| Ty::Tuple(elements.into());
    let narrow = tuple(vec![Ty::Int]);
    let wide = tuple(vec![optional(Ty::Int)]);
    let both = union(vec![narrow.clone(), wide.clone()]);
    match ctx.normalize(&both) {
        Ty::Union(members) => assert_eq!(members.len(), 2, "{members:?}"),
        other => panic!("`(int,) | (int?,)` collapsed to {other:?}"),
    }
    assert!(!ctx.equivalent(&both, &wide));
    assert!(!ctx.equivalent(&both, &narrow));
    let spelled = tuple(vec![union(vec![lit_int(5), Ty::Int])]);
    assert!(ctx.equivalent(&union(vec![narrow.clone(), spelled]), &narrow));
}

#[test]
fn products_of_unions_and_unions_of_products_are_unrelated() {
    let ctx = Ctx::default();
    let tuple = |elements: Vec<Ty>| Ty::Tuple(elements.into());
    let product = tuple(vec![union(vec![Ty::Int, Ty::String]), Ty::Bool]);
    let distributed = union(vec![
        tuple(vec![Ty::Int, Ty::Bool]),
        tuple(vec![Ty::String, Ty::Bool]),
    ]);
    assert!(!ctx.is_subtype(&product, &distributed));
    assert!(!ctx.is_subtype(&distributed, &product));
    assert!(!ctx.equivalent(&product, &distributed));
    assert!(!ctx.is_subtype(&class1("Box", product), &class1("Box", distributed)));
    let correlated = union(vec![
        tuple(vec![Ty::Int, Ty::String]),
        tuple(vec![Ty::String, Ty::Int]),
    ]);
    let joined = tuple(vec![
        union(vec![Ty::Int, Ty::String]),
        union(vec![Ty::Int, Ty::String]),
    ]);
    assert!(!ctx.is_subtype(&correlated, &joined));
    assert!(!ctx.is_subtype(&joined, &correlated));
    let nested = tuple(vec![
        tuple(vec![union(vec![Ty::Int, Ty::String])]),
        Ty::Bool,
    ]);
    let nested_distributed = union(vec![
        tuple(vec![tuple(vec![Ty::Int]), Ty::Bool]),
        tuple(vec![tuple(vec![Ty::String]), Ty::Bool]),
    ]);
    assert!(!ctx.is_subtype(&nested, &nested_distributed));
    assert!(!ctx.is_subtype(&nested_distributed, &nested));
    assert!(!ctx.is_subtype(&Ty::Int, &tuple(vec![Ty::Int])));
}
