//! Unit tests for the canonical type algebra, driven by a hand-built
//! [`TypeContext`] stub that records nominal facts directly.

use std::collections::HashMap;

use super::*;
use crate::{Freshness, Literal, Name, QualifiedTypeName, Ty, TyAttr};

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
    var_bounds: HashMap<Name, Ty>,
    enums: HashMap<QualifiedTypeName, Vec<Name>>,
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

    fn implements_interface(&self, concrete: &Ty, interface: &Ty) -> bool {
        let Some(iface) = nominal_head(interface) else {
            return false;
        };
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

    fn type_var_bound(&self, name: &Name) -> Option<Ty> {
        self.var_bounds.get(name).cloned()
    }

    fn interface_requires(&self, sub: &Ty, sup: &Ty) -> bool {
        let (Some(a), Some(b)) = (nominal_head(sub), nominal_head(sup)) else {
            return false;
        };
        a == b || self.requires.iter().any(|(x, y)| *x == a && *y == b)
    }

    fn enum_variants(&self, name: &QualifiedTypeName) -> Option<Vec<Name>> {
        self.enums.get(name).cloned()
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
    ctx.var_bounds.insert(Name::new("T"), iface("Animal"));

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
