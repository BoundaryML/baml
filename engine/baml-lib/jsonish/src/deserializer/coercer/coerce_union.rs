use anyhow::Result;
use baml_types::LiteralValue;
use internal_baml_core::ir::{TypeIR, TypeValue};

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::{
    coercer::array_helper, deserialize_flags::Flag, score::WithScore, types::BamlValueWithFlags,
};

pub(super) fn try_cast_union(
    ctx: &ParsingContext,
    union_target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Option<BamlValueWithFlags> {
    let TypeIR::Union(options, _) = union_target else {
        unreachable!("try_cast_union");
    };

    let value = value?;

    if matches!(value, crate::jsonish::Value::Null) && options.is_optional() {
        let mut result = BamlValueWithFlags::Null(union_target.clone(), Default::default());

        // Check completion state
        match value.completion_state() {
            baml_types::CompletionState::Complete => {}
            baml_types::CompletionState::Incomplete => {
                result.add_flag(crate::deserializer::deserialize_flags::Flag::Incomplete);
            }
            baml_types::CompletionState::Pending => {
                unreachable!("jsonish::Value may never be in a Pending state.")
            }
        }

        return Some(result);
    }

    let all_options = options.iter_skip_null();

    // Optimization: If we have a hint from a previous array element, try that variant first.
    if let Some(hint_idx) = ctx.union_variant_hint {
        if hint_idx < all_options.len() {
            if let Some(mut cast_result) =
                all_options[hint_idx].try_cast(ctx, union_target, Some(value))
            {
                if cast_result.score() == 0 {
                    log::debug!(
                        "scope: {scope} :: try_cast union hint {hint_idx} succeeded for {name}",
                        scope = ctx.display_scope(),
                        name = union_target,
                    );
                    cast_result.add_flag(Flag::UnionMatch(hint_idx, vec![]));
                    return Some(cast_result);
                }
            }
        }
    }

    // Collect try_cast results, short-circuit if we find a perfect match (score 0)
    let mut filtered_options: Vec<(usize, BamlValueWithFlags)> = Vec::new();
    for (i, opt) in all_options.iter().enumerate() {
        if let Some(mut cast_result) = opt.try_cast(ctx, union_target, Some(value)) {
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
            union_target,
            &filtered_options
                .into_iter()
                .map(|(_, v)| Ok(v))
                .collect::<Vec<_>>(),
        )
        .ok(),
    };

    // Check completion state
    if let Some(ref mut res) = result {
        match value.completion_state() {
            baml_types::CompletionState::Complete => {}
            baml_types::CompletionState::Incomplete => {
                res.add_flag(crate::deserializer::deserialize_flags::Flag::Incomplete);
            }
            baml_types::CompletionState::Pending => {
                unreachable!("jsonish::Value may never be in a Pending state.")
            }
        }
    }

    result
}

pub(super) fn coerce_union(
    ctx: &ParsingContext,
    union_target: &TypeIR,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    assert!(matches!(union_target, TypeIR::Union(_, _)));
    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = union_target,
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let options = match union_target {
        TypeIR::Union(options, _) => options,
        _ => unreachable!("coerce_union"),
    };

    let all_options = options.iter_include_null();

    // Optimization: If we have a hint from a previous array element, try that variant first.
    // This helps with arrays of unions where elements are typically homogeneous.
    if let Some(hint_idx) = ctx.union_variant_hint {
        if hint_idx < all_options.len() {
            let hinted_option = all_options[hint_idx];
            let result = hinted_option.coerce(ctx, union_target, value);
            if let Ok(mut val) = result {
                // If the hinted variant gives a perfect match, return immediately
                if val.score() == 0 {
                    log::debug!(
                        "scope: {scope} :: union hint {hint_idx} succeeded for {name}",
                        scope = ctx.display_scope(),
                        name = union_target,
                    );
                    // Add UnionMatch flag so subsequent array elements can use this hint
                    val.add_flag(Flag::UnionMatch(hint_idx, vec![]));
                    return Ok(val);
                }
            }
        }
    }

    // Standard path: try all variants with early termination on perfect match
    let mut parsed: Vec<Result<BamlValueWithFlags, ParsingError>> = Vec::new();
    let mut best_score = i32::MAX;

    for (i, option) in all_options.iter().enumerate() {
        let result = option.coerce(ctx, union_target, value);
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

    array_helper::pick_best(ctx, union_target, &parsed)
}
