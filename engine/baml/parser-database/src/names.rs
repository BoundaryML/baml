mod validate_reserved_names;

use crate::{
    ast::{self, TopId, WithAttributes, WithName, WithSpan},
    Context, DatamodelError, StaticType, StringId,
};

use internal_baml_schema_ast::ast::{ConfigBlockProperty, WithIdentifier};

use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use validate_reserved_names::*;

use self::validate_reserved_names::{validate_enum_value_name, validate_function_name};

/// Resolved names for use in the validation process.
#[derive(Default)]
pub(super) struct Names {
    /// Models, enums, composite types and type aliases
    pub(super) tops: HashMap<StringId, TopId>,
    /// Generators have their own namespace.
    pub(super) generators: HashMap<StringId, TopId>,
    pub(super) model_fields: HashMap<(ast::ClassId, StringId), ast::FieldId>,
    // pub(super) composite_type_fields: HashMap<(ast::CompositeTypeId, StringId), ast::FieldId>,
}

/// `resolve_names()` is responsible for populating `ParserDatabase.names` and
/// validating that there are no name collisions in the following namespaces:
///
/// - Model, enum and type alias names
/// - Generators
/// - Model fields for each model
/// - Enum variants for each enum
pub(super) fn resolve_names(ctx: &mut Context<'_>) {
    let mut tmp_names: HashSet<&str> = HashSet::default(); // throwaway container for duplicate checking
    let mut names = Names::default();

    for (top_id, top) in ctx.ast.iter_tops() {
        assert_is_not_a_reserved_scalar_type(top.identifier(), ctx);

        let namespace = match (top_id, top) {
            (_, ast::Top::Enum(ast_enum)) => {
                tmp_names.clear();
                validate_enum_name(ast_enum, ctx.diagnostics);
                validate_attribute_identifiers(ast_enum, ctx);

                for value in &ast_enum.values {
                    validate_enum_value_name(value, ctx.diagnostics);
                    validate_attribute_identifiers(value, ctx);

                    if !tmp_names.insert(&value.name.name()) {
                        ctx.push_error(DatamodelError::new_duplicate_enum_value_error(
                            &ast_enum.name.name(),
                            &value.name.name(),
                            value.span.clone(),
                        ))
                    }
                }

                &mut names.tops
            }
            (ast::TopId::Class(model_id), ast::Top::Class(ast_class)) => {
                validate_class_name(ast_class, ctx.diagnostics);
                validate_attribute_identifiers(ast_class, ctx);

                for (field_id, field) in ast_class.iter_fields() {
                    validate_class_fiel_name(field, ctx.diagnostics);
                    validate_attribute_identifiers(field, ctx);

                    let field_name_id = ctx.interner.intern(field.name());
                    if names
                        .model_fields
                        .insert((model_id, field_name_id), field_id)
                        .is_some()
                    {
                        ctx.push_error(DatamodelError::new_duplicate_field_error(
                            &ast_class.identifier().name(),
                            field.name(),
                            "class",
                            field.identifier().span().clone(),
                        ))
                    }
                }

                &mut names.tops
            }
            (ast::TopId::Function(_function_id), ast::Top::Function(ast_function)) => {
                validate_function_name(ast_function, ctx.diagnostics);
                validate_attribute_identifiers(ast_function, ctx);

                &mut names.tops
            }
            (_, ast::Top::Generator(generator)) => {
                validate_generator_name(generator, ctx.diagnostics);
                check_for_duplicate_properties(top, &generator.fields, &mut tmp_names, ctx);
                &mut names.generators
            }
            (_, ast::Top::Variant(variant)) => {
                validate_variant_name(variant, ctx.diagnostics);
                check_for_duplicate_properties(top, &variant.fields, &mut tmp_names, ctx);
                &mut names.tops
            }
            (_, ast::Top::Client(client)) => {
                validate_client_name(client, ctx.diagnostics);
                check_for_duplicate_properties(top, &client.fields, &mut tmp_names, ctx);
                &mut names.tops
            }
            _ => unreachable!(
                "Encountered impossible top during name resolution: {:?}",
                top_id
            ),
        };

        insert_name(top_id, top, namespace, ctx)
    }

    let _ = std::mem::replace(ctx.names, names);
}

fn insert_name(
    top_id: TopId,
    top: &ast::Top,
    namespace: &mut HashMap<StringId, TopId>,
    ctx: &mut Context<'_>,
) {
    let name = ctx.interner.intern(top.name());
    if let Some(existing) = namespace.insert(name, top_id) {
        ctx.push_error(duplicate_top_error(&ctx.ast[existing], top));
    }
}

fn duplicate_top_error(existing: &ast::Top, duplicate: &ast::Top) -> DatamodelError {
    DatamodelError::new_duplicate_top_error(
        duplicate.name(),
        duplicate.get_type(),
        existing.get_type(),
        duplicate.identifier().span().clone(),
    )
}

fn assert_is_not_a_reserved_scalar_type(ident: &ast::Identifier, ctx: &mut Context<'_>) {
    if StaticType::try_from_str(&ident.name()).is_some() {
        ctx.push_error(DatamodelError::new_reserved_scalar_type_error(
            &ident.name(),
            ident.span().clone(),
        ));
    }
}

fn check_for_duplicate_properties<'a>(
    top: &ast::Top,
    props: &'a [ConfigBlockProperty],
    tmp_names: &mut HashSet<&'a str>,
    ctx: &mut Context<'_>,
) {
    tmp_names.clear();
    for arg in props {
        if !tmp_names.insert(&arg.name.name()) {
            ctx.push_error(DatamodelError::new_duplicate_config_key_error(
                &format!("{} \"{}\"", top.get_type(), top.name()),
                &arg.name.name(),
                arg.name.span().clone(),
            ));
        }
    }
}

fn validate_attribute_identifiers(with_attrs: &dyn WithAttributes, ctx: &mut Context<'_>) {
    for attribute in with_attrs.attributes() {
        validate_attribute_name(attribute, ctx.diagnostics);
    }
}
