use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_identifier::parse_identifier,
    Rule,
};
use crate::ast::{Attribute, Comment, Serializer, SerializerField};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub fn parse_serializer(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Serializer {
    let comment = doc_comment.and_then(parse_comment_block);
    let pair_span = pair.as_span();
    let mut name = None;
    let mut attributes: Vec<Attribute> = vec![];
    let mut fields = vec![];

    for current in pair.into_inner().peekable() {
        match current.as_rule() {
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE | Rule::SERIALIZER_KEYWORD => {}
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::serializer_contents => {
                let mut pending_value_comment = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics))
                        }
                        Rule::serializer_field => {
                            match parse_serializer_field(
                                item,
                                pending_value_comment.take(),
                                diagnostics,
                            ) {
                                Ok(field) => fields.push(field),
                                Err(err) => diagnostics.push_error(err),
                            }
                        }
                        Rule::comment_block => pending_value_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not valid.",
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(&item, "serializer_content"),
                    }
                }
            }
            _ => parsing_catch_all(&current, "serializer"),
        }
    }

    match name {
        Some(name) => Serializer {
            name,
            fields,
            attributes,
            documentation: comment,
            span: diagnostics.span(pair_span),
        },
        _ => {
            panic!("Encountered impossible serializer declaration during parsing, name is missing.",)
        }
    }
}

fn parse_serializer_field(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<SerializerField, DatamodelError> {
    let (pair_str, pair_span) = (pair.as_str(), pair.as_span());
    let mut name = None;
    let mut attributes = vec![];
    let mut comment = doc_comment.and_then(parse_comment_block);

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::field_attribute => attributes.push(parse_attribute(current, diagnostics)),
            Rule::trailing_comment => {
                comment = match (comment, parse_trailing_comment(current)) {
                    (None, a) | (a, None) => a,
                    (Some(a), Some(b)) => Some(Comment {
                        text: [a.text, b.text].join("\n"),
                    }),
                };
            }
            Rule::comment_block => {
                parse_comment_block(current);
            }
            _ => parsing_catch_all(&current, "serializer field"),
        }
    }

    match name {
        Some(name) => Ok(SerializerField {
            name,
            attributes,
            documentation: comment,
            span: diagnostics.span(pair_span),
        }),
        _ => panic!("Encountered impossible serializer field declaration during parsing, name is missing: {pair_str:?}",),
    }
}
