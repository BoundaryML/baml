use crate::baml_value::BamlInt;
use crate::deserializer::deserialize_flags::DeserializerConditions;
use crate::deserializer::types::{DeserializerMeta, ValueWithFlags};
use crate::sap_model::{
    BoolLiteralTy, BoolTy, IntLiteralTy, IntTy, LiteralTy, PrimitiveTy, StringLiteralTy, StringTy,
    TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent,
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

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for IntLiteralTy {
    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, Self::Value, N>> {
        let mut result = match value {
            Some(crate::jsonish::Value::Number(number, _))
                if number.as_i64().map(|n| n == target.ty.0).unwrap_or(false) =>
            {
                Some(ValueWithFlags::new(
                    BamlInt {
                        value: number.as_i64().unwrap(),
                    },
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(
                            TyResolvedRef::Primitive(PrimitiveTy::Int(IntTy)),
                            target.meta,
                        ),
                    },
                ))
            }
            _ => None,
        };

        if let Some(v) = value {
            match v.completion_state() {
                crate::jsonish::CompletionState::Complete => {}
                crate::jsonish::CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|r| r.meta.flags.add_flag(Flag::Incomplete));
                }
            }
        }

        result
    }

    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, Self::Value, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: int literal {literal} (current: {current})",
            literal = target.ty.0,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        let value = match value {
            None | Some(jsonish::Value::Null) => {
                return Err(ctx.error_unexpected_null(target.ty));
            }
            Some(v) => v,
        };

        if let jsonish::Value::Object(obj, completion_state) = value {
            if obj.len() == 1 {
                let (_key, inner_value) = obj.iter().next().unwrap();
                match inner_value {
                    jsonish::Value::Number(_, _)
                    | jsonish::Value::Boolean(_)
                    | jsonish::Value::String(_, _) => {
                        let mut result = Self::coerce(ctx, target, Some(inner_value))?;
                        result.meta.flags.add_flag(Flag::ObjectToPrimitive(
                            jsonish::Value::Object(obj.clone(), completion_state.clone()),
                        ));
                        return Ok(result);
                    }
                    _ => {}
                }
            }
        }

        let int_target = TyWithMeta::new(&IntTy, target.meta);
        let coerced_int = IntTy::coerce(ctx, int_target, Some(value))?;

        if coerced_int.value.value == target.ty.0 {
            Ok(coerced_int)
        } else {
            Err(ctx.error_unexpected_type(&target, &value))
        }
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for BoolLiteralTy {
    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, Self::Value, N>> {
        let mut result = match value {
            Some(crate::jsonish::Value::Boolean(b)) if *b == target.ty.0 => {
                Some(ValueWithFlags::new(
                    Self::Value { value: *b },
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(
                            TyResolvedRef::Primitive(PrimitiveTy::Bool(BoolTy)),
                            target.meta,
                        ),
                    },
                ))
            }
            _ => None,
        };

        if let Some(v) = value {
            match v.completion_state() {
                crate::jsonish::CompletionState::Complete => {}
                crate::jsonish::CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|r| r.meta.flags.add_flag(Flag::Incomplete));
                }
            }
        }

        result
    }

    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, Self::Value, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: bool literal {literal} (current: {current})",
            literal = target.ty.0,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        let value = match value {
            None | Some(jsonish::Value::Null) => {
                return Err(ctx.error_unexpected_null(&target));
            }
            Some(v) => v,
        };

        if let jsonish::Value::Object(obj, completion_state) = value {
            if obj.len() == 1 {
                let (_key, inner_value) = obj.iter().next().unwrap();
                match inner_value {
                    jsonish::Value::Number(_, _)
                    | jsonish::Value::Boolean(_)
                    | jsonish::Value::String(_, _) => {
                        let mut result = Self::coerce(ctx, target, Some(inner_value))?;
                        result.meta.flags.add_flag(Flag::ObjectToPrimitive(
                            jsonish::Value::Object(obj.clone(), completion_state.clone()),
                        ));
                        return Ok(result);
                    }
                    _ => {}
                }
            }
        }

        let bool_target = TyWithMeta::new(&BoolTy, target.meta);
        let coerced_bool = BoolTy::coerce(ctx, bool_target, Some(value))?;

        if coerced_bool.value.value == target.ty.0 {
            Ok(coerced_bool)
        } else {
            Err(ctx.error_unexpected_type(&target, &value))
        }
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for StringLiteralTy<'t> {
    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, Self::Value, N>> {
        let mut result = match value {
            Some(crate::jsonish::Value::String(s, _)) if s == target.ty.0.as_ref() => {
                Some(ValueWithFlags::new(
                    Self::Value {
                        value: s.to_string(),
                    },
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(
                            TyResolvedRef::Primitive(PrimitiveTy::String(StringTy)),
                            target.meta,
                        ),
                    },
                ))
            }
            _ => None,
        };

        if let Some(v) = value {
            match v.completion_state() {
                crate::jsonish::CompletionState::Complete => {}
                crate::jsonish::CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|r| r.meta.flags.add_flag(Flag::Incomplete));
                }
            }
        }

        result
    }

    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, Self::Value, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: string literal {literal:?} (current: {current})",
            literal = target.ty.0,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        let value = match value {
            None | Some(jsonish::Value::Null) => {
                return Err(ctx.error_unexpected_null(&target));
            }
            Some(v) => v,
        };

        if let jsonish::Value::Object(obj, completion_state) = value {
            if obj.len() == 1 {
                let (_key, inner_value) = obj.iter().next().unwrap();
                match inner_value {
                    jsonish::Value::Number(_, _)
                    | jsonish::Value::Boolean(_)
                    | jsonish::Value::String(_, _) => {
                        let mut result = Self::coerce(ctx, target, Some(inner_value))?;
                        result.meta.flags.add_flag(Flag::ObjectToPrimitive(
                            jsonish::Value::Object(obj.clone(), completion_state.clone()),
                        ));
                        return Ok(result);
                    }
                    _ => {}
                }
            }
        }

        let candidates = vec![(target.ty.0.as_ref(), vec![target.ty.0.to_string()])];
        // Can't construct TyResolvedRef::Literal(&LiteralTy) without a persistent reference,
        // so use Primitive(String) which is semantically close for error messages.
        let literal_match = match_string(
            ctx,
            TyWithMeta::new(
                TyResolvedRef::Primitive(PrimitiveTy::String(StringTy)),
                target.meta,
            ),
            Some(value),
            &candidates,
            true,
        )?;

        Ok(literal_match)
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for LiteralTy<'t> {
    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, Self::Value, N>> {
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
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, Self::Value, N>, ParsingError> {
        match target.ty {
            LiteralTy::Int(lit) => {
                let result = IntLiteralTy::coerce(ctx, TyWithMeta::new(lit, target.meta), value)?;
                Ok(ValueWithFlags::new(
                    Self::Value::Int(result.value),
                    result.meta,
                ))
            }
            LiteralTy::Bool(lit) => {
                let result = BoolLiteralTy::coerce(ctx, TyWithMeta::new(lit, target.meta), value)?;
                Ok(ValueWithFlags::new(
                    Self::Value::Bool(result.value),
                    result.meta,
                ))
            }
            LiteralTy::String(lit) => {
                let result =
                    StringLiteralTy::coerce(ctx, TyWithMeta::new(lit, target.meta), value)?;
                Ok(ValueWithFlags::new(
                    Self::Value::String(result.value),
                    result.meta,
                ))
            }
        }
    }
}
