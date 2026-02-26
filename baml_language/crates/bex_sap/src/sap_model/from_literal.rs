//! Contains [`FromLiteral`] trait and implementations.

use std::ops::Deref;

use crate::{
    deserializer::{coercer::ParsingContext, deserialize_flags::DeserializerConditions},
    sap_model::*,
};

pub trait FromLiteral<'t, N: TypeIdent>: TypeValue {
    /// Converts from a SAP model literal (used in attributes) into a BAML value.
    ///
    /// ## Errors
    /// If the literal cannot be converted for the type.
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError>;
}

impl<'t, N> FromLiteral<'t, N> for IntTy
where
    N: TypeIdent,
{
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::Int(i) => Ok(BamlInt { value: *i }),
            _ => Err(ctx.error_internal("attribute literal must match the type: int")),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for FloatTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::Float(f) => Ok(BamlFloat { value: *f }),
            _ => Err(ctx.error_internal("attribute literal must match the type: float")),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for BoolTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::Bool(b) => Ok(BamlBool { value: *b }),
            _ => Err(ctx.error_internal("attribute literal must match the type: bool")),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for StringTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::String(s) => Ok(BamlString {
                value: s.to_string(),
            }),
            _ => Err(ctx.error_internal("attribute literal must match the type: string")),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for NullTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::Null => Ok(BamlNull),
            _ => Err(ctx.error_internal("attribute literal must match the type: null")),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for MediaTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        Err(ctx.error_internal("media literals are not currently supported"))
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for PrimitiveTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
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

impl<'t, N: TypeIdent> FromLiteral<'t, N> for IntLiteralTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::Int(i) if *i == self.0 => Ok(BamlInt { value: *i }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for BoolLiteralTy {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::Bool(b) if *b == self.0 => Ok(BamlBool { value: *b }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for StringLiteralTy<'t> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match literal {
            Literal::String(s) if s == self.0.as_ref() => Ok(BamlString {
                value: s.to_string(),
            }),
            _ => Err(ctx.error_internal(format!(
                "attribute literal must match the type: {}",
                self.type_name()
            ))),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for LiteralTy<'t> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        match self {
            LiteralTy::String(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::String),
            LiteralTy::Int(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::Int),
            LiteralTy::Bool(lit) => lit.from_literal(literal, ctx).map(BamlPrimitive::Bool),
        }
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for ArrayTy<'t, N> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        let Literal::Array(items) = literal else {
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

impl<'t, N: TypeIdent> FromLiteral<'t, N> for MapTy<'t, N> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        let Literal::Object { name, data } = literal else {
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
                Ok((key.to_string(), BamlValueWithFlags::new(value, meta)))
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

impl<'t, N: TypeIdent> FromLiteral<'t, N> for ClassTy<'t, N> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        let Literal::Object { name, data } = literal else {
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
                field_data.insert(name.to_string(), BamlValueWithFlags::new(value, meta));
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

impl<'t, N: TypeIdent + 't> FromLiteral<'t, N> for EnumTy<'t, N> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        let Literal::String(s) = literal else {
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
            value: value.to_string(),
        })
    }
}

impl<'t, N: TypeIdent> FromLiteral<'t, N> for UnionTy<'t, N> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
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

impl<'t, N: TypeIdent> FromLiteral<'t, N> for StreamStateTy<'t, N> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
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
impl<'t, N: TypeIdent> TyResolvedRef<'t, N> {
    pub fn from_literal(
        self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<BamlValue<'t, N>, ParsingError> {
        match self {
            TyResolvedRef::Primitive(ty) => ty
                .as_static_ref()
                .from_literal(literal, ctx)
                .map(BamlValue::from),
            TyResolvedRef::Literal(ty) => ty.from_literal(literal, ctx).map(BamlValue::from),
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

impl<'t, N: TypeIdent> FromLiteral<'t, N> for Ty<'t, N> {
    fn from_literal(
        &'t self,
        literal: &'t Literal<'t, N>,
        ctx: &ParsingContext<'t, N>,
    ) -> Result<Self::Value, ParsingError> {
        let resolved = ctx
            .db
            .resolve(self)
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        TyResolvedRef::from_literal(resolved, literal, ctx)
    }
}
