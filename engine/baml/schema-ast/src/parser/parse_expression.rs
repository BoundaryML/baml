use super::{
    helpers::{parsing_catch_all, Pair},
    Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Expression {
    let first_child = token.into_inner().next().unwrap();
    let span = Span::from(first_child.as_span());
    match first_child.as_rule() {
        Rule::numeric_literal => Expression::NumericValue(first_child.as_str().to_string(), span),
        Rule::string_literal => {
            Expression::StringValue(parse_string_literal(first_child, diagnostics), span)
        }
        Rule::identifier => Expression::ConstantValue(first_child.as_str().to_string(), span),
        Rule::dict_expression => parse_dict(first_child, diagnostics),
        Rule::array_expression => parse_array(first_child, diagnostics),
        _ => unreachable!(
            "Encountered impossible literal during parsing: {:?}",
            first_child.tokens()
        ),
    }
}

fn parse_array(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    let mut elements: Vec<Expression> = vec![];
    let span = token.as_span();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::expression => elements.push(parse_expression(current, diagnostics)),
            _ => parsing_catch_all(&current, "array"),
        }
    }

    Expression::Array(elements, Span::from(span))
}

fn parse_string_literal(token: Pair<'_>, diagnostics: &mut Diagnostics) -> String {
    assert!(token.as_rule() == Rule::string_literal);
    let contents = token.clone().into_inner().next().unwrap();
    let contents_str = contents.as_str();

    contents_str.to_string()
}

fn parse_dict(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    let mut entries: Vec<(Expression, Expression)> = vec![];
    let span = token.as_span();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::dict_entry => entries.push(parse_dict_entry(current, diagnostics)),
            _ => parsing_catch_all(&current, "dict"),
        }
    }

    Expression::Map(entries, Span::from(span))
}

fn parse_dict_entry(token: Pair<'_>, diagnostics: &mut Diagnostics) -> (Expression, Expression) {
    let mut inner = token.into_inner();
    let key = parse_expression(inner.next().unwrap(), diagnostics);
    let value = parse_expression(inner.next().unwrap(), diagnostics);

    (key, value)
}
