//! Contains [`FromLiteral`] trait and implementations.

use std::ops::Deref;

use crate::{
    deserializer::{coercer::ParsingContext, deserialize_flags::DeserializerConditions},
    sap_model::*,
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
    fn from_literal(
        &'t self,
        literal: &'t AttrLiteral<'t, N>,
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            AttrLiteral::Int(i) => Ok(BamlInt { value: *i }),
            _ => Err(ctx.error_internal("attribute literal must match the type: int")),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for FloatTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &'t AttrLiteral<'t, N>,
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
        literal: &'t AttrLiteral<'t, N>,
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
        literal: &'t AttrLiteral<'t, N>,
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
        literal: &'t AttrLiteral<'t, N>,
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
        _literal: &'t AttrLiteral<'t, N>,
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match self {
            PrimitiveTy::Int(ty) => ty.from_literal(literal, ctx).map(BamlPrimitive::Int),
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
        literal: &'t AttrLiteral<'t, N>,
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

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for BoolLiteralTy
where
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &'t AttrLiteral<'t, N>,
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
        literal: &'t AttrLiteral<'t, N>,
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        match self {
            LiteralTy::String(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::String),
            LiteralTy::Int(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::Int),
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
        literal: &'t AttrLiteral<'t, N>,
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
                self.ty.ty.from_literal(item, ctx).map(|item| {
                    BamlValueWithFlags::new(
                        item,
                        DeserializerMeta {
                            flags: Default::default(),
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let AttrLiteral::Map(data) = literal else {
            return Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            )));
        };
        let value_ty = ctx
            .db
            .resolve_with_meta(self.value.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let data = data
            .iter()
            .map(|(key, value)| {
                let value = self.value.ty.from_literal(value, ctx)?;
                let meta = DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: value_ty.clone(),
                };
                Ok((key.to_string().into(), BamlValueWithFlags::new(value, meta)))
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let AttrLiteral::Object { name, data } = literal else {
            return Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            )));
        };
        let mut field_data = IndexMap::new();
        for field in &self.fields {
            let AnnotatedField { name, ty, .. } = field;
            if let Some(value) = data.get(name.as_ref()) {
                let ty = ctx
                    .db
                    .resolve_with_meta(ty.as_ref())
                    .map_err(|ident| ctx.error_type_resolution(ident))?;
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
            } else if !field.ty.ty.is_optional(ctx.db) {
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let AttrLiteral::String(s) = literal else {
            return Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            )));
        };
        let value = self
            .variants
            .iter()
            .find_map(|variant| {
                if variant.name == *s {
                    Some(&*variant.name)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                ctx.error_internal(format!(
                    "attribute literal must match the type: {}",
                    self.type_name()
                ))
            })?;
        Ok(BamlEnum {
            name: &self.name,
            value,
        })
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for UnionTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        for TyWithMeta { ty, .. } in &self.variants {
            if let Ok(value) = ty.from_literal(literal, ctx) {
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let inner_ty = ctx
            .db
            .resolve_with_meta(self.value.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let value = self.value.ty.from_literal(literal, ctx)?;
        let value = BamlValueWithFlags::new(
            value,
            DeserializerMeta {
                flags: Default::default(),
                ty: inner_ty,
            },
        );
        Ok(BamlStreamState::Complete(Box::new(value)))
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
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<BamlValue<'s, 'v, 't, N>, ParsingError> {
        match self {
            TyResolvedRef::Int(_) => {
                const TY: &IntTy = &IntTy;
                TY.from_literal(literal, ctx)
                    .map(BamlPrimitive::Int)
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
            TyResolvedRef::LiteralBool(ty) => ty
                .from_literal(literal, ctx)
                .map(BamlPrimitive::Bool)
                .map(BamlValue::from),
            TyResolvedRef::Array(ty) => ty.from_literal(literal, ctx).map(BamlValue::Array),
            TyResolvedRef::Map(ty) => ty.from_literal(literal, ctx).map(BamlValue::Map),
            TyResolvedRef::Class(ty) => ty.from_literal(literal, ctx).map(BamlValue::Class),
            TyResolvedRef::Enum(ty) => ty.from_literal(literal, ctx).map(BamlValue::Enum),
            TyResolvedRef::Union(ty) => ty.from_literal(literal, ctx),
            TyResolvedRef::StreamState(ty) => {
                ty.from_literal(literal, ctx).map(BamlValue::StreamState)
            }
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> FromLiteral<'s, 'v, 't, N> for Ty<'t, N>
where
    't: 's,
    's: 'v,
{
    fn from_literal(
        &'t self,
        literal: &'t AttrLiteral<'t, N>,
        ctx: &ParsingContext<'s, 'v, 't, N>,
    ) -> Result<Self::Value, ParsingError> {
        let resolved = ctx
            .db
            .resolve(self)
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        TyResolvedRef::from_literal(resolved, literal, ctx)
    }
}
