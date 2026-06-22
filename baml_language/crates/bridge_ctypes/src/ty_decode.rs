//! Decode the wire `Ty` (`baml_type.proto`) into a `baml_type::RuntimeTy`.
//!
//! Two host-provided "type as a value" paths share this:
//!   - `CallFunctionArgs.type_args` — explicit, named `TypeVar` bindings for a
//!     generic function/method call (`TyArg { type_var, type_value }`). The
//!     host sends them in De Bruijn order (enclosing class params first, then
//!     the callee's own params); the engine maps each named binding onto the
//!     entry frame's `type_args` slot by `TypeVar` name in
//!     `set_entry_point_with_type_args`.
//!   - `InboundValue.ty_value` — a reflected type passed as an argument value
//!     (decoded into a `type`-valued `BexExternalAdt::Type`).

use baml_type::{
    FunctionParamMode, MediaKind, Name, RuntimeFunctionParamTy, RuntimeTy, TyAttr, TypeName,
};
use bex_project::{BexExternalAdt, BexExternalValue};
use indexmap::IndexMap;

use crate::{
    baml_core::cffi::{
        Ty, TyArg, TyFunctionParam, TyFunctionParamMode, TyMediaKind, TyPrimitiveKind,
        ty::Ty as TyVariant, ty_literal::Literal as TyLiteralVariant,
    },
    error::CtypesError,
};

/// Decode `CallFunctionArgs.type_args` (a list of named `TyArg`s) into a
/// `TypeVar name -> concrete RuntimeTy` map, preserving wire order (the map's
/// insertion order is the host's De Bruijn order). A `TyArg` with an absent
/// `type_value` decodes to the unknown/top type, mirroring
/// [`proto_ty_to_runtime_ty`]'s rollout-safe default. A repeated `type_var`
/// keeps the last binding. The engine resolves the names against the callee's
/// generic params when seeding the entry frame.
pub fn proto_ty_args_to_named(
    type_args: &[TyArg],
) -> Result<IndexMap<String, RuntimeTy>, CtypesError> {
    type_args
        .iter()
        .map(|arg| {
            let ty = match arg.type_value.as_ref() {
                Some(ty) => proto_ty_to_runtime_ty(ty)?,
                None => RuntimeTy::unknown(),
            };
            Ok((arg.type_var.clone(), ty))
        })
        .collect()
}

/// Convert a wire `Ty` into a `RuntimeTy`. A `Ty` with no variant set decodes
/// to the unknown/top type — the rollout-safe default (an absent binding
/// constrains nothing).
pub fn proto_ty_to_runtime_ty(ty: &Ty) -> Result<RuntimeTy, CtypesError> {
    let Some(variant) = ty.ty.as_ref() else {
        return Ok(RuntimeTy::unknown());
    };
    Ok(match variant {
        TyVariant::Primitive(p) => primitive_to_runtime_ty(p.kind),
        TyVariant::ClassTy(c) => {
            let type_name = TypeName::from_dotted_path(&c.name);
            let args = decode_type_args(&c.type_args)?;
            RuntimeTy::class_with_args(type_name, args)
        }
        TyVariant::Enum(e) => {
            RuntimeTy::Enum(TypeName::from_dotted_path(&e.name), TyAttr::default())
        }
        TyVariant::List(l) => RuntimeTy::list(opt_to_runtime_ty(l.item.as_deref())?),
        TyVariant::Map(m) => RuntimeTy::Map {
            key: Box::new(opt_to_runtime_ty(m.key.as_deref())?),
            value: Box::new(opt_to_runtime_ty(m.value.as_deref())?),
            attr: TyAttr::default(),
        },
        TyVariant::Optional(o) => RuntimeTy::optional(opt_to_runtime_ty(o.inner.as_deref())?),
        TyVariant::Union(u) => RuntimeTy::union(
            u.options
                .iter()
                .map(proto_ty_to_runtime_ty)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        TyVariant::Literal(lit) => literal_to_runtime_ty(lit.literal.as_ref()),
        TyVariant::TypeAlias(n) => {
            RuntimeTy::TypeAlias(TypeName::from_dotted_path(&n.name), TyAttr::default())
        }
        TyVariant::Unknown(_) => RuntimeTy::unknown(),
        TyVariant::Media(m) => RuntimeTy::Media(media_kind_to_runtime(m.kind), TyAttr::default()),
        TyVariant::Interface(i) => {
            let type_name = TypeName::from_dotted_path(&i.name);
            let args = decode_type_args(&i.type_args)?;
            let bindings = i
                .bindings
                .iter()
                .map(|b| Ok((Name::new(&b.name), opt_to_runtime_ty(b.ty.as_ref())?)))
                .collect::<Result<Vec<_>, CtypesError>>()?;
            RuntimeTy::Interface(type_name, args, bindings, TyAttr::default())
        }
        TyVariant::EnumVariant(e) => RuntimeTy::EnumVariant(
            TypeName::from_dotted_path(&e.name),
            Name::new(&e.variant),
            TyAttr::default(),
        ),
        TyVariant::Function(f) => RuntimeTy::Function {
            generic_params: f.generic_params.iter().map(Name::new).collect(),
            generic_param_bounds: f
                .generic_param_bounds
                .iter()
                .map(|b| b.ty.as_ref().map(proto_ty_to_runtime_ty).transpose())
                .collect::<Result<Vec<_>, _>>()?,
            params: f
                .params
                .iter()
                .map(function_param_to_runtime)
                .collect::<Result<Vec<_>, _>>()?,
            ret: Box::new(opt_to_runtime_ty(f.ret.as_deref())?),
            throws: Box::new(opt_to_runtime_ty(f.throws.as_deref())?),
            attr: TyAttr::default(),
        },
        TyVariant::Future(fut) => RuntimeTy::Future(
            Box::new(opt_to_runtime_ty(fut.value.as_deref())?),
            Box::new(opt_to_runtime_ty(fut.error.as_deref())?),
            TyAttr::default(),
        ),
        TyVariant::RustType(_) => RuntimeTy::RustType {
            attr: TyAttr::default(),
        },
        TyVariant::MetaType(_) => RuntimeTy::Type {
            attr: TyAttr::default(),
        },
        TyVariant::Resource(_) => RuntimeTy::Resource {
            attr: TyAttr::default(),
        },
        TyVariant::PromptAst(_) => RuntimeTy::PromptAst {
            attr: TyAttr::default(),
        },
        TyVariant::Void(_) => RuntimeTy::Void {
            attr: TyAttr::default(),
        },
        TyVariant::WatchAccessor(w) => RuntimeTy::WatchAccessor(
            Box::new(opt_to_runtime_ty(w.inner.as_deref())?),
            TyAttr::default(),
        ),
        TyVariant::TypeVar(v) => RuntimeTy::TypeVar(Name::new(&v.name), TyAttr::default()),
        TyVariant::AssociatedTypeProjection(p) => RuntimeTy::AssociatedTypeProjection {
            base: Box::new(opt_to_runtime_ty(p.base.as_deref())?),
            interface: p
                .interface
                .as_deref()
                .map(proto_ty_to_runtime_ty)
                .transpose()?
                .map(Box::new),
            member: Name::new(&p.member),
            attr: TyAttr::default(),
        },
        TyVariant::Never(_) => RuntimeTy::Never {
            attr: TyAttr::default(),
        },
    })
}

/// Decode a wire `Ty` into a `type`-valued external value (for
/// `InboundValue.ty_value`).
pub fn proto_ty_to_external(ty: &Ty) -> Result<BexExternalValue, CtypesError> {
    Ok(BexExternalValue::Adt(BexExternalAdt::Type(
        proto_ty_to_runtime_ty(ty)?,
    )))
}

fn decode_type_args(type_args: &[Ty]) -> Result<Vec<RuntimeTy>, CtypesError> {
    type_args
        .iter()
        .map(proto_ty_to_runtime_ty)
        .collect::<Result<Vec<_>, _>>()
}

fn opt_to_runtime_ty(opt: Option<&Ty>) -> Result<RuntimeTy, CtypesError> {
    match opt {
        Some(t) => proto_ty_to_runtime_ty(t),
        None => Ok(RuntimeTy::unknown()),
    }
}

fn primitive_to_runtime_ty(kind: i32) -> RuntimeTy {
    match TyPrimitiveKind::try_from(kind).unwrap_or(TyPrimitiveKind::TyPrimitiveUnspecified) {
        TyPrimitiveKind::TyPrimitiveString => RuntimeTy::string(),
        TyPrimitiveKind::TyPrimitiveInt => RuntimeTy::int(),
        TyPrimitiveKind::TyPrimitiveFloat => RuntimeTy::float(),
        TyPrimitiveKind::TyPrimitiveBool => RuntimeTy::bool(),
        TyPrimitiveKind::TyPrimitiveNull => RuntimeTy::null(),
        TyPrimitiveKind::TyPrimitiveBytes => RuntimeTy::Uint8Array {
            attr: TyAttr::default(),
        },
        TyPrimitiveKind::TyPrimitiveBigint => RuntimeTy::Bigint {
            attr: TyAttr::default(),
        },
        TyPrimitiveKind::TyPrimitiveUnspecified => RuntimeTy::unknown(),
    }
}

fn media_kind_to_runtime(kind: i32) -> MediaKind {
    match TyMediaKind::try_from(kind).unwrap_or(TyMediaKind::Unspecified) {
        TyMediaKind::Image => MediaKind::Image,
        TyMediaKind::Audio => MediaKind::Audio,
        TyMediaKind::Video => MediaKind::Video,
        TyMediaKind::Pdf => MediaKind::Pdf,
        // An unspecified kind is the permissive "any media" type.
        TyMediaKind::Generic | TyMediaKind::Unspecified => MediaKind::Generic,
    }
}

fn function_param_to_runtime(p: &TyFunctionParam) -> Result<RuntimeFunctionParamTy, CtypesError> {
    Ok(RuntimeFunctionParamTy {
        name: p.name.as_ref().map(Name::new),
        ty: opt_to_runtime_ty(p.ty.as_ref())?,
        mode: function_param_mode(p.mode),
    })
}

fn function_param_mode(mode: i32) -> FunctionParamMode {
    match TyFunctionParamMode::try_from(mode).unwrap_or(TyFunctionParamMode::Unspecified) {
        TyFunctionParamMode::Optional => FunctionParamMode::Optional,
        // Required is the conservative default for an unspecified mode.
        TyFunctionParamMode::Required | TyFunctionParamMode::Unspecified => {
            FunctionParamMode::Required
        }
    }
}

fn literal_to_runtime_ty(lit: Option<&TyLiteralVariant>) -> RuntimeTy {
    // Literal types widen to their base primitive: binding a TypeVar to a
    // literal is exotic, and the base type is the safe lowering.
    match lit {
        Some(TyLiteralVariant::StringValue(_)) => RuntimeTy::string(),
        Some(TyLiteralVariant::IntValue(_)) => RuntimeTy::int(),
        Some(TyLiteralVariant::BoolValue(_)) => RuntimeTy::bool(),
        Some(TyLiteralVariant::BigintValue(_)) => RuntimeTy::bigint(),
        Some(TyLiteralVariant::FloatValue(_)) => RuntimeTy::float(),
        None => RuntimeTy::unknown(),
    }
}
