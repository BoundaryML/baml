use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_config::parse_key_value,
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
    let mut fields = vec![];
    let mut has_arrow = false;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::FUNCTION_KEYWORD | Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::ARROW => {
                has_arrow = true;
            }
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::named_argument_list => match parse_named_arguement_list(current, diagnostics) {
                Ok(FunctionArgs::Named(arg)) => input = Some(FunctionArgs::Named(arg)),
                Ok(FunctionArgs::Unnamed(arg)) => {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "Unnamed arguments are not supported for function input. Define a new class instead.",
                        arg.span,
                    ))
                }
                Err(err) => diagnostics.push_error(err),
            },
            Rule::field_type => {
              match parse_function_arg(current, diagnostics) {
                Ok(arg) => output = Some(FunctionArgs::Unnamed(arg)),
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
                        return Ok(Function {
                            name,
                            input,
                            output,
                            attributes,
                            fields,
                            documentation: doc_comment.and_then(parse_comment_block),
                            span: diagnostics.span(pair_span),
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

fn parse_named_arguement_list(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<FunctionArgs, DatamodelError> {
    assert!(
        pair.as_rule() == Rule::named_argument_list,
        "parse_named_arguement_list called on the wrong rule: {:?}",
        pair.as_rule()
    );
    let span = diagnostics.span(pair.as_span());
    let mut args: Vec<(Identifier, FunctionArg)> = Vec::new();
    for named_arg in pair.into_inner() {
        if matches!(named_arg.as_rule(), Rule::SPACER_TEXT) {
            continue;
        }
        assert_correct_parser!(named_arg, Rule::named_argument);

        let mut name = None;
        let mut r#type = None;
        for arg in named_arg.into_inner() {
            match arg.as_rule() {
                Rule::identifier => {
                    name = Some(parse_identifier(arg, diagnostics));
                }
                Rule::colon => {}
                Rule::field_type => {
                    r#type = Some(parse_function_arg(arg, diagnostics)?);
                }
                _ => parsing_catch_all(&arg, "named_argument_list"),
            }
        }

        match (name, r#type) {
            (Some(name), Some(r#type)) => args.push((name, r#type)),
            (Some(name), None) => diagnostics.push_error(DatamodelError::new_validation_error(
                &format!(
                    "No type specified for argument: {name}. Expected: `{name}: type`",
                    name = name.name()
                ),
                name.span().clone(),
            )),
            (None, _) => {
                unreachable!("parse_function_field_type: unexpected rule:")
            }
        }
    }

    Ok(FunctionArgs::Named(NamedFunctionArgList {
        documentation: None,
        args,
        span,
    }))
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
