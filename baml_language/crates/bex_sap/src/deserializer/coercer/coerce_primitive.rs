use std::{borrow::Cow, sync::LazyLock};

use num_bigint::BigInt;
use regex::Regex;

use super::{ParsingContext, ParsingError, array_helper::coerce_array_to_singular};
use crate::{
    baml_value::{
        BamlBigint, BamlBool, BamlFloat, BamlInt, BamlMedia, BamlNull, BamlString, BamlValue,
    },
    deserializer::{
        coercer::TypeCoercer,
        deserialize_flags::{DeserializerConditions, Flag},
        types::{DeserializerMeta, ValueWithFlags},
    },
    jsonish::{self, CompletionState},
    sap_model::{
        AttrLiteral, BigintTy, BoolTy, FloatTy, FromLiteral as _, IntTy, MediaTy, NullTy,
        PrimitiveTy, StringTy, TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent,
    },
};

/// Parse a decimal byte slice into a `BigInt`, rejecting inputs that would
/// exceed the workspace bigint cap ([`baml_type::MAX_BIGINT_DECIMAL_DIGITS`]).
///
/// The digit-count check is a cheap pre-flight reject (so a malicious LLM
/// payload doesn't reach `BigInt::parse_bytes` at all); the exact `bi.bits()`
/// check after parsing catches borderline cases. Mirrors the VM
/// (`vm.rs:try_alloc_bigint`) and FFI (`bridge_ctypes/src/value_decode.rs`)
/// guards so a host- or LLM-supplied value can't drive an unbounded
/// allocation through SAP deserialization.
fn parse_bigint_decimal_bounded(bytes: &[u8]) -> Option<BigInt> {
    if bytes.len() > baml_type::MAX_BIGINT_DECIMAL_DIGITS {
        return None;
    }
    let bi = BigInt::parse_bytes(bytes, 10)?;
    if bi.bits() > baml_type::MAX_BIGINT_BITS {
        return None;
    }
    Some(bi)
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for PrimitiveTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        match target.ty {
            PrimitiveTy::String(ty) => {
                StringTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map(|v| v.map_value(Into::into)))
            }
            PrimitiveTy::Int(ty) => IntTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map(|v| v.map_value(Into::into))),
            PrimitiveTy::Bigint(ty) => {
                BigintTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map(|v| v.map_value(Into::into)))
            }
            PrimitiveTy::Float(ty) => FloatTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map(|v| v.map_value(Into::into))),
            PrimitiveTy::Bool(ty) => BoolTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map(|v| v.map_value(Into::into))),
            PrimitiveTy::Null(ty) => NullTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map(|v| v.map_value(Into::into))),
            PrimitiveTy::Media(ty) => MediaTy::coerce(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map(|v| v.map_value(Into::into))),
        }
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        match target.ty {
            PrimitiveTy::String(ty) => {
                StringTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            PrimitiveTy::Int(ty) => IntTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            PrimitiveTy::Bigint(ty) => {
                BigintTy::try_cast(ctx, TyWithMeta::new(ty, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
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

#[allow(clippy::cast_possible_truncation)]
impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for IntTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        // Parsed from JSONish
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlInt, N>>, ParsingError> {
        let mut flags = DeserializerConditions::new();

        let result = match (value, target.meta.in_progress.as_ref()) {
            (jsonish::Value::Number(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Number(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::Number(n, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                if let Some(n) = n.as_i64() {
                    BamlInt { value: n } // also covers u64
                } else if n.as_u64().is_some() {
                    return Err(ctx.error_integer_out_of_bounds(n));
                } else if let Some(f) = n.as_f64() {
                    let rounded = f.round();
                    #[allow(clippy::cast_precision_loss)]
                    if rounded.is_nan() || rounded > i64::MAX as f64 || rounded < i64::MIN as f64 {
                        return Err(ctx.error_integer_out_of_bounds(n));
                    }
                    flags.add_flag(Flag::FloatToInt(f));
                    BamlInt {
                        value: rounded as i64,
                    }
                } else {
                    return Err(ctx.error_integer_out_of_bounds(n));
                }
            }
            (jsonish::Value::String(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::String(s, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                flags.add_flag(Flag::StringToInt(s.clone()));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::String(s, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let s = s.trim();
                // Trim trailing commas
                let s = s.trim_end_matches(',');
                if let Ok(n) = s.parse::<i64>() {
                    BamlInt { value: n }
                } else if let Ok(n) = s.parse::<u64>() {
                    let Ok(n) = i64::try_from(n) else {
                        return Err(ctx.error_integer_out_of_bounds(&serde_json::Number::from(n)));
                    };
                    BamlInt { value: n }
                } else if let Some(n) = s
                    .parse::<f64>()
                    .ok()
                    .or_else(|| float_from_maybe_fraction(s))
                    .or_else(|| float_from_comma_separated(s))
                {
                    if !n.is_finite() {
                        return Err(ctx.error_unexpected_type(&target, &value));
                    }
                    let rounded = n.round();
                    #[allow(clippy::cast_precision_loss)]
                    if rounded < (i64::MIN as f64) || (i64::MAX as f64) < rounded {
                        return Err(ctx.error_integer_out_of_bounds(
                            &serde_json::Number::from_f64(rounded).unwrap_or_else(|| {
                                unreachable!(
                                    "serde_json::Number::from_f64 only fails on non-finite floats"
                                )
                            }),
                        ));
                    }
                    flags.add_flag(Flag::FloatToInt(n));
                    BamlInt {
                        value: rounded as i64,
                    }
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::Array(items, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let target_ty = target.ty;
                let target_meta = target.meta;
                let Some(singular) = coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Int(IntTy), target_meta),
                    items.iter(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), value)
                            .map(|v| v.map(|v| v.map_value(Into::into)))
                    },
                )?
                else {
                    return Ok(None);
                };
                flags.flags.extend_from_slice(&singular.meta.flags.flags);
                let BamlValue::Int(singular) = singular.value else {
                    unreachable!("coerce_array_to_singular should only return Int");
                };
                singular
            }
            _ => return Err(ctx.error_unexpected_type(&target, &value)),
        };
        let result = ValueWithFlags::new(
            result,
            DeserializerMeta {
                flags,
                ty: target.map_ty(|_| TyResolvedRef::Int(IntTy)),
            },
        );
        Ok(Some(result))
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlInt, N>> {
        let jsonish::Value::Number(num, completion_state) = value else {
            return None;
        };

        let flags = match (completion_state, target.meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return None,
            (CompletionState::Incomplete, Some(lit)) => {
                return target
                    .ty
                    .from_literal(lit, ctx)
                    .map(|ret| {
                        ValueWithFlags::new(
                            ret,
                            DeserializerMeta {
                                flags: DeserializerConditions::new()
                                    .with_flag(Flag::DefaultButHadValue(Cow::Borrowed(value))),
                                ty: target.map_ty(|_| TyResolvedRef::Int(IntTy)),
                            },
                        )
                    })
                    .ok();
            }
            (CompletionState::Incomplete, None) => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            (CompletionState::Complete, _) => DeserializerConditions::new(),
        };

        Some(ValueWithFlags::new(
            BamlInt {
                value: num.as_i64()?,
            },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Int(IntTy), target.meta),
            },
        ))
    }
}

/// Parses a `serde_json::Number` that exceeded `i64`/`u64` range as a `BigInt`.
///
/// `serde_json` keeps the original digit sequence in the `Number`'s `Display`
/// output, so for arbitrary-precision integer literals the string form is the
/// canonical source — `as_i64`/`as_u64` only succeed for in-range values.
fn parse_bigint_from_number_text(n: &serde_json::Number) -> Option<BigInt> {
    let s = n.to_string();
    // Reject anything that looks like a non-integer (decimal point, exponent).
    // The float-fallback path below handles those cases explicitly.
    if s.contains('.') || s.contains('e') || s.contains('E') {
        return None;
    }
    parse_bigint_decimal_bounded(s.as_bytes())
}

/// Converts a finite `f64` to a `BigInt` via "round half away from zero".
///
/// Returns `None` for NaN or infinity (callers should reject those before
/// invoking this).
///
/// For typical doubles like `42.0`, this rounds to `42` and produces
/// `BigInt::from(42)`. For huge floats beyond `i128` range, we go through the
/// decimal-text representation so we don't lose precision near the upper bound
/// of the float-representable integers.
fn bigint_from_finite_f64(f: f64) -> Option<BigInt> {
    if !f.is_finite() {
        return None;
    }
    let rounded = f.round();
    // Fast path: in i128 range, the cast is exact (and avoids the formatting hit).
    // `i128::MAX as f64` rounds to a power of two slightly above `i128::MAX`,
    // so the range check alone can admit values that truncate during cast.
    // Round-trip the candidate back through `f64` to detect that case.
    #[allow(clippy::cast_precision_loss)]
    if (i128::MIN as f64) <= rounded && rounded <= (i128::MAX as f64) {
        #[allow(clippy::cast_possible_truncation)]
        let candidate = rounded as i128;
        // Exact-equality is intentional — anything else admits the truncation.
        #[expect(clippy::float_cmp)]
        let lossless = (candidate as f64) == rounded;
        if lossless {
            return Some(BigInt::from(candidate));
        }
    }
    // Out-of-i128-range (or lossy near the boundary): format with no fractional
    // digits and parse via the bounded helper. (An `f64` past `i128` range is
    // `> 1.7e38`, well under the bigint cap, so this never actually overflows
    // in practice — using the bounded parser keeps the SAP path uniform.)
    let s = format!("{rounded:.0}");
    parse_bigint_decimal_bounded(s.as_bytes())
}

#[allow(clippy::cast_precision_loss)]
impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for BigintTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlBigint, N>>, ParsingError> {
        let mut flags = DeserializerConditions::new();

        let result = match (value, target.meta.in_progress.as_ref()) {
            (jsonish::Value::Number(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Number(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::Number(n, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                if let Some(i) = n.as_i64() {
                    BamlBigint {
                        value: BigInt::from(i),
                    }
                } else if let Some(u) = n.as_u64() {
                    BamlBigint {
                        value: BigInt::from(u),
                    }
                } else if let Some(parsed) = parse_bigint_from_number_text(n) {
                    // Arbitrary-precision integer that exceeded i64/u64.
                    BamlBigint { value: parsed }
                } else if let Some(f) = n.as_f64() {
                    if !f.is_finite() {
                        return Err(ctx.error_unexpected_type(&target, &value));
                    }
                    let Some(bi) = bigint_from_finite_f64(f) else {
                        return Err(ctx.error_unexpected_type(&target, &value));
                    };
                    flags.add_flag(Flag::FloatToBigint(f));
                    BamlBigint { value: bi }
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            (jsonish::Value::String(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::String(s, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                flags.add_flag(Flag::StringToBigint(s.clone()));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::String(s, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let trimmed = s.trim();
                // Trim trailing commas
                let trimmed = trimmed.trim_end_matches(',');
                if let Some(bi) = parse_bigint_decimal_bounded(trimmed.as_bytes()) {
                    flags.add_flag(Flag::StringToBigint(s.clone()));
                    BamlBigint { value: bi }
                } else if let Some(n) = trimmed
                    .parse::<f64>()
                    .ok()
                    .or_else(|| float_from_maybe_fraction(trimmed))
                    .or_else(|| float_from_comma_separated(trimmed))
                {
                    if !n.is_finite() {
                        return Err(ctx.error_unexpected_type(&target, &value));
                    }
                    let Some(bi) = bigint_from_finite_f64(n) else {
                        return Err(ctx.error_unexpected_type(&target, &value));
                    };
                    flags.add_flag(Flag::StringToBigint(s.clone()));
                    flags.add_flag(Flag::FloatToBigint(n));
                    BamlBigint { value: bi }
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::Array(items, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let target_ty = target.ty;
                let target_meta = target.meta;
                let Some(singular) = coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Bigint(BigintTy), target_meta),
                    items.iter(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), value)
                            .map(|v| v.map(|v| v.map_value(Into::into)))
                    },
                )?
                else {
                    return Ok(None);
                };
                flags.flags.extend_from_slice(&singular.meta.flags.flags);
                let BamlValue::Bigint(singular) = singular.value else {
                    unreachable!("coerce_array_to_singular should only return Bigint");
                };
                singular
            }
            _ => return Err(ctx.error_unexpected_type(&target, &value)),
        };
        let result = ValueWithFlags::new(
            result,
            DeserializerMeta {
                flags,
                ty: target.map_ty(|_| TyResolvedRef::Bigint(BigintTy)),
            },
        );
        Ok(Some(result))
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlBigint, N>> {
        let jsonish::Value::Number(num, completion_state) = value else {
            return None;
        };

        let flags = match (completion_state, target.meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return None,
            (CompletionState::Incomplete, Some(lit)) => {
                return target
                    .ty
                    .from_literal(lit, ctx)
                    .map(|ret| {
                        ValueWithFlags::new(
                            ret,
                            DeserializerMeta {
                                flags: DeserializerConditions::new()
                                    .with_flag(Flag::DefaultButHadValue(Cow::Borrowed(value))),
                                ty: target.map_ty(|_| TyResolvedRef::Bigint(BigintTy)),
                            },
                        )
                    })
                    .ok();
            }
            (CompletionState::Incomplete, None) => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            (CompletionState::Complete, _) => DeserializerConditions::new(),
        };

        // Only accept exact JSON integer numbers — no float fallback, no string parsing.
        let bi = if let Some(i) = num.as_i64() {
            BigInt::from(i)
        } else if let Some(u) = num.as_u64() {
            BigInt::from(u)
        } else {
            // Try arbitrary-precision parse from the raw digits.
            parse_bigint_from_number_text(num)?
        };

        Some(ValueWithFlags::new(
            BamlBigint { value: bi },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Bigint(BigintTy), target.meta),
            },
        ))
    }
}

#[allow(clippy::cast_precision_loss)]
impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for FloatTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlFloat, N>>, ParsingError> {
        let mut flags = DeserializerConditions::new();

        let result = match (value, target.meta.in_progress.as_ref()) {
            (jsonish::Value::Number(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Number(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::Number(n, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                if let Some(n) = n.as_f64() {
                    BamlFloat { value: n }
                } else if let Some(n) = n.as_i64() {
                    BamlFloat { value: n as f64 }
                } else if let Some(n) = n.as_u64() {
                    BamlFloat { value: n as f64 }
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            (jsonish::Value::String(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::String(s, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                flags.add_flag(Flag::StringToFloat(s.clone()));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::String(s, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let s = s.trim();
                // Trim trailing commas
                let s = s.trim_end_matches(',');
                if let Ok(n) = s.parse::<f64>() {
                    BamlFloat { value: n }
                } else if let Ok(n) = s.parse::<i64>() {
                    BamlFloat { value: n as f64 }
                } else if let Ok(n) = s.parse::<u64>() {
                    BamlFloat { value: n as f64 }
                } else if let Some(frac) = float_from_maybe_fraction(s) {
                    BamlFloat { value: frac }
                } else if let Some(frac) = float_from_comma_separated(s) {
                    // Add flag here to penalize strings like
                    // "1 cup unsalted butter, room temperature".
                    // If we're trying to parse this to a float it should work
                    // anyway but unions like "float | string" should still coerce
                    // this to a string.
                    flags.add_flag(Flag::StringToFloat(s.to_string().into()));
                    BamlFloat { value: frac }
                } else {
                    return Err(ctx.error_unexpected_type(&target, &value));
                }
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::Array(items, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let target_ty = target.ty;
                let target_meta = target.meta;
                let Some(singular) = coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Float(FloatTy), target_meta),
                    items.iter(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), value)
                            .map(|v| v.map(|v| v.map_value(Into::into)))
                    },
                )?
                else {
                    return Ok(None);
                };
                flags.flags.extend_from_slice(&singular.meta.flags.flags);
                let BamlValue::Float(singular) = singular.value else {
                    unreachable!("coerce_array_to_singular should only return Float");
                };
                singular
            }
            _ => return Err(ctx.error_unexpected_type(&target, &value)),
        };
        let result = ValueWithFlags::new(
            result,
            DeserializerMeta {
                flags,
                ty: target.map_ty(|_| TyResolvedRef::Float(FloatTy)),
            },
        );
        Ok(Some(result))
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlFloat, N>> {
        let jsonish::Value::Number(num, completion_state) = value else {
            return None;
        };

        let flags = match (completion_state, target.meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return None,
            (CompletionState::Incomplete, Some(lit)) => {
                return target
                    .ty
                    .from_literal(lit, ctx)
                    .map(|ret| {
                        ValueWithFlags::new(
                            ret,
                            DeserializerMeta {
                                flags: DeserializerConditions::new()
                                    .with_flag(Flag::DefaultButHadValue(Cow::Borrowed(value))),
                                ty: target.map_ty(|_| TyResolvedRef::Float(FloatTy)),
                            },
                        )
                    })
                    .ok();
            }
            (CompletionState::Incomplete, None) => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            (CompletionState::Complete, _) => DeserializerConditions::new(),
        };

        Some(ValueWithFlags::new(
            BamlFloat {
                value: num.as_f64()?,
            },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Float(FloatTy), target.meta),
            },
        ))
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for BoolTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlBool, N>>, ParsingError> {
        let mut flags = DeserializerConditions::new();
        let result = match (value, target.meta.in_progress.as_ref()) {
            (crate::jsonish::Value::Boolean(b), _) => BamlBool { value: *b },
            (jsonish::Value::String(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::String(s, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                flags.add_flag(Flag::StringToBool(s.clone()));
                target.ty.from_literal(lit, ctx)?
            }
            (crate::jsonish::Value::String(s, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                match s.to_lowercase().as_str() {
                    "true" => {
                        flags.add_flag(Flag::StringToBool(s.clone()));
                        BamlBool { value: true }
                    }
                    "false" => {
                        flags.add_flag(Flag::StringToBool(s.clone()));
                        BamlBool { value: false }
                    }
                    _ => match super::match_string::match_string(
                        ctx,
                        TyWithMeta::new(TyResolvedRef::Bool(BoolTy), target.meta),
                        Cow::Borrowed(value),
                        &[
                            ("true", vec!["true", "True", "TRUE"]),
                            ("false", vec!["false", "False", "FALSE"]),
                        ],
                        true,
                    ) {
                        Ok(val) => match val.value {
                            "true" => {
                                flags.add_flag(Flag::StringToBool(Cow::Borrowed(val.value)));
                                BamlBool { value: true }
                            }
                            "false" => {
                                flags.add_flag(Flag::StringToBool(Cow::Borrowed(val.value)));
                                BamlBool { value: false }
                            }
                            _ => return Err(ctx.error_unexpected_type(&target, &value)),
                        },
                        Err(_) => return Err(ctx.error_unexpected_type(&target, &value)),
                    },
                }
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Array(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (crate::jsonish::Value::Array(items, c), _) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let target_ty = target.ty;
                let target_meta = target.meta;
                let Some(singular) = coerce_array_to_singular(
                    ctx,
                    TyWithMeta::new(TyResolvedRef::Bool(BoolTy), target_meta),
                    items.iter(),
                    &|value| {
                        Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), value)
                            .map(|v| v.map(|v| v.map_value(Into::into)))
                    },
                )?
                else {
                    return Ok(None);
                };
                flags.flags.extend_from_slice(&singular.meta.flags.flags);
                let BamlValue::Bool(singular) = singular.value else {
                    unreachable!("coerce_array_to_singular should only return Bool");
                };
                singular
            }
            _ => return Err(ctx.error_unexpected_type(&target, &value)),
        };
        let value = ValueWithFlags::new(
            result,
            DeserializerMeta {
                flags,
                ty: target.map_ty(|_| TyResolvedRef::Bool(BoolTy)),
            },
        );
        Ok(Some(value))
    }

    fn try_cast(
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlBool, N>> {
        // Boolean doesn't carry CompletionState, so it's always complete — no in_progress handling needed.
        let crate::jsonish::Value::Boolean(b) = value else {
            return None;
        };

        Some(ValueWithFlags::new(
            BamlBool { value: *b },
            DeserializerMeta {
                flags: DeserializerConditions::new(),
                ty: TyWithMeta::new(TyResolvedRef::Bool(BoolTy), target.meta),
            },
        ))
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for NullTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlNull, N>>, ParsingError> {
        if target.meta.parse_without_null {
            return Err(ctx.error_unexpected_null(&target));
        }
        let mut flags = DeserializerConditions::new();

        // Handle in_progress for all incomplete values
        match (value.completion_state(), target.meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return Ok(None),
            (CompletionState::Incomplete, Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                let result = target.ty.from_literal(lit, ctx)?;
                return Ok(Some(ValueWithFlags::new(
                    result,
                    DeserializerMeta {
                        flags,
                        ty: target.map_ty(|_| TyResolvedRef::Null(NullTy)),
                    },
                )));
            }
            (CompletionState::Incomplete, None) => {
                flags.add_flag(Flag::Incomplete);
            }
            (CompletionState::Complete, _) => {}
        }

        match value {
            crate::jsonish::Value::Null => {}
            v => flags.add_flag(Flag::DefaultButHadValue(Cow::Borrowed(v))),
        }

        let result = BamlNull;

        Ok(Some(ValueWithFlags::new(
            result,
            DeserializerMeta {
                flags,
                ty: target.map_ty(|_| TyResolvedRef::Null(NullTy)),
            },
        )))
    }

    fn try_cast(
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlNull, N>> {
        if target.meta.parse_without_null {
            return None;
        }
        // Null doesn't carry CompletionState, so it's always complete — no in_progress handling needed.
        let crate::jsonish::Value::Null = value else {
            return None;
        };

        Some(ValueWithFlags::new(
            BamlNull,
            DeserializerMeta {
                flags: DeserializerConditions::new(),
                ty: TyWithMeta::new(TyResolvedRef::Null(NullTy), target.meta),
            },
        ))
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for StringTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlString<'s>, N>>, ParsingError> {
        let mut flags = DeserializerConditions::new();

        // Handle in_progress for all incomplete values
        match (value.completion_state(), target.meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return Ok(None),
            (CompletionState::Incomplete, Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                let result = target.ty.from_literal(lit, ctx)?;
                return Ok(Some(ValueWithFlags::new(
                    result,
                    DeserializerMeta {
                        flags,
                        ty: target.map_ty(|_| TyResolvedRef::String(StringTy)),
                    },
                )));
            }
            (CompletionState::Incomplete, None) => {
                flags.add_flag(Flag::Incomplete);
            }
            (CompletionState::Complete, _) => {}
        }

        let result: String = match value {
            crate::jsonish::Value::String(s, _) => s.to_string(),
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
                            if original_string.starts_with(s.as_ref()) || s == original_string =>
                        {
                            Some((s.clone(), *completion_state))
                        }
                        _ => None,
                    })
                    .max_by_key(|(s, _)| s.len());

                let (string_val, _completion_state) = string_value
                    .unwrap_or_else(|| (original_string.clone(), *value.completion_state()));

                string_val.into_owned()
            }
            v => {
                flags.add_flag(Flag::JsonToString(Cow::Borrowed(v)));
                v.to_string()
            }
        };

        let result = BamlString {
            value: result.into(),
        };

        Ok(Some(ValueWithFlags::new(
            result,
            DeserializerMeta {
                flags,
                ty: target.map_ty(|_| TyResolvedRef::String(StringTy)),
            },
        )))
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlString<'s>, N>> {
        let jsonish::Value::String(s, completion_state) = value else {
            return None;
        };

        let flags = match (completion_state, target.meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return None,
            (CompletionState::Incomplete, Some(lit)) => {
                return target
                    .ty
                    .from_literal(lit, ctx)
                    .map(|ret| {
                        ValueWithFlags::new(
                            ret,
                            DeserializerMeta {
                                flags: DeserializerConditions::new()
                                    .with_flag(Flag::DefaultButHadValue(Cow::Borrowed(value))),
                                ty: target.map_ty(|_| TyResolvedRef::String(StringTy)),
                            },
                        )
                    })
                    .ok();
            }
            (CompletionState::Incomplete, None) => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            (CompletionState::Complete, _) => DeserializerConditions::new(),
        };

        Some(ValueWithFlags::new(
            BamlString {
                value: s.to_string().into(),
            },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::String(StringTy), target.meta),
            },
        ))
    }
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for MediaTy
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        _value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlMedia, N>>, ParsingError> {
        let e = match target.ty {
            MediaTy::Image => ctx.error_image_not_supported(),
            MediaTy::Audio => ctx.error_audio_not_supported(),
            MediaTy::Pdf => ctx.error_pdf_not_supported(),
            MediaTy::Video => ctx.error_video_not_supported(),
        };
        // TODO: media
        Err(e)
    }

    fn try_cast(
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        _target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        _value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, BamlMedia, N>> {
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

static COMMA_SEPARATED_NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"([-+]?)\$?(?:\d+(?:,\d+)*(?:\.\d+)?|\d+\.\d+|\d+|\.\d+)(?:e[-+]?\d+)?").unwrap()
});
static CURRENCY_SYMBOL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\p{Sc}").unwrap());
fn float_from_comma_separated(value: &str) -> Option<f64> {
    let matches: Vec<_> = COMMA_SEPARATED_NUMBER_RE.find_iter(value).collect();

    if matches.len() != 1 {
        return None;
    }

    let number_str = matches[0].as_str();
    let without_commas = number_str.replace(',', "");
    // Remove all Unicode currency symbols
    let without_currency = CURRENCY_SYMBOL_RE.replace_all(&without_commas, "");

    without_currency.parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;
    use crate::sap_model::TypeRefDb;

    #[test]
    fn test_float_from_comma_separated() {
        // Note we don't handle european numbers correctly.
        let test_cases = vec![
            // European Formats
            // Valid German format (comma as decimal separator)
            ("3,14", Some(314.0)),
            ("1.234,56", None),
            ("1.234.567,89", None),
            ("€1.234,56", None),
            ("-€1.234,56", None),
            ("€1.234", Some(1.234)), // TODO - technically incorrect
            ("1.234€", Some(1.234)), // TODO - technically incorrect
            // Valid currencies with European formatting
            ("€1.234,56", None),
            ("€1,234.56", Some(1234.56)), // Incorrect format for Euro
            // US Formats
            // Valid US format (comma as thousands separator)
            ("3,000", Some(3000.0)),
            ("3,100.00", Some(3100.00)),
            ("1,234.56", Some(1234.56)),
            ("1,234,567.89", Some(1_234_567.89)),
            ("$1,234.56", Some(1234.56)),
            ("-$1,234.56", Some(-1234.56)),
            ("$1,234", Some(1234.0)),
            ("1,234$", Some(1234.0)),
            ("$1,234.56", Some(1234.56)),
            ("+$1,234.56", Some(1234.56)),
            ("-$1,234.56", Some(-1234.56)),
            ("$9,999,999,999", Some(9_999_999_999.0)),
            ("$1.23.456", None),
            ("$1.234.567.890", None),
            // Valid currencies with US formatting
            ("$1,234", Some(1234.0)),
            ("$314", Some(314.0)),
            // Indian Formats
            // Assuming Indian numbering system (not present in original tests, added for categorization)
            ("$1,23,456", Some(123_456.0)),
            // Additional Indian format test cases can be added here

            // Percentages and Strings with Numbers
            // Percentages
            ("50%", Some(50.0)),
            ("3.15%", Some(3.15)),
            (".009%", Some(0.009)),
            ("1.234,56%", None),
            ("$1,234.56%", Some(1234.56)),
            // Strings containing numbers
            ("The answer is 10,000", Some(10000.0)),
            ("The total is €1.234,56 today", None),
            ("You owe $3,000 for the service", Some(3000.0)),
            ("Save up to 20% on your purchase", Some(20.0)),
            ("Revenue grew by 1,234.56 this quarter", Some(1234.56)),
            ("Profit is -€1.234,56 in the last month", None),
            // Sentences with Multiple Numbers
            ("The answer is 10,000 and $3,000", None),
            ("We earned €1.234,56 and $2,345.67 this year", None),
            ("Increase of 5% and a profit of $1,000", None),
            ("Loss of -€500 and a gain of 1,200.50", None),
            ("Targets: 2,000 units and €3.000,75 revenue", None),
            // trailing periods and commas
            ("12,111,123.", Some(12_111_123.0)),
            ("12,111,123,", Some(12_111_123.0)),
        ];

        for (input, expected) in test_cases {
            let result = float_from_comma_separated(input);
            assert_eq!(
                result, expected,
                "Failed to parse '{input}'. Expected {expected:?}, got {result:?}"
            );
        }
    }

    #[test]
    fn test_coerce_anyof_to_string() {
        // Create an AnyOf value similar to what the parser creates
        let anyof_value = jsonish::Value::AnyOf(
            vec![
                jsonish::Value::String(Cow::Borrowed("[json\n"), CompletionState::Incomplete),
                jsonish::Value::Object(vec![], CompletionState::Incomplete),
            ],
            Cow::Borrowed("[json\nAnyOf[{,AnyOf[{,{},],]"), // This is the raw string
        );

        let db: TypeRefDb<'_, &str> = TypeRefDb::new();
        let ctx = ParsingContext::new(&db);

        let annotations: TypeAnnotations<'_, &str> = TypeAnnotations::default();
        let result = StringTy::coerce(&ctx, TyWithMeta::new(&StringTy, &annotations), &anyof_value);

        // The bug would cause this to return "AnyOf[..."
        // The fix should prefer the String variant from the choices if available
        assert!(result.is_ok());
        let baml_value = result.unwrap().unwrap();
        // Should NOT start with "AnyOf[" - that's the bug!
        assert!(
            !baml_value.value.value.starts_with("AnyOf["),
            "Got parsing artifact in string: {}",
            baml_value.value.value
        );
        // Should be the String variant from the choices, not the Display repr
        assert_eq!(&*baml_value.value.value, "[json\n");
    }

    #[test]
    fn test_coerce_anyof_to_string_no_string_variant() {
        // Create an AnyOf value with NO string variant - should fall back to raw string
        let anyof_value = jsonish::Value::AnyOf(
            vec![
                jsonish::Value::Object(vec![], CompletionState::Incomplete),
                jsonish::Value::Array(vec![], CompletionState::Incomplete),
            ],
            Cow::Borrowed("some raw input"),
        );

        let db: TypeRefDb<'_, &str> = TypeRefDb::new();
        let ctx = ParsingContext::new(&db);

        let annotations: TypeAnnotations<'_, &str> = TypeAnnotations::default();
        let result = StringTy::coerce(&ctx, TyWithMeta::new(&StringTy, &annotations), &anyof_value);

        assert!(result.is_ok());
        let baml_value = result.unwrap().unwrap();
        // Should fall back to the raw input string
        assert_eq!(&*baml_value.value.value, "some raw input");
    }

    // ── Bigint size-cap helper tests ─────────────────────────────────────
    //
    // The integration path (through SAP coercion) needs a string that
    // exceeds the bigint cap, which is ~80M digits — too expensive to
    // construct on every test run. These unit tests exercise the bounds
    // logic directly with small inputs, and the cap branch is verified via
    // an `oversize` test gated on the cap constant.

    #[test]
    fn parse_bigint_decimal_bounded_accepts_small() {
        let bi = super::parse_bigint_decimal_bounded(b"42").unwrap();
        assert_eq!(bi, BigInt::from(42));
    }

    #[test]
    fn parse_bigint_decimal_bounded_accepts_negative() {
        let bi = super::parse_bigint_decimal_bounded(b"-42").unwrap();
        assert_eq!(bi, BigInt::from(-42));
    }

    #[test]
    fn parse_bigint_decimal_bounded_rejects_garbage() {
        // The underlying parser refuses non-digit bytes.
        assert!(super::parse_bigint_decimal_bounded(b"not-a-number").is_none());
        assert!(super::parse_bigint_decimal_bounded(b"").is_none());
    }

    #[test]
    fn parse_bigint_decimal_bounded_rejects_oversized() {
        // Construct a digit string one byte past the pre-flight cap. The
        // function must reject it without performing the BigInt allocation.
        let oversized: Vec<u8> = vec![b'9'; baml_type::MAX_BIGINT_DECIMAL_DIGITS + 1];
        assert!(super::parse_bigint_decimal_bounded(&oversized).is_none());
    }
}
