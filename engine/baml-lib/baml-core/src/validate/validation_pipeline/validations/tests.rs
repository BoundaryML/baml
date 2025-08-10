use baml_types::{Constraint, ConstraintLevel};
use internal_baml_ast::ast::WithName;
use internal_baml_diagnostics::{DatamodelError, DatamodelWarning, Span};
use internal_baml_jinja_types::{validate_expression, JinjaContext, PredefinedTypes, Type};

use crate::validate::validation_pipeline::context::Context;

pub(super) fn validate(ctx: &mut Context<'_>) {
    let tests = ctx.db.walk_test_cases().collect::<Vec<_>>();
    tests.iter().for_each(|walker| {
        // Validate that test fields don't have @assert or @check attributes
        let test_ast = walker.ast_node();
        for (_field_id, field) in test_ast.iter_fields() {
            for attr in &field.attributes {
                if attr.name.name() == "assert" || attr.name.name() == "check" {
                    ctx.push_error(DatamodelError::new_validation_error(
                        &format!(
                            "@{} is not allowed on test fields. Use @@{} at the test block level instead.",
                            attr.name.name(),
                            attr.name.name()
                        ),
                        attr.span.clone(),
                    ));
                }
            }
        }

        let constraints = &walker.test_case().constraints;
        let args = &walker.test_case().args;
        let mut check_names: Vec<String> = Vec::new();
        for (
            Constraint {
                label,
                level,
                expression,
            },
            constraint_span,
            expr_span,
        ) in constraints.iter()
        {
            let mut defined_types = PredefinedTypes::default(JinjaContext::Parsing);
            defined_types.add_variable("this", Type::Unknown);
            defined_types.add_class(
                "Checks",
                check_names
                    .iter()
                    .map(|check_name| (check_name.clone(), Type::Unknown))
                    .collect(),
            );
            defined_types.add_class(
                "_",
                vec![
                    ("checks".to_string(), Type::ClassRef("Checks".to_string())),
                    ("result".to_string(), Type::Unknown),
                    ("latency_ms".to_string(), Type::Number),
                ]
                .into_iter()
                .collect(),
            );
            defined_types.add_variable("_", Type::ClassRef("_".to_string()));
            args.keys()
                .for_each(|arg_name| defined_types.add_variable(arg_name, Type::Unknown));
            if let (ConstraintLevel::Check, Some(check_name)) = (level, label) {
                check_names.push(check_name.to_string());
            }
            match validate_expression(expression.0.as_str(), &mut defined_types) {
                Ok(_) => {}
                Err(e) => {
                    if let Some(e) = e.parsing_errors {
                        let range = match e.range() {
                            Some(range) => range,
                            None => {
                                ctx.push_error(DatamodelError::new_validation_error(
                                    &format!("Error parsing jinja template: {e}"),
                                    expr_span.clone(),
                                ));
                                continue;
                            }
                        };

                        let start_offset = expr_span.start + range.start;
                        let end_offset = expr_span.start + range.end;

                        let span = Span::new(expr_span.file.clone(), start_offset, end_offset);

                        ctx.push_error(DatamodelError::new_validation_error(
                            &format!("Error parsing jinja template: {e}"),
                            span,
                        ))
                    } else {
                        e.errors.iter().for_each(|t| {
                            let tspan = t.span();
                            let span = Span::new(
                                expr_span.file.clone(),
                                expr_span.start + tspan.start_offset as usize,
                                expr_span.start + tspan.end_offset as usize,
                            );
                            ctx.push_warning(DatamodelWarning::new(t.message().to_string(), span))
                        })
                    }
                }
            }
        }
    });
}
