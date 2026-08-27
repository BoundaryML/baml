//! End-to-end exercise of the `child: interned(Handle)` member mode, on a
//! self-contained miniature family (the real family adopts the mode at the
//! `TyKind` cutover). What this pins:
//!
//! - the type-shape transform: `Box<Ty<N>>`/bare `Ty<N>` → handle,
//!   `Box<[Ty<N>]>` → `Box<[Handle]>`, `Box<Sat<N>>` → inline twin,
//!   `Box<[(Name, Ty<N>)]>` → `Box<[(Name, Handle)]>`, `Option` recursion,
//!   and the head parameter fixed at its declared default;
//! - axis membership: the interned member includes its axes' variants
//!   (`Var`) and excludes the rest (`Hole`), proven by exhaustive matches;
//! - the `attr`/`with_attr` accessors, including the attr-less fallback;
//! - `Ord` parity with declaration order;
//! - zero behavior delta for the plain members: the deep equal-size pair
//!   (`WideTy` ⊂ `Ty`) keeps its transmute matrix alongside the interned
//!   member, which takes no part in it.

use baml_type_macros::ty_family;

/// Minimal stand-in for the attribute payload the macro requires by name.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TyAttr {
    pub streaming: bool,
}

impl TyAttr {
    pub const EMPTY: TyAttr = TyAttr { streaming: false };
}

/// A member name (a field, a binding) — not a type head.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name(&'static str);

/// The default head representation, proving the interned member fixes the
/// parameter at this default rather than staying generic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TestName(&'static str);

/// Stand-in intern handle. The macro only splices the type; identity
/// semantics live with the (hand-written) pool, so an id suffices here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Handle(u32);

ty_family! {
    axes { core, wide, hole, var }

    type Ty       { includes: [core, wide, hole], child: Self }
    // Deep and includes `wide` (the largest-payload axis), so the plain
    // transmute pair `WideTy` ⊂ `Ty` stays equal-size.
    type WideTy   { includes: [core, wide],       child: Self }
    type InternTy { includes: [core, wide, var],  child: interned(Handle) }

    satellite Param<N: Clone = TestName> {
        pub name: Option<Name>,
        pub ty: Ty<N>,
    } methods {
        pub fn of(name: Option<Name>, ty: Ty<N>) -> Self {
            Self { name, ty }
        }
    }

    satellite Ref<N: Clone = TestName> {
        pub head: N,
        pub args: Box<[Ty<N>]>,
        pub bindings: Box<[(Name, Ty<N>)]>,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum Ty<N: Clone = TestName> {
        #[axis(core)]
        Leaf {
            attr: TyAttr,
        } = 0,
        #[axis(core)]
        Named(N, TyAttr) = 1,
        #[axis(core)]
        List(Box<Ty<N>>, TyAttr) = 2,
        #[axis(core)]
        Many(Box<[Ty<N>]>, TyAttr) = 3,
        #[axis(core)]
        Fun {
            params: Box<[Param<N>]>,
            ret: Box<Ty<N>>,
            attr: TyAttr,
        } = 4,
        #[axis(core)]
        Opt {
            inner: Option<Box<Ty<N>>>,
            attr: TyAttr,
        } = 5,
        #[axis(wide)]
        Proj {
            base: Box<Ty<N>>,
            iface: Box<Ref<N>>,
            member: Name,
            attr: TyAttr,
        } = 6,
        #[axis(wide)]
        Pairs(Box<[(Name, Ty<N>)]>, TyAttr) = 7,
        #[axis(hole)]
        Hole {
            attr: TyAttr,
        } = 8,
        #[axis(var)]
        Var {
            var: u32,
            attr: TyAttr,
        } = 9,
        /// Attr-less leaf, exercising the accessor fallbacks.
        #[axis(var)]
        Marker(u32) = 10,
    }
}

fn a() -> TyAttr {
    TyAttr { streaming: false }
}

/// Every transformed field shape, constructed: bare-head fix, handle
/// positions (boxed, bare, sliced, paired), inline twins, `Option` recursion.
#[test]
fn interned_shapes_construct() {
    let named = InternTy::Named(TestName("Point"), a());
    let list = InternTy::List(Handle(1), a());
    let many = InternTy::Many(Box::new([Handle(1), Handle(2)]), a());
    let fun = InternTy::Fun {
        params: Box::new([InternParam {
            name: Some(Name("x")),
            ty: Handle(3),
        }]),
        ret: Handle(4),
        attr: a(),
    };
    let opt = InternTy::Opt {
        inner: Some(Handle(5)),
        attr: a(),
    };
    let proj = InternTy::Proj {
        base: Handle(6),
        iface: InternRef {
            head: TestName("Iterator"),
            args: Box::new([Handle(7)]),
            bindings: Box::new([(Name("Item"), Handle(8))]),
        },
        member: Name("Item"),
        attr: a(),
    };
    let pairs = InternTy::Pairs(Box::new([(Name("k"), Handle(9))]), a());
    let var = InternTy::Var { var: 0, attr: a() };

    for ty in [named, list, many, fun, opt, proj, pairs, var] {
        // The accessors work on every shape.
        assert_eq!(ty.attr(), &a());
        let replaced = ty.with_attr(TyAttr { streaming: true });
        assert!(replaced.attr().streaming);
    }
}

/// The attr-less variant borrows `TyAttr::EMPTY` and drops `with_attr`.
#[test]
fn interned_attrless_fallback() {
    let marker = InternTy::Marker(11);
    assert_eq!(marker.attr(), &TyAttr::EMPTY);
    let same = marker.clone().with_attr(TyAttr { streaming: true });
    assert_eq!(same, marker);
}

/// `InternTy` includes exactly its axes' variants: `Var`/`Marker` in, `Hole`
/// out. The match is exhaustive *without* a `Hole` arm — adding `hole` to the
/// member's include-set would turn this into a compile error.
#[test]
fn interned_axis_membership() {
    let v = InternTy::Var { var: 7, attr: a() };
    let seen = match v {
        InternTy::Leaf { .. } => "leaf",
        InternTy::Named(..) => "named",
        InternTy::List(..) => "list",
        InternTy::Many(..) => "many",
        InternTy::Fun { .. } => "fun",
        InternTy::Opt { .. } => "opt",
        InternTy::Proj { .. } => "proj",
        InternTy::Pairs(..) => "pairs",
        InternTy::Var { .. } => "var",
        InternTy::Marker(..) => "marker",
    };
    assert_eq!(seen, "var");
}

/// Derived `Ord` follows declaration order, matching the plain members' Ord
/// over their (monotone) explicit discriminants.
#[test]
fn interned_ord_parity() {
    let leaf = InternTy::Leaf { attr: a() };
    let var = InternTy::Var { var: 0, attr: a() };
    let marker = InternTy::Marker(0);
    assert!(leaf < var);
    assert!(var < marker);
    // Same relative order as the plain member over shared variants.
    let p_leaf = Ty::Leaf { attr: a() };
    let p_named = Ty::Named(TestName("n"), a());
    assert!(p_leaf < p_named);
    let i_named = InternTy::Named(TestName("n"), a());
    assert!(leaf < i_named);
}

/// The plain side of the family is unaffected: the deep equal-size pair keeps
/// its full matrix (upcast view, owned widening, validated narrowing) with
/// the interned member declared alongside.
#[test]
fn plain_matrix_unaffected() {
    let wide: WideTy = WideTy::Proj {
        base: Box::new(WideTy::Leaf { attr: a() }),
        iface: Box::new(WideRef {
            head: TestName("Iterator"),
            args: Box::new([WideTy::Leaf { attr: a() }]),
            bindings: Box::new([]),
        }),
        member: Name("Item"),
        attr: a(),
    };
    let as_ty: &Ty = wide.as_ty();
    assert!(matches!(as_ty, Ty::Proj { .. }));
    let widened: Ty = Ty::from(wide.clone());
    let narrowed = WideTy::try_from(widened).expect("no `hole` variant nested");
    assert_eq!(narrowed, wide);

    let holey: Ty = Ty::List(Box::new(Ty::Hole { attr: a() }), a());
    assert_eq!(WideTy::try_from(holey), Err(NotWideTy { variant: "Hole" }));
}
