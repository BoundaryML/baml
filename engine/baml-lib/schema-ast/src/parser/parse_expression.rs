use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{assert_correct_parser, ast::*, unreachable_rule};
use internal_baml_diagnostics::Diagnostics;

pub(crate) fn parse_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<Expression> {
    let first_child = token.into_inner().next().unwrap();
    let span = diagnostics.span(first_child.as_span());
    match first_child.as_rule() {
        Rule::numeric_literal => Some(Expression::NumericValue(first_child.as_str().into(), span)),
        Rule::string_literal => Some(parse_string_literal(first_child, diagnostics)),
        Rule::map_expression => Some(parse_map(first_child, diagnostics)),
        Rule::array_expression => Some(parse_array(first_child, diagnostics)),

        Rule::identifier => Some(Expression::Identifier(parse_identifier(
            first_child,
            diagnostics,
        ))),

        Rule::BLOCK_LEVEL_CATCH_ALL => {
            diagnostics.push_error(
                internal_baml_diagnostics::DatamodelError::new_validation_error(
                    "This is not a valid expression.",
                    span,
                ),
            );
            None
        }

        _ => unreachable_rule!(first_child, Rule::expression),
    }
}

fn parse_array(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    let mut elements: Vec<Expression> = vec![];
    let span = token.as_span();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::expression => {
                if let Some(expr) = parse_expression(current, diagnostics) {
                    elements.push(expr);
                }
            }
            _ => parsing_catch_all(current, "array"),
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
            Expression::StringValue(unescape_string(contents.as_str()), span)
        }
        Rule::unquoted_string_literal => {
            let raw_content = contents.as_str();
            // If the content starts or ends with a space, trim it
            let content = raw_content.trim().to_string();

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

fn parse_map(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    let mut entries: Vec<(Expression, Expression)> = vec![];
    let span = token.as_span();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::map_entry => {
                if let Some(f) = parse_map_entry(current, diagnostics) {
                    entries.push(f)
                }
            }
            Rule::BLOCK_LEVEL_CATCH_ALL => {}
            _ => parsing_catch_all(current, "map key value"),
        }
    }

    Expression::Map(entries, diagnostics.span(span))
}

fn parse_map_entry(
    token: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<(Expression, Expression)> {
    assert_correct_parser!(token, Rule::map_entry);

    let mut key = None;
    let mut value = None;
    let token_span = token.as_span(); // Store the span before moving token

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::map_key => key = Some(parse_map_key(current, diagnostics)),
            Rule::expression => value = parse_expression(current, diagnostics),
            Rule::ENTRY_CATCH_ALL => {
                diagnostics.push_error(
                    internal_baml_diagnostics::DatamodelError::new_validation_error(
                        "This map entry is missing a valid value or has an incorrect syntax.",
                        diagnostics.span(token_span), // Use the stored span here
                    ),
                );
                return None;
            }
            Rule::BLOCK_LEVEL_CATCH_ALL => {}
            _ => parsing_catch_all(current, "dict entry"),
        }
    }

    match (key, value) {
        (Some(key), Some(value)) => Some((key, value)),
        (Some(_), None) => {
            diagnostics.push_error(
                internal_baml_diagnostics::DatamodelError::new_validation_error(
                    "This map entry is missing a valid value or has an incorrect syntax.",
                    diagnostics.span(token_span), // Use the stored span here
                ),
            );
            None
        }
        _ => None,
    }
}

fn parse_map_key(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    assert_correct_parser!(token, Rule::map_key);

    let span = diagnostics.span(token.as_span());
    if let Some(current) = token.into_inner().next() {
        return match current.as_rule() {
            Rule::identifier => Expression::Identifier(parse_identifier(current, diagnostics)),
            Rule::quoted_string_literal => Expression::StringValue(
                current.into_inner().next().unwrap().as_str().to_string(),
                span,
            ),
            Rule::unquoted_string_literal => Expression::StringValue(
                current.into_inner().next().unwrap().as_str().to_string(),
                span,
            ),
            _ => unreachable_rule!(current, Rule::map_key),
        };
    }
    unreachable!("Encountered impossible map key during parsing")
}

pub(super) fn parse_raw_string(token: Pair<'_>, diagnostics: &mut Diagnostics) -> RawString {
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

fn unescape_string(val: &str) -> String {
    let mut result = String::with_capacity(val.len());
    let mut chars = val.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('0') => result.push('\0'),
                Some('\'') => result.push('\''),
                Some('\"') => result.push('\"'),
                Some('\\') => result.push('\\'),
                Some('x') => {
                    let mut hex = String::new();
                    hex.push(chars.next().unwrap());
                    hex.push(chars.next().unwrap());
                    result.push(u8::from_str_radix(&hex, 16).unwrap() as char);
                }
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }

    result
}
