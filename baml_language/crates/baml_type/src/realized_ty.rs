//! Inherent impls and conversions for [`RealizedTy`], the deep subset of
//! [`Ty`] that contains no type variables at any depth.
//!
//! [`RealizedTy`] (and [`ConcreteRealizedTy`]) are *defined* in
//! [`crate::family`] by the `ty_family!` macro; this module holds their
//! hand-written behaviour. [`RealizedTy`] is equivalent to [`RuntimeTy`] except
//! it deeply excludes the `typevar`-axis variants (`TypeVar`,
//! `AssociatedTypeProjection`).

use crate::{RealizedFunctionParamTy, RealizedTy, RuntimeTy, Ty};

/// Error returned by [`RealizedTy::try_from`] when a [`Ty`] (or one of its
/// nested children) is a compiler-only variant that cannot exist at runtime.
///
/// Records only the *name* of the offending variant — never the value itself —
/// to keep the diagnostic bounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NotRealizedTy {
    pub variant: &'static str,
}

impl std::fmt::Display for NotRealizedTy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` is not a valid realized type", self.variant)
    }
}

impl std::error::Error for NotRealizedTy {}

macro_rules! into_realized_ty {
    ($ty:ty, {$($variant:pat => $variant_name:literal),+$(,)?}) => {
        impl TryFrom<&$ty> for RealizedTy {
            type Error = NotRealizedTy;

            fn try_from(ty: &$ty) -> Result<Self, Self::Error> {
                fn try_vec(tys: &[$ty]) -> Result<Vec<RealizedTy>, NotRealizedTy> {
                    tys.iter().map(RealizedTy::try_from).collect()
                }
                type Source = $ty;
                Ok(match ty {
                    Source::Int { attr } => RealizedTy::Int { attr: attr.clone() },
                    Source::Bigint { attr } => RealizedTy::Bigint { attr: attr.clone() },
                    Source::Float { attr } => RealizedTy::Float { attr: attr.clone() },
                    Source::String { attr } => RealizedTy::String { attr: attr.clone() },
                    Source::Bool { attr } => RealizedTy::Bool { attr: attr.clone() },
                    Source::Null { attr } => RealizedTy::Null { attr: attr.clone() },
                    Source::Uint8Array { attr } => RealizedTy::Uint8Array { attr: attr.clone() },
                    Source::Media(kind, attr) => RealizedTy::Media(*kind, attr.clone()),
                    Source::Literal(lit, freshness, attr) => {
                        RealizedTy::Literal(lit.clone(), *freshness, attr.clone())
                    }
                    Source::Class(name, args, attr) => {
                        RealizedTy::Class(name.clone(), try_vec(args)?, attr.clone())
                    }
                    Source::Interface(name, args, bindings, attr) => {
                        let args = try_vec(args)?;
                        let bindings = bindings
                            .iter()
                            .map(|(n, t)| Ok((n.clone(), RealizedTy::try_from(t)?)))
                            .collect::<Result<Vec<_>, NotRealizedTy>>()?;
                        RealizedTy::Interface(name.clone(), args, bindings, attr.clone())
                    }
                    Source::Enum(name, attr) => RealizedTy::Enum(name.clone(), attr.clone()),
                    Source::EnumVariant(name, variant, attr) => {
                        RealizedTy::EnumVariant(name.clone(), variant.clone(), attr.clone())
                    }
                    Source::List(inner, attr) => {
                        RealizedTy::List(Box::new(RealizedTy::try_from(&**inner)?), attr.clone())
                    }
                    Source::Map { key, value, attr } => RealizedTy::Map {
                        key: Box::new(RealizedTy::try_from(&**key)?),
                        value: Box::new(RealizedTy::try_from(&**value)?),
                        attr: attr.clone(),
                    },
                    Source::Union(members, attr) => RealizedTy::Union(try_vec(members)?, attr.clone()),
                    Source::Function {
                        generic_params,
                        generic_param_bounds,
                        params,
                        ret,
                        throws,
                        attr,
                    } => {
                        let generic_param_bounds = generic_param_bounds
                            .iter()
                            .map(|b| b.as_ref().map(RealizedTy::try_from).transpose())
                            .collect::<Result<Vec<_>, NotRealizedTy>>()?;
                        let params = params
                            .iter()
                            .map(|p| {
                                Ok(RealizedFunctionParamTy {
                                    name: p.name.clone(),
                                    ty: RealizedTy::try_from(&p.ty)?,
                                    mode: p.mode,
                                })
                            })
                            .collect::<Result<Vec<_>, NotRealizedTy>>()?;
                        RealizedTy::Function {
                            generic_params: generic_params.clone(),
                            generic_param_bounds,
                            params,
                            ret: Box::new(RealizedTy::try_from(&**ret)?),
                            throws: Box::new(RealizedTy::try_from(&**throws)?),
                            attr: attr.clone(),
                        }
                    }
                    Source::Future(value, error, attr) => RealizedTy::Future(
                        Box::new(RealizedTy::try_from(&**value)?),
                        Box::new(RealizedTy::try_from(&**error)?),
                        attr.clone(),
                    ),
                    Source::RustType { attr } => RealizedTy::RustType { attr: attr.clone() },
                    Source::Type { attr } => RealizedTy::Type { attr: attr.clone() },
                    Source::Resource { attr } => RealizedTy::Resource { attr: attr.clone() },
                    Source::PromptAst { attr } => RealizedTy::PromptAst { attr: attr.clone() },
                    Source::Void { attr } => RealizedTy::Void { attr: attr.clone() },
                    Source::WatchAccessor(inner, attr) => RealizedTy::WatchAccessor(
                        Box::new(RealizedTy::try_from(&**inner)?),
                        attr.clone(),
                    ),
                    Source::TypeAlias(name, attr) => RealizedTy::TypeAlias(name.clone(), attr.clone()),
                    Source::BuiltinUnknown { attr } => {
                        RealizedTy::BuiltinUnknown { attr: attr.clone() }
                    }
                    Source::Never { attr } => RealizedTy::Never { attr: attr.clone() },

                    $(
                      $variant => {
                          return Err(NotRealizedTy { variant: $variant_name });
                      }
                    )+
                })
            }
        }

        impl TryFrom<$ty> for RealizedTy {
            type Error = NotRealizedTy;

            fn try_from(ty: $ty) -> Result<Self, Self::Error> {
                RealizedTy::try_from(&ty)
            }
        }
    };
}
into_realized_ty!(Ty, {
    Ty::TypeVar { .. } => "TypeVar",
    Ty::AssociatedTypeProjection { .. } => "AssociatedTypeProjection",
    Ty::Unknown { .. } => "Unknown",
    Ty::Error { .. } => "Error",
    Ty::EvolvingList { .. } => "EvolvingList",
    Ty::EvolvingMap { .. } => "EvolvingMap",
});
into_realized_ty!(RuntimeTy, {
    RuntimeTy::TypeVar { .. } => "TypeVar",
    RuntimeTy::AssociatedTypeProjection { .. } => "AssociatedTypeProjection",
});

macro_rules! from_realized_ty {
    ($ty:ty, $param:ty) => {
        impl From<&RealizedTy> for $ty {
            fn from(ty: &RealizedTy) -> Self {
                fn from_vec(tys: &[RealizedTy]) -> Vec<$ty> {
                    tys.iter().map(<$ty>::from).collect()
                }
                type Target = $ty;
                type Param = $param;

                match ty {
                    RealizedTy::Int { attr } => Target::Int { attr: attr.clone() },
                    RealizedTy::Bigint { attr } => Target::Bigint { attr: attr.clone() },
                    RealizedTy::Float { attr } => Target::Float { attr: attr.clone() },
                    RealizedTy::String { attr } => Target::String { attr: attr.clone() },
                    RealizedTy::Bool { attr } => Target::Bool { attr: attr.clone() },
                    RealizedTy::Null { attr } => Target::Null { attr: attr.clone() },
                    RealizedTy::Uint8Array { attr } => Target::Uint8Array { attr: attr.clone() },
                    RealizedTy::Media(kind, attr) => Target::Media(*kind, attr.clone()),
                    RealizedTy::Literal(lit, freshness, attr) => {
                        Target::Literal(lit.clone(), *freshness, attr.clone())
                    }
                    RealizedTy::Class(name, args, attr) => {
                        Target::Class(name.clone(), from_vec(args), attr.clone())
                    }
                    RealizedTy::Interface(name, args, bindings, attr) => {
                        let bindings = bindings
                            .iter()
                            .map(|(n, t)| (n.clone(), Target::from(t)))
                            .collect();
                        Target::Interface(name.clone(), from_vec(args), bindings, attr.clone())
                    }
                    RealizedTy::Enum(name, attr) => Target::Enum(name.clone(), attr.clone()),
                    RealizedTy::EnumVariant(name, variant, attr) => {
                        Target::EnumVariant(name.clone(), variant.clone(), attr.clone())
                    }
                    RealizedTy::List(inner, attr) => {
                        Target::List(Box::new(Target::from(&**inner)), attr.clone())
                    }
                    RealizedTy::Map { key, value, attr } => Target::Map {
                        key: Box::new(Target::from(&**key)),
                        value: Box::new(Target::from(&**value)),
                        attr: attr.clone(),
                    },
                    RealizedTy::Union(members, attr) => {
                        Target::Union(from_vec(members), attr.clone())
                    }
                    RealizedTy::Function {
                        generic_params,
                        generic_param_bounds,
                        params,
                        ret,
                        throws,
                        attr,
                    } => Target::Function {
                        generic_params: generic_params.clone(),
                        generic_param_bounds: generic_param_bounds
                            .iter()
                            .map(|b| b.as_ref().map(Target::from))
                            .collect(),
                        params: params
                            .iter()
                            .map(|p| Param {
                                name: p.name.clone(),
                                ty: Target::from(&p.ty),
                                mode: p.mode,
                            })
                            .collect(),
                        ret: Box::new(Target::from(&**ret)),
                        throws: Box::new(Target::from(&**throws)),
                        attr: attr.clone(),
                    },
                    RealizedTy::Future(value, error, attr) => Target::Future(
                        Box::new(Target::from(&**value)),
                        Box::new(Target::from(&**error)),
                        attr.clone(),
                    ),
                    RealizedTy::RustType { attr } => Target::RustType { attr: attr.clone() },
                    RealizedTy::Type { attr } => Target::Type { attr: attr.clone() },
                    RealizedTy::Resource { attr } => Target::Resource { attr: attr.clone() },
                    RealizedTy::PromptAst { attr } => Target::PromptAst { attr: attr.clone() },
                    RealizedTy::Void { attr } => Target::Void { attr: attr.clone() },
                    RealizedTy::WatchAccessor(inner, attr) => {
                        Target::WatchAccessor(Box::new(Target::from(&**inner)), attr.clone())
                    }
                    RealizedTy::TypeAlias(name, attr) => {
                        Target::TypeAlias(name.clone(), attr.clone())
                    }
                    RealizedTy::BuiltinUnknown { attr } => {
                        Target::BuiltinUnknown { attr: attr.clone() }
                    }
                    RealizedTy::Never { attr } => Target::Never { attr: attr.clone() },
                }
            }
        }
        impl From<RealizedTy> for $ty {
            fn from(ty: RealizedTy) -> Self {
                ::core::convert::From::from(&ty)
            }
        }
    };
}

from_realized_ty!(Ty, crate::FunctionParamTy);
from_realized_ty!(RuntimeTy, crate::RuntimeFunctionParamTy);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Name, TyAttr, TypeName};

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
            generic_params: vec![Name::new("T")],
            generic_param_bounds: vec![Some(Ty::String { attr: def() })],
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
            interface: Some(Box::new(Ty::TypeAlias(qtn("Iterator"), def()))),
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
            generic_params: vec![],
            generic_param_bounds: vec![],
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
