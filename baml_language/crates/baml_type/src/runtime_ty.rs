//! Deep runtime-facing subset of [`Ty`].
//!
//! [`RuntimeTy`] is a hand-written deep mirror of [`Ty`] that contains only the
//! variants which can legitimately exist outside the compiler. Its nested
//! positions hold `RuntimeTy` (not `Ty`), so a `RuntimeTy` value is statically
//! guaranteed to be free of compiler-only variants all the way down.
//!
//! [`ConcreteTy`] and [`RealizedTy`] are `subenum`-generated subsets *of*
//! `RuntimeTy`, giving the taxonomy `ConcreteTy ⊆ RealizedTy ⊆ RuntimeTy`:
//! - **ConcreteTy** — types with concrete memory layouts and method
//!   implementations.
//! - **RealizedTy** — realized types (excludes type aliases and unrealized type
//!   args).
//! - **RuntimeTy** — every type that can exist outside the compiler.
//!
//! Conversions:
//! - [`RuntimeTy::try_from`] (`&Ty`/`Ty`) is fallible: it rejects the four
//!   compiler-only variants (`Unknown`, `Error`, `EvolvingList`, `EvolvingMap`)
//!   even when nested, returning [`NotRuntimeTy`].
//! - [`Ty::from`] (`RuntimeTy`/`&RuntimeTy`) is infallible.

use borsh::{BorshDeserialize, BorshSerialize};
use subenum::subenum;

use crate::{Freshness, FunctionParamMode, Literal, MediaKind, Name, Ty, TyAttr, TypeName};

/// A single parameter of a [`RuntimeTy::Function`] — the runtime mirror of
/// [`crate::FunctionParamTy`], holding a [`RuntimeTy`] instead of a [`Ty`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct RuntimeFunctionParamTy {
    pub name: Option<Name>,
    pub ty: RuntimeTy,
    pub mode: FunctionParamMode,
}

impl RuntimeFunctionParamTy {
    pub fn required(name: Option<Name>, ty: RuntimeTy) -> Self {
        Self {
            name,
            ty,
            mode: FunctionParamMode::Required,
        }
    }

    pub fn optional(name: Option<Name>, ty: RuntimeTy) -> Self {
        Self {
            name,
            ty,
            mode: FunctionParamMode::Optional,
        }
    }

    pub fn is_required(&self) -> bool {
        matches!(self.mode, FunctionParamMode::Required)
    }

    pub fn is_optional(&self) -> bool {
        matches!(self.mode, FunctionParamMode::Optional)
    }
}

/// Deep runtime-facing subset of [`Ty`]. See the module docs for the taxonomy
/// and conversion contract.
#[subenum(
    ConcreteTy(
        doc = "Concrete types that have concrete memory layouts and method implementations."
    ),
    RealizedTy(doc = "Realized types (excludes type aliases and unrealized type args)")
)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub enum RuntimeTy {
    #[subenum(ConcreteTy, RealizedTy)]
    Int { attr: TyAttr },
    #[subenum(ConcreteTy, RealizedTy)]
    Bigint { attr: TyAttr },
    #[subenum(ConcreteTy, RealizedTy)]
    Float { attr: TyAttr },
    #[subenum(ConcreteTy, RealizedTy)]
    String { attr: TyAttr },
    #[subenum(ConcreteTy, RealizedTy)]
    Bool { attr: TyAttr },
    #[subenum(ConcreteTy, RealizedTy)]
    Null { attr: TyAttr },
    #[subenum(ConcreteTy, RealizedTy)]
    Uint8Array { attr: TyAttr },
    #[subenum(ConcreteTy, RealizedTy)]
    Media(MediaKind, TyAttr),
    /// A literal type — a single value (`1`, `"hi"`, `true`) as a type.
    #[subenum(RealizedTy)]
    Literal(Literal, Freshness, TyAttr),
    #[subenum(ConcreteTy, RealizedTy)]
    Class(TypeName, Vec<RuntimeTy>, TyAttr),
    #[subenum(RealizedTy)]
    Interface(TypeName, Vec<RuntimeTy>, Vec<(Name, RuntimeTy)>, TyAttr),
    #[subenum(ConcreteTy, RealizedTy)]
    Enum(TypeName, TyAttr),
    /// A specific enum variant — `Status.HttpError`.
    #[subenum(RealizedTy)]
    EnumVariant(TypeName, Name, TyAttr),
    #[subenum(ConcreteTy, RealizedTy)]
    List(Box<RuntimeTy>, TyAttr),
    #[subenum(ConcreteTy, RealizedTy)]
    Map {
        key: Box<RuntimeTy>,
        value: Box<RuntimeTy>,
        attr: TyAttr,
    },
    #[subenum(RealizedTy)]
    Union(Vec<RuntimeTy>, TyAttr),

    /// Function/arrow type: `<G…>(T1, T2, ...) -> R throws E`.
    #[subenum(ConcreteTy, RealizedTy)]
    Function {
        generic_params: Vec<Name>,
        generic_param_bounds: Vec<Option<RuntimeTy>>,
        params: Vec<RuntimeFunctionParamTy>,
        ret: Box<RuntimeTy>,
        throws: Box<RuntimeTy>,
        attr: TyAttr,
    },
    /// A future handle — the result of `schedule_future` or `spawn` before
    /// `await`. Carries both the resolved value type and the error type.
    #[subenum(ConcreteTy, RealizedTy)]
    Future(Box<RuntimeTy>, Box<RuntimeTy>, TyAttr),
    /// Opaque Rust-managed state (`$rust_type` fields in builtin class stubs).
    #[subenum(ConcreteTy, RealizedTy)]
    RustType { attr: TyAttr },
    /// The `type` metatype keyword — a runtime value that wraps a `Ty`.
    #[subenum(ConcreteTy, RealizedTy)]
    Type { attr: TyAttr },
    /// Opaque resource handle — file, socket, or HTTP response body.
    #[subenum(ConcreteTy, RealizedTy)]
    Resource { attr: TyAttr },
    /// Opaque structured prompt tree for LLM calls.
    #[subenum(ConcreteTy, RealizedTy)]
    PromptAst { attr: TyAttr },

    /// Void type — the type of effectful expressions (was VIR `Unit`).
    #[subenum(RealizedTy)]
    Void { attr: TyAttr },
    /// Watch accessor type: represents `x.$watch` on a watched variable.
    #[subenum(RealizedTy)]
    WatchAccessor(Box<RuntimeTy>, TyAttr),

    /// Only recursive aliases survive lower_ty; non-recursive are expanded.
    TypeAlias(TypeName, TyAttr),
    /// A type variable (generic parameter) — e.g. `T` in `Array<T>`.
    TypeVar(Name, TyAttr),
    /// Associated type projection, e.g. `P.Output` or `(T as Iterator).Item`.
    AssociatedTypeProjection {
        base: Box<RuntimeTy>,
        interface: Option<Box<RuntimeTy>>,
        member: Name,
        attr: TyAttr,
    },

    /// The top type — may have any concrete value.
    #[subenum(RealizedTy)]
    BuiltinUnknown { attr: TyAttr },
    /// The bottom type — an expression that never produces a value.
    #[subenum(RealizedTy)]
    Never { attr: TyAttr },
}

/// Error returned by [`RuntimeTy::try_from`] when a [`Ty`] (or one of its
/// nested children) is a compiler-only variant that cannot exist at runtime.
///
/// Records only the *name* of the offending variant — never the value itself —
/// to keep the diagnostic bounded (a `Ty` may hold arbitrarily large literals
/// or deeply nested children).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NotRuntimeTy {
    pub variant: &'static str,
}

impl std::fmt::Display for NotRuntimeTy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "`Ty::{}` is a compiler-only type and has no runtime representation",
            self.variant
        )
    }
}

impl std::error::Error for NotRuntimeTy {}

impl TryFrom<&Ty> for RuntimeTy {
    type Error = NotRuntimeTy;

    fn try_from(ty: &Ty) -> Result<Self, Self::Error> {
        Ok(match ty {
            Ty::Int { attr } => RuntimeTy::Int { attr: attr.clone() },
            Ty::Bigint { attr } => RuntimeTy::Bigint { attr: attr.clone() },
            Ty::Float { attr } => RuntimeTy::Float { attr: attr.clone() },
            Ty::String { attr } => RuntimeTy::String { attr: attr.clone() },
            Ty::Bool { attr } => RuntimeTy::Bool { attr: attr.clone() },
            Ty::Null { attr } => RuntimeTy::Null { attr: attr.clone() },
            Ty::Uint8Array { attr } => RuntimeTy::Uint8Array { attr: attr.clone() },
            Ty::Media(kind, attr) => RuntimeTy::Media(*kind, attr.clone()),
            Ty::Literal(lit, freshness, attr) => {
                RuntimeTy::Literal(lit.clone(), *freshness, attr.clone())
            }
            Ty::Class(name, args, attr) => {
                RuntimeTy::Class(name.clone(), try_vec(args)?, attr.clone())
            }
            Ty::Interface(name, args, bindings, attr) => {
                let args = try_vec(args)?;
                let bindings = bindings
                    .iter()
                    .map(|(n, t)| Ok((n.clone(), RuntimeTy::try_from(t)?)))
                    .collect::<Result<Vec<_>, NotRuntimeTy>>()?;
                RuntimeTy::Interface(name.clone(), args, bindings, attr.clone())
            }
            Ty::Enum(name, attr) => RuntimeTy::Enum(name.clone(), attr.clone()),
            Ty::EnumVariant(name, variant, attr) => {
                RuntimeTy::EnumVariant(name.clone(), variant.clone(), attr.clone())
            }
            Ty::List(inner, attr) => {
                RuntimeTy::List(Box::new(RuntimeTy::try_from(&**inner)?), attr.clone())
            }
            Ty::Map { key, value, attr } => RuntimeTy::Map {
                key: Box::new(RuntimeTy::try_from(&**key)?),
                value: Box::new(RuntimeTy::try_from(&**value)?),
                attr: attr.clone(),
            },
            Ty::Union(members, attr) => RuntimeTy::Union(try_vec(members)?, attr.clone()),
            Ty::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws,
                attr,
            } => {
                let generic_param_bounds = generic_param_bounds
                    .iter()
                    .map(|b| b.as_ref().map(RuntimeTy::try_from).transpose())
                    .collect::<Result<Vec<_>, NotRuntimeTy>>()?;
                let params = params
                    .iter()
                    .map(|p| {
                        Ok(RuntimeFunctionParamTy {
                            name: p.name.clone(),
                            ty: RuntimeTy::try_from(&p.ty)?,
                            mode: p.mode,
                        })
                    })
                    .collect::<Result<Vec<_>, NotRuntimeTy>>()?;
                RuntimeTy::Function {
                    generic_params: generic_params.clone(),
                    generic_param_bounds,
                    params,
                    ret: Box::new(RuntimeTy::try_from(&**ret)?),
                    throws: Box::new(RuntimeTy::try_from(&**throws)?),
                    attr: attr.clone(),
                }
            }
            Ty::Future(value, error, attr) => RuntimeTy::Future(
                Box::new(RuntimeTy::try_from(&**value)?),
                Box::new(RuntimeTy::try_from(&**error)?),
                attr.clone(),
            ),
            Ty::RustType { attr } => RuntimeTy::RustType { attr: attr.clone() },
            Ty::Type { attr } => RuntimeTy::Type { attr: attr.clone() },
            Ty::Resource { attr } => RuntimeTy::Resource { attr: attr.clone() },
            Ty::PromptAst { attr } => RuntimeTy::PromptAst { attr: attr.clone() },
            Ty::Void { attr } => RuntimeTy::Void { attr: attr.clone() },
            Ty::WatchAccessor(inner, attr) => {
                RuntimeTy::WatchAccessor(Box::new(RuntimeTy::try_from(&**inner)?), attr.clone())
            }
            Ty::TypeAlias(name, attr) => RuntimeTy::TypeAlias(name.clone(), attr.clone()),
            Ty::TypeVar(name, attr) => RuntimeTy::TypeVar(name.clone(), attr.clone()),
            Ty::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => RuntimeTy::AssociatedTypeProjection {
                base: Box::new(RuntimeTy::try_from(&**base)?),
                interface: interface
                    .as_ref()
                    .map(|i| Ok::<_, NotRuntimeTy>(Box::new(RuntimeTy::try_from(&**i)?)))
                    .transpose()?,
                member: member.clone(),
                attr: attr.clone(),
            },
            Ty::BuiltinUnknown { attr } => RuntimeTy::BuiltinUnknown { attr: attr.clone() },
            Ty::Never { attr } => RuntimeTy::Never { attr: attr.clone() },

            // Compiler-only variants have no runtime representation.
            Ty::Unknown { .. } => return Err(NotRuntimeTy { variant: "Unknown" }),
            Ty::Error { .. } => return Err(NotRuntimeTy { variant: "Error" }),
            Ty::EvolvingList(..) => {
                return Err(NotRuntimeTy {
                    variant: "EvolvingList",
                });
            }
            Ty::EvolvingMap(..) => {
                return Err(NotRuntimeTy {
                    variant: "EvolvingMap",
                });
            }
        })
    }
}

impl TryFrom<Ty> for RuntimeTy {
    type Error = NotRuntimeTy;

    fn try_from(ty: Ty) -> Result<Self, Self::Error> {
        RuntimeTy::try_from(&ty)
    }
}

/// Convert each [`Ty`] in `tys` to a [`RuntimeTy`], short-circuiting on the
/// first compiler-only variant encountered (at any nesting depth).
fn try_vec(tys: &[Ty]) -> Result<Vec<RuntimeTy>, NotRuntimeTy> {
    tys.iter().map(RuntimeTy::try_from).collect()
}

impl From<&RuntimeTy> for Ty {
    fn from(ty: &RuntimeTy) -> Self {
        match ty {
            RuntimeTy::Int { attr } => Ty::Int { attr: attr.clone() },
            RuntimeTy::Bigint { attr } => Ty::Bigint { attr: attr.clone() },
            RuntimeTy::Float { attr } => Ty::Float { attr: attr.clone() },
            RuntimeTy::String { attr } => Ty::String { attr: attr.clone() },
            RuntimeTy::Bool { attr } => Ty::Bool { attr: attr.clone() },
            RuntimeTy::Null { attr } => Ty::Null { attr: attr.clone() },
            RuntimeTy::Uint8Array { attr } => Ty::Uint8Array { attr: attr.clone() },
            RuntimeTy::Media(kind, attr) => Ty::Media(*kind, attr.clone()),
            RuntimeTy::Literal(lit, freshness, attr) => {
                Ty::Literal(lit.clone(), *freshness, attr.clone())
            }
            RuntimeTy::Class(name, args, attr) => {
                Ty::Class(name.clone(), from_vec(args), attr.clone())
            }
            RuntimeTy::Interface(name, args, bindings, attr) => {
                let bindings = bindings
                    .iter()
                    .map(|(n, t)| (n.clone(), Ty::from(t)))
                    .collect();
                Ty::Interface(name.clone(), from_vec(args), bindings, attr.clone())
            }
            RuntimeTy::Enum(name, attr) => Ty::Enum(name.clone(), attr.clone()),
            RuntimeTy::EnumVariant(name, variant, attr) => {
                Ty::EnumVariant(name.clone(), variant.clone(), attr.clone())
            }
            RuntimeTy::List(inner, attr) => Ty::List(Box::new(Ty::from(&**inner)), attr.clone()),
            RuntimeTy::Map { key, value, attr } => Ty::Map {
                key: Box::new(Ty::from(&**key)),
                value: Box::new(Ty::from(&**value)),
                attr: attr.clone(),
            },
            RuntimeTy::Union(members, attr) => Ty::Union(from_vec(members), attr.clone()),
            RuntimeTy::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws,
                attr,
            } => Ty::Function {
                generic_params: generic_params.clone(),
                generic_param_bounds: generic_param_bounds
                    .iter()
                    .map(|b| b.as_ref().map(Ty::from))
                    .collect(),
                params: params
                    .iter()
                    .map(|p| crate::FunctionParamTy {
                        name: p.name.clone(),
                        ty: Ty::from(&p.ty),
                        mode: p.mode,
                    })
                    .collect(),
                ret: Box::new(Ty::from(&**ret)),
                throws: Box::new(Ty::from(&**throws)),
                attr: attr.clone(),
            },
            RuntimeTy::Future(value, error, attr) => Ty::Future(
                Box::new(Ty::from(&**value)),
                Box::new(Ty::from(&**error)),
                attr.clone(),
            ),
            RuntimeTy::RustType { attr } => Ty::RustType { attr: attr.clone() },
            RuntimeTy::Type { attr } => Ty::Type { attr: attr.clone() },
            RuntimeTy::Resource { attr } => Ty::Resource { attr: attr.clone() },
            RuntimeTy::PromptAst { attr } => Ty::PromptAst { attr: attr.clone() },
            RuntimeTy::Void { attr } => Ty::Void { attr: attr.clone() },
            RuntimeTy::WatchAccessor(inner, attr) => {
                Ty::WatchAccessor(Box::new(Ty::from(&**inner)), attr.clone())
            }
            RuntimeTy::TypeAlias(name, attr) => Ty::TypeAlias(name.clone(), attr.clone()),
            RuntimeTy::TypeVar(name, attr) => Ty::TypeVar(name.clone(), attr.clone()),
            RuntimeTy::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            } => Ty::AssociatedTypeProjection {
                base: Box::new(Ty::from(&**base)),
                interface: interface.as_ref().map(|i| Box::new(Ty::from(&**i))),
                member: member.clone(),
                attr: attr.clone(),
            },
            RuntimeTy::BuiltinUnknown { attr } => Ty::BuiltinUnknown { attr: attr.clone() },
            RuntimeTy::Never { attr } => Ty::Never { attr: attr.clone() },
        }
    }
}

impl From<RuntimeTy> for Ty {
    fn from(ty: RuntimeTy) -> Self {
        Ty::from(&ty)
    }
}

/// Infallibly convert each [`RuntimeTy`] back into a [`Ty`].
fn from_vec(tys: &[RuntimeTy]) -> Vec<Ty> {
    tys.iter().map(Ty::from).collect()
}

// ── Subset-hierarchy upcast ──────────────────────────────────────────────────
// `subenum` generates `From<ConcreteTy> for RuntimeTy`, `From<RealizedTy> for
// RuntimeTy`, and each subset's `TryFrom<RuntimeTy>`, but not the child→child
// cast. `ConcreteTy ⊆ RealizedTy` is guaranteed by the membership tags, so this
// upcast is infallible: round through `RuntimeTy`; the `TryFrom` cannot fail for
// a value already in the subset.
impl From<ConcreteTy> for RealizedTy {
    fn from(value: ConcreteTy) -> Self {
        RuntimeTy::from(value)
            .try_into()
            .unwrap_or_else(|_| unreachable!("every ConcreteTy is a RealizedTy"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> TyAttr {
        TyAttr::default()
    }

    fn qtn(name: &str) -> TypeName {
        TypeName::local(Name::new(name))
    }

    /// `Ty::from(RuntimeTy::try_from(&ty)) == ty` for a set of deeply nested
    /// runtime types.
    fn assert_round_trips(ty: Ty) {
        let runtime =
            RuntimeTy::try_from(&ty).unwrap_or_else(|e| panic!("expected a runtime type, got {e}"));
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
    fn round_trip_associated_type_projection() {
        let ty = Ty::AssociatedTypeProjection {
            base: Box::new(Ty::TypeVar(Name::new("T"), def())),
            interface: Some(Box::new(Ty::TypeAlias(qtn("Iterator"), def()))),
            member: Name::new("Item"),
            attr: def(),
        };
        assert_round_trips(ty);
    }

    #[test]
    fn nested_unknown_in_list_blocks_conversion() {
        let ty = Ty::List(Box::new(Ty::Unknown { attr: def() }), def());
        assert_eq!(
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy { variant: "Unknown" })
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
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy { variant: "Error" })
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
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy {
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
            RuntimeTy::try_from(&ty),
            Err(NotRuntimeTy {
                variant: "EvolvingMap"
            })
        );
    }

    #[test]
    fn concrete_upcasts_to_realized() {
        let concrete = ConcreteTy::Int { attr: def() };
        let realized: RealizedTy = concrete.into();
        assert_eq!(realized, RealizedTy::Int { attr: def() });
    }
}
