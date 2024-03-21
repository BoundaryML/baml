use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{assert_correct_parser, ast::*, unreachable_rule};
use internal_baml_diagnostics::{DatamodelWarning, Diagnostics};

pub(crate) fn parse_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Expression {
    let first_child = token.into_inner().next().unwrap();
    let span = diagnostics.span(first_child.as_span());
    match first_child.as_rule() {
        Rule::numeric_literal => Expression::NumericValue(first_child.as_str().into(), span),
        Rule::string_literal => parse_string_literal(first_child, diagnostics),
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

fn parse_string_literal(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    assert_correct_parser!(token, Rule::string_literal);
    let contents = token.clone().into_inner().next().unwrap();
    let span = diagnostics.span(contents.as_span());
    match contents.as_rule() {
        Rule::raw_string_literal => {
            Expression::RawStringValue(parse_raw_string(contents, diagnostics))
        }
        Rule::quoted_string_literal => {
            let contents = contents.into_inner().next().unwrap();
            Expression::StringValue(contents.as_str().to_string(), span)
        }
        Rule::unquoted_string_literal => {
            let raw_content = contents.as_str();
            // If the content starts or ends with a space, trim it
            let content = raw_content.trim().to_string();
            // If its trimmed put a warning
            if content.len() != raw_content.len() {
                diagnostics.push_warning(DatamodelWarning::new(
                    "Trailing or leading whitespace trimmed. If you meant to include it, please wrap the string with \"...\"".into(),
                    span.clone(),
                ))
            }

            if content.contains(' ') {
                Expression::StringValue(content, span)
            } else {
                match Identifier::from((content.as_str(), span.clone())) {
                    Identifier::Invalid(..) | Identifier::String(..) => {
                        Expression::StringValue(content, span)
                    }
                    identifier => Expression::Identifier(identifier),
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
                if let Some(f) = parse_dict_entry(current, diagnostics) {
                    entries.push(f)
                }
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
    if let Some(current) = token.into_inner().next() {
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

fn parse_raw_string(token: Pair<'_>, diagnostics: &mut Diagnostics) -> RawString {
    assert_correct_parser!(token, Rule::raw_string_literal);

    let mut language = None;
    let mut content = None;

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::single_word => {
                let contents = current.as_str().to_string();
                language = Some((contents, diagnostics.span(current.as_span())));
            }
            Rule::raw_string_literal_content_1
            | Rule::raw_string_literal_content_2
            | Rule::raw_string_literal_content_3
            | Rule::raw_string_literal_content_4
            | Rule::raw_string_literal_content_5 => {
                content = Some((
                    current.as_str().to_string(),
                    diagnostics.span(current.as_span()),
                ));
            }
            _ => unreachable_rule!(current, Rule::raw_string_literal),
        };
    }
    match content {
        Some((content, span)) => RawString::new(content, span, language),
        _ => unreachable!("Encountered impossible raw string during parsing"),
    }
}
