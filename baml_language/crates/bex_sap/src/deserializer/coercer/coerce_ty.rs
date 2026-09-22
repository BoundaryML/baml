use std::borrow::Cow;

use crate::{
    baml_value::{BamlPrimitive, BamlString, BamlValue},
    deserializer::{
        coercer::{
            ParsingContext, ParsingError, TypeCoercer, array_helper, match_string::match_string,
        },
        deserialize_flags::Flag,
        types::{BamlValueWithFlags, DeserializerMeta},
    },
    jsonish::{self, CompletionState},
    sap_model::{
        ArrayTy, BigintLiteralTy, BigintTy, BoolLiteralTy, BoolTy, ClassTy, EnumTy, EnumVariantTy,
        FloatTy, IntLiteralTy, IntTy, MapTy, MediaTy, NullTy, PrimitiveTy, StreamStateTy,
        StringLiteralTy, StringTy, TyResolvedRef, TypeIdent, UnionTy,
    },
};

/// Dispatch methods for `TyResolvedRef` that delegate to the appropriate
/// `TypeCoercer` implementation based on the variant.
///
/// These are inherent methods (not trait impl) because `TyResolvedRef` is Copy
/// and returned by value from `resolve`. The `TypeCoercer` trait
/// requires `&'t Self`, which would require a `'t`-lived reference to a local.
/// By taking `Self` by value (Copy), we avoid that lifetime issue entirely.
impl<'s, 'v, 't, N: TypeIdent> TyResolvedRef<'t, N>
where
    't: 's,
    's: 'v,
{
    /// Whether an incomplete value of this type has a partial parse (BEP-075's "can be
    /// partial?" column). A type without one yields nothing until its value completes, and
    /// its container substitutes a default.
    ///
    /// Classes and unions decide per value: the class coercer applies `@@stream.done` and the
    /// field defaults, and a union is partial when its matching member is.
    pub fn has_partial_parse(self) -> bool {
        match self {
            TyResolvedRef::String(_)
            | TyResolvedRef::Array(_)
            | TyResolvedRef::Map(_)
            | TyResolvedRef::Class(_)
            | TyResolvedRef::Union(_)
            | TyResolvedRef::StreamState(_) => true,
            TyResolvedRef::Int(_)
            | TyResolvedRef::Bigint(_)
            | TyResolvedRef::Float(_)
            | TyResolvedRef::Bool(_)
            | TyResolvedRef::Null(_)
            | TyResolvedRef::Media(_)
            | TyResolvedRef::LiteralString(_)
            | TyResolvedRef::LiteralInt(_)
            | TyResolvedRef::LiteralBigint(_)
            | TyResolvedRef::LiteralBool(_)
            | TyResolvedRef::Enum(_)
            | TyResolvedRef::EnumVariant(_) => false,
        }
    }

    /// Returns `None` if the value is incomplete and the type has no partial parse.
    pub fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<BamlValueWithFlags<'s, 'v, 't, N>> {
        if !target.has_partial_parse() && value.completion_state() == &CompletionState::Incomplete {
            return None;
        }
        match target {
            TyResolvedRef::Int(v) => {
                let p = PrimitiveTy::Int(v);
                PrimitiveTy::try_cast(ctx, p.as_static_ref(), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Bigint(v) => {
                let p = PrimitiveTy::Bigint(v);
                PrimitiveTy::try_cast(ctx, p.as_static_ref(), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Float(v) => {
                let p = PrimitiveTy::Float(v);
                PrimitiveTy::try_cast(ctx, p.as_static_ref(), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::String(v) => {
                let p = PrimitiveTy::String(v);
                PrimitiveTy::try_cast(ctx, p.as_static_ref(), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Bool(v) => {
                let p = PrimitiveTy::Bool(v);
                PrimitiveTy::try_cast(ctx, p.as_static_ref(), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Null(v) => {
                let p = PrimitiveTy::Null(v);
                PrimitiveTy::try_cast(ctx, p.as_static_ref(), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Media(v) => {
                let p = PrimitiveTy::Media(v);
                PrimitiveTy::try_cast(ctx, p.as_static_ref(), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::LiteralString(l) => StringLiteralTy::try_cast(ctx, l, value)
                .map(|v| v.map_value(BamlPrimitive::String).map_value(Into::into)),
            TyResolvedRef::LiteralInt(l) => IntLiteralTy::try_cast(ctx, l, value)
                .map(|v| v.map_value(BamlPrimitive::Int).map_value(Into::into)),
            TyResolvedRef::LiteralBigint(l) => BigintLiteralTy::try_cast(ctx, l, value)
                .map(|v| v.map_value(BamlPrimitive::Bigint).map_value(Into::into)),
            TyResolvedRef::LiteralBool(l) => BoolLiteralTy::try_cast(ctx, l, value)
                .map(|v| v.map_value(BamlPrimitive::Bool).map_value(Into::into)),
            TyResolvedRef::Array(a) => {
                ArrayTy::try_cast(ctx, a, value).map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Map(m) => {
                MapTy::try_cast(ctx, m, value).map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Class(c) => {
                ClassTy::try_cast(ctx, c, value).map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Enum(e) => {
                EnumTy::try_cast(ctx, e, value).map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::EnumVariant(e) => {
                EnumVariantTy::try_cast(ctx, e, value).map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Union(u) => {
                UnionTy::try_cast(ctx, u, value).map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::StreamState(s) => {
                StreamStateTy::try_cast(ctx, s, value).map(|v| v.map_value(Into::into))
            }
        }
    }

    /// Returns `Ok(None)` if the value is incomplete and the type has no partial parse.
    pub fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError> {
        if !target.has_partial_parse() && value.completion_state() == &CompletionState::Incomplete {
            return Ok(None);
        }

        let result = match value {
            jsonish::Value::AnyOf(candidates, primitive) => match target {
                TyResolvedRef::String(_) => {
                    // If the complete response is a JSON string literal, honor
                    // that structure and return the decoded value. Plain text,
                    // partial strings, and prose containing JSON stay verbatim.
                    let value = serde_json::from_str::<String>(primitive.as_ref())
                        .map(Cow::Owned)
                        .unwrap_or_else(|_| primitive.clone());

                    BamlValueWithFlags::new(
                        BamlValue::String(BamlString { value }),
                        DeserializerMeta::new(target),
                    )
                }
                TyResolvedRef::Enum(enum_ty) => {
                    let primitive =
                        jsonish::Value::String(primitive.clone(), CompletionState::Complete);
                    let ret = EnumTy::coerce_from_cow(ctx, enum_ty, Cow::Owned(primitive), [])?;
                    match ret {
                        Some(v) => v.map_value(BamlValue::Enum),
                        None => return Ok(None),
                    }
                }
                TyResolvedRef::EnumVariant(ev_ty) => {
                    let primitive =
                        jsonish::Value::String(primitive.clone(), CompletionState::Complete);
                    let ret =
                        EnumVariantTy::coerce_from_cow(ctx, ev_ty, Cow::Owned(primitive), [])?;
                    match ret {
                        Some(v) => v.map_value(BamlValue::Enum),
                        None => return Ok(None),
                    }
                }
                TyResolvedRef::LiteralString(s) => {
                    let candidates = [(&*s.0, vec![&*s.0])];
                    let ret = match_string(
                        ctx,
                        TyResolvedRef::String(StringTy),
                        Cow::Borrowed(value),
                        &candidates,
                        true,
                    )?;
                    ret.map_value(|v| BamlValue::String(BamlString { value: v.into() }))
                }
                _ => match array_helper::coerce_array_to_singular(
                    ctx,
                    target,
                    candidates.iter(),
                    &|val| Self::coerce(ctx, target, val),
                )? {
                    Some(v) => v,
                    None => return Ok(None),
                },
            },
            crate::jsonish::Value::Markdown(_t, v, _completion) => {
                let Some(ret) = Self::coerce(ctx, target, v)? else {
                    return Ok(None);
                };

                let flag = if matches!(target, TyResolvedRef::String(_)) {
                    Flag::ObjectFromMarkdown(1)
                } else {
                    Flag::ObjectFromMarkdown(0)
                };
                ret.with_flag(flag)
            }
            crate::jsonish::Value::FixedJson(v, fixes) => {
                let Some(ret) = Self::coerce(ctx, target, v)? else {
                    return Ok(None);
                };
                ret.with_flag(Flag::ObjectFromFixedJson(fixes.clone()))
            }
            _ => {
                if let Some(value) = Self::try_cast(ctx, target, value) {
                    value
                } else {
                    let ret = match target {
                        // Primitives: reconstruct PrimitiveTy and delegate
                        TyResolvedRef::Int(_) => {
                            IntTy::coerce(ctx, &IntTy, value)?.map(|v| v.map_value(BamlValue::Int))
                        }
                        TyResolvedRef::Bigint(_) => BigintTy::coerce(ctx, &BigintTy, value)?
                            .map(|v| v.map_value(BamlValue::Bigint)),
                        TyResolvedRef::Float(_) => FloatTy::coerce(ctx, &FloatTy, value)?
                            .map(|v| v.map_value(BamlValue::Float)),
                        TyResolvedRef::String(_) => StringTy::coerce(ctx, &StringTy, value)?
                            .map(|v| v.map_value(BamlValue::String)),
                        TyResolvedRef::Bool(_) => BoolTy::coerce(ctx, &BoolTy, value)?
                            .map(|v| v.map_value(BamlValue::Bool)),
                        TyResolvedRef::Null(_) => NullTy::coerce(ctx, &NullTy, value)?
                            .map(|v| v.map_value(BamlValue::Null)),
                        TyResolvedRef::Media(MediaTy::Image) => {
                            MediaTy::coerce(ctx, &MediaTy::Image, value)?
                                .map(|v| v.map_value(BamlValue::Media))
                        }
                        TyResolvedRef::Media(MediaTy::Audio) => {
                            MediaTy::coerce(ctx, &MediaTy::Audio, value)?
                                .map(|v| v.map_value(BamlValue::Media))
                        }
                        TyResolvedRef::Media(MediaTy::Pdf) => {
                            MediaTy::coerce(ctx, &MediaTy::Pdf, value)?
                                .map(|v| v.map_value(BamlValue::Media))
                        }
                        TyResolvedRef::Media(MediaTy::Video) => {
                            MediaTy::coerce(ctx, &MediaTy::Video, value)?
                                .map(|v| v.map_value(BamlValue::Media))
                        }
                        TyResolvedRef::LiteralString(l) => StringLiteralTy::coerce(ctx, l, value)?
                            .map(|v| v.map_value(BamlValue::String)),
                        TyResolvedRef::LiteralInt(l) => IntLiteralTy::coerce(ctx, l, value)?
                            .map(|v| v.map_value(BamlValue::Int)),
                        TyResolvedRef::LiteralBigint(l) => BigintLiteralTy::coerce(ctx, l, value)?
                            .map(|v| v.map_value(BamlValue::Bigint)),
                        TyResolvedRef::LiteralBool(l) => BoolLiteralTy::coerce(ctx, l, value)?
                            .map(|v| v.map_value(BamlValue::Bool)),
                        TyResolvedRef::Array(a) => {
                            ArrayTy::coerce(ctx, a, value)?.map(|v| v.map_value(BamlValue::Array))
                        }
                        TyResolvedRef::Map(m) => {
                            MapTy::coerce(ctx, m, value)?.map(|v| v.map_value(BamlValue::Map))
                        }
                        TyResolvedRef::Class(c) => {
                            ClassTy::coerce(ctx, c, value)?.map(|v| v.map_value(BamlValue::Class))
                        }
                        TyResolvedRef::Enum(e) => {
                            EnumTy::coerce(ctx, e, value)?.map(|v| v.map_value(BamlValue::Enum))
                        }
                        TyResolvedRef::EnumVariant(e) => EnumVariantTy::coerce(ctx, e, value)?
                            .map(|v| v.map_value(BamlValue::Enum)),
                        TyResolvedRef::Union(u) => UnionTy::coerce(ctx, u, value)?,
                        TyResolvedRef::StreamState(s) => StreamStateTy::coerce(ctx, s, value)?
                            .map(|v| v.map_value(BamlValue::StreamState)),
                    };
                    let Some(ret) = ret else {
                        return Ok(None);
                    };
                    ret
                }
            }
        };
        // Post-check: reject top-level InferedObject(String) coercion.
        // When a bare string is wrapped into a single-field class via implied-key
        // at the top level, the result is likely wrong. Only reject at root scope
        // so that nested implied-key coercion (e.g. {"item": "hello"} → Inner { value: string })
        // is still allowed.
        if ctx.scope.is_empty() {
            if result.conditions().flags.iter().any(|f| {
                matches!(
                    f,
                    Flag::InferedObject(Cow::Borrowed(crate::jsonish::Value::String(..)))
                )
            }) {
                return Err(ctx.error_unexpected_type(&target, &"string (inferred object)"));
            }
        }

        Ok(Some(result))
    }
}
