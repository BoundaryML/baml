use anyhow::Result;
use baml_types::{FieldType, StreamingBehavior};
use internal_baml_jinja::types::Enum;

use crate::deserializer::{
    coercer::{
        ir_ref::coerce_class::apply_constraints, match_string::match_string, ParsingError,
        TypeCoercer,
    },
    types::BamlValueWithFlags,
};

use super::ParsingContext;

fn enum_match_candidates(enm: &Enum) -> Vec<(&str, Vec<String>)> {
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

        let constraints = ctx
            .of
            .find_enum(self.name.real_name())
            .map_or(vec![], |class| class.constraints.clone());

        let variant_match = match_string(ctx, target, value, &enum_match_candidates(self))?;
        let enum_match = apply_constraints(
            target,
            vec![],
            BamlValueWithFlags::Enum(self.name.real_name().to_string(), variant_match),
            constraints.clone(),
            StreamingBehavior::default(),
        )?;

        Ok(enum_match)
    }
}
