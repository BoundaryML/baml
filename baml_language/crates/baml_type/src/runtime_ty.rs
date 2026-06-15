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

use std::collections::{HashMap, HashSet};

use borsh::{BorshDeserialize, BorshSerialize};
use subenum::subenum;

use crate::{
    Freshness, FunctionParamMode, Literal, MediaKind, Name, QualifiedTypeName, Ty, TyAttr, TypeName,
};

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

impl RuntimeTy {
    // --- TyAttr accessor ---

    /// Get the [`TyAttr`] for this type.
    pub fn attr(&self) -> &TyAttr {
        match self {
            RuntimeTy::Int { attr }
            | RuntimeTy::Bigint { attr }
            | RuntimeTy::Float { attr }
            | RuntimeTy::String { attr }
            | RuntimeTy::Bool { attr }
            | RuntimeTy::Null { attr }
            | RuntimeTy::Uint8Array { attr }
            | RuntimeTy::Map { attr, .. }
            | RuntimeTy::Function { attr, .. }
            | RuntimeTy::AssociatedTypeProjection { attr, .. }
            | RuntimeTy::RustType { attr }
            | RuntimeTy::Type { attr }
            | RuntimeTy::Resource { attr }
            | RuntimeTy::PromptAst { attr }
            | RuntimeTy::Void { attr }
            | RuntimeTy::BuiltinUnknown { attr }
            | RuntimeTy::Never { attr } => attr,
            RuntimeTy::Media(_, attr)
            | RuntimeTy::Literal(_, _, attr)
            | RuntimeTy::Class(_, _, attr)
            | RuntimeTy::Interface(_, _, _, attr)
            | RuntimeTy::Enum(_, attr)
            | RuntimeTy::EnumVariant(_, _, attr)
            | RuntimeTy::List(_, attr)
            | RuntimeTy::Union(_, attr)
            | RuntimeTy::Future(_, _, attr)
            | RuntimeTy::WatchAccessor(_, attr)
            | RuntimeTy::TypeAlias(_, attr)
            | RuntimeTy::TypeVar(_, attr) => attr,
        }
    }

    /// Replace the [`TyAttr`] on this type, returning a new value with `attr`.
    pub fn with_attr(self, attr: TyAttr) -> RuntimeTy {
        match self {
            RuntimeTy::Int { .. } => RuntimeTy::Int { attr },
            RuntimeTy::Bigint { .. } => RuntimeTy::Bigint { attr },
            RuntimeTy::Float { .. } => RuntimeTy::Float { attr },
            RuntimeTy::String { .. } => RuntimeTy::String { attr },
            RuntimeTy::Bool { .. } => RuntimeTy::Bool { attr },
            RuntimeTy::Null { .. } => RuntimeTy::Null { attr },
            RuntimeTy::Uint8Array { .. } => RuntimeTy::Uint8Array { attr },
            RuntimeTy::Media(kind, _) => RuntimeTy::Media(kind, attr),
            RuntimeTy::Literal(lit, freshness, _) => RuntimeTy::Literal(lit, freshness, attr),
            RuntimeTy::Class(tn, args, _) => RuntimeTy::Class(tn, args, attr),
            RuntimeTy::Interface(tn, args, bindings, _) => {
                RuntimeTy::Interface(tn, args, bindings, attr)
            }
            RuntimeTy::Enum(tn, _) => RuntimeTy::Enum(tn, attr),
            RuntimeTy::EnumVariant(tn, v, _) => RuntimeTy::EnumVariant(tn, v, attr),
            RuntimeTy::List(inner, _) => RuntimeTy::List(inner, attr),
            RuntimeTy::Map { key, value, .. } => RuntimeTy::Map { key, value, attr },
            RuntimeTy::Union(members, _) => RuntimeTy::Union(members, attr),
            RuntimeTy::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws,
                ..
            } => RuntimeTy::Function {
                generic_params,
                generic_param_bounds,
                params,
                ret,
                throws,
                attr,
            },
            RuntimeTy::Future(value, error, _) => RuntimeTy::Future(value, error, attr),
            RuntimeTy::RustType { .. } => RuntimeTy::RustType { attr },
            RuntimeTy::Type { .. } => RuntimeTy::Type { attr },
            RuntimeTy::Resource { .. } => RuntimeTy::Resource { attr },
            RuntimeTy::PromptAst { .. } => RuntimeTy::PromptAst { attr },
            RuntimeTy::Void { .. } => RuntimeTy::Void { attr },
            RuntimeTy::WatchAccessor(inner, _) => RuntimeTy::WatchAccessor(inner, attr),
            RuntimeTy::TypeAlias(tn, _) => RuntimeTy::TypeAlias(tn, attr),
            RuntimeTy::TypeVar(name, _) => RuntimeTy::TypeVar(name, attr),
            RuntimeTy::AssociatedTypeProjection {
                base,
                interface,
                member,
                ..
            } => RuntimeTy::AssociatedTypeProjection {
                base,
                interface,
                member,
                attr,
            },
            RuntimeTy::BuiltinUnknown { .. } => RuntimeTy::BuiltinUnknown { attr },
            RuntimeTy::Never { .. } => RuntimeTy::Never { attr },
        }
    }

    // --- Primitive constructors (default TyAttr) ---

    /// `int` with default attributes.
    pub fn int() -> Self {
        RuntimeTy::Int {
            attr: TyAttr::default(),
        }
    }

    /// `bigint` with default attributes.
    pub fn bigint() -> Self {
        RuntimeTy::Bigint {
            attr: TyAttr::default(),
        }
    }

    /// `float` with default attributes.
    pub fn float() -> Self {
        RuntimeTy::Float {
            attr: TyAttr::default(),
        }
    }

    /// `string` with default attributes.
    pub fn string() -> Self {
        RuntimeTy::String {
            attr: TyAttr::default(),
        }
    }

    /// `bool` with default attributes.
    pub fn bool() -> Self {
        RuntimeTy::Bool {
            attr: TyAttr::default(),
        }
    }

    /// `null` with default attributes.
    pub fn null() -> Self {
        RuntimeTy::Null {
            attr: TyAttr::default(),
        }
    }

    /// `uint8array` with default attributes.
    pub fn uint8array() -> Self {
        RuntimeTy::Uint8Array {
            attr: TyAttr::default(),
        }
    }

    // --- Compound constructors (default TyAttr) ---

    /// `T[]` (list) with default attributes.
    pub fn list(inner: RuntimeTy) -> Self {
        RuntimeTy::List(Box::new(inner), TyAttr::default())
    }

    /// `A | B | ...` (union) with default attributes.
    pub fn union(members: impl IntoIterator<Item = RuntimeTy>) -> Self {
        RuntimeTy::Union(members.into_iter().collect(), TyAttr::default())
    }

    /// `T?` (optional) — sugar for `T | null`. Mirrors [`Ty::optional`]: the
    /// result is flattened and idempotent.
    pub fn optional(inner: RuntimeTy) -> Self {
        match inner {
            RuntimeTy::Union(mut members, attr) => {
                if !members.iter().any(RuntimeTy::is_null) {
                    members.push(RuntimeTy::null());
                }
                RuntimeTy::Union(members, attr)
            }
            n @ RuntimeTy::Null { .. } => n,
            other => RuntimeTy::Union(vec![other, RuntimeTy::null()], TyAttr::default()),
        }
    }

    /// `Class(name)` with default attributes (local module path), no type args.
    pub fn class(name: &str) -> Self {
        RuntimeTy::Class(TypeName::local(name.into()), Vec::new(), TyAttr::default())
    }

    /// `Class(name, args)` — a parametric class instantiation.
    pub fn class_with_args(name: TypeName, args: Vec<RuntimeTy>) -> Self {
        RuntimeTy::Class(name, args, TyAttr::default())
    }

    /// `Class(name)` under the implicit `user` package, no type args.
    pub fn user_class(name: &str) -> Self {
        RuntimeTy::Class(
            TypeName::local(Name::new(name)),
            Vec::new(),
            TyAttr::default(),
        )
    }

    /// `Class(name, args)` under the implicit `user` package.
    pub fn user_class_with_args(name: &str, args: Vec<RuntimeTy>) -> Self {
        RuntimeTy::Class(TypeName::local(Name::new(name)), args, TyAttr::default())
    }

    /// `unknown` (the top type) with default attributes.
    pub fn unknown() -> Self {
        RuntimeTy::BuiltinUnknown {
            attr: TyAttr::default(),
        }
    }

    /// Opaque resource handle type (file, socket, HTTP response body).
    pub fn resource() -> Self {
        RuntimeTy::Resource {
            attr: TyAttr::default(),
        }
    }

    /// Opaque structured prompt tree type for LLM calls.
    pub fn prompt_ast() -> Self {
        RuntimeTy::PromptAst {
            attr: TyAttr::default(),
        }
    }

    /// Meta-type — a runtime value that wraps a [`RuntimeTy`].
    pub fn type_type() -> Self {
        RuntimeTy::Type {
            attr: TyAttr::default(),
        }
    }

    // --- Queries ---

    /// True if this is exactly the `null` type.
    pub fn is_null(&self) -> bool {
        matches!(self, RuntimeTy::Null { .. })
    }

    /// True if this is a union that includes `null` — i.e. an optional type.
    pub fn is_nullable_union(&self) -> bool {
        matches!(self, RuntimeTy::Union(members, _) if members.iter().any(RuntimeTy::is_null))
    }

    /// Check if this is the void type.
    pub fn is_void(&self) -> bool {
        matches!(self, RuntimeTy::Void { .. })
    }

    /// Check if this is a primitive type (including literals of primitive types).
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            RuntimeTy::Int { .. }
                | RuntimeTy::Bigint { .. }
                | RuntimeTy::Float { .. }
                | RuntimeTy::String { .. }
                | RuntimeTy::Bool { .. }
                | RuntimeTy::Null { .. }
                | RuntimeTy::Uint8Array { .. }
                | RuntimeTy::Literal(..)
        )
    }

    // --- Transforms ---

    /// Remove `null` from a nullable union, collapsing the result. The inverse
    /// of [`RuntimeTy::optional`]; mirrors [`Ty::strip_null`].
    pub fn strip_null(&self) -> RuntimeTy {
        match self {
            RuntimeTy::Union(members, attr) => {
                let non_null: Vec<RuntimeTy> =
                    members.iter().filter(|m| !m.is_null()).cloned().collect();
                match non_null.len() {
                    0 => self.clone(),
                    1 => non_null
                        .into_iter()
                        .next()
                        .unwrap_or_else(|| unreachable!("len checked")),
                    _ => RuntimeTy::Union(non_null, attr.clone()),
                }
            }
            _ => self.clone(),
        }
    }

    // --- Rendering / subtyping ---
    //
    // These reuse `Ty`'s implementation via the infallible upcast so the
    // structural logic lives in exactly one place. The value remains a
    // statically runtime-safe `RuntimeTy`; the upcast is purely to share the
    // algorithm. None of these are on a VM-hot path.

    /// User-facing rendering — see [`Ty::render_user_facing`].
    pub fn render_user_facing(&self) -> String {
        Ty::from(self).render_user_facing()
    }

    /// Canonical structural rendering — see [`Ty::render_canonical`].
    pub fn render_canonical(&self) -> String {
        Ty::from(self).render_canonical()
    }

    /// Render with a custom strategy — see [`Ty::render_with`].
    pub fn render_with(&self, s: &dyn crate::TyRenderStrategy) -> String {
        Ty::from(self).render_with(s)
    }

    /// Structural subtyping — see [`Ty::is_subtype_of`].
    pub fn is_subtype_of(&self, other: &RuntimeTy) -> bool {
        Ty::from(self).is_subtype_of(&Ty::from(other))
    }
}

impl std::fmt::Display for RuntimeTy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&Ty::from(self), f)
    }
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

// ── Ty → RuntimeTy erasure ───────────────────────────────────────────────────
// The erasing counterpart of `RuntimeTy::try_from`: where `try_from` *rejects*
// compiler-only variants, `convert_tir_ty_for_runtime` *erases* them and additionally
// expands non-recursive type aliases inline. This is the single boundary the
// compiler crosses to hand a `Ty` to the runtime.

/// The resolved type-alias environment needed to erase a [`Ty`] into a
/// [`RuntimeTy`]: the alias targets to expand and the set of recursive aliases
/// to keep opaque. Built per package by the compiler (see
/// `baml_compiler2_mir::resolved_aliases_for_package`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolvedAliases {
    pub aliases: HashMap<QualifiedTypeName, Ty>,
    pub recursive: HashSet<QualifiedTypeName>,
}

impl ResolvedAliases {
    /// Lower a [`Ty`] into a [`RuntimeTy`] using this alias environment.
    ///
    /// This is the compiler's ergonomic entry point and asserts the conversion
    /// succeeds. Per the type-system golden rule (prefer compiler errors over
    /// type-erasure), reaching here with an `Unknown`/`Error` sentinel means an
    /// error-recovery type slipped past type-checking into MIR lowering — a
    /// compiler bug — so it panics loudly rather than silently producing a
    /// degraded type. Callers that genuinely tolerate failure use
    /// [`lower_to_runtime`] directly.
    pub fn convert(&self, ty: &Ty) -> RuntimeTy {
        lower_to_runtime(ty, self).unwrap_or_else(|e| {
            unreachable!("{e}: an error-recovery type reached runtime lowering")
        })
    }
}

/// Lower a compiler-facing [`Ty`] into a runtime-safe [`RuntimeTy`], expanding
/// non-recursive type aliases inline and freezing evolving containers
/// (`EvolvingList`/`EvolvingMap` → `List`/`Map`). Every other variant —
/// including `Never`, `TypeVar`, and `AssociatedTypeProjection` — maps
/// faithfully to its same-named [`RuntimeTy`] variant: the runtime carries them
/// for reflection and dynamic dispatch, and erasing them would violate the
/// type contract.
///
/// Fails with [`NotRuntimeTy`] on the error-recovery sentinels `Unknown` and
/// `Error`: those exist only during compilation, so a type-checked program can
/// never contain one. Reaching this boundary with one is a compiler bug — we
/// surface it instead of erasing it to a degraded runtime type.
pub fn lower_to_runtime(ty: &Ty, resolved: &ResolvedAliases) -> Result<RuntimeTy, NotRuntimeTy> {
    Ok(match ty {
        // Primitives — same-named runtime variant.
        Ty::Int { attr } => RuntimeTy::Int { attr: attr.clone() },
        Ty::Bigint { attr } => RuntimeTy::Bigint { attr: attr.clone() },
        Ty::Float { attr } => RuntimeTy::Float { attr: attr.clone() },
        Ty::String { attr } => RuntimeTy::String { attr: attr.clone() },
        Ty::Bool { attr } => RuntimeTy::Bool { attr: attr.clone() },
        Ty::Null { attr } => RuntimeTy::Null { attr: attr.clone() },
        Ty::Uint8Array { attr } => RuntimeTy::Uint8Array { attr: attr.clone() },
        Ty::Media(kind, attr) => RuntimeTy::Media(*kind, attr.clone()),

        // Named types
        Ty::Class(qtn, type_args, attr) => {
            RuntimeTy::Class(qtn.clone(), lower_vec(type_args, resolved)?, attr.clone())
        }
        Ty::Interface(qtn, type_args, associated_bindings, attr) => {
            let resolved_args = lower_vec(type_args, resolved)?;
            let resolved_bindings = associated_bindings
                .iter()
                .map(|(name, ty)| Ok((name.clone(), lower_to_runtime(ty, resolved)?)))
                .collect::<Result<Vec<_>, NotRuntimeTy>>()?;
            RuntimeTy::Interface(qtn.clone(), resolved_args, resolved_bindings, attr.clone())
        }
        Ty::Enum(qtn, attr) => RuntimeTy::Enum(qtn.clone(), attr.clone()),
        Ty::TypeAlias(qtn, attr) => {
            if resolved.recursive.contains(qtn) {
                // Keep recursive aliases opaque — they need runtime resolution
                RuntimeTy::TypeAlias(qtn.clone(), attr.clone())
            } else if let Some(target) = resolved.aliases.get(qtn) {
                // Expand non-recursive aliases inline
                lower_to_runtime(target, resolved)?
            } else {
                // Unknown alias (e.g. from another package) — keep opaque
                RuntimeTy::TypeAlias(qtn.clone(), attr.clone())
            }
        }

        // EnumVariant → preserve variant-level type info
        Ty::EnumVariant(qtn, variant, attr) => {
            RuntimeTy::EnumVariant(qtn.clone(), variant.clone(), attr.clone())
        }

        // Containers
        Ty::List(inner, attr) => {
            RuntimeTy::List(Box::new(lower_to_runtime(inner, resolved)?), attr.clone())
        }
        Ty::Map {
            key: k,
            value: v,
            attr,
        } => RuntimeTy::Map {
            key: Box::new(lower_to_runtime(k, resolved)?),
            value: Box::new(lower_to_runtime(v, resolved)?),
            attr: attr.clone(),
        },
        Ty::Union(members, attr) => RuntimeTy::Union(lower_vec(members, resolved)?, attr.clone()),
        // Freshness is a compiler-only flag; runtime literal types are uniform,
        // so normalize to `Regular` at the boundary.
        Ty::Literal(lit, _freshness, attr) => {
            RuntimeTy::Literal(lit.clone(), Freshness::Regular, attr.clone())
        }

        // Evolving containers → freeze to regular containers
        Ty::EvolvingList(inner, attr) => {
            RuntimeTy::List(Box::new(lower_to_runtime(inner, resolved)?), attr.clone())
        }
        Ty::EvolvingMap(k, v, attr) => RuntimeTy::Map {
            key: Box::new(lower_to_runtime(k, resolved)?),
            value: Box::new(lower_to_runtime(v, resolved)?),
            attr: attr.clone(),
        },

        // Functions — preserve the declared generics + param metadata (kept at
        // runtime for reflection); body type-vars are resolved faithfully by
        // the recursive `lower_to_runtime` calls.
        Ty::Function {
            generic_params,
            generic_param_bounds,
            params,
            ret,
            throws,
            attr,
        } => RuntimeTy::Function {
            generic_params: generic_params.clone(),
            generic_param_bounds: generic_param_bounds
                .iter()
                .map(|b| {
                    b.as_ref()
                        .map(|t| lower_to_runtime(t, resolved))
                        .transpose()
                })
                .collect::<Result<Vec<_>, NotRuntimeTy>>()?,
            params: params
                .iter()
                .map(|param| {
                    Ok(RuntimeFunctionParamTy {
                        name: param.name.clone(),
                        ty: lower_to_runtime(&param.ty, resolved)?,
                        mode: param.mode,
                    })
                })
                .collect::<Result<Vec<_>, NotRuntimeTy>>()?,
            ret: Box::new(lower_to_runtime(ret, resolved)?),
            throws: Box::new(lower_to_runtime(throws, resolved)?),
            attr: attr.clone(),
        },

        // Bottom, opaque-leaf, and reflection types map faithfully.
        Ty::Never { attr } => RuntimeTy::Never { attr: attr.clone() },
        Ty::Void { attr } => RuntimeTy::Void { attr: attr.clone() },
        Ty::BuiltinUnknown { attr } => RuntimeTy::BuiltinUnknown { attr: attr.clone() },
        Ty::RustType { attr } => RuntimeTy::RustType { attr: attr.clone() },
        Ty::Type { attr } => RuntimeTy::Type { attr: attr.clone() },
        Ty::Resource { attr } => RuntimeTy::Resource { attr: attr.clone() },
        Ty::PromptAst { attr } => RuntimeTy::PromptAst { attr: attr.clone() },
        Ty::TypeVar(name, attr) => RuntimeTy::TypeVar(name.clone(), attr.clone()),
        Ty::AssociatedTypeProjection {
            base,
            interface,
            member,
            attr,
        } => RuntimeTy::AssociatedTypeProjection {
            base: Box::new(lower_to_runtime(base, resolved)?),
            interface: interface
                .as_ref()
                .map(|i| Ok::<_, NotRuntimeTy>(Box::new(lower_to_runtime(i, resolved)?)))
                .transpose()?,
            member: member.clone(),
            attr: attr.clone(),
        },

        // BEP-034: future types pass through unchanged with both
        // value and error type parameters mapped.
        Ty::Future(value, error, attr) => RuntimeTy::Future(
            Box::new(lower_to_runtime(value, resolved)?),
            Box::new(lower_to_runtime(error, resolved)?),
            attr.clone(),
        ),
        Ty::WatchAccessor(inner, attr) => {
            RuntimeTy::WatchAccessor(Box::new(lower_to_runtime(inner, resolved)?), attr.clone())
        }

        // Error-recovery sentinels cannot exist in a type-checked program.
        Ty::Unknown { .. } => return Err(NotRuntimeTy { variant: "Unknown" }),
        Ty::Error { .. } => return Err(NotRuntimeTy { variant: "Error" }),
    })
}

/// Lower each [`Ty`] in `tys`, short-circuiting on the first error-recovery
/// sentinel encountered (at any nesting depth).
fn lower_vec(tys: &[Ty], resolved: &ResolvedAliases) -> Result<Vec<RuntimeTy>, NotRuntimeTy> {
    tys.iter().map(|t| lower_to_runtime(t, resolved)).collect()
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
