use std::collections::VecDeque;

use baml_types::{LiteralValue, TypeValue};
use either::Either;
use internal_baml_ast::ast::{
    Argument, Attribute, Expression, FieldArity, FieldType, Identifier, WithName, WithSpan,
};
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Span};
use internal_baml_parser_database::TypeWalker;

use crate::validate::validation_pipeline::context::Context;

fn errors_with_names(ctx: &mut Context<'_>, idn: &Identifier) {
    // Push the error with the appropriate message
    ctx.push_error(DatamodelError::new_type_not_found_error(
        idn.name(),
        ctx.db.valid_type_names(),
        idn.span().clone(),
    ));
}

/// Called for each type in the baml_src tree, validates that it is well-formed.
///
/// Operates in three passes:
///
///   1. Verify that the type is resolveable (for REF types).
///   2. Verify that the type is well-formed/allowed in the language.
///   3. Verify that constraints on the type are well-formed.
pub(crate) fn validate_type(ctx: &mut Context<'_>, field_type: &FieldType) {
    validate_type_exists(ctx, field_type);
    validate_type_allowed(ctx, field_type);
    validate_type_constraints(ctx, field_type);
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
        FieldType::Map(arity, kv_types, ..) => {
            match &kv_types.0 {
                // String key.
                FieldType::Primitive(FieldArity::Required, TypeValue::String, ..) => {}

                // Enum key.
                FieldType::Symbol(FieldArity::Required, identifier, _)
                    if ctx
                        .db
                        .find_type(identifier)
                        .is_some_and(|t| matches!(t, TypeWalker::Enum(_))) => {}

                // Literal string key.
                FieldType::Literal(FieldArity::Required, LiteralValue::String(_), ..) => {}

                // Literal string union.
                FieldType::Union(FieldArity::Required, items, ..) => {
                    let mut queue = VecDeque::from_iter(items.iter());

                    while let Some(item) = queue.pop_front() {
                        match item {
                            // Ok, literal string.
                            FieldType::Literal(
                                FieldArity::Required,
                                LiteralValue::String(_),
                                ..,
                            ) => {}

                            // Nested union, "recurse" but it's iterative.
                            FieldType::Union(FieldArity::Required, nested, ..) => {
                                queue.extend(nested.iter());
                            }

                            other => {
                                ctx.push_error(
                                    DatamodelError::new_type_not_allowed_as_map_key_error(
                                        other.span().clone(),
                                    ),
                                );
                            }
                        }
                    }
                }

                other => {
                    ctx.push_error(DatamodelError::new_type_not_allowed_as_map_key_error(
                        other.span().clone(),
                    ));
                }
            }
            validate_type_allowed(ctx, &kv_types.1);
            // TODO:assert key_type is string or int or null
        }

        FieldType::Primitive(..) => {}
        FieldType::Literal(..) => {}
        FieldType::Symbol(..) => {}

        FieldType::List(arity, field_type, ..) => validate_type_allowed(ctx, field_type),
        FieldType::Tuple(_, field_types, ..) | FieldType::Union(_, field_types, ..) => {
            for field_type in field_types {
                validate_type_allowed(ctx, field_type);
            }
        }
    }
}

fn validate_type_constraints(ctx: &mut Context<'_>, field_type: &FieldType) {
    let constraint_attrs = field_type
        .attributes()
        .iter()
        .filter(|attr| ["assert", "check"].contains(&attr.name.name()))
        .collect::<Vec<_>>();
    for Attribute {
        arguments,
        span,
        name,
        ..
    } in constraint_attrs.iter()
    {
        let arg_expressions = arguments
            .arguments
            .iter()
            .map(|Argument { value, .. }| value)
            .collect::<Vec<_>>();

        match arg_expressions.as_slice() {
            [Expression::Identifier(Identifier::Local(s, _)), Expression::JinjaExpressionValue(expr, span)] =>
            {
                let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
                    internal_baml_jinja_types::JinjaContext::Parsing,
                );
                defined_types.add_variable("this", internal_baml_jinja_types::Type::Unknown);
                match internal_baml_jinja_types::validate_expression(&expr.0, &mut defined_types) {
                    Ok(_) => {}
                    Err(e) => {
                        if let Some(e) = e.parsing_errors {
                            let range = match e.range() {
                                Some(range) => range,
                                None => {
                                    ctx.push_error(DatamodelError::new_validation_error(
                                        &format!("Error parsing jinja template: {e}"),
                                        span.clone(),
                                    ));
                                    continue;
                                }
                            };

                            let start_offset = span.start + range.start;
                            let end_offset = span.start + range.end;

                            let span = Span::new(span.file.clone(), start_offset, end_offset);

                            ctx.push_error(DatamodelError::new_validation_error(
                                &format!("Error parsing jinja template: {e}"),
                                span,
                            ))
                        } else {
                            e.errors.iter().for_each(|t| {
                                let tspan = t.span();
                                let span = Span::new(
                                    span.file.clone(),
                                    span.start + tspan.start_offset as usize,
                                    span.start + tspan.end_offset as usize,
                                );
                                ctx.push_warning(DatamodelWarning::new(
                                    t.message().to_string(),
                                    span,
                                ))
                            })
                        }
                    }
                }
            }
            [Expression::JinjaExpressionValue(expr, span)] => {
                let mut defined_types = internal_baml_jinja_types::PredefinedTypes::default(
                    internal_baml_jinja_types::JinjaContext::Parsing,
                );
                defined_types.add_variable("this", internal_baml_jinja_types::Type::Unknown);
                match internal_baml_jinja_types::validate_expression(&expr.0, &mut defined_types) {
                    Ok(_) => {}
                    Err(e) => {
                        if let Some(e) = e.parsing_errors {
                            let range = match e.range() {
                                Some(range) => range,
                                None => {
                                    ctx.push_error(DatamodelError::new_validation_error(
                                        &format!("Error parsing jinja template: {e}"),
                                        span.clone(),
                                    ));
                                    continue;
                                }
                            };

                            let start_offset = span.start + range.start;
                            let end_offset = span.start + range.end;

                            let span = Span::new(span.file.clone(), start_offset, end_offset);

                            ctx.push_error(DatamodelError::new_validation_error(
                                &format!("Error parsing jinja template: {e}"),
                                span,
                            ))
                        } else {
                            e.errors.iter().for_each(|t| {
                                let tspan = t.span();
                                let span = Span::new(
                                    span.file.clone(),
                                    span.start + tspan.start_offset as usize,
                                    span.start + tspan.end_offset as usize,
                                );
                                ctx.push_warning(DatamodelWarning::new(
                                    t.message().to_string(),
                                    span,
                                ))
                            })
                        }
                    }
                }

                if name.to_string() == "check" {
                    ctx.push_error(DatamodelError::new_validation_error(
                        r#"
Check constraints must have a valid identifier. e.g.:
@check( valid_name_length, {{ this|length > 0 }} )
                        "#,
                        span.clone(),
                    ))
                }
            }
            _ => {
                ctx.push_error(DatamodelError::new_validation_error(
                    r#"
A check or assert has an optional identifier and a Jinja argument. e.g.:
@check( valid_name_length, {{ this|length > 0 }} )
@assert( {{ this|length > 0 }} )
@assert( valid_name_length, {{ this|length > 0 }} )
                        "#,
                    span.clone(),
                ));
            }
        }
    }
}
