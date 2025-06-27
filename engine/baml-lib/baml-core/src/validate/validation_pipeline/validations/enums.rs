use std::collections::HashSet;

use baml_types::GeneratorOutputType;
use internal_baml_ast::ast::{WithName, WithSpan};
use internal_baml_diagnostics::DatamodelError;

use super::types::validate_type;
use crate::validate::validation_pipeline::{
    context::Context, validations::reserved_names::RESERVED_NAMES_PYTHON,
};

pub(super) fn validate(ctx: &mut Context<'_>) {
    let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
        internal_baml_jinja_types::JinjaContext::Prompt,
    );
    for enm in ctx.db.walk_enums() {
        for args in enm.walk_input_args() {
            let arg = args.ast_arg();
            validate_type(ctx, &arg.1.field_type)
        }

        defined_types.start_scope();

        enm.walk_input_args().for_each(|arg| {
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

/// Enforce that keywords in Python do not appear as enum values when using the python/pydantic generator.
pub(super) fn assert_no_enum_value_collisions(
    ctx: &mut Context<'_>,
    generator_output_types: &HashSet<GeneratorOutputType>,
) {
    if generator_output_types.contains(&GeneratorOutputType::PythonPydantic) {
        for e in ctx.db.walk_enums() {
            for value in e.values() {
                let value_name = value.name();
                if RESERVED_NAMES_PYTHON.contains(&value_name) {
                    ctx.push_error(DatamodelError::new_field_validation_error(
                        format!("Enum value '{value_name}' is a reserved word in Python, try changing the name and using `OtherValueName @alias(\"{value_name}\")`."),
                        "enum",
                        e.name(),
                        value_name,
                        value.span().clone(),
                    ))
                }
            }
        }
    }
}
