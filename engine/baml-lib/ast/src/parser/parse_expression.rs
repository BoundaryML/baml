use baml_types::JinjaExpression;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{
    helpers::{assert_correct_parser, parsing_catch_all, unreachable_rule, Pair},
    parse_expr::{
        parse_expr_block, parse_expr_fn, parse_fn_app, parse_generic_fn_app, parse_if_expression,
        parse_lambda,
    },
    parse_identifier::{parse_identifier, parse_path_identifier},
    Rule,
};
use crate::{
    ast::*,
    parser::parse_expr::{consume_if_rule, consume_span_if_rule},
};

pub(crate) fn parse_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<Expression> {
    use pest::pratt_parser::{Assoc, Op, PrattParser};

    assert_correct_parser(&token, &[Rule::expression], diagnostics);

    // TODO: Initialize this shit once and pass it in (consider parallel parsing with .par_iter(), use some sync once cell or something).
    let pratt = PrattParser::new()
        .op(Op::infix(Rule::OR, Assoc::Left))
        .op(Op::infix(Rule::AND, Assoc::Left))
        .op(Op::infix(Rule::EQ, Assoc::Left)
            | Op::infix(Rule::NEQ, Assoc::Left)
            | Op::infix(Rule::LT, Assoc::Left)
            | Op::infix(Rule::LTEQ, Assoc::Left)
            | Op::infix(Rule::GT, Assoc::Left)
            | Op::infix(Rule::GTEQ, Assoc::Left)
            | Op::infix(Rule::INSTANCE_OF, Assoc::Left))
        .op(Op::infix(Rule::BIT_OR, Assoc::Left))
        .op(Op::infix(Rule::BIT_XOR, Assoc::Left))
        .op(Op::infix(Rule::BIT_AND, Assoc::Left))
        .op(Op::infix(Rule::BIT_SHL, Assoc::Left) | Op::infix(Rule::BIT_SHR, Assoc::Left))
        .op(Op::infix(Rule::ADD, Assoc::Left) | Op::infix(Rule::SUB, Assoc::Left))
        .op(Op::infix(Rule::MUL, Assoc::Left)
            | Op::infix(Rule::DIV, Assoc::Left)
            | Op::infix(Rule::MOD, Assoc::Left))
        .op(Op::prefix(Rule::NOT))
        .op(Op::prefix(Rule::NEG))
        .op(Op::postfix(Rule::array_accessor))
        .op(Op::postfix(Rule::method_call))
        .op(Op::postfix(Rule::field_accessor));

    let span = diagnostics.span(token.as_span());

    let diagnostics_ptr: *mut internal_baml_diagnostics::Diagnostics = diagnostics;

    let mut parser = pratt
        .map_primary(|primary| {
            // Ah yes, Rust superiority.
            #[allow(unsafe_code)]
            let diagnostics = unsafe { &mut *diagnostics_ptr };

            match primary.as_rule() {
                Rule::expression => parse_expression(primary, diagnostics),
                _ => parse_primary_expression(primary.into_inner().next()?, diagnostics),
            }
        })
        .map_prefix(|operator, right| {
            let operator = match operator.as_rule() {
                Rule::NEG => UnaryOperator::Neg,
                Rule::NOT => UnaryOperator::Not,
                _ => unreachable!("Unexpected prefix operator: {:?}", operator.as_rule()),
            };

            right.map(|right| Expression::UnaryOperation {
                operator,
                expr: Box::new(right),
                span: span.clone(),
            })
        })
        .map_postfix(|left, operator| {
            let left = left?;

            Some(match operator.as_rule() {
                Rule::array_accessor => {
                    let index = parse_expression(operator.into_inner().next()?, diagnostics)?;

                    Expression::ArrayAccess(Box::new(left), Box::new(index), span.clone())
                }

                Rule::field_accessor => {
                    let field = parse_identifier(operator.into_inner().next()?, diagnostics);

                    Expression::FieldAccess(Box::new(left), field, span.clone())
                }

                Rule::method_call => {
                    let inner = operator.into_inner().next()?;

                    match inner.as_rule() {
                        Rule::fn_app => match parse_fn_app(inner, diagnostics)? {
                            Expression::App(fn_call) => Expression::MethodCall {
                                receiver: Box::new(left),
                                method: fn_call.name,
                                args: fn_call.args,
                                type_args: fn_call.type_args,
                                span: span.clone(),
                            },

                            _ => {
                                unreachable!("expected function call when parsing method call")
                            }
                        },

                        Rule::generic_fn_app => match parse_generic_fn_app(inner, diagnostics)? {
                            Expression::App(fn_call) => Expression::MethodCall {
                                receiver: Box::new(left),
                                method: fn_call.name,
                                args: fn_call.args,
                                type_args: fn_call.type_args,
                                span: span.clone(),
                            },

                            _ => {
                                unreachable!("expected function call when parsing method call")
                            }
                        },

                        _ => unreachable!("Unexpected method call rule: {:?}", inner.as_rule()),
                    }
                }
                _ => unreachable!("Unexpected postfix operator: {:?}", operator.as_rule()),
            })
        })
        .map_infix(|left, operator, right| {
            let operator = match operator.as_rule() {
                Rule::EQ => BinaryOperator::Eq,
                Rule::NEQ => BinaryOperator::Neq,
                Rule::LT => BinaryOperator::Lt,
                Rule::LTEQ => BinaryOperator::LtEq,
                Rule::GT => BinaryOperator::Gt,
                Rule::GTEQ => BinaryOperator::GtEq,
                Rule::ADD => BinaryOperator::Add,
                Rule::SUB => BinaryOperator::Sub,
                Rule::MUL => BinaryOperator::Mul,
                Rule::DIV => BinaryOperator::Div,
                Rule::MOD => BinaryOperator::Mod,
                Rule::BIT_AND => BinaryOperator::BitAnd,
                Rule::BIT_OR => BinaryOperator::BitOr,
                Rule::BIT_XOR => BinaryOperator::BitXor,
                Rule::BIT_SHL => BinaryOperator::Shl,
                Rule::BIT_SHR => BinaryOperator::Shr,
                Rule::OR => BinaryOperator::Or,
                Rule::AND => BinaryOperator::And,
                Rule::INSTANCE_OF => BinaryOperator::InstanceOf,
                _ => unreachable!("Unexpected infix operator: {:?}", operator.as_rule()),
            };

            Some(Expression::BinaryOperation {
                left: Box::new(left?),
                operator,
                right: Box::new(right?),
                span: span.clone(),
            })
        });

    parser.parse(token.into_inner())
}

fn parse_primary_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<Expression> {
    let span = diagnostics.span(token.as_span());
    match token.as_rule() {
        Rule::numeric_literal => Some(Expression::NumericValue(token.as_str().into(), span)),
        Rule::string_literal => Some(parse_string_literal(token, diagnostics)),
        Rule::raw_string_literal => Some(Expression::RawStringValue(parse_raw_string(
            token,
            diagnostics,
        ))),
        Rule::quoted_string_literal => {
            let contents = token.into_inner().next().unwrap();
            Some(Expression::StringValue(
                unescape_string(contents.as_str()),
                span,
            ))
        }
        Rule::map_expression => Some(parse_map(token, diagnostics)),
        Rule::array_expression => Some(parse_array(token, diagnostics)),
        Rule::jinja_expression => Some(parse_jinja_expression(token, diagnostics)),
        Rule::identifier => match token.as_str() {
            "true" => Some(Expression::BoolValue(true, span)),
            "false" => Some(Expression::BoolValue(false, span)),
            _ => Some(Expression::Identifier(parse_identifier(token, diagnostics))),
        },
        Rule::class_constructor => Some(parse_class_constructor(token, diagnostics)),
        Rule::fn_app => parse_fn_app(token, diagnostics),
        Rule::generic_fn_app => parse_generic_fn_app(token, diagnostics),
        Rule::lambda => parse_lambda(token, diagnostics),
        Rule::expr_block => {
            parse_expr_block(token, diagnostics).map(|block| Expression::ExprBlock(block, span))
        }
        Rule::if_expression => parse_if_expression(token, diagnostics),

        // Nested expr in parens.
        Rule::expression => {
            parse_expression(token, diagnostics).map(|expr| Expression::Paren(Box::new(expr), span))
        }

        Rule::BLOCK_LEVEL_CATCH_ALL => {
            diagnostics.push_error(
                internal_baml_diagnostics::DatamodelError::new_validation_error(
                    "This is not a valid expression!",
                    span,
                ),
            );
            None
        }

        _ => {
            unreachable_rule(&token, "primary_expression", diagnostics);
            None
        }
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
            Rule::ARRAY_CATCH_ALL => {
                diagnostics.push_error(
                    internal_baml_diagnostics::DatamodelError::new_validation_error(
                        "Invalid array syntax detected.",
                        diagnostics.span(current.as_span()),
                    ),
                );
            }
            _ => parsing_catch_all(current, "array", diagnostics),
        }
    }

    Expression::Array(elements, diagnostics.span(span))
}

fn parse_string_literal(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    assert_correct_parser(&token, &[Rule::string_literal], diagnostics);
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
            } else if content.eq("true") || content.eq("false") {
                Expression::BoolValue(content.eq("true"), span)
            } else {
                match Identifier::from((content.as_str(), span.clone())) {
                    Identifier::Invalid(..) | Identifier::String(..) => {
                        Expression::StringValue(content, span)
                    }
                    identifier => Expression::Identifier(identifier),
                }
            }
        }
        _ => {
            unreachable_rule(&contents, "string_literal", diagnostics);
            Expression::StringValue(String::new(), span)
        }
    }
}

fn parse_map(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    fn parse_expr_map_entry(
        pair: Pair<'_>,
        diagnostics: &mut Diagnostics,
    ) -> Option<(Expression, Expression)> {
        assert_correct_parser(&pair, &[Rule::expr_map_entry], diagnostics);

        let mut inner = pair.into_inner();

        let key_rule = inner.next()?;
        let colon = consume_if_rule(&mut inner, Rule::COLON);
        let value_rule = inner.next()?;

        let key = parse_expression(key_rule, diagnostics)?;
        let value = parse_expression(value_rule, diagnostics)?;

        if colon.is_none() {
            diagnostics.push_error(DatamodelError::new_validation_error(
                "Missing colon between key expression & value expression",
                Span {
                    file: key.span().file.clone(),
                    start: key.span().end,
                    end: value.span().start,
                },
            ));
        }

        Some((key, value))
    }

    fn parse_ident_map_entry(
        pair: Pair<'_>,
        diagnostics: &mut Diagnostics,
    ) -> Option<(Expression, Expression)> {
        assert_correct_parser(&pair, &[Rule::ident_map_entry], diagnostics);

        let mut inner = pair.into_inner();

        let ident = parse_identifier(inner.next()?, diagnostics);

        let value = parse_expression(inner.next()?, diagnostics)?;

        Some((
            Expression::StringValue(ident.to_string(), ident.span().clone()),
            value,
        ))
    }

    fn parse_map_entry(
        pair: Pair<'_>,
        diagnostics: &mut Diagnostics,
    ) -> Option<(Expression, Expression)> {
        match pair.as_rule() {
            Rule::expr_map_entry => parse_expr_map_entry(pair, diagnostics),
            Rule::ident_map_entry => parse_ident_map_entry(pair, diagnostics),
            _ => {
                unreachable_rule(&pair, "map_expression", diagnostics);
                None
            }
        }
    }

    let span = token.as_span();

    let mut inner = token
        .into_inner()
        .filter(|pair| !matches!(pair.as_rule(), Rule::NEWLINE));

    // Option<(rule, span of inference)>
    // We'll be reporting

    let entries = if let Some(first) = inner.next() {
        let first_rule = first.as_rule();

        let first_entry = parse_map_entry(first, diagnostics).into_iter();

        let rest_of_entries = inner.filter_map(|pair| {

            if first_rule != pair.as_rule() {
                diagnostics.push_error(DatamodelError::new_validation_error("Inconsistent use of key-value pair syntax. Consider using python-style if any of the keys is an identifier to avoid confusion", diagnostics.span(pair.as_span())));
            }

            parse_map_entry(pair, diagnostics)


        });

        first_entry.chain(rest_of_entries).collect()
    } else {
        Vec::new()
    };

    Expression::Map(entries, diagnostics.span(span))
}

pub fn parse_config_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<Expression> {
    assert_correct_parser(&token, &[Rule::config_expression], diagnostics);
    parse_config_primary_expression(token.into_inner().next()?, diagnostics)
}

pub fn parse_config_primary_expression(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<Expression> {
    assert_correct_parser(&token, &[Rule::config_primary_expression], diagnostics);
    let span = diagnostics.span(token.as_span());

    let token = token.into_inner().next()?;

    match token.as_rule() {
        Rule::numeric_literal => Some(Expression::NumericValue(token.as_str().into(), span)),
        Rule::string_literal => Some(parse_string_literal(token, diagnostics)),
        Rule::config_array_expression => Some(parse_config_array(token, diagnostics)),
        Rule::jinja_expression => Some(parse_jinja_expression(token, diagnostics)),
        Rule::config_map_expression => Some(parse_config_map(token, diagnostics)),
        Rule::identifier => Some(Expression::Identifier(parse_identifier(token, diagnostics))),
        Rule::fn_app => parse_fn_app(token, diagnostics),
        Rule::generic_fn_app => parse_generic_fn_app(token, diagnostics),
        _ => {
            unreachable_rule(&token, "config_primary_expression", diagnostics);
            None
        }
    }
}

fn parse_config_array(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    let mut elements: Vec<Expression> = vec![];
    let span = token.as_span();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::config_expression => {
                if let Some(expr) = parse_config_expression(current, diagnostics) {
                    elements.push(expr);
                }
            }
            Rule::ARRAY_CATCH_ALL => {
                diagnostics.push_error(
                    internal_baml_diagnostics::DatamodelError::new_validation_error(
                        "Invalid array syntax detected.",
                        diagnostics.span(current.as_span()),
                    ),
                );
            }
            _ => parsing_catch_all(current, "array", diagnostics),
        }
    }

    Expression::Array(elements, diagnostics.span(span))
}

fn parse_config_map(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    let mut entries: Vec<(Expression, Expression)> = vec![];
    let span = token.as_span();

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::config_map_entry => {
                if let Some(f) = parse_config_map_entry(current, diagnostics) {
                    entries.push(f)
                }
            }
            Rule::BLOCK_LEVEL_CATCH_ALL => {}
            _ => parsing_catch_all(current, "config map key value", diagnostics),
        }
    }

    Expression::Map(entries, diagnostics.span(span))
}

fn parse_config_map_entry(
    token: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Option<(Expression, Expression)> {
    assert_correct_parser(&token, &[Rule::config_map_entry], diagnostics);

    let mut key = None;
    let mut value = None;
    let token_span = token.as_span(); // Store the span before moving token

    for current in token.into_inner() {
        match current.as_rule() {
            Rule::config_map_key => key = Some(parse_config_map_key(current, diagnostics)),
            Rule::config_expression => value = parse_config_expression(current, diagnostics),
            Rule::COLON => {
                if key.is_none() {
                    diagnostics.push_error(
                        internal_baml_diagnostics::DatamodelError::new_validation_error(
                            "This map entry is missing a valid key or has an incorrect syntax.",
                            diagnostics.span(token_span), // Use the stored span here
                        ),
                    );
                    return None;
                }
            }
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
            _ => parsing_catch_all(current, "config dict entry", diagnostics),
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

fn parse_config_map_key(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    assert_correct_parser(&token, &[Rule::config_map_key], diagnostics);

    let span = diagnostics.span(token.as_span());
    if let Some(current) = token.into_inner().next() {
        return match current.as_rule() {
            Rule::identifier => Expression::Identifier(parse_identifier(current, diagnostics)),
            Rule::quoted_string_literal => Expression::StringValue(
                current.into_inner().next().unwrap().as_str().to_string(),
                span,
            ),
            _ => {
                unreachable_rule(&current, "config_map_key", diagnostics);
                Expression::Identifier(Identifier::Local(String::new(), span))
            }
        };
    }
    unreachable!("Encountered impossible config map key during parsing")
}

pub(super) fn parse_raw_string(token: Pair<'_>, diagnostics: &mut Diagnostics) -> RawString {
    assert_correct_parser(&token, &[Rule::raw_string_literal], diagnostics);

    let mut content = None;

    for current in token.into_inner() {
        match current.as_rule() {
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
            _ => unreachable_rule(&current, "raw_string_literal", diagnostics),
        };
    }
    match content {
        Some((content, span)) => RawString::new(content, span),
        _ => unreachable!("Encountered impossible raw string during parsing"),
    }
}

// NOTE(sam): this doesn't handle unicode escape sequences e.g. \u1234
// also this has panicks in it (see the hex logic)
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

/// Parse a `JinjaExpression` from raw source. Escape backslashes,
/// because we want the user's backslash intent to be preserved in
/// the string backing the `JinjaExpression`. In other words, control
/// sequences like `\n` are intended to be forwarded to the Jinja
/// processing engine, not to break a Jinja Expression into two lines,
/// therefor the backing string should be contain "\\n".
pub fn parse_jinja_expression(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    assert_correct_parser(&token, &[Rule::jinja_expression], diagnostics);
    let value = token
        .into_inner()
        .map(|token| match token.as_rule() {
            Rule::jinja_body => {
                let mut inner_text = String::new();
                for c in token.as_str().chars() {
                    match c {
                        // When encountering a single backslash, produce two backslashes.
                        '\\' => inner_text.push_str("\\\\"),
                        // Otherwise, just copy the character.
                        _ => inner_text.push(c),
                    }
                }
                Expression::JinjaExpressionValue(
                    JinjaExpression(inner_text),
                    diagnostics.span(token.as_span()),
                )
            }
            _ => {
                unreachable_rule(&token, "jinja_expression", diagnostics);
                Expression::JinjaExpressionValue(
                    JinjaExpression(String::new()),
                    diagnostics.span(token.as_span()),
                )
            }
        })
        .next();

    if let Some(value) = value {
        value
    } else {
        unreachable!("Encountered impossible jinja expression during parsing")
    }
}

pub fn parse_class_constructor(token: Pair<'_>, diagnostics: &mut Diagnostics) -> Expression {
    assert_correct_parser(&token, &[Rule::class_constructor], diagnostics);

    let span = diagnostics.span(token.as_span());
    let mut tokens = token.into_inner();

    let ident_token = tokens.next().expect("Guaranteed by the grammar");

    let class_name = match ident_token.as_rule() {
        Rule::identifier => parse_identifier(ident_token, diagnostics),
        Rule::path_identifier => parse_path_identifier(ident_token, diagnostics),
        _ => panic!("Encountered impossible class constructor during parsing"),
    };

    let mut fields = Vec::new();
    while let Some(field_or_close_bracket) = tokens.next() {
        if field_or_close_bracket.as_str() == "}" {
            break;
        }
        if field_or_close_bracket.as_str() == "," {
            continue;
        }
        if field_or_close_bracket.as_rule() == Rule::NEWLINE {
            continue;
        }

        assert_correct_parser(
            &field_or_close_bracket,
            &[Rule::class_field_value_pair],
            diagnostics,
        );

        let mut field_tokens = field_or_close_bracket.into_inner();
        let identifier_or_spread = field_tokens.next().expect("Guaranteed by the grammar");
        match identifier_or_spread.as_rule() {
            Rule::struct_spread => {
                let mut struct_spread_tokens = identifier_or_spread.into_inner();
                let maybe_expr = parse_expression(
                    struct_spread_tokens
                        .next()
                        .expect("Guaranteed by the grammar"),
                    diagnostics,
                );
                if let Some(expr) = maybe_expr {
                    fields.push(ClassConstructorField::Spread(expr));
                }
            }
            Rule::identifier => {
                let field_name = parse_identifier(identifier_or_spread, diagnostics);

                let _colon = field_tokens.next();
                let maybe_expr = parse_expression(
                    field_tokens.next().expect("Guaranteed by the grammar"),
                    diagnostics,
                );
                if let Some(expr) = maybe_expr {
                    fields.push(ClassConstructorField::Named(field_name, expr));
                }
            }
            _ => unreachable_rule(&identifier_or_spread, "class_field_value_pair", diagnostics),
        }
        let _maybe_comma = tokens.next();
    }
    let class_constructor = ClassConstructor { class_name, fields };

    Expression::ClassConstructor(class_constructor, span)
}

#[cfg(test)]
mod tests {
    use internal_baml_diagnostics::{Diagnostics, SourceFile};
    use pest::{consumes_to, parses_to, Parser};

    use super::{
        super::{parse_expr::parse_expr_block, BAMLParser, Rule},
        *,
    };

    #[test]
    fn test_parse_jinja_expression() {
        let input = "{{ 1 + 1 }}";
        let root_path = "test_file.baml";
        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);

        let pair = BAMLParser::parse(Rule::jinja_expression, input)
            .unwrap()
            .next()
            .unwrap();
        let expr = parse_jinja_expression(pair, &mut diagnostics);
        match expr {
            Expression::JinjaExpressionValue(JinjaExpression(s), _) => assert_eq!(s, "1 + 1"),
            _ => panic!("Expected JinjaExpression, got {expr:?}"),
        }
    }

    #[test]
    fn test_comment_header_parsing() {
        println!("\n=== Testing Comment Header Parsing ===");

        let input = r#"{
            //# Level 1 Header
            let x = "hello";

            //## Level 2 Header
            let y = "world";

            //########### Level 11 Header

            //### Level 3 Headers
            x + y
        }"#;

        let root_path = "test_file.baml";
        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);

        println!("Parsing expression block with comment headers...");

        let pair_result = BAMLParser::parse(Rule::expr_block, input);
        match pair_result {
            Ok(mut pairs) => {
                let pair = pairs.next().unwrap();
                let expr = parse_expr_block(pair, &mut diagnostics);
                match expr {
                    Some(expr_block) => {
                        println!("✓ Successfully parsed expression block: {expr_block:?}")
                    }
                    None => println!("✗ Failed to parse expression block"),
                }
            }
            Err(e) => println!("✗ Parse error: {e:?}"),
        }

        println!("Diagnostics:");
        for error in diagnostics.errors() {
            println!("  Error: {error:?}");
        }
        for warning in diagnostics.warnings() {
            println!("  Warning: {warning:?}");
        }
    }

    #[test]
    fn test_complex_header_hierarchy() {
        println!("\n=== Testing Complex Header Hierarchy ===");

        let input = r#"//# Loop Processing
fn ForLoopWithHeaders() -> int {
    let items = [1, 2, 3, 4, 5];
    let result = 0;

    //## Main Loop
    for (item in items) {
        //### Item Processing
        let processed = item * 2;

        //#### Accumulation
        result = result + processed;
    }

    //## Final Result
    result
}"#;

        let root_path = "test_file.baml";
        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);

        println!("Parsing function with complex header hierarchy...");

        let pair_result = BAMLParser::parse(Rule::schema, input);
        match pair_result {
            Ok(mut pairs) => {
                let schema_pair = pairs.next().unwrap();
                println!("✓ Successfully parsed schema");

                // Look for expr_fn within the schema
                for item in schema_pair.into_inner() {
                    match item.as_rule() {
                        Rule::expr_fn => {
                            let expr_fn = parse_expr_fn(item, &mut diagnostics);
                            match expr_fn {
                                Some(expr_fn) => {
                                    println!(
                                        "✓ Found and parsed function: {}",
                                        expr_fn.name.name()
                                    );
                                }
                                None => println!("✗ Failed to parse function"),
                            }
                        }
                        Rule::comment_block => {
                            println!("✓ Found top-level comment block");
                        }
                        _ => {
                            println!("Found other item: {:?}", item.as_rule());
                        }
                    }
                }
            }
            Err(e) => {
                println!("✗ Parse error: {e:?}");
                return;
            }
        }

        println!("Diagnostics errors: {}", diagnostics.errors().len());
        for error in diagnostics.errors() {
            println!("  Error: {error:?}");
        }
    }
}
