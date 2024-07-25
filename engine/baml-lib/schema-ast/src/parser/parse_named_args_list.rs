use internal_baml_diagnostics::Diagnostics;

use crate::{
    assert_correct_parser,
    ast::{Identifier, RefIdentifier},
    parser::Rule,
    unreachable_rule,
};

use super::helpers::Pair;

pub fn parse_named_arguement_list(
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

pub fn parse_function_arg(
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
