//! Inherent impls and regression tests for [`RealizedTy`], the deep subset of
//! [`Ty`] with no type variables at any depth.
//!
//! [`RealizedTy`] and its conversions are generated in [`crate::family`] by the
//! `ty_family!` macro; this module holds its hand-written behaviour, mirroring
//! [`crate::runtime_ty`]. The tests pin the narrowing contract — `RealizedTy`
//! deeply excludes the `typevar`-axis variants (`TypeVar`,
//! `AssociatedTypeProjection`) and rejects them by name.

use crate::{RealizedTy, TyAttr};

impl RealizedTy {
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
}

impl std::fmt::Display for RealizedTy {
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
        let ty = Ty::List(
            Box::new(Ty::Class(qtn("Box"), vec![Ty::Int { attr: def() }], def())),
            def(),
        );
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_map() {
        let ty = Ty::Map {
            key: Box::new(Ty::String { attr: def() }),
            value: Box::new(Ty::List(Box::new(Ty::Bool { attr: def() }), def())),
            attr: def(),
        };
        assert_round_trips(ty);
    }

    #[test]
    fn round_trip_union() {
        let ty = Ty::Union(
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
        let ty = Ty::Function {
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
        let ty = Ty::Interface(
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
        let ty = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::TypeVar(Name::new("T"), def())),
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
    fn nested_unknown_in_list_blocks_conversion() {
        let ty = Ty::List(Box::new(Ty::Unknown { attr: def() }), def());
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy { variant: "Unknown" })
        );
    }

    #[test]
    fn nested_error_in_map_value_blocks_conversion() {
        let ty = Ty::Map {
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
    fn nested_evolving_list_in_union_blocks_conversion() {
        let ty = Ty::Union(
            vec![
                Ty::Int { attr: def() },
                Ty::EvolvingList(Box::new(Ty::Never { attr: def() }), def()),
            ],
            def(),
        );
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy {
                variant: "EvolvingList"
            })
        );
    }

    #[test]
    fn nested_evolving_map_in_function_ret_blocks_conversion() {
        let ty = Ty::Function {
            params: vec![],
            ret: Box::new(Ty::EvolvingMap(
                Box::new(Ty::Never { attr: def() }),
                Box::new(Ty::Never { attr: def() }),
                def(),
            )),
            throws: Box::new(Ty::Void { attr: def() }),
            attr: def(),
        };
        assert_eq!(
            RealizedTy::try_from(&ty),
            Err(NotRealizedTy {
                variant: "EvolvingMap"
            })
        );
    }
}
