use anyhow::Result;
use internal_baml_core::ir::{FieldType, TypeValue};

use crate::deserializer::{
    coercer::TypeCoercer, deserialize_flags::Flag, types::BamlValueWithFlags,
};

use super::{
    array_helper, coerce_array::coerce_array, coerce_optional::coerce_optional,
    coerce_union::coerce_union, ir_ref::IrRef, ParsingContext, ParsingError,
};

impl TypeCoercer for FieldType {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        match value {
            Some(crate::jsonish::Value::AnyOf(candidates, primitive)) => {
                if matches!(target, FieldType::Primitive(TypeValue::String)) {
                    self.coerce(
                        ctx,
                        target,
                        Some(&crate::jsonish::Value::String(primitive.clone())),
                    )
                } else {
                    array_helper::coerce_array_to_singular(
                        ctx,
                        target,
                        &candidates.iter().collect::<Vec<_>>(),
                        &|val| self.coerce(ctx, target, Some(val)),
                    )
                }
            }
            Some(crate::jsonish::Value::Markdown(_t, v)) => {
                self.coerce(ctx, target, Some(v)).and_then(|mut v| {
                    v.add_flag(Flag::ObjectFromMarkdown(
                        if matches!(target, FieldType::Primitive(TypeValue::String)) {
                            1
                        } else {
                            0
                        },
                    ));

                    Ok(v)
                })
            }
            Some(crate::jsonish::Value::FixedJson(v, fixes)) => {
                let mut v = self.coerce(ctx, target, Some(v))?;
                v.add_flag(Flag::ObjectFromFixedJson(fixes.to_vec()));
                Ok(v)
            }
            _ => match self {
                FieldType::Primitive(p) => p.coerce(ctx, target, value),
                FieldType::Enum(e) => IrRef::Enum(e).coerce(ctx, target, value),
                FieldType::Class(c) => IrRef::Class(c).coerce(ctx, target, value),
                FieldType::List(_) => coerce_array(ctx, self, value),
                FieldType::Union(_) => coerce_union(ctx, self, value),
                FieldType::Optional(_) => coerce_optional(ctx, self, value),
                FieldType::Map(_, _) => Err(ctx.error_internal("Map not supported")),
                FieldType::Tuple(_) => Err(ctx.error_internal("Tuple not supported")),
            },
        }
    }
}
