use std::collections::{HashMap, HashSet};

use baml_types::GeneratorOutputType;
use internal_baml_ast::ast::{Field, FieldType, WithIdentifier, WithName, WithSpan};
use internal_baml_diagnostics::DatamodelError;
use itertools::join;

use super::{
    reserved_names::{reserved_client_code_names, ReservedNamesMode},
    types::validate_type,
};
use crate::validate::validation_pipeline::{
    context::Context,
    validations::reserved_names::{
        baml_keywords, RESERVED_NAMES_FUNCTION_PARAMETERS, RESERVED_NAMES_PYTHON,
        RESERVED_NAMES_TYPESCRIPT,
    },
};
pub(super) fn validate(ctx: &mut Context<'_>) {
    let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
        internal_baml_jinja_types::JinjaContext::Prompt,
    );

    for cls in ctx.db.walk_classes() {
        for c in cls.static_fields() {
            let field = c.ast_field();
            if let Some(ft) = &field.expr {
                validate_type(ctx, ft);

                if let Some(skip) = field.attributes.iter().find(|a| a.name.name() == "skip") {
                    if !ft.is_optional() {
                        ctx.push_error(DatamodelError::new_validation_error(
                            &format!("Class field with @skip attribute must be optional. Try making the type nullable: {} {}", field.name(), ft.to_nullable()),
                            field.span().clone(),
                        ));
                    }
                }
            }
        }

        for args in cls.walk_input_args() {
            let arg = args.ast_arg();
            validate_type(ctx, &arg.1.field_type)
        }

        defined_types.start_scope();

        cls.walk_input_args().for_each(|arg| {
            let name = match arg.ast_arg().0 {
                Some(arg) => arg.name(),
                None => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Argument name is missing.",
                        arg.ast_arg().1.span().clone(),
                    ));
                    return;
                }
            };

            let field_type = ctx.db.to_jinja_type(&arg.ast_arg().1.field_type);

            defined_types.add_variable(name, field_type);
        });

        defined_types.end_scope();
        defined_types.errors_mut().clear();
    }
}

/// Enforce that keywords in the user's requested target languages
/// do not appear as field names in BAML classes, and that field
/// names are not equal to type names when using Pydantic.
pub(super) fn assert_no_field_name_collisions(
    ctx: &mut Context<'_>,
    generator_output_types: &HashSet<GeneratorOutputType>,
) {
    // The list of reserved words for all user-requested codegen targets.
    let reserved =
        reserved_client_code_names(generator_output_types, ReservedNamesMode::FieldNames);

    for cls in ctx.db.walk_classes() {
        for c in cls.static_fields() {
            let field: &Field<FieldType> = c.ast_field();

            // Check for keyword in field name.
            if let Some(langs) = reserved.get(field.name()) {
                let msg = match langs.as_slice() {
                    [lang] => format!("Field name is a reserved word in generated {lang} clients."),
                    _ => format!(
                        "Field name is a reserved word in language clients: {}.",
                        join(langs, ", ")
                    ),
                };
                ctx.push_error(DatamodelError::new_field_validation_error(
                    msg,
                    "class",
                    c.name(),
                    field.name(),
                    field.identifier().span().clone(),
                ))
            }

            // Check for collision between field name and type name when using Pydantic.
            if generator_output_types.contains(&GeneratorOutputType::PythonPydantic) {
                let type_name = field
                    .expr
                    .as_ref()
                    .map_or("".to_string(), |r#type| r#type.name());
                if field.name() == type_name {
                    ctx.push_error(DatamodelError::new_field_validation_error(
                        "When using the python/pydantic generator, a field name must not be exactly equal to the type name. Consider changing the field name and using an alias.".to_string(),
                        "class",
                        c.name(),
                        field.name(),
                        field.identifier().span().clone(),
                    ))
                }
            }

            // Check for BAML language keywords.
            if baml_keywords().contains(field.name()) {
                ctx.push_error(DatamodelError::new_field_validation_error(
                    "Field name cannot be a BAML language keyword.".to_string(),
                    "class",
                    c.name(),
                    field.name(),
                    field.identifier().span().clone(),
                ))
            }
        }
    }

    // check for reserved names in function parameters
    let reserved = reserved_client_code_names(
        generator_output_types,
        ReservedNamesMode::FunctionParameters,
    );
    for func in ctx.db.walk_functions() {
        for param in func.walk_input_args() {
            if let Some(id) = param.ast_arg().0 {
                if let Some(langs) = reserved.get(id.name()) {
                    match langs.as_slice() {
                        [lang] => {
                            ctx.push_error(DatamodelError::new_validation_error(
                                &format!("{} is a reserved word in {}", id.name(), lang),
                                id.span().clone(),
                            ));
                        }
                        _ => {
                            ctx.push_error(DatamodelError::new_validation_error(
                                &format!(
                                    "{} is a reserved word in language clients: {}",
                                    id.name(),
                                    join(langs, ", ")
                                ),
                                id.span().clone(),
                            ));
                        }
                    }
                }
            }
        }
    }
}
