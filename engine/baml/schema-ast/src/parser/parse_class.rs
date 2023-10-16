use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_field::parse_field,
    parse_identifier::parse_identifier,
    Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_class(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Class {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut fields: Vec<Field> = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::CLASS_KEYWORD | Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::identifier => name = Some(parse_identifier(current.into(), diagnostics)),
            Rule::class_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::field_declaration => match parse_field(
                            &name.as_ref().unwrap().name,
                            "class",
                            item,
                            pending_field_comment.take(),
                            diagnostics,
                        ) {
                            Ok(field) => fields.push(field),
                            Err(err) => diagnostics.push_error(err),
                        },
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
            _ => parsing_catch_all(&current, "model"),
        }
    }

    match name {
        Some(name) => Class {
            name,
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: Span::from(pair_span),
        },
        _ => panic!("Encountered impossible model declaration during parsing",),
    }
}
