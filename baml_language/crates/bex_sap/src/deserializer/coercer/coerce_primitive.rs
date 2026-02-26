use crate::baml_value::{
    BamlBool, BamlFloat, BamlInt, BamlMedia, BamlNull, BamlPrimitive, BamlString, BamlValue,
};
use crate::deserializer::types::{DeserializerMeta, ValueWithFlags};
use crate::jsonish::CompletionState;
use crate::sap_model::{
    BoolTy, FloatTy, IntTy, Literal, MediaTy, NullTy, PrimitiveTy, StringTy, TyResolvedRef,
    TyWithMeta, TypeAnnotations, TypeIdent,
};
use anyhow::Result;
use regex::Regex;

use super::{ParsingContext, ParsingError, array_helper::coerce_array_to_singular};
use crate::deserializer::{
    coercer::TypeCoercer,
    deserialize_flags::{DeserializerConditions, Flag},
};

// Constants for constructing TyResolvedRef::Primitive in DeserializerMeta.
// Needed because the specific type impls (IntTy, FloatTy, etc.) only have &IntTy etc.,
// not PrimitiveTy, and TyResolvedRef::Primitive requires PrimitiveTy by value.
const PRIM_INT: PrimitiveTy = PrimitiveTy::Int(IntTy);
const PRIM_FLOAT: PrimitiveTy = PrimitiveTy::Float(FloatTy);
const PRIM_STRING: PrimitiveTy = PrimitiveTy::String(StringTy);
const PRIM_BOOL: PrimitiveTy = PrimitiveTy::Bool(BoolTy);
const PRIM_NULL: PrimitiveTy = PrimitiveTy::Null(NullTy);

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for PrimitiveTy {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, Self::Value, N>, ParsingError> {
        match target.ty {
            PrimitiveTy::String(ty) => {
                StringTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            PrimitiveTy::Int(ty) => IntTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Float(ty) => FloatTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Bool(ty) => BoolTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Null(ty) => NullTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Media(ty) => MediaTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
        }
    }

    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, Self::Value, N>> {
        match target.ty {
            PrimitiveTy::String(ty) => {
                StringTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            PrimitiveTy::Int(ty) => IntTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Float(ty) => {
                FloatTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            PrimitiveTy::Bool(ty) => BoolTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Null(ty) => NullTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Media(ty) => {
                MediaTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
        }
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for IntTy {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        // Parsed from JSONish
        value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, BamlInt, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: int (current: {current})",
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );
        log::trace!(
            "content: {}",
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<null>".into())
        );

        let Some(value) = value else {
            return Err(ctx.error_unexpected_null(target.ty));
        };

        let mut flags = DeserializerConditions::new();

        if matches!(value.completion_state(), CompletionState::Incomplete)
            && let Some(ref in_progress) = target.meta.in_progress
        {
            let Literal::Int(in_progress) = in_progress else {
                return Err(ctx.error_internal("`in_progress` must match the type: int"));
            };
            flags.add_flag(Flag::DefaultFromInProgress(value.clone()));
            return Ok(ValueWithFlags::new(
                BamlInt {
                    value: *in_progress,
                },
                DeserializerMeta {
                    flags,
                    ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_INT), target.meta),
                },
            ));
        }

        let result: i64 = match value {
            crate::jsonish::Value::Number(n, _) => {
                if let Some(n) = n.as_i64() {
                    n // also covers u64
                } else if let Some(n) = n.as_f64() {
                    flags.add_flag(Flag::FloatToInt(n));
                    n.round() as i64
                } else {
                    return Err(ctx.error_integer_out_of_bounds(n));
                }
            }
            crate::jsonish::Value::String(s, _) => {
                let s = s.trim();
                // Trim trailing commas
                let s = s.trim_end_matches(',');
                if let Ok(n) = s.parse::<i64>() {
                    n
                } else if let Ok(n) = s.parse::<u64>() {
                    match i64::try_from(n) {
                        Ok(n) => n,
                        Err(_) => {
                            return Err(ParsingError {
                                scope: ctx.scope.clone(),
                                reason: "Integer was out of bounds.".to_string(),
                                causes: Vec::new(),
                            });
                        }
                    }
                } else if let Ok(n) = s.parse::<f64>() {
                    flags.add_flag(Flag::FloatToInt(n));
                    n.round() as i64
                } else if let Some(frac) = float_from_maybe_fraction(s) {
                    flags.add_flag(Flag::FloatToInt(frac));
                    frac.round() as i64
                } else if let Some(frac) = float_from_comma_separated(s) {
                    flags.add_flag(Flag::FloatToInt(frac));
                    frac.round() as i64
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            crate::jsonish::Value::Array(items, _) => {
                let target_ty = target.ty;
                let target_meta = target.meta;
                let singular = coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Primitive(PRIM_INT), target_meta),
                    &items.iter().collect::<Vec<_>>(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), Some(value))
                            .map(|v| v.map_value(Into::into))
                    },
                )?;
                flags = singular.meta.flags;
                let BamlValue::Int(singular) = singular.value else {
                    unreachable!("coerce_array_to_singular should only return Int");
                };
                singular.value
            }
            _ => return Err(ctx.error_unexpected_type(&target, &value)),
        };

        if matches!(value.completion_state(), CompletionState::Incomplete) {
            flags.add_flag(Flag::Incomplete);
        }
        Ok(ValueWithFlags::new(
            BamlInt { value: result },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_INT), target.meta),
            },
        ))
    }

    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, BamlInt, N>> {
        let mut result = match value {
            Some(crate::jsonish::Value::Number(n, _)) => n.as_i64().map(|i| {
                ValueWithFlags::new(
                    BamlInt { value: i },
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_INT), target.meta),
                    },
                )
            }),
            _ => None,
        };

        // Check completion state exactly like coerce methods do
        if let Some(v) = value {
            match v.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|v| v.meta.flags.add_flag(Flag::Incomplete));
                }
            }
        }

        result
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for FloatTy {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, BamlFloat, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: float (current: {current})",
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );
        log::trace!(
            "content: {}",
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<null>".into())
        );

        let Some(value) = value else {
            return Err(ctx.error_unexpected_null(&target));
        };

        let mut flags = DeserializerConditions::new();

        if matches!(value.completion_state(), CompletionState::Incomplete)
            && let Some(ref in_progress) = target.meta.in_progress
        {
            let Literal::Float(in_progress) = in_progress else {
                return Err(ctx.error_internal("`in_progress` must match the type: float"));
            };
            flags.add_flag(Flag::DefaultFromInProgress(value.clone()));
            return Ok(ValueWithFlags::new(
                BamlFloat {
                    value: *in_progress,
                },
                DeserializerMeta {
                    flags,
                    ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_FLOAT), target.meta),
                },
            ));
        }

        let result: f64 = match value {
            crate::jsonish::Value::Number(n, _) => {
                if let Some(n) = n.as_f64() {
                    n
                } else if let Some(n) = n.as_i64() {
                    n as f64
                } else if let Some(n) = n.as_u64() {
                    n as f64
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            crate::jsonish::Value::String(s, _) => {
                let s = s.trim();
                // Trim trailing commas
                let s = s.trim_end_matches(',');
                if let Ok(n) = s.parse::<f64>() {
                    n
                } else if let Ok(n) = s.parse::<i64>() {
                    n as f64
                } else if let Ok(n) = s.parse::<u64>() {
                    n as f64
                } else if let Some(frac) = float_from_maybe_fraction(s) {
                    frac
                } else if let Some(frac) = float_from_comma_separated(s) {
                    // Add flag here to penalize strings like
                    // "1 cup unsalted butter, room temperature".
                    // If we're trying to parse this to a float it should work
                    // anyway but unions like "float | string" should still coerce
                    // this to a string.
                    flags.add_flag(Flag::StringToFloat(s.to_string()));
                    frac
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            crate::jsonish::Value::Array(items, _) => {
                let target_ty = target.ty;
                let target_meta = target.meta;
                let singular = coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Primitive(PRIM_FLOAT), target_meta),
                    &items.iter().collect::<Vec<_>>(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), Some(value))
                            .map(|v| v.map_value(Into::into))
                    },
                )?;
                flags = singular.meta.flags;
                let BamlValue::Float(singular) = singular.value else {
                    unreachable!("coerce_array_to_singular should only return Float");
                };
                singular.value
            }
            _ => return Err(ctx.error_unexpected_type(&target, &value)),
        };

        if matches!(value.completion_state(), CompletionState::Incomplete) {
            flags.add_flag(Flag::Incomplete);
        }
        Ok(ValueWithFlags::new(
            BamlFloat { value: result },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_FLOAT), target.meta),
            },
        ))
    }

    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, BamlFloat, N>> {
        let mut result = match value {
            Some(crate::jsonish::Value::Number(n, _)) => n.as_f64().map(|f| {
                ValueWithFlags::new(
                    BamlFloat { value: f },
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_FLOAT), target.meta),
                    },
                )
            }),
            _ => None,
        };

        if let Some(v) = value {
            match v.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|v| v.meta.flags.add_flag(Flag::Incomplete));
                }
            }
        }

        result
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for BoolTy {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, BamlBool, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: bool (current: {current})",
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );
        log::trace!(
            "content: {}",
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<null>".into())
        );

        let Some(value) = value else {
            return Err(ctx.error_unexpected_null(&target));
        };

        let mut flags = DeserializerConditions::new();

        if matches!(value.completion_state(), CompletionState::Incomplete)
            && let Some(ref in_progress) = target.meta.in_progress
        {
            let Literal::Bool(in_progress) = in_progress else {
                return Err(ctx.error_internal("`in_progress` must match the type: bool"));
            };
            flags.add_flag(Flag::DefaultFromInProgress(value.clone()));
            return Ok(ValueWithFlags::new(
                BamlBool {
                    value: *in_progress,
                },
                DeserializerMeta {
                    flags,
                    ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_BOOL), target.meta),
                },
            ));
        }

        let result: bool = match value {
            crate::jsonish::Value::Boolean(b) => *b,
            crate::jsonish::Value::String(s, _) => match s.to_lowercase().as_str() {
                "true" => {
                    flags.add_flag(Flag::StringToBool(s.clone()));
                    true
                }
                "false" => {
                    flags.add_flag(Flag::StringToBool(s.clone()));
                    false
                }
                _ => {
                    match super::match_string::match_string(
                        ctx,
                        TyWithMeta::new(TyResolvedRef::Primitive(PRIM_BOOL), target.meta),
                        Some(value),
                        &[
                            ("true", vec!["true".into(), "True".into(), "TRUE".into()]),
                            (
                                "false",
                                vec!["false".into(), "False".into(), "FALSE".into()],
                            ),
                        ],
                        true,
                    ) {
                        Ok(val) => match val.value.value.as_str() {
                            "true" => {
                                flags.add_flag(Flag::StringToBool(val.value.value.clone()));
                                true
                            }
                            "false" => {
                                flags.add_flag(Flag::StringToBool(val.value.value.clone()));
                                false
                            }
                            _ => return Err(ctx.error_unexpected_type(&target, &value)),
                        },
                        Err(_) => return Err(ctx.error_unexpected_type(&target, &value)),
                    }
                }
            },
            crate::jsonish::Value::Array(items, _) => {
                let target_ty = target.ty;
                let target_meta = target.meta;
                let singular = coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Primitive(PRIM_BOOL), target_meta),
                    &items.iter().collect::<Vec<_>>(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), Some(value))
                            .map(|v| v.map_value(Into::into))
                    },
                )?;
                flags = singular.meta.flags;
                let BamlValue::Bool(singular) = singular.value else {
                    unreachable!("coerce_array_to_singular should only return Bool");
                };
                singular.value
            }
            _ => return Err(ctx.error_unexpected_type(&target, &value)),
        };

        if matches!(value.completion_state(), CompletionState::Incomplete) {
            flags.add_flag(Flag::Incomplete);
        }
        Ok(ValueWithFlags::new(
            BamlBool { value: result },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_BOOL), target.meta),
            },
        ))
    }

    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, BamlBool, N>> {
        let mut result = match value {
            Some(crate::jsonish::Value::Boolean(b)) => Some(ValueWithFlags::new(
                BamlBool { value: *b },
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_BOOL), target.meta),
                },
            )),
            _ => None,
        };

        if let Some(v) = value {
            match v.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|v| v.meta.flags.add_flag(Flag::Incomplete));
                }
            }
        }

        result
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for NullTy {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, BamlNull, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: null (current: {current})",
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );
        log::trace!(
            "content: {}",
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<null>".into())
        );

        let mut flags = DeserializerConditions::new();

        if let Some(value) = value {
            if matches!(value.completion_state(), CompletionState::Incomplete)
                && let Some(ref in_progress) = target.meta.in_progress
            {
                let Literal::Null = in_progress else {
                    return Err(ctx.error_internal("`in_progress` must match the type: null"));
                };
                flags.add_flag(Flag::DefaultFromInProgress(value.clone()));
                return Ok(ValueWithFlags::new(
                    BamlNull,
                    DeserializerMeta {
                        flags,
                        ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_NULL), target.meta),
                    },
                ));
            }
        }

        match value {
            Some(crate::jsonish::Value::Null) | None => {}
            Some(v) => flags.add_flag(Flag::DefaultButHadValue(v.clone())),
        }
        Ok(ValueWithFlags::new(
            BamlNull,
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_NULL), target.meta),
            },
        ))
    }

    fn try_cast(
        _ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, BamlNull, N>> {
        match value {
            Some(crate::jsonish::Value::Null) | None => Some(ValueWithFlags::new(
                BamlNull,
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_NULL), target.meta),
                },
            )),
            _ => None,
        }
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for StringTy {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, BamlString, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: string (current: {current})",
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );
        log::trace!(
            "content: {}",
            value
                .map(|v| v.to_string())
                .unwrap_or_else(|| "<null>".into())
        );

        let Some(value) = value else {
            return Err(ctx.error_unexpected_null(&target));
        };

        let mut flags = DeserializerConditions::new();

        if matches!(value.completion_state(), CompletionState::Incomplete)
            && let Some(ref in_progress) = target.meta.in_progress
        {
            let Literal::String(in_progress) = in_progress else {
                return Err(ctx.error_internal("`in_progress` must match the type: string"));
            };
            flags.add_flag(Flag::DefaultFromInProgress(value.clone()));
            return Ok(ValueWithFlags::new(
                BamlString {
                    value: in_progress.to_string(),
                },
                DeserializerMeta {
                    flags,
                    ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_STRING), target.meta),
                },
            ));
        }

        let result: String = match value {
            crate::jsonish::Value::String(s, completion_state) => {
                if completion_state == &CompletionState::Incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                s.to_string()
            }
            crate::jsonish::Value::Null => return Err(ctx.error_unexpected_null(&target)),
            // Handle AnyOf explicitly to extract the string content.
            // If one of the variants is a String, prefer that over the raw input.
            // Otherwise, use the original raw string.
            crate::jsonish::Value::AnyOf(choices, original_string) => {
                // Prefer a String choice only when it looks like it comes from the original raw input.
                // In streaming/partial cases the String choice is often a prefix of the raw input.
                // Some parse paths can also produce derived String choices (e.g. extracted from an object);
                // in those cases fall back to the raw string to preserve the user's content.
                let string_value = choices
                    .iter()
                    .filter_map(|choice| match choice {
                        crate::jsonish::Value::String(s, completion_state)
                            if original_string.starts_with(s) || s == original_string =>
                        {
                            Some((s.clone(), completion_state.clone()))
                        }
                        _ => None,
                    })
                    .max_by_key(|(s, _)| s.len());

                let (string_val, completion_state) = string_value
                    .unwrap_or_else(|| (original_string.clone(), value.completion_state().clone()));

                if completion_state == CompletionState::Incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                string_val
            }
            v => {
                flags.add_flag(Flag::JsonToString(v.clone()));
                v.to_string()
            }
        };
        Ok(ValueWithFlags::new(
            BamlString { value: result },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_STRING), target.meta),
            },
        ))
    }

    fn try_cast(
        _ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, BamlString, N>> {
        let mut result = match value {
            Some(crate::jsonish::Value::String(s, _)) => Some(ValueWithFlags::new(
                BamlString {
                    value: s.to_string(),
                },
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyWithMeta::new(TyResolvedRef::Primitive(PRIM_STRING), target.meta),
                },
            )),
            _ => None,
        };

        if let Some(v) = value {
            match v.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    result
                        .iter_mut()
                        .for_each(|v| v.meta.flags.add_flag(Flag::Incomplete));
                }
            }
        }

        result
    }
}

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for MediaTy {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        _value: Option<&crate::jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, BamlMedia, N>, ParsingError> {
        match target.ty {
            MediaTy::Image => Err(ctx.error_image_not_supported()),
            MediaTy::Audio => Err(ctx.error_audio_not_supported()),
            MediaTy::Pdf => Err(ctx.error_pdf_not_supported()),
            MediaTy::Video => Err(ctx.error_video_not_supported()),
        }
    }

    fn try_cast(
        _ctx: &ParsingContext<'t, N>,
        _target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        _value: Option<&crate::jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, BamlMedia, N>> {
        None
    }
}

fn float_from_maybe_fraction(value: &str) -> Option<f64> {
    if let Some((numerator, denominator)) = value.split_once('/') {
        match (
            numerator.trim().parse::<f64>(),
            denominator.trim().parse::<f64>(),
        ) {
            (Ok(num), Ok(denom)) if denom != 0.0 => Some(num / denom),
            _ => None,
        }
    } else {
        None
    }
}

fn float_from_comma_separated(value: &str) -> Option<f64> {
    let re = Regex::new(r"([-+]?)\$?(?:\d+(?:,\d+)*(?:\.\d+)?|\d+\.\d+|\d+|\.\d+)(?:e[-+]?\d+)?")
        .unwrap();
    let matches: Vec<_> = re.find_iter(value).collect();

    if matches.len() != 1 {
        return None;
    }

    let number_str = matches[0].as_str();
    let without_commas = number_str.replace(",", "");
    // Remove all Unicode currency symbols
    let re_currency = Regex::new(r"\p{Sc}").unwrap();
    let without_currency = re_currency.replace_all(&without_commas, "");

    without_currency.parse::<f64>().ok()
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     #[test]
//     fn test_float_from_comma_separated() {
//         // Note we don't handle european numbers correctly.
//         let test_cases = vec![
//             // European Formats
//             // Valid German format (comma as decimal separator)
//             ("3,14", Some(314.0)),
//             ("1.234,56", None),
//             ("1.234.567,89", None),
//             ("€1.234,56", None),
//             ("-€1.234,56", None),
//             ("€1.234", Some(1.234)), // TODO - technically incorrect
//             ("1.234€", Some(1.234)), // TODO - technically incorrect
//             // Valid currencies with European formatting
//             ("€1.234,56", None),
//             ("€1,234.56", Some(1234.56)), // Incorrect format for Euro
//             // US Formats
//             // Valid US format (comma as thousands separator)
//             ("3,000", Some(3000.0)),
//             ("3,100.00", Some(3100.00)),
//             ("1,234.56", Some(1234.56)),
//             ("1,234,567.89", Some(1234567.89)),
//             ("$1,234.56", Some(1234.56)),
//             ("-$1,234.56", Some(-1234.56)),
//             ("$1,234", Some(1234.0)),
//             ("1,234$", Some(1234.0)),
//             ("$1,234.56", Some(1234.56)),
//             ("+$1,234.56", Some(1234.56)),
//             ("-$1,234.56", Some(-1234.56)),
//             ("$9,999,999,999", Some(9999999999.0)),
//             ("$1.23.456", None),
//             ("$1.234.567.890", None),
//             // Valid currencies with US formatting
//             ("$1,234", Some(1234.0)),
//             ("$314", Some(314.0)),
//             // Indian Formats
//             // Assuming Indian numbering system (not present in original tests, added for categorization)
//             ("$1,23,456", Some(123456.0)),
//             // Additional Indian format test cases can be added here

//             // Percentages and Strings with Numbers
//             // Percentages
//             ("50%", Some(50.0)),
//             ("3.15%", Some(3.15)),
//             (".009%", Some(0.009)),
//             ("1.234,56%", None),
//             ("$1,234.56%", Some(1234.56)),
//             // Strings containing numbers
//             ("The answer is 10,000", Some(10000.0)),
//             ("The total is €1.234,56 today", None),
//             ("You owe $3,000 for the service", Some(3000.0)),
//             ("Save up to 20% on your purchase", Some(20.0)),
//             ("Revenue grew by 1,234.56 this quarter", Some(1234.56)),
//             ("Profit is -€1.234,56 in the last month", None),
//             // Sentences with Multiple Numbers
//             ("The answer is 10,000 and $3,000", None),
//             ("We earned €1.234,56 and $2,345.67 this year", None),
//             ("Increase of 5% and a profit of $1,000", None),
//             ("Loss of -€500 and a gain of 1,200.50", None),
//             ("Targets: 2,000 units and €3.000,75 revenue", None),
//             // trailing periods and commas
//             ("12,111,123.", Some(12111123.0)),
//             ("12,111,123,", Some(12111123.0)),
//         ];

//         for (input, expected) in test_cases {
//             let result = float_from_comma_separated(input);
//             assert_eq!(
//                 result, expected,
//                 "Failed to parse '{input}'. Expected {expected:?}, got {result:?}"
//             );
//         }
//     }

//     #[test]
//     fn test_coerce_anyof_to_string() {
//         use crate::{
//             helpers::{load_test_ir, render_output_format},
//             jsonish::Value,
//             sap_model::AnnotatedTy,
//         };

//         // Create an AnyOf value similar to what the parser creates
//         let anyof_value = Value::AnyOf(
//             vec![
//                 Value::String("[json\n".to_string(), CompletionState::Incomplete),
//                 Value::Object(vec![], CompletionState::Incomplete),
//             ],
//             "[json\nAnyOf[{,AnyOf[{,{},],]".to_string(), // This is the raw string
//         );

//         let ir = load_test_ir("");
//         let target = AnnotatedTy::Primitive(PrimitiveTy::String, Default::default());
//         let output_format = render_output_format(
//             &ir,
//             &target,
//             &Default::default(),
//             crate::StreamingMode::Streaming,
//         )
//         .unwrap();
//         let ctx = ParsingContext::new(&output_format, crate::StreamingMode::Streaming);

//         let annotations = Default::default();
//         let result = StringTy::coerce(
//             &ctx,
//             TyWithMeta::new(&StringTy, &annotations),
//             Some(&anyof_value),
//         );

//         // The bug would cause this to return "AnyOf[..."
//         // The fix should prefer the String variant from the choices if available
//         assert!(result.is_ok());
//         let baml_value = result.unwrap();
//         // Should NOT start with "AnyOf[" - that's the bug!
//         assert!(
//             !baml_value.value.value.starts_with("AnyOf["),
//             "Got parsing artifact in string: {}",
//             baml_value.value.value
//         );
//         // Should be the String variant from the choices, not the Display repr
//         assert_eq!(baml_value.value.value, "[json\n");
//     }

//     #[test]
//     fn test_coerce_anyof_to_string_no_string_variant() {
//         use crate::{
//             helpers::{load_test_ir, render_output_format},
//             jsonish::Value,
//             sap_model::AnnotatedTy,
//         };

//         // Create an AnyOf value with NO string variant - should fall back to raw string
//         let anyof_value = Value::AnyOf(
//             vec![
//                 Value::Object(vec![], CompletionState::Incomplete),
//                 Value::Array(vec![], CompletionState::Incomplete),
//             ],
//             "some raw input".to_string(),
//         );

//         let ir = load_test_ir("");
//         let target = AnnotatedTy::Primitive(PrimitiveTy::String, Default::default());
//         let output_format = render_output_format(
//             &ir,
//             &target,
//             &Default::default(),
//             crate::StreamingMode::Streaming,
//         )
//         .unwrap();
//         let ctx = ParsingContext::new(&output_format, crate::StreamingMode::Streaming);

//         let annotations = Default::default();
//         let result = StringTy::coerce(
//             &ctx,
//             TyWithMeta::new(&StringTy, &annotations),
//             Some(&anyof_value),
//         );

//         assert!(result.is_ok());
//         let baml_value = result.unwrap();
//         // Should fall back to the raw input string
//         assert_eq!(baml_value.value.value, "some raw input");
//     }
// }
