use std::borrow::Cow;

use super::{ParsingContext, ParsingError};
use crate::{
    baml_value::{BamlInt, BamlString},
    deserializer::{
        coercer::{TypeCoercer, match_string::match_string},
        deserialize_flags::Flag,
        types::{DeserializerMeta, ValueWithFlags},
    },
    jsonish,
    jsonish::CompletionState,
    sap_model::{
        BigintLiteralTy, BigintTy, BoolLiteralTy, BoolTy, IntLiteralTy, IntTy, LiteralTy,
        StringLiteralTy, StringTy, TyResolvedRef, TypeIdent,
    },
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for IntLiteralTy
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // A literal has no partial parse: an incomplete number may still grow.
        let jsonish::Value::Number(num, CompletionState::Complete) = value else {
            return None;
        };

        let n = num.as_i64()?;
        if n != target.0 {
            return None;
        }

        Some(ValueWithFlags::new(
            BamlInt { value: n },
            DeserializerMeta::new(TyResolvedRef::Int(IntTy)),
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        match value {
            jsonish::Value::Null => Err(ctx.error_unexpected_null(target)),
            // A literal has no partial parse.
            jsonish::Value::Object(_, CompletionState::Incomplete) => Ok(None),
            jsonish::Value::Object(obj, CompletionState::Complete) => match obj.as_slice() {
                [
                    (
                        _,
                        v @ (jsonish::Value::Number(_, _)
                        | jsonish::Value::Boolean(_)
                        | jsonish::Value::String(_, _)),
                    ),
                ] => Self::coerce(ctx, target, v).map(|ret| {
                    ret.map(|ret| ret.with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value))))
                }),
                _ => Err(ctx.error_unexpected_type(target, value)),
            },
            _ => {
                // inner coerce will handle the completion state
                match IntTy::coerce(ctx, &IntTy, value) {
                    Ok(Some(ret)) if ret.value.value == target.0 => Ok(Some(ret)),
                    Ok(Some(_ret)) => Err(ctx.error_unexpected_type(target, value)),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for BigintLiteralTy
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // Delegate to `BigintTy::try_cast` (exact, complete JSON integer numbers only)
        // and then check exact value equality.
        let inner = BigintTy::try_cast(ctx, &BigintTy, value)?;
        if inner.value.value != target.0 {
            return None;
        }
        Some(inner)
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        match value {
            jsonish::Value::Null => Err(ctx.error_unexpected_null(target)),
            // A literal has no partial parse.
            jsonish::Value::Object(_, CompletionState::Incomplete) => Ok(None),
            jsonish::Value::Object(obj, CompletionState::Complete) => match obj.as_slice() {
                [
                    (
                        _,
                        v @ (jsonish::Value::Number(_, _)
                        | jsonish::Value::Boolean(_)
                        | jsonish::Value::String(_, _)),
                    ),
                ] => Self::coerce(ctx, target, v).map(|ret| {
                    ret.map(|ret| ret.with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value))))
                }),
                _ => Err(ctx.error_unexpected_type(target, value)),
            },
            _ => {
                // inner coerce will handle the completion state
                match BigintTy::coerce(ctx, &BigintTy, value) {
                    Ok(Some(ret)) if ret.value.value == target.0 => Ok(Some(ret)),
                    Ok(Some(_ret)) => Err(ctx.error_unexpected_type(target, value)),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                }
            }
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
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // Boolean doesn't carry CompletionState, so it's always complete.
        let crate::jsonish::Value::Boolean(b) = value else {
            return None;
        };

        if *b != target.0 {
            return None;
        }

        Some(ValueWithFlags::new(
            Self::Value { value: *b },
            DeserializerMeta::new(TyResolvedRef::Bool(BoolTy)),
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        match value {
            jsonish::Value::Null => Err(ctx.error_unexpected_null(target)),
            // A literal has no partial parse.
            jsonish::Value::Object(_, CompletionState::Incomplete) => Ok(None),
            jsonish::Value::Object(obj, CompletionState::Complete) => match obj.as_slice() {
                [
                    (
                        _,
                        v @ (jsonish::Value::Number(_, _)
                        | jsonish::Value::Boolean(_)
                        | jsonish::Value::String(_, _)),
                    ),
                ] => Self::coerce(ctx, target, v).map(|ret| {
                    ret.map(|ret| ret.with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value))))
                }),
                _ => Err(ctx.error_unexpected_type(target, value)),
            },
            _ => {
                // inner coerce will handle the completion state
                match BoolTy::coerce(ctx, &BoolTy, value) {
                    Ok(Some(ret)) if ret.value.value == target.0 => Ok(Some(ret)),
                    Ok(Some(_ret)) => Err(ctx.error_unexpected_type(target, value)),
                    Ok(None) => Ok(None),
                    Err(e) => Err(e),
                }
            }
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
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // A literal has no partial parse: a prefix of the literal is not the literal.
        let jsonish::Value::String(s, CompletionState::Complete) = value else {
            return None;
        };

        if s != target.0.as_ref() {
            return None;
        }

        Some(ValueWithFlags::new(
            Self::Value {
                value: s.to_string().into(),
            },
            DeserializerMeta::new(TyResolvedRef::String(StringTy)),
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        // A literal has no partial parse: a prefix of the literal is not the literal.
        if value.completion_state() == &CompletionState::Incomplete {
            return Ok(None);
        }
        match value {
            jsonish::Value::Null => Err(ctx.error_unexpected_null(target)),
            jsonish::Value::Object(obj, _) => match obj.as_slice() {
                [
                    (
                        _,
                        v @ (jsonish::Value::Number(_, _)
                        | jsonish::Value::Boolean(_)
                        | jsonish::Value::String(_, _)),
                    ),
                ] => Self::coerce(ctx, target, v).map(|ret| {
                    ret.map(|ret| ret.with_flag(Flag::ObjectToPrimitive(Cow::Borrowed(value))))
                }),
                _ => Err(ctx.error_unexpected_type(target, value)),
            },
            _ => {
                let candidates = vec![(target.0.as_ref(), vec![&*target.0])];
                let literal_match = match_string(
                    ctx,
                    TyResolvedRef::LiteralString(target),
                    Cow::Borrowed(value),
                    &candidates,
                    true,
                )?;
                Ok(Some(
                    literal_match.map_value(|s| BamlString { value: s.into() }),
                ))
            }
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
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        match target {
            LiteralTy::Int(lit) => {
                let result = IntLiteralTy::try_cast(ctx, lit, value)?;
                Some(ValueWithFlags::new(
                    Self::Value::Int(result.value),
                    result.meta,
                ))
            }
            LiteralTy::Bigint(lit) => {
                let result = BigintLiteralTy::try_cast(ctx, lit, value)?;
                Some(ValueWithFlags::new(
                    Self::Value::Bigint(result.value),
                    result.meta,
                ))
            }
            LiteralTy::Bool(lit) => {
                let result = BoolLiteralTy::try_cast(ctx, lit, value)?;
                Some(ValueWithFlags::new(
                    Self::Value::Bool(result.value),
                    result.meta,
                ))
            }
            LiteralTy::String(lit) => {
                let result = StringLiteralTy::try_cast(ctx, lit, value)?;
                Some(ValueWithFlags::new(
                    Self::Value::String(result.value),
                    result.meta,
                ))
            }
        }
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        match target {
            LiteralTy::Int(lit) => IntLiteralTy::coerce(ctx, lit, value)
                .map(|opt| opt.map(|v| ValueWithFlags::new(Self::Value::Int(v.value), v.meta))),
            LiteralTy::Bigint(lit) => BigintLiteralTy::coerce(ctx, lit, value)
                .map(|opt| opt.map(|v| ValueWithFlags::new(Self::Value::Bigint(v.value), v.meta))),
            LiteralTy::Bool(lit) => BoolLiteralTy::coerce(ctx, lit, value)
                .map(|opt| opt.map(|v| ValueWithFlags::new(Self::Value::Bool(v.value), v.meta))),
            LiteralTy::String(lit) => StringLiteralTy::coerce(ctx, lit, value)
                .map(|opt| opt.map(|v| ValueWithFlags::new(Self::Value::String(v.value), v.meta))),
        }
    }
}
