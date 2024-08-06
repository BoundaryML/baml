use baml_types::TypeValue;
use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{FieldArity, FieldType, Identifier, WithName, WithSpan};

use crate::validate::validation_pipeline::context::Context;

fn errors_with_names<'a>(ctx: &'a mut Context<'_>, idn: &Identifier) {
    // Push the error with the appropriate message
    ctx.push_error(DatamodelError::new_type_not_found_error(
        idn.name(),
        ctx.db.valid_type_names(),
        idn.span().clone(),
    ));
}

/// Called for each type in the baml_src tree, validates that it is well-formed.
///
/// Operates in two passes:
///
///   1. Verify that the type is resolveable (for REF types)
///   2. Verify that the type is well-formed/allowed in the language
pub(crate) fn validate_type(ctx: &mut Context<'_>, field_type: &FieldType) {
    if !validate_type_exists(ctx, field_type) || matches!(field_type, FieldType::Map(..)) {
        validate_type_allowed(ctx, field_type);
    }
}

fn validate_type_exists(ctx: &mut Context<'_>, field_type: &FieldType) -> bool {
    let mut errors = false;
    field_type
        .flat_idns()
        .iter()
        .for_each(|f| match ctx.db.find_type(f) {
            Some(_) => {}

            None => match field_type {
                FieldType::Primitive(..) => {}
                _ => {
                    errors_with_names(ctx, f);
                    errors = true
                }
            },
        });
    errors
}

fn validate_type_allowed(ctx: &mut Context<'_>, field_type: &FieldType) {
    match field_type {
        FieldType::Map(kv_types, ..) => {
            match &kv_types.0 {
                FieldType::Primitive(FieldArity::Required, TypeValue::String, ..) => {}
                key_type => {
                    ctx.push_error(DatamodelError::new_validation_error(
                        "Maps may only have strings as keys",
                        key_type.span().clone(),
                    ));
                }
            }
            validate_type_allowed(ctx, &kv_types.1);
            // TODO:assert key_type is string or int or null
        }

        FieldType::Primitive(_, type_value, span, _) => {
            let primitives = vec!["int", "float", "bool", "string", "image", "audio", "null"];
            if !primitives.contains(&type_value.to_string().as_str()) {
                ctx.push_error(DatamodelError::not_found_error(
                    "Primitive type",
                    &type_value.to_string(),
                    span.clone(),
                    primitives.iter().map(|&s| s.to_string()).collect(),
                ));
            }
        }
        FieldType::Symbol(_, idn, _) => {
            if ctx.db.find_type(idn).is_none() {
                ctx.push_error(DatamodelError::not_found_error(
                    "Type",
                    idn.name(),
                    idn.span().clone(),
                    ctx.db
                        .walk_classes()
                        .chain(ctx.db.walk_enums())
                        .map(|c| c.name().to_string())
                        .collect(),
                ));
            }
        }
        FieldType::List(field_type, ..) => validate_type_allowed(ctx, field_type),
        FieldType::Tuple(_, field_types, ..) | FieldType::Union(_, field_types, ..) => {
            for field_type in field_types {
                validate_type_allowed(ctx, field_type);
            }
        }
    }
}
