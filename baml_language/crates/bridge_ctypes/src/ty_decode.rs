//! Decode the wire `BamlTy` (`baml_type.proto`) into a wire type.
//!
//! The parse is pure: a compiled declaration's head is its name, a runtime
//! declaration's head is the host's table key, carried as it arrived. Nothing
//! here touches the handle table; the engine's decoder ([`crate::InboundOwners`])
//! journals and drains the keys a statement carries, and the engine resolves
//! names and follows handles when it anchors the type.
//!
//! Two host-provided "type as a value" paths share this:
//!   - `CallFunctionArgs.type_args` — explicit, named `TypeVar` bindings for a
//!     generic function/method call (`BamlTyArg { type_var, type_value }`).
//!   - `InboundValue.ty_value` — a reflected type passed as an argument value.

use baml_type::{
    Freshness, FunctionParamMode, Literal, MediaKind, Name, RealizedFunctionParamTy,
    RealizedInterface, RealizedTy, TypeName, WireHead, WireTy,
};
use indexmap::IndexMap;

use crate::{
    baml_bridge::cffi::{
        BamlTy, BamlTyArg, BamlTyFunctionParam, BamlTyFunctionParamMode, BamlTyMediaKind,
        BamlTyPrimitiveKind, baml_ty::Ty as TyVariant,
        baml_ty_literal::Literal as TyLiteralVariant,
    },
    error::CtypesError,
    handle_table::TableKey,
};

/// A type as a host stated it, parsed: runtime declarations by the table key
/// the host holds for each, not yet resolved. A wire type is realized: a type
/// variable or an associated-type projection determines no value, so the
/// parse refuses one.
pub type ParsedTy = WireTy<TableKey>;

/// A wire `BamlTy` as a host in the engine's process holds it: parsed, and
/// each runtime declaration resolved to the engine object, read without
/// draining the host's rows.
pub fn proto_ty_to_stated(
    ty: &BamlTy,
    table: &crate::CffiHandleTable,
) -> Result<bex_project::WireTy, CtypesError> {
    resolve_parsed(&proto_ty_to_wire_ty(ty)?, table)
}

/// A call's explicit type arguments as [`proto_ty_to_stated`] reads each.
pub fn proto_ty_args_to_stated(
    type_args: &[BamlTyArg],
    table: &crate::CffiHandleTable,
) -> Result<IndexMap<String, bex_project::WireTy>, CtypesError> {
    proto_ty_args_to_named(type_args)?
        .type_args
        .into_iter()
        .map(|(name, ty)| Ok((name, resolve_parsed(&ty, table)?)))
        .collect()
}

/// `ty` over the engine objects its keys name, read without draining: a host
/// in the engine's process holds the declarations by handle for as long as it
/// needs them, independent of the rows it was handed.
pub fn resolve_parsed(
    ty: &ParsedTy,
    table: &crate::CffiHandleTable,
) -> Result<bex_project::WireTy, CtypesError> {
    ty.try_map_live(&mut |key: &TableKey| {
        let row = table
            .resolve(*key)
            .ok_or(CtypesError::InvalidHandleKey(key.get()))?;
        match &*row {
            crate::CffiHandleTableEntry::BexHeapHandle(handle) => Ok(handle.clone()),
            _ => Err(CtypesError::LiveHeadNotADeclaration(key.get())),
        }
    })
}

#[derive(Debug, Default)]
pub struct DecodedTypeArgs {
    pub type_args: IndexMap<String, ParsedTy>,
}

/// Decode `CallFunctionArgs.type_args` (a list of named `BamlTyArg`s) into a
/// `TypeVar name -> stated type` map, preserving wire order (the map's
/// insertion order is the host's De Bruijn order). A `BamlTyArg` must carry a
/// type; a host that means `unknown` says so. A repeated `type_var` keeps the
/// last binding. The engine resolves the names against the callee's generic
/// params when seeding the entry frame.
pub fn proto_ty_args_to_named(type_args: &[BamlTyArg]) -> Result<DecodedTypeArgs, CtypesError> {
    let mut decoded = DecodedTypeArgs::default();
    for arg in type_args {
        let ty = opt_to_wire_ty(arg.type_value.as_ref(), "type argument's type")?;
        decoded.type_args.insert(arg.type_var.clone(), ty);
    }
    Ok(decoded)
}

/// A wire type spelled by name alone, for a reader with no handle table: a
/// program's own declarations. A runtime declaration in it is refused.
pub fn proto_ty_to_runtime_ty(ty: &BamlTy) -> Result<RealizedTy, CtypesError> {
    proto_ty_to_wire_ty(ty)?
        .try_into_named()
        .map_err(|head| match head {
            WireHead::Static(_) => unreachable!("a static head is named"),
            WireHead::Live(key) => CtypesError::LiveHeadNotADeclaration(key.get()),
            WireHead::Anonymous(label) => CtypesError::AnonymousHeadStated {
                label: bex_project::Bounded::text(label),
            },
        })
}

/// A nominal wire type's head, as the host stated it.
trait NominalHead {
    fn head_of(&self) -> Result<WireHead<TableKey>, CtypesError>;
    /// The table key of a `live` head, read off the raw message; `None` for a
    /// name, a label, a missing head or key 0.
    fn live_key(&self) -> Option<TableKey>;
}

/// Each nominal message carries the same `head` oneof under its own Rust
/// type; the reading is one.
macro_rules! nominal_head {
    ($($message:ident => $module:ident),* $(,)?) => {$(
        impl NominalHead for crate::baml_bridge::cffi::$message {
            fn head_of(&self) -> Result<WireHead<TableKey>, CtypesError> {
                use crate::baml_bridge::cffi::$module::Head;
                match &self.head {
                    Some(Head::Name(name)) => Ok(WireHead::Static(TypeName::from_dotted_path(name))),
                    Some(Head::Live(key)) => TableKey::new(*key)
                        .map(WireHead::Live)
                        .ok_or(CtypesError::InvalidHandleKey(*key)),
                    Some(Head::AnonymousLabel(label)) => Err(CtypesError::AnonymousHeadStated {
                        label: bex_project::Bounded::text(label),
                    }),
                    None => Err(CtypesError::IncompleteType { position: "head" }),
                }
            }

            fn live_key(&self) -> Option<TableKey> {
                use crate::baml_bridge::cffi::$module::Head;
                match &self.head {
                    Some(Head::Live(key)) => TableKey::new(*key),
                    Some(Head::Name(_) | Head::AnonymousLabel(_)) | None => None,
                }
            }
        }
    )*};
}

/// Every `live` head under `ty`, one call per occurrence, read off the raw
/// message rather than its parse: a statement the parse refuses (an
/// unrealized member, a missing part, a label) still transferred one engine
/// reference per `live` head, and the journal must account for each so a
/// refusal releases it.
pub(crate) fn live_keys(ty: &BamlTy, f: &mut impl FnMut(TableKey)) {
    let Some(variant) = ty.ty.as_ref() else {
        return;
    };
    let mut head = |key: Option<TableKey>| {
        if let Some(key) = key {
            f(key);
        }
    };
    match variant {
        TyVariant::ClassTy(c) => {
            head(c.live_key());
            for arg in &c.type_args {
                live_keys(arg, f);
            }
        }
        TyVariant::Enum(e) => head(e.live_key()),
        TyVariant::EnumVariant(e) => head(e.live_key()),
        TyVariant::TypeAlias(n) => {
            head(n.live_key());
            for arg in &n.type_args {
                live_keys(arg, f);
            }
        }
        TyVariant::Interface(i) => {
            head(i.live_key());
            for arg in &i.type_args {
                live_keys(arg, f);
            }
            for binding in &i.bindings {
                if let Some(ty) = &binding.ty {
                    live_keys(ty, f);
                }
            }
        }
        TyVariant::List(l) => {
            if let Some(item) = l.item.as_deref() {
                live_keys(item, f);
            }
        }
        TyVariant::Map(m) => {
            for part in [m.key.as_deref(), m.value.as_deref()].into_iter().flatten() {
                live_keys(part, f);
            }
        }
        TyVariant::Optional(o) => {
            if let Some(inner) = o.inner.as_deref() {
                live_keys(inner, f);
            }
        }
        TyVariant::Union(u) => {
            for option in &u.options {
                live_keys(option, f);
            }
        }
        TyVariant::Function(func) => {
            for param in &func.params {
                if let Some(ty) = &param.ty {
                    live_keys(ty, f);
                }
            }
            for part in [func.ret.as_deref(), func.throws.as_deref()]
                .into_iter()
                .flatten()
            {
                live_keys(part, f);
            }
        }
        TyVariant::Future(fut) => {
            for part in [fut.value.as_deref(), fut.error.as_deref()]
                .into_iter()
                .flatten()
            {
                live_keys(part, f);
            }
        }
        // Leaves: no head, no children.
        TyVariant::Primitive(_)
        | TyVariant::Literal(_)
        | TyVariant::Unknown(_)
        | TyVariant::Media(_)
        | TyVariant::RustType(_)
        | TyVariant::MetaType(_)
        | TyVariant::Resource(_)
        | TyVariant::PromptAst(_)
        | TyVariant::TypeVar(_)
        | TyVariant::AssociatedTypeProjection(_)
        | TyVariant::Never(_) => {}
    }
}

nominal_head! {
    BamlTyClass => baml_ty_class,
    BamlTyEnum => baml_ty_enum,
    BamlTyTypeAlias => baml_ty_type_alias,
    BamlTyInterface => baml_ty_interface,
    BamlTyEnumVariant => baml_ty_enum_variant,
}

pub fn proto_ty_to_wire_ty(ty: &BamlTy) -> Result<ParsedTy, CtypesError> {
    let Some(variant) = ty.ty.as_ref() else {
        return Err(CtypesError::IncompleteType {
            position: "variant",
        });
    };
    Ok(match variant {
        TyVariant::Primitive(p) => primitive_to_runtime_ty(p.kind)?,
        TyVariant::ClassTy(c) => RealizedTy::Class(c.head_of()?, decode_type_args(&c.type_args)?),
        TyVariant::Enum(e) => RealizedTy::Enum(e.head_of()?),
        TyVariant::List(l) => {
            RealizedTy::list(opt_to_wire_ty(l.item.as_deref(), "list's item type")?)
        }
        TyVariant::Map(m) => RealizedTy::Map {
            key: Box::new(opt_to_wire_ty(m.key.as_deref(), "map's key type")?),
            value: Box::new(opt_to_wire_ty(m.value.as_deref(), "map's value type")?),
        },
        TyVariant::Optional(o) => {
            RealizedTy::optional(opt_to_wire_ty(o.inner.as_deref(), "optional's inner type")?)
        }
        TyVariant::Union(u) => RealizedTy::Union(
            u.options
                .iter()
                .map(proto_ty_to_wire_ty)
                .collect::<Result<_, _>>()?,
        ),
        TyVariant::Literal(lit) => literal_to_runtime_ty(lit.literal.as_ref())?,
        // A type alias is not generic at run time, and a function type
        // carries no type parameters of its own: a value in either field
        // would be dropped, so it is refused rather than ignored.
        TyVariant::TypeAlias(n) => {
            if !n.type_args.is_empty() {
                return Err(CtypesError::UnsupportedTypeField {
                    field: "BamlTyTypeAlias.type_args",
                });
            }
            RealizedTy::TypeAlias(n.head_of()?)
        }
        TyVariant::Unknown(_) => RealizedTy::Unknown,
        TyVariant::Media(m) => RealizedTy::Media(media_kind_to_runtime(m.kind)?),
        TyVariant::Interface(i) => {
            let bindings = i
                .bindings
                .iter()
                .map(|b| {
                    Ok((
                        Name::new(&b.name),
                        opt_to_wire_ty(b.ty.as_ref(), "associated type binding's type")?,
                    ))
                })
                .collect::<Result<Vec<_>, CtypesError>>()?;
            // The wire order is untrusted (a non-BAML or future producer may send
            // bindings in any order) and `RuntimeInterface`'s derived `Eq`/`Hash`
            // are order-sensitive, so route through `RuntimeInterface::new` to sort
            // them, then lift the constraint to its existential type.
            let constraint = RealizedInterface::new(
                i.head_of()?,
                decode_type_args(&i.type_args)?,
                bindings.into(),
            );
            RealizedTy::Interface(
                constraint.name,
                constraint.generics,
                constraint.associated_types,
            )
        }
        TyVariant::EnumVariant(e) => RealizedTy::EnumVariant(e.head_of()?, Name::new(&e.variant)),
        TyVariant::Function(f) if !f.generic_params.is_empty() => {
            return Err(CtypesError::UnsupportedTypeField {
                field: "BamlTyFunction.generic_params",
            });
        }
        TyVariant::Function(f) => RealizedTy::Function {
            params: f
                .params
                .iter()
                .map(function_param_to_runtime)
                .collect::<Result<Box<[_]>, _>>()?,
            ret: Box::new(opt_to_wire_ty(f.ret.as_deref(), "function's return type")?),
            throws: Box::new(opt_to_wire_ty(
                f.throws.as_deref(),
                "function's throws type",
            )?),
        },
        TyVariant::Future(fut) => RealizedTy::Future(
            Box::new(opt_to_wire_ty(fut.value.as_deref(), "future's value type")?),
            Box::new(opt_to_wire_ty(fut.error.as_deref(), "future's error type")?),
        ),
        TyVariant::RustType(_) => RealizedTy::RustType,
        TyVariant::MetaType(_) => RealizedTy::Type,
        TyVariant::Resource(_) => RealizedTy::Resource,
        TyVariant::PromptAst(_) => RealizedTy::PromptAst,
        TyVariant::TypeVar(_) => {
            return Err(CtypesError::UnrealizedType {
                variant: "type variable",
            });
        }
        TyVariant::AssociatedTypeProjection(_) => {
            return Err(CtypesError::UnrealizedType {
                variant: "associated-type projection",
            });
        }
        TyVariant::Never(_) => RealizedTy::Never,
    })
}

fn decode_type_args(type_args: &[BamlTy]) -> Result<Box<[ParsedTy]>, CtypesError> {
    type_args
        .iter()
        .map(proto_ty_to_wire_ty)
        .collect::<Result<Box<[_]>, _>>()
}

/// The type at a position every type must fill; `position` names it in the
/// refusal of a wire type that left it empty.
fn opt_to_wire_ty(opt: Option<&BamlTy>, position: &'static str) -> Result<ParsedTy, CtypesError> {
    match opt {
        Some(t) => proto_ty_to_wire_ty(t),
        None => Err(CtypesError::IncompleteType { position }),
    }
}

fn primitive_to_runtime_ty(kind: i32) -> Result<ParsedTy, CtypesError> {
    Ok(
        match BamlTyPrimitiveKind::try_from(kind)
            .unwrap_or(BamlTyPrimitiveKind::BamlTyPrimitiveUnspecified)
        {
            BamlTyPrimitiveKind::BamlTyPrimitiveString => RealizedTy::String,
            BamlTyPrimitiveKind::BamlTyPrimitiveInt => RealizedTy::Int,
            BamlTyPrimitiveKind::BamlTyPrimitiveFloat => RealizedTy::Float,
            BamlTyPrimitiveKind::BamlTyPrimitiveBool => RealizedTy::Bool,
            BamlTyPrimitiveKind::BamlTyPrimitiveNull => RealizedTy::Null,
            BamlTyPrimitiveKind::BamlTyPrimitiveBytes => RealizedTy::Uint8Array,
            BamlTyPrimitiveKind::BamlTyPrimitiveBigint => RealizedTy::Bigint,
            BamlTyPrimitiveKind::BamlTyPrimitiveUnspecified => {
                return Err(CtypesError::IncompleteType {
                    position: "primitive kind",
                });
            }
        },
    )
}

fn media_kind_to_runtime(kind: i32) -> Result<MediaKind, CtypesError> {
    Ok(
        match BamlTyMediaKind::try_from(kind).unwrap_or(BamlTyMediaKind::Unspecified) {
            BamlTyMediaKind::Image => MediaKind::Image,
            BamlTyMediaKind::Audio => MediaKind::Audio,
            BamlTyMediaKind::Video => MediaKind::Video,
            BamlTyMediaKind::Pdf => MediaKind::Pdf,
            BamlTyMediaKind::Generic => MediaKind::Generic,
            BamlTyMediaKind::Unspecified => {
                return Err(CtypesError::IncompleteType {
                    position: "media kind",
                });
            }
        },
    )
}

fn function_param_to_runtime(
    p: &BamlTyFunctionParam,
) -> Result<RealizedFunctionParamTy<WireHead<TableKey>>, CtypesError> {
    Ok(RealizedFunctionParamTy {
        name: p.name.as_ref().map(Name::new),
        ty: opt_to_wire_ty(p.ty.as_ref(), "function parameter's type")?,
        mode: function_param_mode(p.mode)?,
    })
}

fn function_param_mode(mode: i32) -> Result<FunctionParamMode, CtypesError> {
    match BamlTyFunctionParamMode::try_from(mode).unwrap_or(BamlTyFunctionParamMode::Unspecified) {
        BamlTyFunctionParamMode::Optional => Ok(FunctionParamMode::Optional),
        BamlTyFunctionParamMode::Required => Ok(FunctionParamMode::Required),
        BamlTyFunctionParamMode::Unspecified => Err(CtypesError::IncompleteType {
            position: "function parameter's mode",
        }),
    }
}

fn literal_to_runtime_ty(lit: Option<&TyLiteralVariant>) -> Result<ParsedTy, CtypesError> {
    let literal = match lit {
        Some(TyLiteralVariant::StringValue(value)) => Literal::String(value.clone()),
        Some(TyLiteralVariant::IntValue(value)) => Literal::Int(*value),
        Some(TyLiteralVariant::BoolValue(value)) => Literal::Bool(*value),
        Some(TyLiteralVariant::BigintValue(value)) => {
            Literal::Bigint(parse_decimal_bigint_literal(value)?)
        }
        // The text must be a finite float: the engine reflects the literal's
        // value by parsing it, and an unparsable one would have none.
        Some(TyLiteralVariant::FloatValue(value)) => {
            if !value.parse::<f64>().is_ok_and(f64::is_finite) {
                return Err(CtypesError::InvalidFloatLiteral { len: value.len() });
            }
            Literal::Float(value.clone())
        }
        None => {
            return Err(CtypesError::IncompleteType {
                position: "literal's value",
            });
        }
    };
    Ok(RealizedTy::Literal(literal, Freshness::Regular))
}

/// The most decimal digits a bigint literal TYPE may spell on the wire. A
/// literal type names one value a program wrote in source; the workspace's
/// value cap (`baml_type::MAX_BIGINT_DECIMAL_DIGITS`, tens of millions of
/// digits) is for values, and decimal parsing is quadratic in the digit
/// count, so a statement is held to a few thousand.
pub(crate) const MAX_TYPE_LITERAL_BIGINT_DIGITS: usize = 4_096;

fn parse_decimal_bigint_literal(value: &str) -> Result<num_bigint::BigInt, CtypesError> {
    parse_decimal_bigint_literal_with_limits(
        value,
        MAX_TYPE_LITERAL_BIGINT_DIGITS,
        baml_type::MAX_BIGINT_BITS,
    )
}

fn parse_decimal_bigint_literal_with_limits(
    value: &str,
    max_digits: usize,
    max_bits: u64,
) -> Result<num_bigint::BigInt, CtypesError> {
    let len = value.len();
    let digits = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);
    if digits.len() > max_digits {
        return Err(CtypesError::InvalidBigintLiteral { len });
    }

    let parsed = num_bigint::BigInt::parse_bytes(value.as_bytes(), 10).ok_or_else(|| {
        CtypesError::InternalError(format!(
            "invalid decimal bigint literal in BAML type descriptor: {value:?}"
        ))
    })?;
    if parsed.bits() > max_bits {
        return Err(CtypesError::InvalidBigintLiteral { len });
    }
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_bigint_literal_is_rejected_before_parse_without_echoing_input() {
        let error = parse_decimal_bigint_literal_with_limits("-1234", 3, u64::MAX).unwrap_err();
        assert!(matches!(
            error,
            CtypesError::InvalidBigintLiteral { len: 5 }
        ));
        assert_eq!(
            error.to_string(),
            "Invalid decimal bigint literal (5 bytes)"
        );
    }

    #[test]
    fn a_float_literal_type_must_spell_a_finite_float() {
        for text in ["x", "NaN", "inf", ""] {
            let error = literal_to_runtime_ty(Some(&TyLiteralVariant::FloatValue(text.into())))
                .expect_err("not a finite float");
            assert!(
                matches!(error, CtypesError::InvalidFloatLiteral { len } if len == text.len()),
                "{text:?}: {error}"
            );
        }
        assert!(literal_to_runtime_ty(Some(&TyLiteralVariant::FloatValue("1.5".into()))).is_ok());
    }

    /// The keys a statement transfers are read off the raw message, so a
    /// statement the parse refuses still accounts for every `live` head.
    #[test]
    fn live_keys_are_read_off_a_statement_the_parse_refuses() {
        use crate::baml_bridge::cffi::{BamlTyClass, BamlTyList, baml_ty_class};
        let live = |key: u64| BamlTy {
            ty: Some(TyVariant::ClassTy(BamlTyClass {
                head: Some(baml_ty_class::Head::Live(key)),
                type_args: vec![],
            })),
        };
        let refused = BamlTy {
            ty: Some(TyVariant::ClassTy(BamlTyClass {
                head: Some(baml_ty_class::Head::Live(11)),
                type_args: vec![
                    live(12),
                    BamlTy { ty: None },
                    BamlTy {
                        ty: Some(TyVariant::List(Box::new(BamlTyList {
                            item: Some(Box::new(live(12))),
                        }))),
                    },
                ],
            })),
        };
        assert!(proto_ty_to_wire_ty(&refused).is_err());
        let mut keys = Vec::new();
        live_keys(&refused, &mut |key| keys.push(key.get()));
        assert_eq!(keys, [11, 12, 12], "one per occurrence, key 0 never");
    }

    #[test]
    fn bounded_invalid_bigint_literal_keeps_format_diagnostic() {
        let error = parse_decimal_bigint_literal_with_limits("12x", 3, u64::MAX).unwrap_err();
        assert!(matches!(error, CtypesError::InternalError(message) if message.contains("12x")));
    }
}
