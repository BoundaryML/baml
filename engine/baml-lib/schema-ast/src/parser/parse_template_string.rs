use super::{
    helpers::{parsing_catch_all, Pair},
    parse_comments::*,
    parse_expression::parse_raw_string,
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{assert_correct_parser, ast::*, parser::parse_types::parse_field_type};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_template_string(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<TemplateString, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let attributes: Vec<Attribute> = Vec::new();
    let mut input = None;
    let mut value = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::TEMPLATE_KEYWORD => {}
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
            Rule::raw_string_literal => {
                value = Some(Expression::RawStringValue(parse_raw_string(current, diagnostics)))
            }
            _ => parsing_catch_all(&current, "function"),
        }
    }

    let response = match name {
        Some(name) => {
            let msg = match value {
                Some(prompt) => {
                    return Ok(TemplateString {
                        name,
                        input,
                        value: prompt,
                        attributes,
                        documentation: doc_comment.and_then(parse_comment_block),
                        span: diagnostics.span(pair_span),
                    });
                }
                None => "Must have a prompt string.",
            };
            (msg, Some(name.name().to_string()))
        }
        None => ("Invalid template_string syntax.", None),
    };

    Err(DatamodelError::new_model_validation_error(
        format!(
            r##"{} Valid template_string syntax is
```
template_string {}(param1: String, param2: String) #"
    your template string here
"#
```"##,
            response.0,
            response.1.as_deref().unwrap_or("MyTemplateString")
        )
        .as_str(),
        "template_string",
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
