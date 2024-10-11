use anyhow::Result;
use baml_types::FieldType;
use internal_baml_jinja::types::Enum;

use crate::deserializer::{
    coercer::{match_string::match_string, ParsingError, TypeCoercer},
    types::BamlValueWithFlags,
};

use super::ParsingContext;

fn enum_match_candidates<'a>(enm: &'a Enum) -> Vec<(&'a str, Vec<String>)> {
    enm.values
        .iter()
        .map(|(name, desc)| {
            (
                name.real_name(),
                match desc.as_ref().map(|d| d.trim()) {
                    Some(d) if !d.is_empty() => vec![
                        name.rendered_name().into(),
                        d.into(),
                        format!("{}: {}", name.rendered_name(), d),
                    ],
                    _ => vec![name.rendered_name().into()],
                },
            )
        })
        .collect()
}

impl TypeCoercer for Enum {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = self.name.real_name(),
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        let variant_match = match_string(ctx, target, value, &enum_match_candidates(self))?;

        Ok(BamlValueWithFlags::Enum(
            self.name.real_name().to_string(),
            variant_match,
        ))
    }
}
