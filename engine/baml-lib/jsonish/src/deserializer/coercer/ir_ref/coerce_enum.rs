use anyhow::Result;
use baml_types::TypeIR;
use internal_baml_jinja::types::Enum;

use super::ParsingContext;
use crate::deserializer::{
    coercer::{
        ir_ref::coerce_class::apply_constraints, match_string::match_string, ParsingError,
        TypeCoercer,
    },
    types::BamlValueWithFlags,
};

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
    fn try_cast(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<BamlValueWithFlags> {
        // Enums can only be cast from string values
        let Some(crate::jsonish::Value::String(s, _)) = value else {
            return None;
        };

        // Check if the string exactly matches any enum variant
        let mut result = None;
        for (variant_name, _) in &self.values {
            if variant_name.rendered_name() == s {
                result = Some(BamlValueWithFlags::Enum(
                    self.name.real_name().to_string(),
                    target.clone(),
                    (variant_name.real_name().to_string(), target).into(),
                ));
                break;
            }
        }

        // Check completion state
        if let Some(v) = value {
            if let Some(ref mut res) = result {
                match v.completion_state() {
                    baml_types::CompletionState::Complete => {}
                    baml_types::CompletionState::Incomplete => {
                        res.add_flag(crate::deserializer::deserialize_flags::Flag::Incomplete);
                    }
                    baml_types::CompletionState::Pending => {
                        unreachable!("jsonish::Value may never be in a Pending state.")
                    }
                }
            }
        }

        result
    }

    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &TypeIR,
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

        let variant_match = match_string(ctx, target, value, &enum_match_candidates(self), true)?;
        let enum_match = apply_constraints(
            target,
            vec![],
            BamlValueWithFlags::Enum(
                self.name.real_name().to_string(),
                target.clone(),
                variant_match,
            ),
            constraints.clone(),
            Default::default(),
        )?;

        Ok(enum_match)
    }
}
