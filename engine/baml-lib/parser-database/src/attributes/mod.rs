use internal_baml_schema_ast::ast::{Top, TopId, TypeExpId, TypeExpressionBlock};

mod alias;
mod description;
mod get;
mod meta;
mod to_string_attribute;

use crate::{context::Context, types::EnumAttributes};

pub(super) fn resolve_attributes(ctx: &mut Context<'_>) {
    for top in ctx.ast.iter_tops() {
        match top {
            (TopId::Class(class_id), Top::Class(ast_class)) => {
                resolve_type_exp_block_attributes(class_id, ast_class, ctx)
            }
            (TopId::Enum(enum_id), Top::Enum(ast_enum)) => {
                resolve_type_exp_block_attributes(enum_id, ast_enum, ctx)
            }
            _ => (),
        }
    }
}

fn resolve_type_exp_block_attributes<'db>(
    enum_id: TypeExpId,
    ast_enum: &'db TypeExpressionBlock,
    ctx: &mut Context<'db>,
) {
    let mut enum_attributes = EnumAttributes::default();

    for (value_idx, _value) in ast_enum.iter_fields() {
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
