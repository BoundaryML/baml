use anyhow::Result;
use baml_types::LiteralValue;
use internal_baml_core::ir::{TypeIR, TypeValue};

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::{
    coercer::array_helper,
    score::WithScore,
    types::BamlValueWithFlags,
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

    // Optimization: collect try_cast results, but short-circuit if we find a perfect match (score 0)
    let mut filtered_options = Vec::new();
    for opt in options.iter_skip_null() {
        if let Some(cast_result) = opt.try_cast(ctx, union_target, Some(value)) {
            let score = cast_result.score();
            filtered_options.push(cast_result);
            // Perfect match - no need to try other options
            if score == 0 {
                break;
            }
        }
    }

    let mut result = match filtered_options.len() {
        0 => None,
        1 => Some(filtered_options.remove(0)),
        _ => array_helper::pick_best(
            ctx,
            union_target,
            &filtered_options.into_iter().map(Ok).collect::<Vec<_>>(),
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

    // Optimization: Use lazy evaluation with early termination for perfect matches
    let mut parsed: Vec<Result<BamlValueWithFlags, ParsingError>> = Vec::new();
    let mut best_score = i32::MAX;

    for option in options.iter_include_null().iter() {
        let result = option.coerce(ctx, union_target, value);
        if let Ok(ref val) = result {
            let score = val.score();
            // If we find a perfect match (score 0), we can stop immediately
            if score == 0 {
                return result;
            }
            if score < best_score {
                best_score = score;
            }
        }
        parsed.push(result);
    }

    array_helper::pick_best(ctx, union_target, &parsed)
}
