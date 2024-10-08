use anyhow::Result;
use internal_baml_core::ir::FieldType;

use crate::deserializer::{coercer::array_helper, types::BamlValueWithFlags};

use super::{ParsingContext, ParsingError, TypeCoercer};

pub(super) fn coerce_union(
    ctx: &ParsingContext,
    union_target: &FieldType,
    value: Option<&crate::jsonish::Value>,
) -> Result<BamlValueWithFlags, ParsingError> {
    assert!(matches!(union_target, FieldType::Union(_)));
    log::debug!(
        "scope: {scope} :: coercing to: {name} (current: {current})",
        name = union_target.to_string(),
        scope = ctx.display_scope(),
        current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
    );

    let options = match union_target {
        FieldType::Union(options) => options,
        _ => unreachable!(),
    };

    let parsed = options
        .iter()
        .map(|option| option.coerce(ctx, option, value))
        .collect::<Vec<_>>();

    array_helper::pick_best(ctx, union_target, &parsed)
}
