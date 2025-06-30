use anyhow::Result;
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta, LiteralValue, TypeIR, TypeValue};
use internal_baml_core::ir::{
    ir_helpers::{infer_type, infer_type_with_meta, map_types},
    IRHelper, IRHelperExtended, IRSemanticStreamingHelper,
};

use crate::{
    runtime_context::{RuntimeClassOverride, RuntimeEnumOverride},
    RuntimeContext,
};

pub struct ScopedIr<'a> {
    ir: &'a internal_baml_core::ir::repr::IntermediateRepr,
    ctx: &'a RuntimeContext,
}

enum FindResult<T, U> {
    Overriden(T, U),
    OnlyDynamic(U),
    OnlyIr(T),
}

impl<'ir> ScopedIr<'ir> {
    pub fn new(
        ir: &'ir internal_baml_core::ir::repr::IntermediateRepr,
        ctx: &'ir RuntimeContext,
    ) -> Self {
        Self { ir, ctx }
    }

    fn find_class(
        &self,
        class_name: &str,
    ) -> Result<FindResult<internal_baml_core::ir::ClassWalker<'ir>, &RuntimeClassOverride>> {
        let class_override = self.ctx.class_override.get(class_name);
        let class = self.ir.find_class(class_name);

        match (class, class_override) {
            (Err(_), None) => Err(anyhow::anyhow!("Class {} not found", class_name)),
            (Ok(class), None) => Ok(FindResult::OnlyIr(class)),
            (Err(_), Some(class_override)) => Ok(FindResult::OnlyDynamic(class_override)),
            (Ok(class), Some(class_override)) => Ok(FindResult::Overriden(class, class_override)),
        }
    }

    fn find_enum(
        &self,
        enum_name: &str,
    ) -> Result<FindResult<internal_baml_core::ir::EnumWalker<'ir>, &RuntimeEnumOverride>> {
        let enum_override = self.ctx.enum_overrides.get(enum_name);
        let r#enum = self.ir.find_enum(enum_name);

        match (r#enum, enum_override) {
            (Err(_), None) => Err(anyhow::anyhow!("Enum {} not found", enum_name)),
            (Ok(r#enum), None) => Ok(FindResult::OnlyIr(r#enum)),
            (Err(_), Some(enum_override)) => Ok(FindResult::OnlyDynamic(enum_override)),
            (Ok(r#enum), Some(enum_override)) => Ok(FindResult::Overriden(r#enum, enum_override)),
        }
    }
}

impl IRSemanticStreamingHelper for ScopedIr<'_> {
    fn class_streaming_needed_fields(
        &self,
        class_name: &str,
    ) -> Result<std::collections::HashSet<String>> {
        let class_type = &self.find_class(class_name)?;
        let result = match class_type {
            FindResult::OnlyIr(cls) | FindResult::Overriden(cls, _) => cls
                .walk_fields()
                .filter_map(|field| {
                    if field.streaming_behavior().needed {
                        Some(field.name().to_string())
                    } else {
                        None
                    }
                })
                .collect(),
            FindResult::OnlyDynamic(_) => Default::default(),
        };

        Ok(result)
    }

    fn class_fields(&self, class_name: &str) -> Result<BamlMap<String, TypeIR>> {
        let class_type = &self.find_class(class_name)?;

        let result = match class_type {
            FindResult::Overriden(cls, overriding_cls) => {
                let new_fields = overriding_cls
                    .new_fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.0.clone()));

                cls.elem()
                    .static_fields
                    .iter()
                    .map(|field_node| {
                        (
                            field_node.elem.name.clone(),
                            field_node.elem.r#type.elem.clone(),
                        )
                    })
                    .chain(new_fields)
                    .collect()
            }
            FindResult::OnlyDynamic(cls) => cls
                .new_fields
                .iter()
                .map(|(k, v)| (k.clone(), v.0.clone()))
                .collect(),
            FindResult::OnlyIr(cls) => cls
                .elem()
                .static_fields
                .iter()
                .map(|field_node| {
                    (
                        field_node.elem.name.clone(),
                        field_node.elem.r#type.elem.clone(),
                    )
                })
                .collect(),
        };

        Ok(result)
    }

    fn find_class_fields_needing_null_filler(
        &self,
        class_name: &str,
        value_names: &std::collections::HashSet<String>,
    ) -> Result<std::collections::HashSet<String>> {
        let class_type = &self.find_class(class_name)?;

        let result = match class_type {
            FindResult::OnlyIr(cls) | FindResult::Overriden(cls, _) => cls
                .walk_fields()
                .filter_map(|field| {
                    if !value_names.contains(field.name()) {
                        Some(field.name().to_string())
                    } else {
                        None
                    }
                })
                .collect(),
            FindResult::OnlyDynamic(_) => Default::default(),
        };

        Ok(result)
    }

    fn get_all_recursive_aliases(&self, alias_name: &str) -> impl Iterator<Item = &TypeIR> {
        self.ctx
            .recursive_type_alias_overrides
            .iter()
            .filter_map(|cycle| cycle.get(alias_name))
            .chain(self.ir.get_all_recursive_aliases(alias_name))
    }
}

impl IRHelperExtended for ScopedIr<'_> {
    fn recursive_alias_definition(&self, alias_name: &str) -> Option<&TypeIR> {
        match self
            .ctx
            .recursive_type_alias_overrides
            .iter()
            .find_map(|cycle| cycle.get(alias_name))
        {
            Some(alias) => Some(alias),
            None => self.ir.recursive_alias_definition(alias_name),
        }
    }
}
