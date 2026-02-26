use crate::{
    baml_value::{BamlNull, BamlValue},
    jsonish::CompletionState,
    sap_model::{
        NullTy, PrimitiveTy, TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent, UnionTy,
    },
};
use anyhow::Result;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::{
    coercer::array_helper,
    deserialize_flags::{DeserializerConditions, Flag},
    score::WithScore,
    types::{BamlValueWithFlags, DeserializerMeta},
};

impl<'t, N: TypeIdent> TypeCoercer<'t, N> for UnionTy<'t, N> {
    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags<'t, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = target,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        let all_variants = &target.ty.variants;

        // Optimization: If we have a hint from a previous array element, try that variant first.
        // This helps with arrays of unions where elements are typically homogeneous.
        if let Some(hint_idx) = ctx.union_variant_hint {
            if hint_idx < all_variants.len() {
                let hinted_option = &all_variants[hint_idx];
                if let Ok(resolved) = ctx.db.resolve_with_meta(hinted_option.as_ref()) {
                    let resolved_ref = TyWithMeta::new(resolved.ty, resolved.meta);
                    let result = TyResolvedRef::coerce(ctx, resolved_ref, value);
                    if let Ok(mut val) = result {
                        // If the hinted variant gives a perfect match, return immediately
                        if val.score() == 0 {
                            log::debug!(
                                "scope: {scope} :: union hint {hint_idx} succeeded for {name}",
                                scope = ctx.display_scope(),
                                name = target,
                            );
                            // Add UnionMatch flag so subsequent array elements can use this hint
                            val.add_flag(Flag::UnionMatch(hint_idx, vec![]));
                            return Ok(val);
                        }
                    }
                }
            }
        }

        // Standard path: try all variants with early termination on perfect match
        let mut parsed: Vec<Result<BamlValueWithFlags<'t, N>, ParsingError>> = Vec::new();
        let mut best_score = i32::MAX;

        for (i, option) in all_variants.iter().enumerate() {
            if let Ok(resolved) = ctx.db.resolve_with_meta(option.as_ref()) {
                let resolved_ref = TyWithMeta::new(resolved.ty, resolved.meta);
                let result = TyResolvedRef::coerce(ctx, resolved_ref, value);
                if let Ok(mut val) = result {
                    let score = val.score();
                    // If we find a perfect match (score 0), we can stop immediately
                    if score == 0 {
                        // Add UnionMatch flag so subsequent array elements can use this hint
                        val.add_flag(Flag::UnionMatch(i, vec![]));
                        return Ok(val);
                    }
                    if score < best_score {
                        best_score = score;
                    }
                    parsed.push(Ok(val));
                } else {
                    parsed.push(result);
                }
            }
        }

        array_helper::pick_best(
            ctx,
            TyWithMeta::new(TyResolvedRef::Union(target.ty), target.meta),
            parsed,
        )
    }

    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<BamlValueWithFlags<'t, N>> {
        let value = value?;

        if matches!(value, crate::jsonish::Value::Null) && target.ty.is_optional(ctx.db) {
            let mut result = BamlValueWithFlags::new(
                BamlValue::Null(BamlNull),
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyWithMeta::new(
                        TyResolvedRef::Primitive(PrimitiveTy::Null(NullTy)),
                        target.meta,
                    ),
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
            .filter(|v| !matches!(v.ty, TyResolvedRef::Primitive(PrimitiveTy::Null(_))))
            .collect();

        // Optimization: If we have a hint from a previous array element, try that variant first.
        if let Some(hint_idx) = ctx.union_variant_hint {
            if let Some(hint_variant) = all_options.get(hint_idx) {
                let opt_ref = TyWithMeta::new(hint_variant.ty, hint_variant.meta);
                if let Some(mut cast_result) = TyResolvedRef::try_cast(ctx, opt_ref, Some(value)) {
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
        let mut filtered_options: Vec<(usize, BamlValueWithFlags<'t, N>)> = Vec::new();
        for (i, opt) in all_options.iter().enumerate() {
            let opt_ref = TyWithMeta::new(opt.ty, opt.meta);
            if let Some(mut cast_result) = TyResolvedRef::try_cast(ctx, opt_ref, Some(value)) {
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
