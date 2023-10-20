use super::{
    helpers::{parsing_catch_all, Pair},
    parse_expression::parse_expression,
    Rule,
};
use crate::ast::{self, Identifier};
use internal_baml_diagnostics::{DatamodelError, Diagnostics, Span};

pub(crate) fn parse_arguments_list(
    token: Pair<'_>,
    arguments: &mut ast::ArgumentsList,
    args_for: &Option<Identifier>,
    diagnostics: &mut Diagnostics,
) {
    debug_assert_eq!(token.as_rule(), Rule::arguments_list);
    for current in token.into_inner() {
        let current_span = current.as_span();
        match current.as_rule() {
            // At the top level only unnamed args are supported.
            // For multiple args, pass in a dictionary.
            Rule::expression => match parse_expression(current, diagnostics) {
                ast::Expression::Map(values, span) => {
                    for (key, value) in values {
                        let start = value.span().start;
                        let name_span = key.span().clone();
                        let arg_name = match key {
                            ast::Expression::StringValue(value, _) => Identifier {
                                path: None,
                                name: value,
                                span: name_span,
                            },
                            ast::Expression::ConstantValue(value, _)
                                if !key.is_env_expression() =>
                            {
                                Identifier {
                                    path: None,
                                    name: value,
                                    span: name_span,
                                }
                            }
                            _ => {
                                diagnostics.push_error(
                                    DatamodelError::new_attribute_validation_error(
                                        "Only string keys are supported in attribute arguments.",
                                        &args_for.as_ref().unwrap().name,
                                        diagnostics.span(current_span),
                                    ),
                                );
                                continue;
                            }
                        };
                        let end = value.span().end;
                        arguments.arguments.push(ast::Argument {
                            name: Some(arg_name),
                            value,
                            span: Span {
                                file: span.file.clone(),
                                start,
                                end,
                            },
                        })
                    }
                }
                arg => arguments.arguments.push(ast::Argument {
                    name: None,
                    value: arg,
                    span: diagnostics.span(current_span),
                }),
            },
            _ => parsing_catch_all(&current, "attribute arguments"),
        }
    }
}
