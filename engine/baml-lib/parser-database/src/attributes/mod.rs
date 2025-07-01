use internal_baml_ast::ast::{Assignment, Top, TopId, TypeAliasId, TypeExpId, TypeExpressionBlock};
use internal_baml_diagnostics::{DatamodelError, Span};

mod alias;
pub mod constraint;
mod description;
mod to_string_attribute;
use baml_types::{Constraint, UnresolvedValue};
use internal_baml_ast::ast::{Expression, SubType};

use crate::{
    context::Context,
    interner::StringId,
    types::{ClassAttributes, EnumAttributes},
};

/// Node attributes.
#[derive(Debug, Default, Clone)]
pub struct Attributes {
    /// Description of the node, used in describing the node to the LLM.
    pub description: Option<UnresolvedValue<Span>>,

    /// Alias for the node used when communicating with the LLM.
    pub alias: Option<UnresolvedValue<Span>>,

    /// Whether the node is a dynamic type.
    pub dynamic_type: Option<bool>,

    /// Whether the node should be skipped during prompt rendering and parsing.
    pub skip: Option<bool>,

    /// @check and @assert attributes attached to the node.
    pub constraints: Vec<Constraint>,

    /// Whether the node has a `@sstream.done` attribute.
    pub streaming_done: Option<bool>,

    /// Whether the node has a `@stream.not_null` attribute.
    pub streaming_needed: Option<bool>,

    /// Whether the node has a `@stream.with_state` attribute.
    pub streaming_state: Option<bool>,
}

impl Attributes {
    /// Set a description.
    pub fn add_description(&mut self, description: UnresolvedValue<Span>) {
        self.description.replace(description);
    }

    /// Get the description.
    pub fn description(&self) -> &Option<UnresolvedValue<Span>> {
        &self.description
    }

    /// Set an alias.
    pub fn add_alias(&mut self, alias: UnresolvedValue<Span>) {
        self.alias.replace(alias);
    }

    /// Get the alias.
    pub fn alias(&self) -> &Option<UnresolvedValue<Span>> {
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

    /// Set skip.
    pub fn set_skip(&mut self) {
        self.skip.replace(true);
    }

    /// Combine two attributes, preferring `self`'s fields when both are present.
    pub fn combine(&self, other: &Attributes) -> Attributes {
        Attributes {
            description: self
                .description
                .as_ref()
                .or(other.description.as_ref())
                .cloned(),
            alias: self.alias.as_ref().or(other.alias.as_ref()).cloned(),
            dynamic_type: self.dynamic_type.or(other.dynamic_type),
            skip: self.skip.or(other.skip),
            constraints: self
                .constraints
                .iter()
                .chain(other.constraints.iter())
                .cloned()
                .collect(),
            streaming_done: self.streaming_done.or(other.streaming_done),
            streaming_needed: self.streaming_needed.or(other.streaming_needed),
            streaming_state: self.streaming_state.or(other.streaming_state),
        }
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
            (TopId::TypeAlias(alias_id), Top::TypeAlias(assignment)) => {
                resolve_type_alias_attributes(alias_id, assignment, ctx)
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
    let span = ast_typexpr.span.clone();
    match sub_type {
        SubType::Enum => {
            let mut enum_attributes = EnumAttributes::default();

            for (value_idx, _value) in ast_typexpr.iter_fields() {
                ctx.assert_all_attributes_processed((type_id, value_idx).into());
                if let Some(attrs) = to_string_attribute::visit(ctx, &span, false) {
                    enum_attributes.value_serializers.insert(value_idx, attrs);
                }
                ctx.validate_visited_attributes();
            }

            // Now validate the enum attributes.
            ctx.assert_all_attributes_processed(type_id.into());
            enum_attributes.serializer = to_string_attribute::visit(ctx, &span, true);
            ctx.validate_visited_attributes();

            ctx.types.enum_attributes.insert(type_id, enum_attributes);
        }
        SubType::Class => {
            let mut class_attributes = ClassAttributes::default();

            for (field_idx, field) in ast_typexpr.iter_fields() {
                ctx.assert_all_attributes_processed((type_id, field_idx).into());
                if let Some(attrs) = to_string_attribute::visit(ctx, &field.span, false) {
                    class_attributes.field_serilizers.insert(field_idx, attrs);
                }
                ctx.validate_visited_attributes();
            }

            // Now validate the class attributes.
            ctx.assert_all_attributes_processed(type_id.into());

            // Process all attributes, including multiple block comments
            while let Some(attrs) = to_string_attribute::visit(ctx, &span, true) {
                class_attributes.extend_serializer(&Some(attrs));
            }
            ctx.validate_visited_attributes();
            ctx.types.class_attributes.insert(type_id, class_attributes);
        }

        _ => (),
    }
}

/// Quick hack to validate type alias attributes.
///
/// Unlike classes and enums, type aliases only support checks and asserts.
/// Everything else is reported as an error. On top of that, checks and asserts
/// must be merged when aliases point to other aliases. We do this recursively
/// when resolving the type alias to its final "virtual" type at
/// [`crate::types::resolve_type_alias`].
///
/// Then checks and asserts are collected from the virtual type and stored in
/// the IR at `engine/baml-lib/baml-core/src/ir/repr.rs`, so there's no need to
/// store them in separate classes like [`ClassAttributes`] or similar, at least
/// for now.
fn resolve_type_alias_attributes<'db>(
    alias_id: TypeAliasId,
    assignment: &'db Assignment,
    ctx: &mut Context<'db>,
) {
    ctx.assert_all_attributes_processed(alias_id.into());

    for _ in 0..assignment.value.attributes().len() {
        // TODO: How does this thing work exactly, the code in the functions
        // above for visiting class fields suggests that this returns "all" the
        // attributes that it finds but it does not return repeated checks and
        // asserts, they are left in the state machine and the Context panics.
        // So we're gonna run this in a for loop so that the visit function
        // calls visit_repeated_attr_from_names enough times to consume all the
        // checks and asserts.
        let type_alias_attributes = to_string_attribute::visit(ctx, assignment.value.span(), false);

        // Some additional specific validation for type alias attributes.
        if let Some(attrs) = &type_alias_attributes {
            if attrs.dynamic_type().is_some()
                || attrs.alias().is_some()
                || attrs.skip().is_some()
                || attrs.description().is_some()
            {
                ctx.diagnostics
                    .push_error(DatamodelError::new_validation_error(
                        "type aliases may only have @check and @assert attributes",
                        assignment.span.clone(),
                    ));
            }
        }
    }

    // Now this should be safe to call and it should not panic because there are
    // checks and asserts left.
    ctx.validate_visited_attributes();
}
