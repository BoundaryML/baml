use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{
    Class, ClassId, Enum, EnumId, Top, TopId, Variant, VariantConfigId, WithSpan,
};
use log::info;

mod alias;
mod meta;
mod to_string_attribute;

use crate::{
    context::Context,
    types::{ClassAttributes, EnumAttributes, SerializerAttributes, VariantAttributes},
};

pub(super) fn resolve_attributes(ctx: &mut Context<'_>) {
    info!("Resolving attributes for");
    for top in ctx.ast.iter_tops() {
        info!("Resolving attributes for {:?}", top.0);
        match top {
            (TopId::Class(class_id), Top::Class(ast_class)) => {
                resolve_class_attributes(class_id, ast_class, ctx)
            }
            (TopId::Enum(enum_id), Top::Enum(ast_enum)) => {
                resolve_enum_attributes(enum_id, ast_enum, ctx)
            }
            (TopId::Variant(ctid), Top::Variant(ast_variant)) if ast_variant.is_llm() => {
                resolve_llm_variant_attributes(ctid, ast_variant, ctx)
            }
            _ => (),
        }
    }
}

fn resolve_enum_attributes<'db>(enum_id: EnumId, ast_enum: &'db Enum, ctx: &mut Context<'db>) {
    let mut enum_attributes = EnumAttributes::default();

    for (value_idx, _value) in ast_enum.iter_values() {
        ctx.visit_attributes((enum_id, value_idx).into());
        if let Some(attrs) = to_string_attribute::visit(ctx, false) {
            enum_attributes.value_serilizers.insert(value_idx, attrs);
        }
        ctx.validate_visited_attributes();
    }

    // Now validate the enum attributes.
    ctx.visit_attributes(enum_id.into());
    enum_attributes.serilizer = to_string_attribute::visit(ctx, true);
    ctx.validate_visited_attributes();

    ctx.types.enum_attributes.insert(enum_id, enum_attributes);
}

fn resolve_class_attributes<'db>(class_id: ClassId, ast_class: &'db Class, ctx: &mut Context<'db>) {
    let mut class_attributes = ClassAttributes::default();

    for (field_id, _) in ast_class.iter_fields() {
        ctx.visit_attributes((class_id, field_id).into());
        if let Some(attrs) = to_string_attribute::visit(ctx, false) {
            class_attributes.field_serilizers.insert(field_id, attrs);
        }
        ctx.validate_visited_attributes();
    }

    // Now validate the class attributes.
    ctx.visit_attributes(class_id.into());
    class_attributes.serilizer = to_string_attribute::visit(ctx, true);
    ctx.validate_visited_attributes();

    ctx.types
        .class_attributes
        .insert(class_id, class_attributes);
}

fn resolve_llm_variant_attributes<'db>(
    variant_id: VariantConfigId,
    ast_variant: &'db Variant,
    ctx: &mut Context<'db>,
) {
    let mut variant_attributes = VariantAttributes::default();

    for (field_id, _) in ast_variant.iter_fields() {
        ctx.visit_attributes((variant_id, field_id).into());
        // Variant fields can have no attributes (for now).
        // TODO: Support expressions to have attributes.
        ctx.validate_visited_attributes();
    }

    for (serializer_idx, serializer) in ast_variant.iter_serializers() {
        let mut serializer_attr = SerializerAttributes::default();
        for (field_id, _value_idx) in serializer.iter_fields() {
            ctx.visit_attributes((variant_id, serializer_idx, field_id).into());
            if let Some(attrs) = to_string_attribute::visit(ctx, false) {
                serializer_attr.field_serilizers.insert(field_id, attrs);
            }
            ctx.validate_visited_attributes();
        }
        if variant_attributes
            .serializers
            .insert(serializer_idx, serializer_attr)
            .is_some()
        {
            ctx.push_error(DatamodelError::new_validation_error(
                "Duplicate serializer name.",
                serializer.name.span().clone(),
            ));
        }
    }

    // Now validate the class attributes.
    ctx.visit_attributes(variant_id.into());
    ctx.validate_visited_attributes();

    ctx.types
        .variant_attributes
        .insert(variant_id, variant_attributes);
}
