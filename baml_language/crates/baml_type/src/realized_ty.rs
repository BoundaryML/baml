//! Inherent impls and regression tests for [`RealizedTy`], the deep subset of
//! [`Ty`] with no type variables at any depth.
//!
//! [`RealizedTy`] and its conversions are generated in [`crate::family`] by the
//! `ty_family!` macro; this module holds its hand-written behaviour, mirroring
//! [`crate::runtime_ty`]. The tests pin the narrowing contract — `RealizedTy`
//! deeply excludes the `typevar`-axis variants (`TypeVar`,
//! `AssociatedTypeProjection`) and rejects them by name.

use crate::{RealizedTy, TyAttr};

// Head-agnostic: none of these mention a nominal head, so they are defined for
// every head representation rather than only the compiler's. A bare
// `RealizedTy::int()` still means `RealizedTy<TypeName>` — a type path uses the
// parameter's default — so the runtime spells its own instantiation explicitly.
impl<N: Clone> RealizedTy<N> {
    // --- Primitive constructors (default TyAttr) ---

    /// `int` with default attributes.
    pub fn int() -> Self {
        RealizedTy::Int {
            attr: TyAttr::default(),
        }
    }

    /// `string` with default attributes.
    pub fn string() -> Self {
        RealizedTy::String {
            attr: TyAttr::default(),
        }
    }

    /// `null` with default attributes.
    pub fn null() -> Self {
        RealizedTy::Null {
            attr: TyAttr::default(),
        }
    }

    /// `unknown` (the top type) with default attributes.
    pub fn unknown() -> Self {
        RealizedTy::BuiltinUnknown {
            attr: TyAttr::default(),
        }
    }

    /// `never` (the bottom type) with default attributes. The error type of a
    /// future whose body statically cannot throw.
    pub fn never() -> Self {
        RealizedTy::Never {
            attr: TyAttr::default(),
        }
    }

    // --- Compound constructors (default TyAttr) ---

    /// `T[]` (list) with default attributes.
    pub fn list(inner: RealizedTy<N>) -> Self {
        RealizedTy::List(Box::new(inner), TyAttr::default())
    }

    /// True if this is exactly the `null` type.
    pub fn is_null(&self) -> bool {
        matches!(self, RealizedTy::Null { .. })
    }

    /// True if this is a union that includes `null` — i.e. an optional type.
    /// Mirrors [`crate::RuntimeTy::is_nullable_union`].
    pub fn is_nullable_union(&self) -> bool {
        matches!(self, RealizedTy::Union(members, _) if members.iter().any(RealizedTy::is_null))
    }

    /// Remove `null` from a nullable union, collapsing the result. Mirrors
    /// [`crate::RuntimeTy::strip_null`].
    pub fn strip_null(&self) -> RealizedTy<N> {
        match self {
            RealizedTy::Union(members, attr) => {
                let non_null: Vec<RealizedTy<N>> =
                    members.iter().filter(|m| !m.is_null()).cloned().collect();
                match non_null.len() {
                    0 => self.clone(),
                    1 => non_null
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| unreachable!("len checked")),
                    _ => RealizedTy::Union(non_null, attr.clone()),
                }
            }
            _ => self.clone(),
        }
    }
}

impl<N: Clone + crate::HeadDisplay> std::fmt::Display for RealizedTy<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Zero-cost borrowed upcast; rendering lives on `Ty`.
        std::fmt::Display::fmt(self.as_ty(), f)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Interface, Name, NotRealizedTy, RealizedTy, Ty, TyAttr, TypeName};

    fn def() -> TyAttr {
        TyAttr::default()
    }

    fn qtn(name: &str) -> TypeName {
        TypeName::local(Name::new(name))
    }

    /// `Ty::from(RealizedTy::try_from(&ty)) == ty` for a set of deeply nested
    /// runtime types.
    fn assert_round_trips(ty: Ty) {
        let runtime = RealizedTy::try_from(&ty)
            .unwrap_or_else(|e| panic!("expected a runtime type, got {e}"));
        assert_eq!(Ty::from(runtime), ty);
    }

    #[test]
    fn round_trip_nested_list_of_class() {
        // list<Class<int>>
        let ty: Ty = Ty::List(
            Box::new(Ty::Class(qtn("Box"), vec![Ty::Int { attr: def() }], def())),
            def(),
        );
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_map() {
        let ty: Ty = Ty::Map {
            key: Box::new(Ty::String { attr: def() }),
            value: Box::new(Ty::List(Box::new(Ty::Bool { attr: def() }), def())),
            attr: def(),
        };
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_union() {
        let ty: Ty = Ty::Union(
            vec![
                Ty::Int { attr: def() },
                Ty::String { attr: def() },
                Ty::Null { attr: def() },
            ],
            def(),
        );
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_function() {
        let ty: Ty = Ty::Function {
            params: vec![
                crate::FunctionParamTy::required(Some(Name::new("a")), Ty::Int { attr: def() }),
                crate::FunctionParamTy::optional(
                    Some(Name::new("b")),
                    Ty::List(Box::new(Ty::Float { attr: def() }), def()),
                ),
            ],
            ret: Box::new(Ty::Bool { attr: def() }),
            throws: Box::new(Ty::Void { attr: def() }),
            attr: def(),
        };
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_interface_with_associated_bindings() {
        let ty: Ty = Ty::Interface(
            qtn("Iterator"),
            vec![Ty::Int { attr: def() }],
            vec![(Name::new("Item"), Ty::String { attr: def() })],
            def(),
        );
        assert_round_trips(ty);
    }

    #[test]
    fn associated_type_projection_is_not_realized() {
        // `AssociatedTypeProjection` is a type variable (the `typevar` axis), so
        // it has no realized form — the conversion rejects it at the top level.
        let ty: Ty = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::type_var("T")),
            interface: Box::new(Interface {
                name: qtn("Iterator"),
                generics: vec![],
                associated_types: vec![],
            }),
            member: Name::new("Item"),
            attr: def(),
        };
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy {
                variant: "AssociatedTypeProjection"
            })
        );
    }

    #[test]
    fn nested_infer_in_list_blocks_conversion() {
        let ty: Ty = Ty::List(Box::new(Ty::Infer { attr: def() }), def());
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy { variant: "Infer" })
        );
    }

    #[test]
    fn nested_error_in_map_value_blocks_conversion() {
        let ty: Ty = Ty::Map {
            key: Box::new(Ty::String { attr: def() }),
            value: Box::new(Ty::Error { attr: def() }),
            attr: def(),
        };
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy { variant: "Error" })
        );
    }

    #[test]
    fn nested_error_in_union_blocks_conversion() {
        let ty: Ty = Ty::Union(
            vec![Ty::Int { attr: def() }, Ty::Error { attr: def() }],
            def(),
        );
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy { variant: "Error" })
        );
    }

    #[test]
    fn nested_infer_in_function_ret_blocks_conversion() {
        let ty: Ty = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::Infer { attr: def() }),
            throws: Box::new(Ty::Void { attr: def() }),
            attr: def(),
        };
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy { variant: "Infer" })
        );
    }
}
