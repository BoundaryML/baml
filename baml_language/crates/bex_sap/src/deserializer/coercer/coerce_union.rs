use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::{
    baml_value::{BamlNull, BamlValue},
    deserializer::{
        coercer::array_helper,
        deserialize_flags::{DeserializerConditions, Flag},
        types::{BamlValueWithFlags, DeserializerMeta},
    },
    jsonish::CompletionState,
    sap_model::{NullTy, TyResolvedRef, TypeIdent, UnionTy},
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for UnionTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError> {
        let all_variants = &target.variants;

        // A union is partial when its matching member is; members without a partial parse
        // yield `None` below and drop out.
        let mut add_flags = Vec::new();
        if value.completion_state() == &CompletionState::Incomplete {
            add_flags.push(Flag::Incomplete);
        }

        // Optimization: If we have a hint from a previous array element, try that variant first.
        // This helps with arrays of unions where elements are typically homogeneous.
        if let Some(hint_idx) = ctx.union_variant_hint
            && let Some(hinted_option) = all_variants.get(hint_idx)
            && let Ok(resolved) = ctx.db.resolve(hinted_option)
        {
            let result = TyResolvedRef::coerce(ctx, resolved, value);

            if let Ok(Some(mut val)) = result
                && val.score() == 0
            {
                // If the hinted variant gives a perfect match, return immediately
                // Add UnionMatch flag so subsequent array elements can use this hint
                val.add_flag(Flag::UnionMatch(hint_idx, vec![]));
                return Ok(Some(val.with_flags(add_flags)));
            }
        }

        // Standard path: try all variants with early termination on perfect match
        let mut variants: Vec<Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError>> =
            Vec::new();

        for (i, option) in all_variants.iter().enumerate() {
            let parsed = ctx
                .db
                .resolve(option)
                .map_err(|ident| ctx.error_type_resolution(ident))
                .and_then(|ty| TyResolvedRef::coerce(ctx, ty, value));
            match parsed {
                Ok(None) => {
                    // The member has no partial parse for this incomplete value.
                    variants.push(Ok(None));
                }
                Ok(Some(mut val)) => {
                    let score = val.score();
                    // If we find a perfect match (score 0), we can stop immediately
                    if score == 0 {
                        // Add UnionMatch flag so subsequent array elements can use this hint
                        val.add_flag(Flag::UnionMatch(i, vec![]));
                        return Ok(Some(val.with_flags(add_flags)));
                    }
                    variants.push(Ok(Some(val)));
                }
                Err(e) => {
                    variants.push(Err(e));
                }
            }
        }

        let best = array_helper::pick_best(ctx, TyResolvedRef::Union(target), variants);
        best.map(|v| v.map(|v| v.with_flags(add_flags)))
    }

    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<BamlValueWithFlags<'s, 'v, 't, N>> {
        let flags = match value.completion_state() {
            CompletionState::Incomplete => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            CompletionState::Complete => DeserializerConditions::new(),
        };

        if matches!(value, crate::jsonish::Value::Null) && target.is_optional(ctx.db) {
            return Some(BamlValueWithFlags::new(
                BamlValue::Null(BamlNull),
                DeserializerMeta {
                    flags: DeserializerConditions::new(),
                    ty: TyResolvedRef::Null(NullTy),
                },
            ));
        }

        let variants: Vec<TyResolvedRef<'t, N>> = target
            .variants
            .iter()
            .map(|v| ctx.db.resolve(v))
            .collect::<Result<_, _>>()
            .ok()?;

        // Carry original indices through the filter so UnionMatch flags
        // and hints use the same index space as coerce().
        let all_options: Vec<_> = variants
            .iter()
            .enumerate()
            .filter(|(_, v)| !matches!(v, TyResolvedRef::Null(_)))
            .collect();

        // Optimization: If we have a hint from a previous array element, try that variant first.
        // The hint index is in the original (unfiltered) index space, so look it up in `variants`.
        if let Some(hint_idx) = ctx.union_variant_hint
            && let Some(hint_variant) = variants.get(hint_idx)
            && !matches!(hint_variant, TyResolvedRef::Null(_))
            && let Some(mut cast_result) = TyResolvedRef::try_cast(ctx, *hint_variant, value)
            && cast_result.score() == 0
        {
            cast_result.add_flag(Flag::UnionMatch(hint_idx, vec![]));
            return Some(cast_result);
        }

        // Collect try_cast results, short-circuit if we find a perfect match (score 0)
        let mut filtered_options: Vec<(usize, BamlValueWithFlags<'s, 'v, 't, N>)> = Vec::new();
        for &(orig_idx, opt) in &all_options {
            if let Some(mut cast_result) = TyResolvedRef::try_cast(ctx, *opt, value) {
                let score = cast_result.score();
                // Perfect match - no need to try other options
                if score == 0 {
                    cast_result.add_flag(Flag::UnionMatch(orig_idx, vec![]));
                    return Some(cast_result.with_flags(flags.flags));
                }
                // Add the flag with the CORRECT original index before storing.
                // This prevents pick_best from adding a flag with wrong (filtered list) index.
                cast_result.add_flag(Flag::UnionMatch(orig_idx, vec![]));
                filtered_options.push((orig_idx, cast_result));
            }
        }

        match filtered_options.len() {
            0 => None,
            1 => {
                let (_, v) = filtered_options.remove(0);
                // Flag already added above with correct index
                Some(v.with_flags(flags.flags))
            }
            // pick_best will see the existing UnionMatch flag and won't add a duplicate
            _ => array_helper::pick_best(
                ctx,
                TyResolvedRef::Union(target),
                filtered_options
                    .into_iter()
                    .map(|(_, v)| Ok(Some(v)))
                    .collect::<Vec<_>>(),
            )
            .ok()
            .flatten()
            .map(|v| v.with_flags(flags.flags)),
        }
    }
}
