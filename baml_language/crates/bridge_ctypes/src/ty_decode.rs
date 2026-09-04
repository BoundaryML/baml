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
    Freshness, FunctionParamMode, Literal, MediaKind, Name, ParamTy, RuntimeFunctionParamTy,
    RuntimeInterface, RuntimeTy, TyAttr, TypeName,
};
use bex_project::{BexExternalAdt, BexExternalValue, TypeDefRef};
use indexmap::IndexMap;

use crate::{
    baml_bridge::cffi::{
        BamlTy, BamlTyArg, BamlTyDef, BamlTyFunctionParam, BamlTyFunctionParamMode,
        BamlTyMediaKind, BamlTyMetadata, BamlTyPrimitiveKind, baml_ty::Ty as TyVariant,
        baml_ty_literal::Literal as TyLiteralVariant,
    },
    error::CtypesError,
};

#[derive(Debug, Default)]
pub struct DecodedTypeArgs {
    pub type_args: IndexMap<String, RuntimeTy>,
    pub type_defs: IndexMap<String, bex_project::PortableTypeDef>,
}

/// Decode `CallFunctionArgs.type_args` (a list of named `BamlTyArg`s) into a
/// `TypeVar name -> concrete RuntimeTy` map, preserving wire order (the map's
/// insertion order is the host's De Bruijn order). A `BamlTyArg` with an absent
/// `type_value` decodes to the unknown/top type, mirroring
/// [`proto_ty_to_runtime_ty`]'s rollout-safe default. A repeated `type_var`
/// keeps the last binding. The engine resolves the names against the callee's
/// generic params when seeding the entry frame.
pub fn proto_ty_args_to_named(type_args: &[BamlTyArg]) -> Result<DecodedTypeArgs, CtypesError> {
    let mut decoded = DecodedTypeArgs::default();
    for arg in type_args {
        if let Some(definition) = arg.type_definition.as_ref() {
            let definition = proto_ty_def_to_portable(definition)?;
            decoded
                .type_args
                .insert(arg.type_var.clone(), definition.root.clone());
            decoded.type_defs.insert(arg.type_var.clone(), definition);
        } else {
            let ty = match arg.type_value.as_ref() {
                Some(ty) => proto_ty_to_runtime_ty(ty)?,
                None => RuntimeTy::unknown(),
            };
            decoded.type_args.insert(arg.type_var.clone(), ty);
        }
    }
    Ok(decoded)
}

pub fn proto_ty_def_to_portable(
    definition: &BamlTyDef,
) -> Result<bex_project::PortableTypeDef, CtypesError> {
    use bex_project::{
        DynWitnessDef, PortableClassDef, PortableClassFieldDef, PortableEnumDef,
        PortableEnumVariantDef, PortableMetadata, PortableTypeDef,
    };
    let metadata = |value: Option<&BamlTyMetadata>| {
        let value = value.cloned().unwrap_or_default();
        PortableMetadata {
            description: value.description,
            alias: value.alias,
            docstring: value.docstring,
            other: value.other.into_iter().collect(),
        }
    };
    Ok(PortableTypeDef {
        root: match definition.root.as_ref() {
            Some(root) => proto_ty_to_runtime_ty(root)?,
            None => RuntimeTy::unknown(),
        },
        classes: definition
            .classes
            .iter()
            .map(|class| {
                Ok(PortableClassDef {
                    name: TypeName::from_dotted_path(&class.name),
                    fields: class
                        .fields
                        .iter()
                        .map(|field| {
                            Ok(PortableClassFieldDef {
                                name: field.name.clone(),
                                ty: match field.ty.as_ref() {
                                    Some(ty) => proto_ty_to_runtime_ty(ty)?,
                                    None => RuntimeTy::unknown(),
                                },
                                metadata: metadata(field.metadata.as_ref()),
                                skip: field.skip,
                            })
                        })
                        .collect::<Result<Vec<_>, CtypesError>>()?,
                    metadata: metadata(class.metadata.as_ref()),
                    generic_param_count: usize::try_from(class.generic_param_count)
                        .unwrap_or(usize::MAX),
                })
            })
            .collect::<Result<Vec<_>, CtypesError>>()?,
        enums: definition
            .enums
            .iter()
            .map(|enm| PortableEnumDef {
                name: TypeName::from_dotted_path(&enm.name),
                variants: enm
                    .variants
                    .iter()
                    .map(|variant| PortableEnumVariantDef {
                        name: variant.name.clone(),
                        metadata: metadata(variant.metadata.as_ref()),
                        skip: variant.skip,
                    })
                    .collect(),
                metadata: metadata(enm.metadata.as_ref()),
            })
            .collect(),
        witnesses: definition
            .witnesses
            .iter()
            .map(|witness| {
                let realized = |ty: &BamlTy, position: &str| {
                    let runtime = proto_ty_to_runtime_ty(ty)?;
                    baml_type::RealizedTy::try_from(runtime).map_err(|error| {
                        CtypesError::InternalError(format!(
                            "host type definition witness {position} is not realized: {error}"
                        ))
                    })
                };
                Ok(DynWitnessDef {
                    interface: TypeName::from_dotted_path(&witness.interface),
                    interface_args: witness
                        .interface_args
                        .iter()
                        .map(|ty| realized(ty, "interface argument"))
                        .collect::<Result<Vec<_>, _>>()?,
                    associated_types: witness
                        .associated_types
                        .iter()
                        .map(|binding| {
                            Ok((
                                Name::new(&binding.name),
                                realized(
                                    binding.ty.as_ref().ok_or_else(|| {
                                        CtypesError::InternalError(
                                            "host type definition witness associated type is missing its type"
                                                .to_string(),
                                        )
                                    })?,
                                    "associated type",
                                )?,
                            ))
                        })
                        .collect::<Result<Vec<_>, CtypesError>>()?,
                    field_links: witness
                        .field_links
                        .iter()
                        .map(|link| {
                            (
                                Name::new(&link.interface_field),
                                Name::new(&link.class_field),
                            )
                        })
                        .collect(),
                })
            })
            .collect::<Result<Vec<_>, CtypesError>>()?,
    })
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
        TyVariant::Literal(lit) => literal_to_runtime_ty(lit.literal.as_ref())?,
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
                bindings.into(),
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
                .collect::<Result<Box<[_]>, _>>()?,
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
        TyVariant::TypeVar(v) => {
            RuntimeTy::TypeVar(ParamTy::new(v.index, Name::new(&v.name)), TyAttr::default())
        }
        TyVariant::AssociatedTypeProjection(p) => RuntimeTy::AssociatedTypeProjection {
            base: Box::new(opt_to_runtime_ty(p.base.as_deref())?),
            // The projection's interface constraint is wired as a `Ty` and must
            // decode to an interface existential, from which the constraint
            // (`RuntimeInterface`) is recovered. A projection always carries its
            // declaring interface, so an absent wire field is a malformed message —
            // reject it rather than fabricate an unqualified projection.
            interface: {
                let ty = p.interface.as_deref().ok_or_else(|| {
                    CtypesError::InternalError(
                        "AssociatedTypeProjection.interface is required".to_string(),
                    )
                })?;
                match proto_ty_to_runtime_ty(ty)? {
                    RuntimeTy::Interface(name, generics, associated_types, _) => {
                        Box::new(RuntimeInterface::new(name, generics, associated_types))
                    }
                    _ => {
                        return Err(CtypesError::InternalError(
                            "AssociatedTypeProjection.interface did not decode to an interface type"
                                .to_string(),
                        ));
                    }
                }
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
///
/// The wire carries names, and the engine's lane carries identities — but for
/// a *compiled* declaration the identity is content-addressed from exactly
/// that name, so this recovers the tag emit assigned rather than inventing
/// one. A name no declaration bears yields a tag nothing matches, which fails
/// at the lookup instead of binding to the wrong declaration.
///
/// A runtime-created declaration cannot arrive this way at all: its identity
/// is a counter mint that no name reproduces. It has to cross as a handle, and
/// that is the gap `BamlTypeHead` closes.
pub fn proto_ty_to_external(ty: &BamlTy) -> Result<BexExternalValue, CtypesError> {
    let named = proto_ty_to_runtime_ty(ty)?;
    let lane = named
        .try_map_heads(&mut |name: &baml_type::TypeName| {
            Ok::<_, std::convert::Infallible>(baml_type::TaggedTypeName::new(
                baml_type::typetag::TypeTag::of_head(&name.render_dotted(false)),
                baml_type::DeclarationName::Declared(name.clone()),
            ))
        })
        .unwrap_or_else(|never| match never {});
    Ok(BexExternalValue::Adt(BexExternalAdt::Type(lane)))
}

pub fn proto_ty_def_to_external(ty: &BamlTyDef) -> Result<BexExternalValue, CtypesError> {
    Ok(BexExternalValue::Adt(BexExternalAdt::TypeDef(
        TypeDefRef::Portable(proto_ty_def_to_portable(ty)?),
    )))
}

fn decode_type_args(type_args: &[BamlTy]) -> Result<Box<[RuntimeTy]>, CtypesError> {
    type_args
        .iter()
        .map(proto_ty_to_runtime_ty)
        .collect::<Result<Box<[_]>, _>>()
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

fn literal_to_runtime_ty(lit: Option<&TyLiteralVariant>) -> Result<RuntimeTy, CtypesError> {
    let literal = match lit {
        Some(TyLiteralVariant::StringValue(value)) => Literal::String(value.clone()),
        Some(TyLiteralVariant::IntValue(value)) => Literal::Int(*value),
        Some(TyLiteralVariant::BoolValue(value)) => Literal::Bool(*value),
        Some(TyLiteralVariant::BigintValue(value)) => {
            Literal::Bigint(parse_decimal_bigint_literal(value)?)
        }
        Some(TyLiteralVariant::FloatValue(value)) => Literal::Float(value.clone()),
        None => return Ok(RuntimeTy::unknown()),
    };
    Ok(RuntimeTy::Literal(
        literal,
        Freshness::Regular,
        TyAttr::default(),
    ))
}

fn parse_decimal_bigint_literal(value: &str) -> Result<num_bigint::BigInt, CtypesError> {
    parse_decimal_bigint_literal_with_limits(
        value,
        baml_type::MAX_BIGINT_DECIMAL_DIGITS,
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
    fn portable_type_definition_round_trips_without_identity() {
        use bex_project::{
            PortableClassDef, PortableClassFieldDef, PortableMetadata, PortableTypeDef,
        };
        let name = TypeName::local(Name::new("Row"));
        let metadata = PortableMetadata {
            description: Some("row description".into()),
            alias: None,
            docstring: Some("docs".into()),
            other: IndexMap::from([("x".into(), "y".into())]),
        };
        let definition = PortableTypeDef {
            root: RuntimeTy::Class(name.clone(), Box::new([]), TyAttr::default()),
            classes: vec![PortableClassDef {
                name,
                fields: vec![PortableClassFieldDef {
                    name: "value".into(),
                    ty: RuntimeTy::int(),
                    metadata: metadata.clone(),
                    skip: false,
                }],
                metadata,
                generic_param_count: 0,
            }],
            enums: Vec::new(),
            witnesses: vec![bex_project::DynWitnessDef {
                interface: TypeName::from_dotted_path("user.RowLike"),
                interface_args: vec![baml_type::RealizedTy::string()],
                associated_types: vec![(Name::new("Item"), baml_type::RealizedTy::int())],
                field_links: vec![(Name::new("item"), Name::new("value"))],
            }],
        };
        let wire = crate::ty_encode::portable_type_def_to_proto(&definition);
        let decoded = proto_ty_def_to_portable(&wire).expect("portable definition decodes");
        assert_eq!(decoded, definition);
    }

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
    fn bounded_invalid_bigint_literal_keeps_format_diagnostic() {
        let error = parse_decimal_bigint_literal_with_limits("12x", 3, u64::MAX).unwrap_err();
        assert!(matches!(error, CtypesError::InternalError(message) if message.contains("12x")));
    }
}
