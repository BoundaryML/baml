use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_expression::parse_expression,
    parse_identifier::parse_identifier,
    parse_template_args::parse_template_args,
    Rule,
};
use crate::{assert_correct_parser, ast::*, unreachable_rule};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_config_block(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<Top, DatamodelError> {
    let pair_span = pair.as_span();
    let mut template_args = None;
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut fields: Vec<ConfigBlockProperty> = Vec::new();
    let mut kw = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::template_args => template_args = parse_template_args(current, diagnostics),
            Rule::config_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::key_value => {
                            fields.push(parse_key_value(
                                item,
                                pending_field_comment.take(),
                                diagnostics,
                            ));
                        }
                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not a valid field or attribute definition. A valid property may look like: 'myProperty \"some value\"' for example, with no colons.",
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(&item, "model"),
                    }
                }
            }
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::TEST_KEYWORD
            | Rule::PRINTER_KEYWORD
            | Rule::RETRY_POLICY_KEYWORD
            | Rule::GENERATOR_KEYWORD
            | Rule::CLIENT_KEYWORD => kw = Some(current.as_str()),
            _ => parsing_catch_all(&current, "client"),
        }
    }

    let span = match kw {
        Some(_) => diagnostics.span(pair_span),
        _ => unreachable!("Encountered impossible model declaration during parsing"),
    };

    match (kw, name, template_args) {
        (Some("printer"), _, None) => Err(DatamodelError::new_validation_error(
            "Missing template for printer. (did you forget <type> or <enum>)",
            span,
        )),
        (Some("printer"), Some(name), Some(args)) => match args.len() {
            1 => Ok(Top::Config(Configuration::Printer(PrinterConfig {
                name,
                fields,
                attributes,
                documentation: doc_comment.and_then(parse_comment_block),
                span,
                printer_type: args.first().unwrap().to_owned(),
            }))),
            _ => Err(DatamodelError::new_validation_error(
                "printer requires 1 template args. (did you forget <type> or <enum>)",
                span,
            )),
        },
        (Some("client"), _, None) => Err(DatamodelError::new_validation_error(
            "Missing template for client. (did you forget <llm>)",
            span,
        )),
        (Some("client"), Some(name), Some(args)) => match args.len() {
            1 => Ok(Top::Client(Client {
                name,
                fields,
                attributes,
                documentation: doc_comment.and_then(parse_comment_block),
                span,
                client_type: args.first().unwrap().to_string(),
            })),
            _ => Err(DatamodelError::new_validation_error(
                "client requires 1 template args. (did you forget <llm>)",
                span,
            )),
        },
        (Some("retry_policy"), _, Some(_)) => Err(DatamodelError::new_validation_error(
            "Template arguments are not allowed for retry_policy.",
            span,
        )),
        (Some("retry_policy"), Some(name), None) => {
            Ok(Top::Config(Configuration::RetryPolicy(RetryPolicyConfig {
                name,
                fields,
                attributes,
                documentation: doc_comment.and_then(parse_comment_block),
                span,
            })))
        }
        (Some("generator"), _, Some(_)) => Err(DatamodelError::new_validation_error(
            "Template arguments are not allowed for generators.",
            span,
        )),
        (Some("generator"), Some(name), None) => Ok(Top::Generator(GeneratorConfig {
            name,
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span,
        })),
        (Some("test"), _, Some(_)) => Err(DatamodelError::new_validation_error(
            "Template arguments are not allowed for test.",
            span,
        )),
        (Some("test"), None, None) => Err(DatamodelError::new_validation_error(
            "Missing name for test.",
            span,
        )),
        (Some("test"), Some(name), None) => {
            Ok(Top::Config(Configuration::TestCase(RetryPolicyConfig {
                name,
                fields,
                attributes,
                documentation: doc_comment.and_then(parse_comment_block),
                span,
            })))
        }
        _ => unreachable!("Encountered impossible model declaration during parsing",),
    }
}

pub(crate) fn parse_key_value(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> ConfigBlockProperty {
    assert_correct_parser!(pair, Rule::key_value);

    let mut name: Option<Identifier> = None;
    let mut value: Option<Expression> = None;
    let mut attributes = Vec::new();
    let mut comment: Option<Comment> = doc_comment.and_then(parse_comment_block);
    let mut template_args = None;
    let (pair_span, pair_str) = (pair.as_span(), pair.as_str());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => {
                name = Some(parse_identifier(current, diagnostics));
            }
            Rule::field_attribute => attributes.push(parse_attribute(current, diagnostics)),
            Rule::expression => value = Some(parse_expression(current, diagnostics)),
            Rule::trailing_comment => {
                comment = match (comment, parse_trailing_comment(current)) {
                    (c, None) | (None, c) => c,
                    (Some(existing), Some(new)) => Some(Comment {
                        text: [existing.text, new.text].join("\n"),
                    }),
                };
            }
            Rule::template_args => {
                template_args = parse_template_args(current, diagnostics);
            }
            _ => unreachable_rule!(current, Rule::key_value),
        }
    }

    match name {
        Some(name) => ConfigBlockProperty {
            name,
            template_args,
            value,
            attributes,
            span: diagnostics.span(pair_span),
            documentation: comment,
        },
        _ => unreachable!(
            "Encountered impossible source property declaration during parsing: {:?}",
            pair_str,
        ),
    }
}
