//! Converting from [`baml_type`] types to SAP model types.

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use baml_type::{TyAttrValue, TypeName};

use crate::sap_model::{
    self, AnnotatedTy, ArrayTy, AttrLiteral, BoolLiteralTy, BoolTy, FloatTy, IntLiteralTy, IntTy,
    MapTy, MediaTy, NullTy, StringLiteralTy, StringTy, TyResolved, TyWithMeta, TypeAnnotations,
    UnionTy,
};

impl crate::sap_model::TypeIdent for TypeName {}

#[derive(thiserror::Error, Debug)]
pub enum ConvertError {
    #[error("Failed to parse float: {0}")]
    ParseFloat(#[from] std::num::ParseFloatError),
    #[error("Unknown media kind")]
    UnknownMediaKind,
    #[error("Float literals cannot be parsed")]
    FloatLiteral,
    #[error("Non-parsable type: {0:?}")]
    NonParsableType(Box<baml_type::Ty>),
}

/// Simplify a type for SAP and then convert it to the SAP model.
///
/// Runs [`baml_type::simplify_sap::simplify`] as a pre-pass (flattening
/// unions, deduplicating variants, distributing attrs, expanding aliases)
/// and then converts the simplified type.
pub fn simplify_and_convert(
    ty: &baml_type::Ty,
    aliases: &HashMap<TypeName, baml_type::Ty>,
    recursive_aliases: &HashSet<TypeName>,
) -> Result<AnnotatedTy<'static, TypeName>, ConvertError> {
    let simplified = baml_type::simplify_sap::simplify(ty.clone(), aliases, recursive_aliases);
    convert(&simplified)
}

pub fn convert(ty: &baml_type::Ty) -> Result<AnnotatedTy<'static, TypeName>, ConvertError> {
    let ty = match ty {
        baml_type::Ty::Int { attr } => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::Int(IntTy)),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Float { attr } => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::Float(FloatTy)),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::String { attr } => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::String(StringTy)),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Bool { attr } => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::Bool(BoolTy)),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Null { attr } => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::Null(NullTy)),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Media(media_kind, ty_attr) => {
            let media_kind = match media_kind {
                baml_type::MediaKind::Image => MediaTy::Image,
                baml_type::MediaKind::Video => MediaTy::Video,
                baml_type::MediaKind::Audio => MediaTy::Audio,
                baml_type::MediaKind::Pdf => MediaTy::Pdf,
                baml_type::MediaKind::Generic => return Err(ConvertError::UnknownMediaKind),
            };
            TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Media(media_kind)),
                convert_ty_attrs(ty_attr)?,
            )
        }
        baml_type::Ty::Literal(baml_type::Literal::Int(i), attr) => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::LiteralInt(IntLiteralTy(*i))),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Literal(baml_type::Literal::Float(..), ..) => {
            return Err(ConvertError::FloatLiteral);
        }
        baml_type::Ty::Literal(baml_type::Literal::String(s), attr) => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::LiteralString(StringLiteralTy(Cow::Owned(
                s.clone(),
            )))),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Literal(baml_type::Literal::Bool(b), attr) => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::LiteralBool(BoolLiteralTy(*b))),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Class(type_name, attr) => TyWithMeta::new(
            sap_model::Ty::Unresolved(type_name.clone()),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Enum(type_name, attr) => TyWithMeta::new(
            sap_model::Ty::Unresolved(type_name.clone()),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Optional(ty, attr) => {
            // becomes a union
            let ty = convert(ty)?;
            TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Union(UnionTy {
                    variants: vec![
                        TyWithMeta::new(
                            sap_model::Ty::Resolved(TyResolved::Null(NullTy)),
                            TypeAnnotations::default(),
                        ),
                        ty,
                    ],
                })),
                convert_ty_attrs(attr)?,
            )
        }
        baml_type::Ty::List(ty, attr) => TyWithMeta::new(
            sap_model::Ty::Resolved(TyResolved::Array(ArrayTy {
                ty: Box::new(convert(ty)?),
            })),
            convert_ty_attrs(attr)?,
        ),
        baml_type::Ty::Map { key, value, attr } => {
            let key = convert(key)?;
            let value = convert(value)?;
            TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Map(MapTy {
                    key: Box::new(key),
                    value: Box::new(value),
                })),
                convert_ty_attrs(attr)?,
            )
        }
        baml_type::Ty::Union(items, ty_attr) => {
            let items = items
                .iter()
                .map(|ty| convert(ty))
                .collect::<Result<Vec<_>, _>>()?;
            TyWithMeta::new(
                sap_model::Ty::Resolved(TyResolved::Union(UnionTy { variants: items })),
                convert_ty_attrs(ty_attr)?,
            )
        }
        baml_type::Ty::TypeAlias(type_name, attr) => TyWithMeta::new(
            sap_model::Ty::Unresolved(type_name.clone()),
            convert_ty_attrs(attr)?,
        ),
        unparsable @ (baml_type::Ty::Opaque(_, _)
        | baml_type::Ty::Function { .. }
        | baml_type::Ty::Void { .. }
        | baml_type::Ty::WatchAccessor(_, _)
        | baml_type::Ty::BuiltinUnknown { .. }
        | baml_type::Ty::EnumVariant(_, _, _)
        | baml_type::Ty::Future(_, _)) => {
            return Err(ConvertError::NonParsableType(Box::new(unparsable.clone())));
        }
    };
    Ok(ty)
}

pub fn convert_ty_attrs(
    attrs: &baml_type::TyAttr,
) -> Result<TypeAnnotations<'static, TypeName>, ConvertError> {
    let in_progress = match attrs.sap_in_progress_never {
        TyAttrValue::Set => Some(AttrLiteral::Never),
        TyAttrValue::Unset => None,
    };
    Ok(TypeAnnotations {
        in_progress,
        parse_without_null: false, // TODO: parse_without_null
        asserts: Vec::new(),       // TODO: assertions
    })
}
