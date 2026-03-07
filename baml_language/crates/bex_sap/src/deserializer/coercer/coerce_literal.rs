use std::borrow::Cow;

use crate::baml_value::{BamlBool, BamlInt, BamlString, BamlValue};
use crate::deserializer::deserialize_flags::DeserializerConditions;
use crate::deserializer::types::{DeserializerMeta, ValueWithFlags};
use crate::jsonish::CompletionState;
use crate::sap_model::{
    AttrLiteral, BoolLiteralTy, BoolTy, FromLiteral as _, IntLiteralTy, IntTy, LiteralTy,
    StringLiteralTy, StringTy, TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent,
};
use anyhow::Result;

use super::{ParsingContext, ParsingError};
use crate::{
    deserializer::{
        coercer::{TypeCoercer, match_string::match_string},
        deserialize_flags::Flag,
    },
    jsonish,
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for IntLiteralTy
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let mut result = match value {
            crate::jsonish::Value::Number(number, _)
                if number.as_i64().map(|n| n == target.ty.0).unwrap_or(false) =>
            {
                Some(ValueWithFlags::new(
                    BamlInt {
                        value: number.as_i64().unwrap(),
                    },
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(TyResolvedRef::Int(IntTy), target.meta),
                    },
                ))
            }
            _ => None,
        };

        match value.completion_state() {
            crate::jsonish::CompletionState::Complete => {}
            crate::jsonish::CompletionState::Incomplete => {
                result
                    .iter_mut()
                    .for_each(|r| r.meta.flags.add_flag(Flag::Incomplete));
            }
        }

        result
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: int literal {literal} (current: {current})",
            literal = target.ty.0,
            scope = ctx.display_scope(),
            current = value.r#type()
        );

        let ret = match value {
            jsonish::Value::Null => Err(ctx.error_unexpected_null(target.ty)),
            jsonish::Value::Object(_, CompletionState::Incomplete) => {
                // The object could be more than one key
                match &target.meta.in_progress {
                    Some(AttrLiteral::Never) => return Ok(None),
                    Some(lit) => {
                        let ret = target.ty.from_literal(lit, ctx).map(|ret| {
                            ValueWithFlags::new(
                                ret,
                                DeserializerMeta {
                                    flags: DeserializerConditions::new().with_flag(
                                        Flag::DefaultFromInProgress(Cow::Borrowed(value)),
                                    ),
                                    ty: target.clone().map_ty(|_| TyResolvedRef::Int(IntTy)),
                                },
                            )
                        });
                        ret.map(Some)
                    }
                    None => {
                        let flags = DeserializerConditions::new()
                            .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)))
                            .with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value)));
                        Ok(Some(ValueWithFlags::new(
                            BamlInt { value: target.ty.0 },
                            DeserializerMeta {
                                flags,
                                ty: target.clone().map_ty(|_| TyResolvedRef::Int(IntTy)),
                            },
                        )))
                    }
                }
            }
            jsonish::Value::Object(obj, CompletionState::Complete) => match obj.as_slice() {
                [
                    (
                        _,
                        v @ (jsonish::Value::Number(_, _)
                        | jsonish::Value::Boolean(_)
                        | jsonish::Value::String(_, _)),
                    ),
                ] => Self::coerce(ctx, target.clone(), v).map(|ret| {
                    ret.map(|ret| ret.with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value))))
                }),
                _ => Err(ctx.error_unexpected_type(target.ty, value)),
            },
            _ => {
                // inner coerce will handle the completion state
                let int_target = TyWithMeta::new(&IntTy, target.meta);
                match IntTy::coerce(ctx, int_target, value) {
                    Ok(Some(ret)) if ret.value.value == target.ty.0 => Ok(Some(ret)),
                    Ok(Some(_ret)) => Err(ctx.error_unexpected_type(&target, value)),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                }
            }
        };

        match ret {
            Ok(Some(ret)) => {
                target
                    .meta
                    .expect_asserts(&BamlValue::Int(ret.value), ctx)?;
                Ok(Some(ret))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for BoolLiteralTy
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let mut result = match value {
            crate::jsonish::Value::Boolean(b) if *b == target.ty.0 => Some(ValueWithFlags::new(
                Self::Value { value: *b },
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyWithMeta::new(TyResolvedRef::Bool(BoolTy), target.meta),
                },
            )),
            _ => None,
        };

        match value.completion_state() {
            crate::jsonish::CompletionState::Complete => {}
            crate::jsonish::CompletionState::Incomplete => {
                result
                    .iter_mut()
                    .for_each(|r| r.meta.flags.add_flag(Flag::Incomplete));
            }
        }

        result
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: bool literal {literal} (current: {current})",
            literal = target.ty.0,
            scope = ctx.display_scope(),
            current = value.r#type()
        );

        let ret = match value {
            jsonish::Value::Null => Err(ctx.error_unexpected_null(target.ty)),
            jsonish::Value::Object(_, CompletionState::Incomplete) => {
                match &target.meta.in_progress {
                    Some(AttrLiteral::Never) => return Ok(None),
                    Some(lit) => {
                        let ret = target.ty.from_literal(lit, ctx).map(|ret| {
                            ValueWithFlags::new(
                                ret,
                                DeserializerMeta {
                                    flags: DeserializerConditions::new().with_flag(
                                        Flag::DefaultFromInProgress(Cow::Borrowed(value)),
                                    ),
                                    ty: target.clone().map_ty(|_| TyResolvedRef::Bool(BoolTy)),
                                },
                            )
                        });
                        ret.map(Some)
                    }
                    None => {
                        let flags = DeserializerConditions::new()
                            .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)))
                            .with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value)));
                        Ok(Some(ValueWithFlags::new(
                            BamlBool {
                                value: target.ty.0,
                            },
                            DeserializerMeta {
                                flags,
                                ty: target.clone().map_ty(|_| TyResolvedRef::Bool(BoolTy)),
                            },
                        )))
                    }
                }
            }
            jsonish::Value::Object(obj, CompletionState::Complete) => match obj.as_slice() {
                [
                    (
                        _,
                        v @ (jsonish::Value::Number(_, _)
                        | jsonish::Value::Boolean(_)
                        | jsonish::Value::String(_, _)),
                    ),
                ] => Self::coerce(ctx, target.clone(), v).map(|ret| {
                    ret.map(|ret| ret.with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value))))
                }),
                _ => Err(ctx.error_unexpected_type(target.ty, value)),
            },
            _ => {
                let bool_target = TyWithMeta::new(&BoolTy, target.meta);
                match BoolTy::coerce(ctx, bool_target, value) {
                    Ok(Some(ret)) if ret.value.value == target.ty.0 => Ok(Some(ret)),
                    Ok(Some(_ret)) => Err(ctx.error_unexpected_type(&target, value)),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                }
            }
        };

        match ret {
            Ok(Some(ret)) => {
                target
                    .meta
                    .expect_asserts(&BamlValue::Bool(ret.value), ctx)?;
                Ok(Some(ret))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for StringLiteralTy<'t>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let mut result = match value {
            crate::jsonish::Value::String(s, _) if s == target.ty.0.as_ref() => {
                Some(ValueWithFlags::new(
                    Self::Value {
                        value: s.to_string().into(),
                    },
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(TyResolvedRef::String(StringTy), target.meta),
                    },
                ))
            }
            _ => None,
        };

        match value.completion_state() {
            crate::jsonish::CompletionState::Complete => {}
            crate::jsonish::CompletionState::Incomplete => {
                result
                    .iter_mut()
                    .for_each(|r| r.meta.flags.add_flag(Flag::Incomplete));
            }
        }

        result
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: string literal {literal:?} (current: {current})",
            literal = target.ty.0,
            scope = ctx.display_scope(),
            current = value.r#type()
        );

        let ret = match value {
            jsonish::Value::Null => Err(ctx.error_unexpected_null(target.ty)),
            jsonish::Value::Object(_, CompletionState::Incomplete) => {
                match &target.meta.in_progress {
                    Some(AttrLiteral::Never) => return Ok(None),
                    Some(lit) => {
                        let ret = target.ty.from_literal(lit, ctx).map(|ret| {
                            ValueWithFlags::new(
                                ret,
                                DeserializerMeta {
                                    flags: DeserializerConditions::new().with_flag(
                                        Flag::DefaultFromInProgress(Cow::Borrowed(value)),
                                    ),
                                    ty: target
                                        .clone()
                                        .map_ty(|_| TyResolvedRef::String(StringTy)),
                                },
                            )
                        });
                        ret.map(Some)
                    }
                    None => {
                        let flags = DeserializerConditions::new()
                            .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)))
                            .with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value)));
                        Ok(Some(ValueWithFlags::new(
                            BamlString {
                                value: target.ty.0.clone(),
                            },
                            DeserializerMeta {
                                flags,
                                ty: target
                                    .clone()
                                    .map_ty(|_| TyResolvedRef::String(StringTy)),
                            },
                        )))
                    }
                }
            }
            jsonish::Value::Object(obj, CompletionState::Complete) => match obj.as_slice() {
                [
                    (
                        _,
                        v @ (jsonish::Value::Number(_, _)
                        | jsonish::Value::Boolean(_)
                        | jsonish::Value::String(_, _)),
                    ),
                ] => Self::coerce(ctx, target.clone(), v).map(|ret| {
                    ret.map(|ret| ret.with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value))))
                }),
                _ => Err(ctx.error_unexpected_type(target.ty, value)),
            },
            _ => {
                let candidates = vec![(target.ty.0.as_ref(), vec![&*target.ty.0])];
                let literal_match = match_string(
                    ctx,
                    target.clone().map_ty(TyResolvedRef::LiteralString),
                    Cow::Borrowed(value),
                    &candidates,
                    true,
                )?;
                Ok(Some(literal_match.map_value(|s| BamlString { value: s.into() })))
            }
        };

        match ret {
            Ok(Some(ret)) => {
                target
                    .meta
                    .expect_asserts(&BamlValue::String(ret.value.clone()), ctx)?;
                Ok(Some(ret))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for LiteralTy<'t>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        match target.ty {
            LiteralTy::Int(lit) => {
                let result = IntLiteralTy::try_cast(ctx, TyWithMeta::new(lit, target.meta), value)?;
                Some(ValueWithFlags::new(
                    Self::Value::Int(result.value),
                    result.meta,
                ))
            }
            LiteralTy::Bool(lit) => {
                let result =
                    BoolLiteralTy::try_cast(ctx, TyWithMeta::new(lit, target.meta), value)?;
                Some(ValueWithFlags::new(
                    Self::Value::Bool(result.value),
                    result.meta,
                ))
            }
            LiteralTy::String(lit) => {
                let result =
                    StringLiteralTy::try_cast(ctx, TyWithMeta::new(lit, target.meta), value)?;
                Some(ValueWithFlags::new(
                    Self::Value::String(result.value),
                    result.meta,
                ))
            }
        }
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        match target.ty {
            LiteralTy::Int(lit) => {
                IntLiteralTy::coerce(ctx, TyWithMeta::new(lit, target.meta), value)
                    .map(|opt| opt.map(|v| ValueWithFlags::new(Self::Value::Int(v.value), v.meta)))
            }
            LiteralTy::Bool(lit) => {
                BoolLiteralTy::coerce(ctx, TyWithMeta::new(lit, target.meta), value)
                    .map(|opt| opt.map(|v| ValueWithFlags::new(Self::Value::Bool(v.value), v.meta)))
            }
            LiteralTy::String(lit) => {
                StringLiteralTy::coerce(ctx, TyWithMeta::new(lit, target.meta), value).map(|opt| {
                    opt.map(|v| ValueWithFlags::new(Self::Value::String(v.value), v.meta))
                })
            }
        }
    }
}
