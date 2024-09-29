use internal_baml_schema_ast::ast::{Top, TopId, TypeExpId, TypeExpressionBlock};

mod alias;
mod description;
mod to_string_attribute;
use crate::interner::StringId;
use crate::{context::Context, types::ClassAttributes, types::EnumAttributes};
use internal_baml_schema_ast::ast::{Expression, SubType};

///
#[derive(Debug, Default)]
pub struct Attributes {
    /// Description of the node, used in describing the node to the LLM.
    pub description: Option<Expression>,

    /// Alias for the node used when communicating with the LLM.
    pub alias: Option<StringId>,

    /// Whether the node is a dynamic type.
    pub dynamic_type: Option<bool>,

    /// Whether the node should be skipped during prompt rendering and parsing.
    pub skip: Option<bool>,
}

impl Attributes {
    /// Set a description.
    pub fn add_description(&mut self, description: Expression) {
        self.description.replace(description);
    }

    /// Get the description.
    pub fn description(&self) -> &Option<Expression> {
        &self.description
    }

    /// Set an alias.
    pub fn add_alias(&mut self, alias: StringId) {
        self.alias.replace(alias);
    }

    /// Get the alias.
    pub fn alias(&self) -> &Option<StringId> {
        &self.alias
    }

    /// Get dynamism of type.
    pub fn dynamic_type(&self) -> &Option<bool> {
        &self.dynamic_type
    }

    /// Set dynamism of type.
    pub fn set_dynamic_type(&mut self) {
        self.dynamic_type.replace(true);
    }

    /// Get skip.
    pub fn skip(&self) -> &Option<bool> {
        &self.skip
    }

    /// Set dynamism of type.
    pub fn set_skip(&mut self) {
        self.skip.replace(true);
    }

}
pub(super) fn resolve_attributes(ctx: &mut Context<'_>) {
    for top in ctx.ast.iter_tops() {
        match top {
            (TopId::Class(class_id), Top::Class(ast_class)) => {
                resolve_type_exp_block_attributes(class_id, ast_class, ctx, SubType::Class)
            }
            (TopId::Enum(enum_id), Top::Enum(ast_enum)) => {
                resolve_type_exp_block_attributes(enum_id, ast_enum, ctx, SubType::Enum)
            }
            _ => (),
        }
    }
}

fn resolve_type_exp_block_attributes<'db>(
    type_id: TypeExpId,
    ast_typexpr: &'db TypeExpressionBlock,
    ctx: &mut Context<'db>,
    sub_type: SubType,
) {
    match sub_type {
        SubType::Enum => {
            let mut enum_attributes = EnumAttributes::default();

            for (value_idx, _value) in ast_typexpr.iter_fields() {
                ctx.visit_attributes((type_id, value_idx).into());
                if let Some(attrs) = to_string_attribute::visit(ctx, false) {
                    enum_attributes.value_serilizers.insert(value_idx, attrs);
                }
                ctx.validate_visited_attributes();
            }

            // Now validate the enum attributes.
            ctx.visit_attributes(type_id.into());
            enum_attributes.serilizer = to_string_attribute::visit(ctx, true);
            ctx.validate_visited_attributes();

            ctx.types.enum_attributes.insert(type_id, enum_attributes);
        }
        SubType::Class => {
            let mut class_attributes = ClassAttributes::default();

            for (field_idx, _field) in ast_typexpr.iter_fields() {
                ctx.visit_attributes((type_id, field_idx).into());
                if let Some(attrs) = to_string_attribute::visit(ctx, false) {
                    class_attributes.field_serilizers.insert(field_idx, attrs);
                }
                ctx.validate_visited_attributes();
            }

            // Now validate the class attributes.
            ctx.visit_attributes(type_id.into());
            class_attributes.serilizer = to_string_attribute::visit(ctx, true);
            ctx.validate_visited_attributes();

            ctx.types.class_attributes.insert(type_id, class_attributes);
        }

        _ => (),
    }
}
