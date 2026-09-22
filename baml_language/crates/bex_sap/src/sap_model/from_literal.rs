//! Contains [`FromLiteral`] trait and implementations.

use indexmap::IndexMap;

use crate::{
    deserializer::{
        coercer::{ParsingContext, ParsingError},
        types::{BamlValueWithFlags, DeserializerMeta},
    },
    sap_model::{
        AnnotatedField, ArrayTy, BamlArray, BamlBigint, BamlBool, BamlClass, BamlEnum, BamlFloat,
        BamlInt, BamlMap, BamlNull, BamlPrimitive, BamlStreamState, BamlString, BamlValue,
        BigintLiteralTy, BigintTy, BoolLiteralTy, BoolTy, ClassTy, DefaultValue, EnumTy,
        EnumVariantTy, FloatTy, IntLiteralTy, IntTy, LiteralTy, MapTy, MediaTy, NullTy,
        PrimitiveTy, StreamStateTy, StringLiteralTy, StringTy, TyResolvedRef, TypeIdent,
        TypeName as _, TypeValue, UnionTy,
    },
};

pub trait FromLiteral<'s, 'v, 't, N: TypeIdent>: TypeValue<'s, 'v, 't>
where
    's: 'v,
{
    /// Builds this type's value from a field default ([`crate::sap_model::TypeRefDb::field_default`]).
    /// Does not perform any transformations: the default should be of the correct type.
    ///
    /// ## Errors
    /// If the default does not belong to the type.
    #[allow(clippy::wrong_self_convention)]
    fn from_literal(
        &'t self,
        literal: &DefaultValue<N>,
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::Int(i) => Ok(BamlInt { value: *i }),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::Bigint(bi) => Ok(BamlBigint { value: bi.clone() }),
            // `int` literals widen into `bigint`.
            DefaultValue::Int(i) => Ok(BamlBigint {
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        // No float default exists (BEP-075: `float` defaults to `never`), and a float value
        // can only be pending inside a nullable union, which defaults to `null` instead.
        let _: &DefaultValue<N> = literal;
        Err::<BamlFloat, _>(ctx.error_internal("attribute literal must match the type: float"))
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for BoolTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::Bool(b) => Ok(BamlBool { value: *b }),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::String(s) => Ok(BamlString {
                value: s.clone().into(),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::Null => Ok(BamlNull),
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
        _literal: &DefaultValue<N>,
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
        literal: &DefaultValue<N>,
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::Int(i) if *i == self.0 => Ok(BamlInt { value: *i }),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::Bigint(bi) if *bi == self.0 => Ok(BamlBigint { value: bi.clone() }),
            // `int` literals matching the bigint literal value widen in.
            DefaultValue::Int(i) if num_bigint::BigInt::from(*i) == self.0 => Ok(BamlBigint {
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::Bool(b) if *b == self.0 => Ok(BamlBool { value: *b }),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::String(s) if s == self.0.as_ref() => Ok(BamlString {
                value: s.clone().into(),
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
        literal: &DefaultValue<N>,
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::EmptyArray => Ok(BamlArray { value: Vec::new() }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            DefaultValue::EmptyMap => Ok(BamlMap {
                value: IndexMap::new(),
            }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let fields = match literal {
            DefaultValue::Object { name, fields } if *name == self.name => fields,
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
                .resolve(ty)
                .map_err(|ident| ctx.error_type_resolution(ident))?;
            // A derived object default carries every field.
            let Some(value) = fields.get(name.as_ref()) else {
                return Err(ctx.error_internal(format!(
                    "default object for {} is missing field {name}",
                    self.type_name()
                )));
            };
            let value = match TyResolvedRef::from_literal(ty, value, ctx) {
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
            field_data.insert(
                &**name,
                BamlValueWithFlags::new(value, DeserializerMeta::new(ty)),
            );
        }
        Ok(BamlClass {
            name: &self.name,
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let (enum_name, variant_name) = match literal {
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } if *enum_name == self.name => (enum_name, variant_name),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let (enum_name, variant_name) = match literal {
            DefaultValue::EnumVariant {
                enum_name,
                variant_name,
            } if *enum_name == self.name => (enum_name, variant_name),
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        for variant in &self.variants {
            let variant = ctx
                .db
                .resolve(variant)
                .map_err(|ident| ctx.error_type_resolution(ident))?;
            if let Ok(value) = variant.from_literal(literal, ctx) {
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
        literal: &DefaultValue<N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let DefaultValue::StreamStatePending(inner) = literal else {
            return Err(ctx.error_internal("attribute literal must match the type: stream_state"));
        };
        let inner_ty = ctx
            .db
            .resolve(&self.value)
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let value = inner_ty.from_literal(inner, ctx)?;
        Ok(BamlStreamState::Pending(Box::new(BamlValueWithFlags::new(
            value,
            DeserializerMeta::new(inner_ty),
        ))))
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
        literal: &DefaultValue<N>,
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
