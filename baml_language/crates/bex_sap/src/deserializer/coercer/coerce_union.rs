use std::borrow::Cow;

use crate::{
    baml_value::{BamlNull, BamlValue},
    jsonish::CompletionState,
    sap_model::{
        AttrLiteral, FromLiteral as _, NullTy, TyResolvedRef, TyWithMeta, TypeAnnotations,
        TypeIdent, UnionTy,
    },
};
use anyhow::Result;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::{
    coercer::array_helper,
    deserialize_flags::{DeserializerConditions, Flag},
    types::{BamlValueWithFlags, DeserializerMeta},
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for UnionTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = target,
            scope = ctx.display_scope(),
            current = value.r#type()
        );

        let all_variants = &target.ty.variants;

        let mut add_flags = Vec::new();
        match (value.completion_state(), target.meta.in_progress.as_ref()) {
            // Incomplete value with `in_progress = never`, ignore
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return Ok(None),
            // Incomplete value with `in_progress = <value>`, use that value
            (CompletionState::Incomplete, Some(lit)) => {
                let ret = target.ty.from_literal(lit, ctx)?;
                return Ok(Some(BamlValueWithFlags::new(
                    ret,
                    DeserializerMeta {
                        flags: DeserializerConditions::new()
                            .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value))),
                        ty: target.map_ty(TyResolvedRef::Union),
                    },
                )));
            }
            // No `in_progress`, use partial value
            (CompletionState::Incomplete, None) => {
                add_flags.push(Flag::Incomplete);
            }
            // Complete value, don't worry about in_progress
            (CompletionState::Complete, _) => {}
        }

        // Optimization: If we have a hint from a previous array element, try that variant first.
        // This helps with arrays of unions where elements are typically homogeneous.
        if let Some(hint_idx) = ctx.union_variant_hint
            && let Some(hinted_option) = all_variants.get(hint_idx)
            && let Ok(resolved) = ctx.db.resolve_with_meta(hinted_option.as_ref())
        {
            let resolved_ref = TyWithMeta::new(resolved.ty, resolved.meta);
            let result = TyResolvedRef::coerce(ctx, resolved_ref, value);

            if let Ok(Some(mut val)) = result
                && val.score() == 0
            {
                // If the hinted variant gives a perfect match, return immediately
                log::debug!(
                    "scope: {scope} :: union hint {hint_idx} succeeded for {name}",
                    scope = ctx.display_scope(),
                    name = target,
                );
                // Add UnionMatch flag so subsequent array elements can use this hint
                val.add_flag(Flag::UnionMatch(hint_idx, vec![]));
                return Ok(Some(val));
            }
        }

        // Standard path: try all variants with early termination on perfect match
        let mut variants: Vec<Result<BamlValueWithFlags<'s, 'v, 't, N>, ParsingError>> = Vec::new();

        for (i, option) in all_variants.iter().enumerate() {
            let parsed = ctx
                .db
                .resolve_with_meta(option.as_ref())
                .map_err(|ident| ctx.error_type_resolution(ident))
                .and_then(|ty| TyResolvedRef::coerce(ctx, ty, value));
            match parsed {
                Ok(None) => {
                    // Variant type with `in_progress = never` means we ignore this variant until it is complete.
                    continue;
                }
                Ok(Some(mut val)) => {
                    if let Err(e) = option.meta.expect_asserts(&val.value, ctx) {
                        variants.push(Err(e));
                        continue;
                    }
                    let score = val.score();
                    // If we find a perfect match (score 0), we can stop immediately
                    if score == 0 {
                        // Add UnionMatch flag so subsequent array elements can use this hint
                        val.add_flag(Flag::UnionMatch(i, vec![]));
                        return Ok(Some(val));
                    }
                    variants.push(Ok(val));
                }
                Err(e) => {
                    variants.push(Err(e));
                }
            }
        }

        let best = array_helper::pick_best(
            ctx,
            TyWithMeta::new(TyResolvedRef::Union(target.ty), target.meta),
            variants,
        );
        best.map(|v| v.with_flags(add_flags))
            .or_else(|err| match &target.meta.on_error {
                // No error fallback, return the error
                AttrLiteral::Never => Err(err),
                lit => match target.ty.from_literal(&lit, ctx) {
                    // Error fallback, return the literal
                    Ok(ret) => {
                        let meta = DeserializerMeta {
                            flags: DeserializerConditions::new()
                                .with_flag(Flag::DefaultButHadUnparseableValue(err)),
                            ty: target.map_ty(TyResolvedRef::Union),
                        };
                        Ok(BamlValueWithFlags::new(ret, meta))
                    }
                    // Error fallback failed, return the error with cause
                    Err(lit_err) => Err(lit_err.with_cause(err)),
                },
            })
            .map(Some)
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<BamlValueWithFlags<'s, 'v, 't, N>> {
        if matches!(value, crate::jsonish::Value::Null) && target.ty.is_optional(ctx.db) {
            let mut result = BamlValueWithFlags::new(
                BamlValue::Null(BamlNull),
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyWithMeta::new(TyResolvedRef::Null(NullTy), target.meta),
                },
            );

            // Check completion state
            match value.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    result.add_flag(crate::deserializer::deserialize_flags::Flag::Incomplete);
                }
            }

            return Some(result);
        }

        let variants: Vec<_> = target
            .ty
            .variants
            .iter()
            .map(|v| ctx.db.resolve_with_meta(v.as_ref()))
            .collect::<Result<_, _>>()
            .ok()?;

        let all_options: Vec<_> = variants
            .iter()
            .filter(|v| !matches!(v.ty, TyResolvedRef::Null(_)))
            .collect();

        // Optimization: If we have a hint from a previous array element, try that variant first.
        if let Some(hint_idx) = ctx.union_variant_hint {
            if let Some(hint_variant) = all_options.get(hint_idx) {
                let opt_ref = TyWithMeta::new(hint_variant.ty, hint_variant.meta);
                if let Some(mut cast_result) = TyResolvedRef::try_cast(ctx, opt_ref, value) {
                    if cast_result.score() == 0 {
                        log::debug!(
                            "scope: {scope} :: try_cast union hint {hint_idx} succeeded for {name}",
                            scope = ctx.display_scope(),
                            name = target,
                        );
                        cast_result.add_flag(Flag::UnionMatch(hint_idx, vec![]));
                        return Some(cast_result);
                    }
                }
            }
        }

        // Collect try_cast results, short-circuit if we find a perfect match (score 0)
        let mut filtered_options: Vec<(usize, BamlValueWithFlags<'s, 'v, 't, N>)> = Vec::new();
        for (i, opt) in all_options.iter().enumerate() {
            let opt_ref = TyWithMeta::new(opt.ty, opt.meta);
            if let Some(mut cast_result) = TyResolvedRef::try_cast(ctx, opt_ref, value) {
                let score = cast_result.score();
                // Perfect match - no need to try other options
                if score == 0 {
                    cast_result.add_flag(Flag::UnionMatch(i, vec![]));
                    return Some(cast_result);
                }
                // Add the flag with the CORRECT original index before storing.
                // This prevents pick_best from adding a flag with wrong (filtered list) index.
                cast_result.add_flag(Flag::UnionMatch(i, vec![]));
                filtered_options.push((i, cast_result));
            }
        }

        let mut result = match filtered_options.len() {
            0 => None,
            1 => {
                let (_, v) = filtered_options.remove(0);
                // Flag already added above with correct index
                Some(v)
            }
            // pick_best will see the existing UnionMatch flag and won't add a duplicate
            _ => array_helper::pick_best(
                ctx,
                TyWithMeta::new(TyResolvedRef::Union(target.ty), target.meta),
                filtered_options
                    .into_iter()
                    .map(|(_, v)| Ok(v))
                    .collect::<Vec<_>>(),
            )
            .ok(),
        };

        // Check completion state
        if let Some(ref mut res) = result {
            match value.completion_state() {
                CompletionState::Complete => {}
                CompletionState::Incomplete => {
                    res.add_flag(crate::deserializer::deserialize_flags::Flag::Incomplete);
                }
            }
        }

        result
    }
}
