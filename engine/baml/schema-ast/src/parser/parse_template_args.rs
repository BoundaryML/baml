use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier_string,
    Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_template_args(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<Vec<Expression>> {
    assert!(token.as_rule() == Rule::template_args);
    let mut template_args = Vec::new();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::empty_template_args => {
                return None;
            }
            Rule::template_arg => {
                template_args.push(parse_template_arg(current, diagnostics));
            }
            _ => parsing_catch_all(&current, "template args"),
        }
    }

    if template_args.is_empty() {
        return None;
    }

    Some(template_args)
}

fn parse_template_arg(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Expression {
    assert!(token.as_rule() == Rule::template_arg);
    let span = token.as_span();
    let mut inner = token.into_inner();

    match inner.peek().unwrap().as_rule() {
        Rule::quoted_string_content => Expression::ConstantValue(
            inner.next().unwrap().as_str().into(),
            diagnostics.span(span),
        ),
        Rule::single_word => Expression::ConstantValue(
            inner.next().unwrap().as_str().into(),
            diagnostics.span(span),
        ),
        _ => unreachable!(),
    }
}
