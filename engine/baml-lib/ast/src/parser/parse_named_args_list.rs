use internal_baml_diagnostics::DatamodelError; // Add this line
use internal_baml_diagnostics::{Diagnostics, Span};

use super::{
    helpers::{assert_correct_parser, parsing_catch_all, Pair},
    parse_field::parse_field_type_chain,
    parse_identifier::parse_identifier,
};
use crate::{
    ast::{BlockArg, BlockArgs, FieldArity, FieldType, Identifier, WithName, WithSpan},
    parser::Rule,
};

pub(crate) fn parse_named_argument_list(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> BlockArgs {
    assert!(
        pair.as_rule() == Rule::named_argument_list,
        "parse_named_argument_list called on the wrong rule: {:?}",
        pair.as_rule()
    );
    let span = diagnostics.span(pair.as_span());
    let mut args: Vec<(Identifier, BlockArg)> = Vec::new();
    for named_arg in pair.into_inner() {
        if matches!(named_arg.as_rule(), Rule::SPACER_TEXT) {
            continue;
        }
        if named_arg.as_rule() == Rule::named_argument || named_arg.as_rule() == Rule::openParen {
            // TODO: THIS IS SUSPECT
            assert_correct_parser(&named_arg, &[named_arg.as_rule()], diagnostics);
        }
        // TODO: THIS IS SUSPECT
        // assert_correct_parser!(named_arg, Rule::named_argument);

        if named_arg.as_rule() == Rule::openParen || named_arg.as_rule() == Rule::closeParen {
            continue;
        }

        let mut name = None;
        let mut r#type = None;
        let is_mutable = true; // Always mutable now after mut keyword removal
        let mut is_self = false;
        for arg in named_arg.into_inner() {
            match arg.as_rule() {
                Rule::identifier => {
                    let ident = parse_identifier(arg, diagnostics);

                    if ident.name() == "self" {
                        is_self = true;
                    }

                    name = Some(ident);
                }
                Rule::COLON => {}
                Rule::field_type | Rule::field_type_chain => {
                    match parse_function_arg(arg, is_mutable, diagnostics) {
                        Ok(t) => r#type = Some(t),
                        Err(e) => diagnostics.push_error(e),
                    }
                }
                _ => parsing_catch_all(arg, "named_argument_list", diagnostics),
            }
        }

        if is_self {
            if !args.is_empty() {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "self must be the first parameter",
                    name.as_ref()
                        .map(|ident| ident.span().to_owned())
                        .unwrap_or(Span::fake()),
                ));
            }

            r#type = Some(BlockArg {
                is_mutable,
                is_self,
                span: name
                    .as_ref()
                    .map(|ident| ident.span().to_owned())
                    .unwrap_or(Span::fake()),
                field_type: FieldType::Symbol(
                    FieldArity::Required,
                    Identifier::Local("Self".to_string(), Span::fake()),
                    None,
                ),
            });
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
                unreachable!("parse_named_args_list:, none for name of field/missing type")
            }
        }
    }

    BlockArgs {
        documentation: None,
        args,
        span,
    }
}

pub fn parse_function_arg(
    pair: Pair<'_>,
    is_mutable: bool,
    diagnostics: &mut Diagnostics,
) -> Result<BlockArg, DatamodelError> {
    assert!(
        [Rule::field_type, Rule::field_type_chain].contains(&pair.as_rule()),
        "parse_function_arg called on the wrong rule: {:?}",
        pair.as_rule()
    );
    let span = diagnostics.span(pair.as_span());

    match parse_field_type_chain(pair, diagnostics) {
        Some(ftype) => Ok(BlockArg {
            is_mutable,
            is_self: false, // Handled in parse_named_argument_list
            span,
            field_type: ftype,
        }),
        None => Err(DatamodelError::new_validation_error(
            "Failed to find type",
            span,
        )),
    }
}
