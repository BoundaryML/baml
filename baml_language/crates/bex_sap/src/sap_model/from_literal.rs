//! Contains [`FromLiteral`] trait and implementations.

use std::ops::Deref;

use ::std::borrow::Cow;
use indexmap::IndexMap;

use crate::{
    deserializer::{
        coercer::{ParsingContext, ParsingError},
        deserialize_flags::DeserializerConditions,
        types::{BamlValueWithFlags, DeserializerMeta},
    },
    sap_model::{
        AnnotatedField, ArrayTy, AttrLiteral, BamlArray, BamlBigint, BamlBool, BamlClass, BamlEnum,
        BamlFloat, BamlInt, BamlMap, BamlNull, BamlPrimitive, BamlStreamState, BamlString,
        BamlValue, BigintLiteralTy, BigintTy, BoolLiteralTy, BoolTy, ClassTy, EnumTy,
        EnumVariantTy, FloatTy, IntLiteralTy, IntTy, LiteralTy, MapTy, MediaTy, NullTy,
        PrimitiveTy, StreamStateTy, StringLiteralTy, StringTy, TyResolvedRef, TypeIdent,
        TypeName as _, TypeValue, UnionTy,
    },
};

pub trait FromLiteral<'s, 'v, 't, N: TypeIdent>: TypeValue<'s, 'v, 't>
where
    's: 'v,
{
    /// Converts from a SAP model literal (used in attributes) into a BAML value.
    /// Does not perform any transformations: the value should be of the correct type.
    ///
    /// ## Errors
    /// If the literal cannot be converted for the type.
    #[allow(clippy::wrong_self_convention)]
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError>;
}

impl<'s, 'v, 't, N> FromLiteral<'s, 'v, 't, N> for IntTy
where
    's: 'v,
    N: TypeIdent,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Int(i) => Ok(BamlInt { value: *i }),
            _ => Err(ctx.error_internal("attribute literal must match the type: int")),
        }
    }
}

impl<'s, 'v, 't, N> FromLiteral<'s, 'v, 't, N> for BigintTy
where
    's: 'v,
    N: TypeIdent,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Bigint(bi) => Ok(BamlBigint { value: bi.clone() }),
            // `int` literals widen into `bigint`.
            AttrLiteral::Int(i) => Ok(BamlBigint {
                value: num_bigint::BigInt::from(*i),
            }),
            _ => Err(ctx.error_internal("attribute literal must match the type: bigint")),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for FloatTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Float(f) => Ok(BamlFloat { value: *f }),
            _ => Err(ctx.error_internal("attribute literal must match the type: float")),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for BoolTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Bool(b) => Ok(BamlBool { value: *b }),
            _ => Err(ctx.error_internal("attribute literal must match the type: bool")),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for StringTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::String(s) => Ok(BamlString {
                value: s.to_string().into(),
            }),
            _ => Err(ctx.error_internal("attribute literal must match the type: string")),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for NullTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Null => Ok(BamlNull),
            _ => Err(ctx.error_internal("attribute literal must match the type: null")),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for MediaTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        _literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        Err(ctx.error_internal("media literals are not currently supported"))
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for PrimitiveTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match self {
            PrimitiveTy::Int(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::Int),
            PrimitiveTy::Bigint(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::Bigint),
            PrimitiveTy::Float(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::Float),
            PrimitiveTy::String(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::String),
            PrimitiveTy::Bool(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::Bool),
            PrimitiveTy::Null(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::Null),
            PrimitiveTy::Media(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::Media),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for IntLiteralTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Int(i) if *i == self.0 => Ok(BamlInt { value: *i }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for BigintLiteralTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Bigint(bi) if *bi == self.0 => Ok(BamlBigint { value: bi.clone() }),
            // `int` literals matching the bigint literal value widen in.
            AttrLiteral::Int(i) if num_bigint::BigInt::from(*i) == self.0 => Ok(BamlBigint {
                value: self.0.clone(),
            }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for BoolLiteralTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Bool(b) if *b == self.0 => Ok(BamlBool { value: *b }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for StringLiteralTy<'t>
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::String(s) if s == self.0.as_ref() => Ok(BamlString {
                value: s.to_string().into(),
            }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for LiteralTy<'t>
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match self {
            LiteralTy::String(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::String),
            LiteralTy::Int(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::Int),
            LiteralTy::Bigint(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::Bigint),
            LiteralTy::Bool(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::Bool),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for ArrayTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let AttrLiteral::Array(items) = literal else {
            return Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            )));
        };
        let ty = ctx
            .db
            .resolve_with_meta(self.ty.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let items = items
            .iter()
            .map(|item| {
                ty.ty.from_literal(item, ctx).map(|item| {
                    BamlValueWithFlags::new(
                        item,
                        DeserializerMeta {
                            flags: DeserializerConditions::default(),
                            ty: ty.clone(),
                        },
                    )
                })
            })
            .collect::<Result<Vec<_>, _>>();
        match items {
            Ok(items) => Ok(BamlArray { value: items }),
            Err(e) => Err(ctx
                .error_internal(format!(
                    "attribute literal must match the type: {}",
                    self.type_name()
                ))
                .with_cause(e)),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for MapTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let AttrLiteral::Map(data) = literal else {
            return Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            )));
        };
        let key_ty = ctx
            .db
            .resolve_with_meta(self.key.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let value_ty = ctx
            .db
            .resolve_with_meta(self.value.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let data = data
            .iter()
            .map(|(key, value)| {
                let key = key_ty
                    .ty
                    .from_literal(&AttrLiteral::String(key.clone()), ctx)?;
                let key = match key {
                    BamlValue::String(s) => s.value,
                    BamlValue::Enum(e) => Cow::Borrowed(e.value), // uses variant name, not aliases
                    _ => return Err(ctx.error_internal("key must be a string-like type")),
                };
                let value = value_ty.ty.from_literal(value, ctx)?;
                let meta = DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: value_ty.clone(),
                };
                Ok((key, BamlValueWithFlags::new(value, meta)))
            })
            .collect::<Result<IndexMap<_, _>, _>>();
        match data {
            Ok(data) => Ok(BamlMap { value: data }),
            Err(e) => Err(ctx
                .error_internal(format!(
                    "attribute literal must match the type: {}",
                    self.type_name()
                ))
                .with_cause(e)),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for ClassTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let (name, data) = match literal {
            AttrLiteral::Object { name, data } if **name == self.name => (name, data),
            _ => {
                return Err(ctx.error_internal(format!(
                    "attribute literal must match the type: {}",
                    self.type_name()
                )));
            }
        };

        let mut field_data = IndexMap::new();
        for field in &self.fields {
            let AnnotatedField { name, ty, .. } = field;
            let ty = ctx
                .db
                .resolve_with_meta(ty.as_ref())
                .map_err(|ident| ctx.error_type_resolution(ident))?;
            if let Some(value) = data.get(name.as_ref()) {
                let value = match TyResolvedRef::from_literal(ty.ty, value, ctx) {
                    Ok(ok) => ok,
                    Err(e) => {
                        return Err(ctx
                            .error_internal(format!(
                                "attribute literal must match the type: {}",
                                self.type_name()
                            ))
                            .with_cause(e));
                    }
                };
                let meta = DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty,
                };
                field_data.insert(&**name, BamlValueWithFlags::new(value, meta));
            } else if field.ty.ty.is_optional(ctx.db) {
                // Add null for missing optional fields.
                field_data.insert(
                    &**name,
                    BamlValueWithFlags::new(BamlValue::Null(BamlNull), DeserializerMeta::new(ty)),
                );
            } else {
                // FromLiteral does not add for missing fields.
                return Err(ctx.error_internal("Provided literal is missing one or more fields."));
            }
        }
        Ok(BamlClass {
            name,
            value: field_data,
        })
    }
}

impl<'s, 'v, 't, N: TypeIdent + 't> FromLiteral<'s, 'v, 't, N> for EnumTy<'t, N>
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let (enum_name, variant_name) = match literal {
            AttrLiteral::EnumVariant {
                enum_name,
                variant_name,
            } if **enum_name == self.name => (enum_name, variant_name),
            _ => {
                return Err(ctx.error_internal(format!(
                    "attribute literal must match the type: {}",
                    self.type_name()
                )));
            }
        };

        if let Some(enum_variant) = self
            .variants
            .iter()
            .find(|variant| variant.name == *variant_name)
        {
            Ok(BamlEnum {
                name: &self.name,
                value: &enum_variant.name,
            })
        } else {
            Err(ctx.error_internal(format!(
                "unknown enum variant '{enum_name}.{variant_name}' in attribute literal"
            )))
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent + 't> FromLiteral<'s, 'v, 't, N> for EnumVariantTy<'t, N>
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let (enum_name, variant_name) = match literal {
            AttrLiteral::EnumVariant {
                enum_name,
                variant_name,
            } if **enum_name == self.name => (enum_name, variant_name),
            _ => {
                return Err(ctx.error_internal(format!(
                    "attribute literal must match the type: {}",
                    self.type_name()
                )));
            }
        };

        if self.value.name == *variant_name {
            Ok(BamlEnum {
                name: &self.name,
                value: &self.value.name,
            })
        } else {
            Err(ctx.error_internal(format!(
                "unknown enum variant '{enum_name}.{variant_name}' in attribute literal"
            )))
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for UnionTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        for variant in &self.variants {
            let variant = ctx
                .db
                .resolve_with_meta(variant.as_ref())
                .map_err(|ident| ctx.error_type_resolution(ident))?;
            if let Ok(value) = variant.ty.from_literal(literal, ctx) {
                return Ok(value);
            }
        }
        Err(ctx.error_internal(format!(
            "attribute literal must match the type: {}",
            self.type_name()
        )))
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for StreamStateTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let inner_ty = ctx
            .db
            .resolve_with_meta(self.value.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        match literal {
            AttrLiteral::StreamStateComplete(value) => {
                let value = inner_ty.ty.from_literal(&**value, ctx)?;
                let value = BamlValueWithFlags::new(
                    value,
                    DeserializerMeta {
                        flags: DeserializerConditions::default(),
                        ty: inner_ty,
                    },
                );
                Ok(BamlStreamState::Complete(Box::new(value)))
            }
            AttrLiteral::StreamStateIncomplete(value) => {
                let value = inner_ty.ty.from_literal(&**value, ctx)?;
                let value = BamlValueWithFlags::new(
                    value,
                    DeserializerMeta {
                        flags: DeserializerConditions::default(),
                        ty: inner_ty,
                    },
                );
                Ok(BamlStreamState::Incomplete(Box::new(value)))
            }
            AttrLiteral::StreamStatePending(value) => {
                let value = inner_ty.ty.from_literal(&**value, ctx)?;
                let value = BamlValueWithFlags::new(
                    value,
                    DeserializerMeta {
                        flags: DeserializerConditions::default(),
                        ty: inner_ty,
                    },
                );
                Ok(BamlStreamState::Pending(Box::new(value)))
            }
            _ => Err(ctx.error_internal("attribute literal must match the type: stream_state")),
        }
    }
}

/// Inherent method for `TyResolvedRef` dispatch, taking `self` by value (Copy)
/// instead of `&'t self`. This avoids the lifetime issue where `resolve` returns
/// a local `TyResolvedRef` that can't satisfy `&'t self` in the `FromLiteral` trait.
impl<'s, 'v, 't, N: TypeIdent> TyResolvedRef<'t, N>
where
    't: 's,
    's: 'v,
{
    pub fn from_literal(
        self,
        literal: &AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<BamlValue<'s, 'v, 't, N>, ParsingError> {
        match self {
            TyResolvedRef::Int(_) => {
                const TY: &IntTy = &IntTy;
                TY.from_literal(literal, ctx)
                    .map(BamlPrimitive::Int)
                    .map(BamlValue::from)
            }
            TyResolvedRef::Bigint(_) => {
                const TY: &BigintTy = &BigintTy;
                TY.from_literal(literal, ctx)
                    .map(BamlPrimitive::Bigint)
                    .map(BamlValue::from)
            }
            TyResolvedRef::Float(_) => {
                const TY: &FloatTy = &FloatTy;
                TY.from_literal(literal, ctx)
                    .map(BamlPrimitive::Float)
                    .map(BamlValue::from)
            }
            TyResolvedRef::String(_) => {
                const TY: &StringTy = &StringTy;
                TY.from_literal(literal, ctx)
                    .map(BamlPrimitive::String)
                    .map(BamlValue::from)
            }
            TyResolvedRef::Bool(_) => {
                const TY: &BoolTy = &BoolTy;
                TY.from_literal(literal, ctx)
                    .map(BamlPrimitive::Bool)
                    .map(BamlValue::from)
            }
            TyResolvedRef::Null(_) => {
                const TY: &NullTy = &NullTy;
                TY.from_literal(literal, ctx)
                    .map(BamlPrimitive::Null)
                    .map(BamlValue::from)
            }
            TyResolvedRef::Media(m) => {
                let ty: &'static MediaTy = match m {
                    MediaTy::Image => &MediaTy::Image,
                    MediaTy::Audio => &MediaTy::Audio,
                    MediaTy::Pdf => &MediaTy::Pdf,
                    MediaTy::Video => &MediaTy::Video,
                };
                ty.from_literal(literal, ctx)
                    .map(BamlPrimitive::Media)
                    .map(BamlValue::from)
            }
            TyResolvedRef::LiteralString(ty) => ty
                .from_literal(literal, ctx)
                .map(BamlPrimitive::String)
                .map(BamlValue::from),
            TyResolvedRef::LiteralInt(ty) => ty
                .from_literal(literal, ctx)
                .map(BamlPrimitive::Int)
                .map(BamlValue::from),
            TyResolvedRef::LiteralBigint(ty) => ty
                .from_literal(literal, ctx)
                .map(BamlPrimitive::Bigint)
                .map(BamlValue::from),
            TyResolvedRef::LiteralBool(ty) => ty
                .from_literal(literal, ctx)
                .map(BamlPrimitive::Bool)
                .map(BamlValue::from),
            TyResolvedRef::Array(ty) => ty.from_literal(literal, ctx).map(BamlValue::Array),
            TyResolvedRef::Map(ty) => ty.from_literal(literal, ctx).map(BamlValue::Map),
            TyResolvedRef::Class(ty) => ty.from_literal(literal, ctx).map(BamlValue::Class),
            TyResolvedRef::Enum(ty) => ty.from_literal(literal, ctx).map(BamlValue::Enum),
            TyResolvedRef::EnumVariant(ty) => ty.from_literal(literal, ctx).map(BamlValue::Enum),
            TyResolvedRef::Union(ty) => ty.from_literal(literal, ctx),
            TyResolvedRef::StreamState(ty) => {
                ty.from_literal(literal, ctx).map(BamlValue::StreamState)
            }
        }
    }
}
