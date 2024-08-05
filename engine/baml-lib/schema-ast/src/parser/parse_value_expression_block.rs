use std::collections::hash_map;

use super::{
    helpers::{parsing_catch_all, Pair},
    parse_comments::*,
    parse_field::{self, parse_expr_as_value},
    parse_identifier::parse_identifier,
    parse_named_args_list::{parse_function_arg, parse_named_argument_list},
    Rule,
};

use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics}; // Add this line

pub(crate) fn parse_value_expression_block(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<ValueExprBlock, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let attributes: Vec<Attribute> = Vec::new();
    let mut input = None;
    let mut output = None;
    let mut fields: Vec<Field<Expression>> = vec![];
    let mut sub_type: Option<ValueExprBlockType> = None;
    let mut has_arrow = false;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::value_expression_keyword => match current.as_str() {
                "function" => sub_type = Some(ValueExprBlockType::Function),
                "test" => sub_type = Some(ValueExprBlockType::Test),
                "client" | "client<llm>" => sub_type = Some(ValueExprBlockType::Client),
                "retry_policy" => sub_type = Some(ValueExprBlockType::RetryPolicy),
                "generator" => sub_type = Some(ValueExprBlockType::Generator),
                _ => panic!("Unexpected value expression keyword: {}", current.as_str()),
            },
            Rule::ARROW => {
                has_arrow = true;
            }
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::named_argument_list => match parse_named_argument_list(current, diagnostics) {
                Ok(arg) => input = Some(arg),
                Err(err) => diagnostics.push_error(err),
            },
            Rule::field_type => match parse_function_arg(current, diagnostics) {
                Ok(arg) => output = Some(arg),
                Err(err) => diagnostics.push_error(err),
            },
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}

            Rule::value_expression_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::value_expression => {
                            fields.push(parse_expr_as_value(
                                &name,
                                sub_type
                                    .clone()
                                    .map(|st| match st {
                                        ValueExprBlockType::Function => "Function",
                                        ValueExprBlockType::Test => "Test",
                                        ValueExprBlockType::Client => "Client",
                                        ValueExprBlockType::RetryPolicy => "RetryPolicy",
                                        ValueExprBlockType::Generator => "Generator",
                                    })
                                    .unwrap_or("Other"),
                                item,
                                pending_field_comment.take(),
                                diagnostics,
                            )?);

                            pending_field_comment = None;
                        }

                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::empty_lines => {}
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not a valid field or attribute definition.",
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(item, "model"),
                    }
                }
            }
            _ => parsing_catch_all(current, "function"),
        }
    }

    log::info!("Input = {:?}", input);

    let response = match name {
        Some(name) => {
            let msg = match (input.is_some(), output.is_some()) {
                (true, true) => {
                    if has_arrow {
                        return Ok(ValueExprBlock {
                            name,
                            input,
                            output,
                            attributes,
                            fields,
                            documentation: doc_comment.and_then(parse_comment_block),
                            span: diagnostics.span(pair_span),
                            block_type: sub_type.unwrap_or(ValueExprBlockType::Function), // Unwrap or provide a default
                        });
                    } else {
                        "Invalid syntax: missing arrow for return type."
                    }
                }
                (true, false) => "No return type specified.",
                (false, true) => "No input parameters specified.",
                _ => "Invalid syntax: missing input parameters and return type.",
            };
            (msg, Some(name.name().to_string()))
        }
        None => ("Invalid syntax: missing name.", None),
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
