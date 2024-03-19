use super::{
    helpers::{parsing_catch_all, Pair},
    parse_expression::parse_expression,
    Rule,
};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use crate::{
    assert_correct_parser,
    ast::{Adapter, FieldType},
    parser::parse_types::parse_field_type,
};

pub fn parse_adapter(
    pair: Pair<'_>,
    _doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Option<Adapter> {
    assert_correct_parser!(pair, Rule::adapter_block);

    let mut types = None;
    let mut expr = None;

    let mut span = diagnostics.span(pair.as_span());

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::field_template_args => {
                span = diagnostics.span(item.as_span());
                types = parse_field_template_args(item, diagnostics);
            }
            Rule::expression => {
                expr = Some(parse_expression(item, diagnostics));
            }
            Rule::BLOCK_LEVEL_CATCH_ALL => {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "This line is not a valid field or attribute definition.",
                    diagnostics.span(item.as_span()),
                ))
            }
            _ => parsing_catch_all(&item, "model"),
        }
    }

    match (types, expr) {
        (Some((from, to)), Some(expr)) => {
            if from == to {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "The adapter's input and output types must be different.",
                    span,
                ));
                return None;
            }
            Some(Adapter {
                from,
                to,
                converter: expr,
                span,
            })
        }
        (None, _) => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                "An adapter must have 2 template args.",
                span,
            ));
            None
        }
        (_, None) => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                "An adapter must have an expression.",
                span,
            ));
            None
        }
    }
}

fn parse_field_template_args(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<(FieldType, FieldType)> {
    assert_correct_parser!(pair, Rule::field_template_args);

    let mut fields = vec![];
    let span = diagnostics.span(pair.as_span());

    for item in pair.into_inner() {
        match item.as_rule() {
            Rule::field_type => {
                if let Some(f) = parse_field_type(item, diagnostics) {
                    fields.push(f)
                }
            }
            Rule::BLOCK_LEVEL_CATCH_ALL => {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "This line is not a valid field or attribute definition.",
                    diagnostics.span(item.as_span()),
                ))
            }
            _ => parsing_catch_all(&item, "model"),
        }
    }

    match fields.len() {
        2 => Some((fields[0].clone(), fields[1].clone())),
        other => {
            diagnostics.push_error(DatamodelError::new_validation_error(
                &format!("An adapter only has 2 template args, but found {}.", other),
                span,
            ));
            None
        }
    }
}
