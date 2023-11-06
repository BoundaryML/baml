use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{assert_correct_parser, ast::*, parser::parse_types::parse_field_type};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_function(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<Function, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut input = None;
    let mut output = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::FUNCTION_KEYWORD | Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::identifier => name = Some(parse_identifier(current.into(), diagnostics)),
            Rule::function_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::output_field_declaration => {
                            if output.is_some() {
                                diagnostics.push_error(DatamodelError::new_duplicate_field_error(
                                    "<unknown>",
                                    "output",
                                    "function",
                                    diagnostics.span(pair_span),
                                ))
                            } else {
                                match parse_function_field_type(item, pending_field_comment.take(), diagnostics) {
                                    Ok(FunctionArgs::Named(arg)) => {
                                        diagnostics.push_error(DatamodelError::new_validation_error(
                                            "Named arguments are not supported for function output. Define a new class instead.",
                                            arg.span,
                                        ))
                                    },
                                    Ok(FunctionArgs::Unnamed(arg)) => output = Some(FunctionArgs::Unnamed(arg)),
                                    Err(err) => diagnostics.push_error(err),
                                }
                            }
                        }
                        Rule::input_field_declaration => {
                            if input.is_some() {
                                diagnostics.push_error(DatamodelError::new_duplicate_field_error(
                                    "<unknown>",
                                    "input",
                                    "function",
                                    diagnostics.span(pair_span),
                                ))
                            } else {
                                match parse_function_field_type(
                                    item,
                                    pending_field_comment.take(),
                                    diagnostics,
                                ) {
                                    Ok(out) => input = Some(out),
                                    Err(err) => diagnostics.push_error(err),
                                }
                            }
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
            _ => parsing_catch_all(&current, "model"),
        }
    }

    match (name, input, output) {
        (Some(name), Some(input), Some(output)) => Ok(Function {
            name,
            input,
            output,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: diagnostics.span(pair_span),
        }),
        (Some(name), _, _) => Err(DatamodelError::new_model_validation_error(
            "This function declaration is invalid. It is missing an input field.",
            "function",
            &name.name(),
            diagnostics.span(pair_span),
        )),
        _ => Err(DatamodelError::new_model_validation_error(
            "This function declaration is invalid. It is either missing a name or a type.",
            "function",
            "<unknown>",
            diagnostics.span(pair_span),
        )),
    }
}

fn parse_function_field_type(
    pair: Pair<'_>,
    block_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<FunctionArgs, DatamodelError> {
    assert!(
        pair.as_rule() == Rule::output_field_declaration
            || pair.as_rule() == Rule::input_field_declaration,
        "parse_function_field_type called on the wrong rule: {:?}",
        pair.as_rule()
    );
    let mut comment = block_comment.and_then(parse_comment_block);
    let span = diagnostics.span(pair.as_span());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::function_field_type => {
                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::field_type => {
                            return Ok(FunctionArgs::Unnamed(parse_function_arg(
                                item,
                                diagnostics,
                            )?));
                        }
                        Rule::trailing_comment => {
                            comment = match (comment, parse_trailing_comment(item)) {
                                (c, None) | (None, c) => c,
                                (Some(existing), Some(new)) => Some(Comment {
                                    text: [existing.text, new.text].join("\n"),
                                }),
                            };
                        }
                        Rule::named_argument_list => {
                            let mut args: Vec<(Identifier, FunctionArg)> = Vec::new();
                            for named_arg in item.into_inner() {
                                assert_correct_parser!(named_arg, Rule::named_argument);

                                let mut name = None;
                                let mut r#type = None;
                                for arg in named_arg.into_inner() {
                                    match arg.as_rule() {
                                        Rule::identifier => {
                                            name = Some(parse_identifier(arg, diagnostics));
                                        }
                                        Rule::field_type => {
                                            r#type = Some(parse_function_arg(arg, diagnostics)?);
                                        }
                                        _ => parsing_catch_all(&arg, "named_argument_list"),
                                    }
                                }

                                match (name, r#type) {
                                    (Some(name), Some(r#type)) => args.push((name, r#type)),
                                    (Some(name), None) => diagnostics.push_error(
                                        DatamodelError::new_validation_error(
                                            &format!(
                                                "No type specified for argument: {}",
                                                name.name()
                                            ),
                                            name.span().clone(),
                                        ),
                                    ),
                                    (None, _) => {
                                        unreachable!("parse_function_field_type: unexpected rule:")
                                    }
                                }
                            }
                            return Ok(FunctionArgs::Named(NamedFunctionArgList {
                                documentation: comment,
                                args,
                                span,
                            }));
                        }
                        _ => unreachable!(
                            "parse_function_field_type: unexpected rule: {:?}",
                            item.as_rule()
                        ),
                    }
                }
            }
            _ => unreachable!(
                "parse_function_field_type: unexpected rule: {:?}",
                current.as_rule()
            ),
        }
    }
    panic!("parse_function_field_type: missing function_field_type")
}

fn parse_function_arg(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<FunctionArg, DatamodelError> {
    assert!(
        pair.as_rule() == Rule::field_type,
        "parse_function_arg called on the wrong rule: {:?}",
        pair.as_rule()
    );
    let span = diagnostics.span(pair.as_span());

    match parse_field_type(pair, diagnostics) {
        Some(ftype) => Ok(FunctionArg {
            span,
            field_type: ftype,
        }),
        None => Err(DatamodelError::new_validation_error(
            "Failed to find type",
            span,
        )),
    }
}
