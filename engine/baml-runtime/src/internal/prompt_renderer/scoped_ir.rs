use crate::{
    runtime_context::{RuntimeClassOverride, RuntimeEnumOverride},
    RuntimeContext,
};
use anyhow::Result;
use baml_types::{
    BamlMap, BamlValue, BamlValueWithMeta, FieldType, LiteralValue, StreamingBehavior, TypeValue,
};
use internal_baml_core::ir::{
    ir_helpers::{infer_type, infer_type_with_meta, item_type, map_types},
    IRHelper, IRHelperExtended, IRSemanticStreamingHelper,
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
                    if field.streaming_needed() {
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

    fn class_fields(&self, class_name: &str) -> Result<BamlMap<String, FieldType>> {
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

    fn find_class_fields_needing_null_filler<'a>(
        &'a self,
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

    fn get_all_recursive_aliases(&self, alias_name: &str) -> impl Iterator<Item = &FieldType> {
        self.ctx
            .recursive_type_alias_overrides
            .iter()
            .filter_map(|cycle| cycle.get(alias_name))
            .chain(self.ir.get_all_recursive_aliases(alias_name))
    }
}

impl IRHelperExtended for ScopedIr<'_> {
    fn recursive_alias_definition(&self, alias_name: &str) -> Option<&FieldType> {
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

    fn distribute_metadata<'a>(
        &'a self,
        field_type: &'a FieldType,
    ) -> (
        &'a FieldType,
        (Vec<baml_types::Constraint>, baml_types::StreamingBehavior),
    ) {
        match field_type {
            FieldType::Class(class_name) => match self.find_class(class_name) {
                Err(_) => (field_type, (Vec::new(), StreamingBehavior::default())),
                Ok(FindResult::Overriden(class_node, _) | FindResult::OnlyIr(class_node)) => (
                    field_type,
                    (
                        class_node.item.attributes.constraints.clone(),
                        class_node.item.attributes.streaming_behavior(),
                    ),
                ),
                Ok(FindResult::OnlyDynamic(_)) => {
                    (field_type, (Vec::new(), StreamingBehavior::default()))
                }
            },
            FieldType::Enum(enum_name) => match self.find_enum(enum_name) {
                Err(_) => (field_type, (Vec::new(), StreamingBehavior::default())),
                Ok(FindResult::Overriden(enum_node, _) | FindResult::OnlyIr(enum_node)) => (
                    field_type,
                    (
                        enum_node.item.attributes.constraints.clone(),
                        StreamingBehavior::default(),
                    ),
                ),
                Ok(FindResult::OnlyDynamic(_)) => {
                    (field_type, (Vec::new(), StreamingBehavior::default()))
                }
            },
            // Check the first level to see if it's constrained.
            FieldType::WithMetadata {
                base,
                constraints,
                streaming_behavior,
            } => {
                match base.as_ref() {
                    // If so, we must check the second level to see if we need to combine
                    // constraints across levels.
                    // The recursion here means that arbitrarily nested `FieldType::WithMetadata`s
                    // will be collapsed before the function returns.
                    FieldType::WithMetadata { .. } => {
                        let (sub_base, (sub_constraints, sub_streaming_behavior)) =
                            self.distribute_metadata(base.as_ref());
                        let combined_constraints = vec![constraints.clone(), sub_constraints]
                            .into_iter()
                            .flatten()
                            .collect();
                        let combined_streaming_behavior =
                            streaming_behavior.combine(&sub_streaming_behavior);
                        (
                            sub_base,
                            (combined_constraints, combined_streaming_behavior),
                        )
                    }
                    _ => (base, (constraints.clone(), streaming_behavior.clone())),
                }
            }
            _ => (field_type, (Vec::new(), StreamingBehavior::default())),
        }
    }
}

const UNIT_TYPE: FieldType = FieldType::Tuple(vec![]);
