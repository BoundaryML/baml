use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{assert_correct_parser, ast::*, unreachable_rule};
use either::Either;
use internal_baml_diagnostics::Diagnostics;

pub(crate) fn parse_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Expression {
    let first_child = token.into_inner().next().unwrap();
    let span = diagnostics.span(first_child.as_span());
    match first_child.as_rule() {
        Rule::numeric_literal => Expression::NumericValue(first_child.as_str().into(), span),
        Rule::string_literal => {
            let string_value = parse_string_literal(first_child, diagnostics);
            match string_value {
                Either::Left((string_value, span)) => Expression::StringValue(string_value, span),
                Either::Right(identifier) => Expression::Identifier(identifier),
            }
        }
        Rule::dict_expression => parse_dict(first_child, diagnostics),
        Rule::array_expression => parse_array(first_child, diagnostics),
        _ => unreachable_rule!(first_child, Rule::expression),
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

    Expression::Array(elements, diagnostics.span(span))
}

fn parse_string_literal(
    token: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Either<(String, Span), Identifier> {
    assert_correct_parser!(token, Rule::string_literal);
    let contents = token.clone().into_inner().next().unwrap();
    let span = diagnostics.span(contents.as_span());
    match contents.as_rule() {
        Rule::raw_string_literal => {
            let contents = contents.into_inner().next_back().unwrap();
            Either::Left((contents.as_str().to_string(), span))
        }
        Rule::quoted_string_literal => {
            let contents = contents.into_inner().next().unwrap();
            Either::Left((contents.as_str().to_string(), span))
        }
        Rule::unquoted_string_literal => {
            let content = contents.as_str().to_string();
            if content.contains(" ") {
                Either::Left((content, span))
            } else {
                match Identifier::from((content.as_str(), span.clone())) {
                    Identifier::Invalid(..) | Identifier::String(..) => {
                        Either::Left((content, span))
                    }
                    identifier => Either::Right(identifier),
                }
            }
        }
        _ => unreachable_rule!(contents, Rule::string_literal),
    }
}

fn parse_dict(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    let mut entries: Vec<(Expression, Expression)> = vec![];
    let span = token.as_span();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::dict_entry => {
                parse_dict_entry(current, diagnostics).map(|f| entries.push(f));
            }
            _ => parsing_catch_all(&current, "dictionary key value"),
        }
    }

    Expression::Map(entries, diagnostics.span(span))
}

fn parse_dict_entry(
    token: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<(Expression, Expression)> {
    assert_correct_parser!(token, Rule::dict_entry);

    let mut key = None;
    let mut value = None;

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::dict_key => key = Some(parse_dict_key(current, diagnostics)),
            Rule::expression => value = Some(parse_expression(current, diagnostics)),
            _ => parsing_catch_all(&current, "dict entry"),
        }
    }

    match (key, value) {
        (Some(key), Some(value)) => Some((key, value)),
        _ => None,
    }
}

fn parse_dict_key(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    assert_correct_parser!(token, Rule::dict_key);

    let span = diagnostics.span(token.as_span());
    for current in token.into_inner() {
        return match current.as_rule() {
            Rule::identifier => Expression::Identifier(parse_identifier(current, diagnostics)),
            Rule::quoted_string_literal => Expression::StringValue(
                current.into_inner().next().unwrap().as_str().to_string(),
                span,
            ),
            _ => unreachable_rule!(current, Rule::dict_key),
        };
    }
    unreachable!("Encountered impossible dict key during parsing")
}
