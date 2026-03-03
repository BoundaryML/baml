use std::borrow::Cow;

use crate::baml_value::{BamlPrimitive, BamlString, BamlValue};
use crate::deserializer::coercer::match_string::match_string;
use crate::deserializer::coercer::{ParsingContext, ParsingError, TypeCoercer, array_helper};
use crate::deserializer::types::DeserializerMeta;
use crate::deserializer::{deserialize_flags::Flag, types::BamlValueWithFlags};
use crate::jsonish::{self, CompletionState};
use crate::sap_model::{
    ArrayTy, AttrLiteral, BoolLiteralTy, BoolTy, ClassTy, EnumTy, FloatTy, IntLiteralTy, IntTy,
    MapTy, MediaTy, NullTy, PrimitiveTy, StreamStateTy, StringLiteralTy, StringTy, TyResolvedRef,
    TyWithMeta, TypeAnnotations, TypeIdent, UnionTy,
};

/// Dispatch methods for `TyResolvedRef` that delegate to the appropriate
/// `TypeCoercer` implementation based on the variant.
///
/// These are inherent methods (not trait impl) because `TyResolvedRef` is Copy
/// and returned by value from `resolve_with_meta`. The `TypeCoercer` trait
/// requires `&'t Self`, which would require a `'t`-lived reference to a local.
/// By taking `Self` by value (Copy), we avoid that lifetime issue entirely.
impl<'s, 'v, 't, N: TypeIdent> TyResolvedRef<'t, N>
where
    't: 's,
    's: 'v,
{
    pub fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<BamlValueWithFlags<'s, 'v, 't, N>> {
        match target.ty {
            TyResolvedRef::Int(v) => {
                let p = PrimitiveTy::Int(v);
                PrimitiveTy::try_cast(ctx, TyWithMeta::new(p.as_static_ref(), target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Float(v) => {
                let p = PrimitiveTy::Float(v);
                PrimitiveTy::try_cast(ctx, TyWithMeta::new(p.as_static_ref(), target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::String(v) => {
                let p = PrimitiveTy::String(v);
                PrimitiveTy::try_cast(ctx, TyWithMeta::new(p.as_static_ref(), target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Bool(v) => {
                let p = PrimitiveTy::Bool(v);
                PrimitiveTy::try_cast(ctx, TyWithMeta::new(p.as_static_ref(), target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Null(v) => {
                let p = PrimitiveTy::Null(v);
                PrimitiveTy::try_cast(ctx, TyWithMeta::new(p.as_static_ref(), target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Media(v) => {
                let p = PrimitiveTy::Media(v);
                PrimitiveTy::try_cast(ctx, TyWithMeta::new(p.as_static_ref(), target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::LiteralString(l) => {
                StringLiteralTy::try_cast(ctx, TyWithMeta::new(l, target.meta), value)
                    .map(|v| v.map_value(BamlPrimitive::String).map_value(Into::into))
            }
            TyResolvedRef::LiteralInt(l) => {
                IntLiteralTy::try_cast(ctx, TyWithMeta::new(l, target.meta), value)
                    .map(|v| v.map_value(BamlPrimitive::Int).map_value(Into::into))
            }
            TyResolvedRef::LiteralBool(l) => {
                BoolLiteralTy::try_cast(ctx, TyWithMeta::new(l, target.meta), value)
                    .map(|v| v.map_value(BamlPrimitive::Bool).map_value(Into::into))
            }
            TyResolvedRef::Array(a) => {
                ArrayTy::try_cast(ctx, TyWithMeta::new(a, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Map(m) => MapTy::try_cast(ctx, TyWithMeta::new(m, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            TyResolvedRef::Class(c) => {
                ClassTy::try_cast(ctx, TyWithMeta::new(c, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Enum(e) => EnumTy::try_cast(ctx, TyWithMeta::new(e, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            TyResolvedRef::Union(u) => {
                UnionTy::try_cast(ctx, TyWithMeta::new(u, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::StreamState(s) => {
                StreamStateTy::try_cast(ctx, TyWithMeta::new(s, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
        }
    }

    /// Returns `None` if the value is incomplete and the `in_progress` is `never`.
    pub fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError> {
        // Extract fields - both are Copy.
        let target_meta = target.meta;

        if value.completion_state() == &CompletionState::Incomplete {
            match &target.meta.in_progress {
                Some(AttrLiteral::Never) => return Ok(None),
                Some(lit) => {
                    let ret = target.ty.from_literal(lit, ctx)?;
                    return Ok(Some(BamlValueWithFlags::new(
                        ret,
                        DeserializerMeta::new(target.clone()),
                    )));
                }
                None => {}
            }
        }

        let result = match value {
            jsonish::Value::AnyOf(candidates, primitive) => {
                log::debug!(
                    "scope: {scope} :: coercing to: {name} (current: {current})",
                    name = target.clone(),
                    scope = ctx.display_scope(),
                    current = value.r#type()
                );

                match target.ty {
                    TyResolvedRef::String(_) => BamlValueWithFlags::new(
                        BamlValue::String(BamlString {
                            value: primitive.clone(),
                        }),
                        DeserializerMeta::new(target.clone()),
                    ),
                    TyResolvedRef::Enum(enum_ty) => {
                        let primitive =
                            jsonish::Value::String(primitive.clone(), CompletionState::Complete);
                        let ret = EnumTy::coerce_from_cow(
                            ctx,
                            TyWithMeta::new(enum_ty, target_meta),
                            Cow::Owned(primitive),
                            [],
                        )?;
                        match ret {
                            Some(v) => v.map_value(BamlValue::Enum),
                            None => return Ok(None),
                        }
                    }
                    TyResolvedRef::LiteralString(s) => {
                        let candidates = [(&*s.0, vec![&*s.0])];
                        let ret = match_string(
                            ctx,
                            TyWithMeta::new(TyResolvedRef::String(StringTy), target.meta),
                            Cow::Borrowed(value),
                            &candidates,
                            true,
                        )?;
                        ret.map_value(|v| BamlValue::String(BamlString { value: v.into() }))
                    }
                    _ => array_helper::coerce_array_to_singular(
                        ctx,
                        target.clone(),
                        candidates.iter(),
                        &|val| Self::coerce(ctx, target.clone(), val),
                    )?,
                }
            }
            crate::jsonish::Value::Markdown(_t, v, _completion) => {
                log::debug!(
                    "scope: {scope} :: coercing to: {name} (current: {current})",
                    name = target,
                    scope = ctx.display_scope(),
                    current = value.r#type()
                );
                let Some(ret) = Self::coerce(ctx, target.clone(), v)? else {
                    return Ok(None);
                };

                let flag = if matches!(target.ty, TyResolvedRef::String(_)) {
                    Flag::ObjectFromMarkdown(1)
                } else {
                    Flag::ObjectFromMarkdown(0)
                };
                ret.with_flag(flag)
            }
            crate::jsonish::Value::FixedJson(v, fixes) => {
                log::debug!(
                    "scope: {scope} :: coercing to: {name} (current: {current})",
                    name = target,
                    scope = ctx.display_scope(),
                    current = value.r#type()
                );
                let Some(ret) = Self::coerce(ctx, target.clone(), v)? else {
                    return Ok(None);
                };
                ret.with_flag(Flag::ObjectFromFixedJson(fixes.to_vec()))
            }
            _ => {
                if let Some(value) = Self::try_cast(ctx, target.clone(), value) {
                    value
                } else {
                    let ret = match target.ty {
                        // Primitives: reconstruct PrimitiveTy and delegate
                        TyResolvedRef::Int(_) => {
                            IntTy::coerce(ctx, TyWithMeta::new(&IntTy, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Int))
                        }
                        TyResolvedRef::Float(_) => {
                            FloatTy::coerce(ctx, TyWithMeta::new(&FloatTy, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Float))
                        }
                        TyResolvedRef::String(_) => {
                            StringTy::coerce(ctx, TyWithMeta::new(&StringTy, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::String))
                        }
                        TyResolvedRef::Bool(_) => {
                            BoolTy::coerce(ctx, TyWithMeta::new(&BoolTy, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Bool))
                        }
                        TyResolvedRef::Null(_) => {
                            NullTy::coerce(ctx, TyWithMeta::new(&NullTy, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Null))
                        }
                        TyResolvedRef::Media(MediaTy::Image) => MediaTy::coerce(
                            ctx,
                            TyWithMeta::new(&MediaTy::Image, target_meta),
                            value,
                        )?
                        .map(|v| v.map_value(BamlValue::Media)),
                        TyResolvedRef::Media(MediaTy::Audio) => MediaTy::coerce(
                            ctx,
                            TyWithMeta::new(&MediaTy::Audio, target_meta),
                            value,
                        )?
                        .map(|v| v.map_value(BamlValue::Media)),
                        TyResolvedRef::Media(MediaTy::Pdf) => MediaTy::coerce(
                            ctx,
                            TyWithMeta::new(&MediaTy::Pdf, target_meta),
                            value,
                        )?
                        .map(|v| v.map_value(BamlValue::Media)),
                        TyResolvedRef::Media(MediaTy::Video) => MediaTy::coerce(
                            ctx,
                            TyWithMeta::new(&MediaTy::Video, target_meta),
                            value,
                        )?
                        .map(|v| v.map_value(BamlValue::Media)),
                        TyResolvedRef::LiteralString(l) => {
                            StringLiteralTy::coerce(ctx, TyWithMeta::new(l, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::String))
                        }
                        TyResolvedRef::LiteralInt(l) => {
                            IntLiteralTy::coerce(ctx, TyWithMeta::new(l, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Int))
                        }
                        TyResolvedRef::LiteralBool(l) => {
                            BoolLiteralTy::coerce(ctx, TyWithMeta::new(l, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Bool))
                        }
                        TyResolvedRef::Array(a) => {
                            ArrayTy::coerce(ctx, TyWithMeta::new(a, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Array))
                        }
                        TyResolvedRef::Map(m) => {
                            MapTy::coerce(ctx, TyWithMeta::new(m, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Map))
                        }
                        TyResolvedRef::Class(c) => {
                            ClassTy::coerce(ctx, TyWithMeta::new(c, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Class))
                        }
                        TyResolvedRef::Enum(e) => {
                            EnumTy::coerce(ctx, TyWithMeta::new(e, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::Enum))
                        }
                        TyResolvedRef::Union(u) => {
                            UnionTy::coerce(ctx, TyWithMeta::new(u, target_meta), value)?
                        }
                        TyResolvedRef::StreamState(s) => {
                            StreamStateTy::coerce(ctx, TyWithMeta::new(s, target_meta), value)?
                                .map(|v| v.map_value(BamlValue::StreamState))
                        }
                    };
                    let Some(ret) = ret else {
                        return Ok(None);
                    };
                    ret
                }
            }
        };
        Ok(Some(result))
    }
}

// TODO: Implement validate_asserts once Assertion/Constraint types are fully defined.
// pub fn validate_asserts(constraints: &[(Constraint, bool)]) -> Result<(), ParsingError> { ... }

// TODO: Implement DefaultValue for AnnotatedTy once Assertion type is fully defined.
// The old implementation matched on AnnotatedTy variants (Enum, List, Class, etc.)
// and provided default values (empty list, null, empty map) for optional types.
// It also validated constraints/asserts on the default values.
