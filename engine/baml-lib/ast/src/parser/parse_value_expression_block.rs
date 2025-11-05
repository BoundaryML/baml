use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_field::parse_value_expr,
    parse_identifier::parse_identifier,
    parse_named_args_list::{parse_function_arg, parse_named_argument_list},
    parse_type_builder_block::parse_type_builder_block,
    Rule,
};
use crate::ast::*;

pub(crate) fn parse_value_expression_block(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<ValueExprBlock, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut input = None;
    let mut output = None;
    let mut type_builder = None;
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
                _ => {
                    diagnostics.push_error(DatamodelError::new_parser_error(
                        format!("Unexpected value expression keyword: {}", current.as_str()),
                        diagnostics.span(current.as_span()),
                    ));
                }
            },
            Rule::ARROW => has_arrow = true,
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::named_argument_list => {
                input = Some(parse_named_argument_list(current, diagnostics))
            }
            Rule::field_type | Rule::field_type_chain => {
                match parse_function_arg(current, false, diagnostics) {
                    Ok(arg) => output = Some(arg),
                    Err(err) => diagnostics.push_error(err),
                }
            }
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}

            Rule::value_expression_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::value_expression => {
                            match parse_value_expr(
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
                            ) {
                                Ok(parsed_value) => {
                                    // if parsed_value.name() == "client" {
                                    //     client = true;
                                    // } else if parsed_value.name() == "prompt" {
                                    //     prompt = true;
                                    // }
                                    fields.push(parsed_value);
                                }
                                Err(err) => diagnostics.push_error(err),
                            }

                            pending_field_comment = None;
                        }

                        Rule::type_builder_block => {
                            let block = parse_type_builder_block(item, diagnostics)?;

                            match type_builder {
                                None => type_builder = Some(block),

                                Some(_) => diagnostics.push_error(DatamodelError::new_validation_error(
                                    "Definition of multiple `type_builder` blocks in the same parent block",
                                    block.span
                                )),
                            }
                        }

                        Rule::comment_block => pending_field_comment = Some(item),
                        // Ignore markdown headers inside value expression blocks.
                        // Top-level headers are handled in parse.rs and bound as annotations.
                        Rule::block_attribute => {
                            let span = item.as_span();
                            let attribute = parse_attribute(item, false, diagnostics);
                            let value_is_test = sub_type == Some(ValueExprBlockType::Test);
                            let attribute_name = attribute.name.to_string();
                            let attribute_is_constraint = &attribute_name == "check" || &attribute_name == "assert";

                            // Only tests may have block attributes, and the only valid block attributes
                            // are checks/asserts.
                            if value_is_test && attribute_is_constraint {
                                // value_expression_block is compatible with the attribute
                                attributes.push(attribute);
                            } else if !value_is_test {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    "Only Tests may contain block-level attributes",
                                    diagnostics.span(span),
                                ))
                            } else {
                                diagnostics.push_error(DatamodelError::new_validation_error(
                                    "Tests may only contain 'check' or 'assert' attributes",
                                    diagnostics.span(span),
                                ))
                            }
                        }
                        Rule::empty_lines => {}
                        Rule::stmt => {
                            // Statements are allowed in expression functions that got parsed as value_expression_block.
                            // They will be handled during HIR lowering when we distinguish between
                            // LLM functions (with client/prompt) and expression functions (with code).
                            // For now, just ignore them during parsing.
                        }
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not a valid field or attribute definition. A valid property may look like: 'myProperty \"some value\"' for example, with no colons.",
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(item, "model", diagnostics),
                    }
                }
            }
            _ => parsing_catch_all(current, "function", diagnostics),
        }
    }

    // Block has no name. Functions, test, clients and generators have names.
    let Some(name) = name else {
        return Err(value_expr_block_syntax_error(
            "Invalid syntax: missing name.",
            None,
            diagnostics.span(pair_span),
        ));
    };

    // Only test blocks can have `type_builder` blocks in them. This is not a
    // "syntax" error so we won't fail yet.
    if let Some(ref t) = type_builder {
        if sub_type != Some(ValueExprBlockType::Test) {
            diagnostics.push_error(DatamodelError::new_validation_error(
                "Only tests may have a type_builder block.",
                t.span.to_owned(),
            ));
        }
    };

    // No arrow means it's not a function. If it's a function then check params
    // and return type. If any of the conditions are met then we're ok.
    if !has_arrow || (input.is_some() && output.is_some()) {
        return Ok(ValueExprBlock {
            name,
            input,
            output,
            attributes,
            fields,
            documentation: doc_comment.and_then(|c| parse_comment_block(c, diagnostics)),
            span: diagnostics.span(pair_span),
            type_builder,
            block_type: sub_type.unwrap_or(ValueExprBlockType::Function),
            annotations: vec![],
        });
    }

    // If we reach this code, we're dealing with a malformed function.
    let message = match (input, output) {
        (Some(_), None) => "No return type specified.",
        (None, Some(_)) => "No input parameters specified.",
        _ => "Invalid syntax: missing input parameters and return type.",
    };

    Err(value_expr_block_syntax_error(
        message,
        Some(name.name()),
        diagnostics.span(pair_span),
    ))
}

fn value_expr_block_syntax_error(message: &str, name: Option<&str>, span: Span) -> DatamodelError {
    let function_name = name.unwrap_or("MyFunction");

    // TODO: Different block types (test, client, generator).
    let correct_syntax = format!(
        r##"{message} Valid function syntax is
```
function {function_name}(param1: String, param2: String) -> ReturnType {{
    client SomeClient
    prompt #"..."#
}}
```"##
    );

    DatamodelError::new_model_validation_error(
        &correct_syntax,
        "value expression",
        name.unwrap_or("<unknown>"),
        span,
    )
}
