use anyhow::Result;
use baml_types::LiteralValue;
use internal_baml_core::ir::{TypeIR, TypeValue};

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::deserializer::{coercer::array_helper, types::BamlValueWithFlags};

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

    let mut filtered_options = options
        .iter_skip_null()
        .into_iter()
        .filter_map(|opt| opt.try_cast(ctx, union_target, Some(value)))
        .collect::<Vec<_>>();

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

    let parsed = options
        .iter_include_null()
        .iter()
        .map(|option| option.coerce(ctx, union_target, value))
        .collect::<Vec<_>>();

    array_helper::pick_best(ctx, union_target, &parsed)
}
