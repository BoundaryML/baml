//! Computing the `Self` receiver type for a method body or signature.
//!
//! `Self` is resolved *once* per method, to a single [`Ty`] the lowering context
//! ([`ScopeCtx`](crate::lower_type_expr::ScopeCtx)) hands back from `lower_self`. What it is
//! depends on where the method lives:
//!
//! - **class method** — the class's full receiver type (`Foo<T>`, carrying its generics as
//!   `TypeVar` args; the builtin containers `Array`/`Map` are their `T[]` / `map<K,V>` sugar).
//! - **interface's own default method** — the rigid `Self` type variable bound by the
//!   interface (so `Self.Assoc` stays symbolic and `Self`-returning methods stay polymorphic
//!   over implementors). Its bound is supplied separately, as a `baml_type::Interface`
//!   constraint in the context's `bounds`.
//! - **out-of-body impl method** — the impl's receiver pattern (`Box<T>`, `T`), which is a
//!   `TypeExpr` lowered through the context like any other type.
//!
//! This module owns the first two (which are pure functions of a declaration); the impl case
//! is an ordinary `TypeExpr` lowering and needs no helper here.

use crate::ty::{ParamTy, QualifiedTypeName, Ty, TyAttr};

/// The rigid `Self` type variable for an interface's own default-method body. Its interface
/// bound is recorded separately (in the lowering context's `bounds`), so `Self.Assoc`
/// projects through that bound rather than collapsing to a concrete type.
pub fn self_type_for_interface_default(param: &ParamTy) -> Ty {
    Ty::TypeVar(param.clone(), TyAttr::default())
}

/// Build a class receiver type at explicit generic `args` (`TypeVar`s or concrete): the builtin
/// container roots are their structural sugar (`baml.Array<int>` → `int[]`), everything else
/// a nominal `Ty::Class`. This is the receiver shape a value actually has — an array value is
/// a `Ty::List`, never a `Ty::Class(baml.Array, …)` — so a static-form call
/// (`baml.Array.length(arr)`) must type its `self` formal structurally too, or call-site
/// inference could never bind `T` from the `Ty::List` actual.
pub fn receiver_type_for_class_at(qtn: QualifiedTypeName, args: Vec<Ty>) -> Ty {
    if qtn.is_builtin_root_type("Array") {
        debug_assert_eq!(
            args.len(),
            1,
            "builtin `baml.Array` has one generic parameter"
        );
        Ty::List(
            Box::new(args.into_iter().next().unwrap_or_else(|| unreachable!())),
            TyAttr::default(),
        )
    } else if qtn.is_builtin_root_type("Map") {
        debug_assert_eq!(
            args.len(),
            2,
            "builtin `baml.Map` has two generic parameters"
        );
        let mut args = args.into_iter();
        let key = args.next().unwrap_or_else(|| unreachable!());
        let value = args.next().unwrap_or_else(|| unreachable!());
        Ty::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    } else {
        Ty::Class(qtn, args, TyAttr::default())
    }
}
