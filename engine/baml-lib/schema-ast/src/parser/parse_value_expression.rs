use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_identifier::parse_identifier,
    parse_named_args_list::{parse_function_arg, parse_named_arguement_list},
    Rule,
};

use crate::{assert_correct_parser, ast::*, parser::parse_types::parse_field_type};
use internal_baml_diagnostics::{DatamodelError, Diagnostics}; // Add this line

pub(crate) fn parse_value_expression(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<ValueExp, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut input = None;
    let mut output = None;
    let mut fields = vec![];
    let mut has_arrow = false;
    let mut sub_type: Option<SubValueExp> = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::FUNCTION_KEYWORD => sub_type = Some(SubValueExp::Function),
            Rule::TEST_KEYWORD => sub_type = Some(SubValueExp::Test),
            Rule::CLIENT_KEYWORD => sub_type = Some(SubValueExp::Client),
            Rule::RETRY_POLICY_KEYWORD => sub_type = Some(SubValueExp::RetryPolicy),
            Rule::GENERATOR_KEYWORD => sub_type = Some(SubValueExp::Generator),
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::ARROW => {
                has_arrow = true;
            }
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::named_argument_list => match parse_named_arguement_list(current, diagnostics) {
                Ok(BlockArgs::Named(arg)) => input = Some(BlockArgs::Named(arg)),
                Ok(BlockArgs::Unnamed(arg)) => {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "Unnamed arguments are not supported for function input. Define a new class instead.",
                        arg.span,
                    ))
                }
                Err(err) => diagnostics.push_error(err),
            },
            Rule::field_type => {
              match parse_function_arg(current, diagnostics) {
                Ok(arg) => output = Some(BlockArgs::Unnamed(arg)),
                Err(err) => diagnostics.push_error(err),
              }
            }
            Rule::function_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::key_value => {
                            fields.push(parse_key_value(
                                item,
                                pending_field_comment.take(),
                                diagnostics,
                            ));
                        }
                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not a valid field or attribute definition.",
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(&item, "model"),
                    }
                }
            }
            _ => parsing_catch_all(&current, "function"),
        }
    }

    let response = match name {
        Some(name) => {
            let msg = match (input, output) {
                (Some(input), Some(output)) => {
                    if has_arrow {
                        return Ok(ValueExp {
                            name,
                            input: Some(input),
                            output: Some(output),
                            attributes,
                            fields,
                            documentation: doc_comment.and_then(parse_comment_block),
                            span: diagnostics.span(pair_span),
                            sub_type: sub_type.unwrap_or(SubValueExp::Function), // Unwrap or provide a default
                        });
                    } else {
                        "Invalid function syntax."
                    }
                }
                (Some(_), None) => "No return type specified.",
                (None, Some(_)) => "No input parameters specified.",
                _ => "Invalid function syntax.",
            };
            (msg, Some(name.name().to_string()))
        }
        None => ("Invalid function syntax.", None),
    };

    Err(DatamodelError::new_model_validation_error(
        format!(
            r##"{} Valid function syntax is
```
function {}(param1: String, param2: String) -> ReturnType {{
    client SomeClient
    prompt #"..."#
}}
```"##,
            response.0,
            response.1.as_deref().unwrap_or("MyFunction")
        )
        .as_str(),
        "function",
        response.1.as_deref().unwrap_or("<unknown>"),
        diagnostics.span(pair_span),
    ))
}
pub(crate) fn parse_key_value(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> ConfigBlockProperty {
    assert_correct_parser!(pair, Rule::key_value);

    let mut name: Option<Identifier> = None;
    let mut value: Option<Expression> = None;
    let mut attributes = Vec::new();
    let mut comment: Option<Comment> = doc_comment.and_then(parse_comment_block);
    let mut template_args = None;
    let (pair_span, pair_str) = (pair.as_span(), pair.as_str());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => {
                name = Some(parse_identifier(current, diagnostics));
            }
            Rule::field_attribute => attributes.push(parse_attribute(current, diagnostics)),
            Rule::expression => value = Some(parse_expression(current, diagnostics)),
            Rule::trailing_comment => {
                comment = match (comment, parse_trailing_comment(current)) {
                    (c, None) | (None, c) => c,
                    (Some(existing), Some(new)) => Some(Comment {
                        text: [existing.text, new.text].join("\n"),
                    }),
                };
            }
            Rule::template_args => {
                template_args = parse_template_args(current, diagnostics);
            }
            _ => unreachable_rule!(current, Rule::key_value),
        }
    }

    match name {
        Some(name) => ConfigBlockProperty {
            name,
            template_args,
            value,
            attributes,
            span: diagnostics.span(pair_span),
            documentation: comment,
        },
        _ => unreachable!(
            "Encountered impossible source property declaration during parsing: {:?}",
            pair_str,
        ),
    }
}
