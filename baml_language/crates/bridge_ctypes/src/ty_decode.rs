//! Decode the wire `BamlTy` (`baml_type.proto`) into a `baml_type::RuntimeTy`.
//!
//! Two host-provided "type as a value" paths share this:
//!   - `CallFunctionArgs.type_args` — explicit, named `TypeVar` bindings for a
//!     generic function/method call (`BamlTyArg { type_var, type_value }`). The
//!     host sends them in De Bruijn order (enclosing class params first, then
//!     the callee's own params); the engine maps each named binding onto the
//!     entry frame's `type_args` slot by `TypeVar` name in
//!     `set_entry_point_with_type_args`.
//!   - `InboundValue.ty_value` — a reflected type passed as an argument value
//!     (decoded into a `type`-valued `BexExternalAdt::Type`).

use baml_type::{
    FunctionParamMode, MediaKind, Name, RuntimeFunctionParamTy, RuntimeInterface, RuntimeTy,
    TyAttr, TypeName,
};
use bex_project::{BexExternalAdt, BexExternalValue};
use indexmap::IndexMap;

use crate::{
    baml_bridge::cffi::{
        BamlTy, BamlTyArg, BamlTyFunctionParam, BamlTyFunctionParamMode, BamlTyMediaKind,
        BamlTyPrimitiveKind, baml_ty::Ty as TyVariant,
        baml_ty_literal::Literal as TyLiteralVariant,
    },
    error::CtypesError,
};

/// Decode `CallFunctionArgs.type_args` (a list of named `BamlTyArg`s) into a
/// `TypeVar name -> concrete RuntimeTy` map, preserving wire order (the map's
/// insertion order is the host's De Bruijn order). A `BamlTyArg` with an absent
/// `type_value` decodes to the unknown/top type, mirroring
/// [`proto_ty_to_runtime_ty`]'s rollout-safe default. A repeated `type_var`
/// keeps the last binding. The engine resolves the names against the callee's
/// generic params when seeding the entry frame.
pub fn proto_ty_args_to_named(
    type_args: &[BamlTyArg],
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

/// Convert a wire `BamlTy` into a `RuntimeTy`. A `BamlTy` with no variant set decodes
/// to the unknown/top type — the rollout-safe default (an absent binding
/// constrains nothing).
pub fn proto_ty_to_runtime_ty(ty: &BamlTy) -> Result<RuntimeTy, CtypesError> {
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
            let bindings = i
                .bindings
                .iter()
                .map(|b| Ok((Name::new(&b.name), opt_to_runtime_ty(b.ty.as_ref())?)))
                .collect::<Result<Vec<_>, CtypesError>>()?;
            // The wire order is untrusted (a non-BAML or future producer may send
            // bindings in any order) and `RuntimeInterface`'s derived `Eq`/`Hash`
            // are order-sensitive, so route through `RuntimeInterface::new` to sort
            // them, then lift the constraint to its existential type.
            let constraint = RuntimeInterface::new(
                TypeName::from_dotted_path(&i.name),
                decode_type_args(&i.type_args)?,
                bindings,
            );
            RuntimeTy::Interface(
                constraint.name,
                constraint.generics,
                constraint.associated_types,
                TyAttr::default(),
            )
        }
        TyVariant::EnumVariant(e) => RuntimeTy::EnumVariant(
            TypeName::from_dotted_path(&e.name),
            Name::new(&e.variant),
            TyAttr::default(),
        ),
        TyVariant::Function(f) => RuntimeTy::Function {
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
            // The projection's interface constraint is wired as a `Ty` and must
            // decode to an interface existential, from which the constraint
            // (`RuntimeInterface`) is recovered.
            interface: match p.interface.as_deref() {
                Some(ty) => match proto_ty_to_runtime_ty(ty)? {
                    RuntimeTy::Interface(name, generics, associated_types, _) => Some(Box::new(
                        RuntimeInterface::new(name, generics, associated_types),
                    )),
                    _ => {
                        return Err(CtypesError::InternalError(
                            "AssociatedTypeProjection.interface did not decode to an interface type"
                                .to_string(),
                        ));
                    }
                },
                None => None,
            },
            member: Name::new(&p.member),
            attr: TyAttr::default(),
        },
        TyVariant::Never(_) => RuntimeTy::Never {
            attr: TyAttr::default(),
        },
    })
}

/// Decode a wire `BamlTy` into a `type`-valued external value (for
/// `InboundValue.ty_value`).
pub fn proto_ty_to_external(ty: &BamlTy) -> Result<BexExternalValue, CtypesError> {
    Ok(BexExternalValue::Adt(BexExternalAdt::Type(
        proto_ty_to_runtime_ty(ty)?,
    )))
}

fn decode_type_args(type_args: &[BamlTy]) -> Result<Vec<RuntimeTy>, CtypesError> {
    type_args
        .iter()
        .map(proto_ty_to_runtime_ty)
        .collect::<Result<Vec<_>, _>>()
}

fn opt_to_runtime_ty(opt: Option<&BamlTy>) -> Result<RuntimeTy, CtypesError> {
    match opt {
        Some(t) => proto_ty_to_runtime_ty(t),
        None => Ok(RuntimeTy::unknown()),
    }
}

fn primitive_to_runtime_ty(kind: i32) -> RuntimeTy {
    match BamlTyPrimitiveKind::try_from(kind)
        .unwrap_or(BamlTyPrimitiveKind::BamlTyPrimitiveUnspecified)
    {
        BamlTyPrimitiveKind::BamlTyPrimitiveString => RuntimeTy::string(),
        BamlTyPrimitiveKind::BamlTyPrimitiveInt => RuntimeTy::int(),
        BamlTyPrimitiveKind::BamlTyPrimitiveFloat => RuntimeTy::float(),
        BamlTyPrimitiveKind::BamlTyPrimitiveBool => RuntimeTy::bool(),
        BamlTyPrimitiveKind::BamlTyPrimitiveNull => RuntimeTy::null(),
        BamlTyPrimitiveKind::BamlTyPrimitiveBytes => RuntimeTy::Uint8Array {
            attr: TyAttr::default(),
        },
        BamlTyPrimitiveKind::BamlTyPrimitiveBigint => RuntimeTy::Bigint {
            attr: TyAttr::default(),
        },
        BamlTyPrimitiveKind::BamlTyPrimitiveUnspecified => RuntimeTy::unknown(),
    }
}

fn media_kind_to_runtime(kind: i32) -> MediaKind {
    match BamlTyMediaKind::try_from(kind).unwrap_or(BamlTyMediaKind::Unspecified) {
        BamlTyMediaKind::Image => MediaKind::Image,
        BamlTyMediaKind::Audio => MediaKind::Audio,
        BamlTyMediaKind::Video => MediaKind::Video,
        BamlTyMediaKind::Pdf => MediaKind::Pdf,
        // An unspecified kind is the permissive "any media" type.
        BamlTyMediaKind::Generic | BamlTyMediaKind::Unspecified => MediaKind::Generic,
    }
}

fn function_param_to_runtime(
    p: &BamlTyFunctionParam,
) -> Result<RuntimeFunctionParamTy, CtypesError> {
    Ok(RuntimeFunctionParamTy {
        name: p.name.as_ref().map(Name::new),
        ty: opt_to_runtime_ty(p.ty.as_ref())?,
        mode: function_param_mode(p.mode),
    })
}

fn function_param_mode(mode: i32) -> FunctionParamMode {
    match BamlTyFunctionParamMode::try_from(mode).unwrap_or(BamlTyFunctionParamMode::Unspecified) {
        BamlTyFunctionParamMode::Optional => FunctionParamMode::Optional,
        // Required is the conservative default for an unspecified mode.
        BamlTyFunctionParamMode::Required | BamlTyFunctionParamMode::Unspecified => {
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
