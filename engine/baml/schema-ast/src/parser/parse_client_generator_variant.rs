use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_expression::parse_expression,
    parse_identifier::{parse_identifier, parse_identifier_string},
    Rule,
};
use crate::ast::*;
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
            Rule::template_args => {
                let mut inner = current.into_inner();
                for current in &mut inner {
                    match current.as_rule() {
                        Rule::empty_template_args => {}
                        Rule::expression => {
                            template_args = Some(parse_expression(current, diagnostics));
                        }
                        _ => parsing_catch_all(&current, "template args"),
                    }
                }
            }
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
                                "This line is not a valid field or attribute definition.",
                                item.as_span().into(),
                            ))
                        }
                        _ => parsing_catch_all(&item, "model"),
                    }
                }
            }
            Rule::identifier => name = Some(parse_identifier(current.into(), diagnostics)),
            Rule::GENERATOR_KEYWORD | Rule::CLIENT_KEYWORD | Rule::VARIANT_KEYWORD => {
                kw = Some(current.as_str())
            }
            _ => parsing_catch_all(&current, "client"),
        }
    }

    match (kw, name, template_args) {
        (Some("client") | Some("variant"), _, None) => Err(DatamodelError::new_validation_error(
            "Missing template for client or variant. (did you forget <llm>)",
            Span::from(pair_span),
        )),
        (Some("client"), Some(name), Some(template_args)) => Ok(Top::Client(Client {
            name,
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: Span::from(pair_span),
            client_type: template_args.to_string(),
        })),
        (Some("variant"), Some(name), Some(template_args)) => Ok(Top::Variant(Variant {
            name,
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: Span::from(pair_span),
            variant_type: template_args.to_string(),
        })),
        (Some("generator"), Some(name), None) => Ok(Top::Generator(GeneratorConfig {
            name: name,
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: Span::from(pair_span),
        })),
        (Some("generator"), _, Some(template_args)) => Err(DatamodelError::new_validation_error(
            "Template arguments are not allowed for generators.",
            Span::from(pair_span),
        )),
        _ => unreachable!("Encountered impossible model declaration during parsing",),
    }
}

fn parse_key_value(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> ConfigBlockProperty {
    let mut name: Option<Identifier> = None;
    let mut value: Option<Expression> = None;
    let mut comment: Option<Comment> = doc_comment.and_then(parse_comment_block);
    let (pair_span, pair_str) = (pair.as_span(), pair.as_str());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::single_word => {
                name = Some(parse_identifier(current.into(), diagnostics));
            }
            Rule::expression => value = Some(parse_expression(current, diagnostics)),
            Rule::trailing_comment => {
                comment = match (comment, parse_trailing_comment(current)) {
                    (c, None) | (None, c) => c,
                    (Some(existing), Some(new)) => Some(Comment {
                        text: [existing.text, new.text].join("\n"),
                    }),
                };
            }
            _ => unreachable!(
                "Encountered impossible source property declaration during parsing: {:?}",
                current.tokens()
            ),
        }
    }

    match name {
        Some(name) => ConfigBlockProperty {
            name,
            value,
            span: Span::from(pair_span),
            documentation: comment,
        },
        _ => unreachable!(
            "Encountered impossible source property declaration during parsing: {:?}",
            pair_str,
        ),
    }
}
