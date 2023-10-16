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

pub(crate) fn parse_function(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Function {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut input: Option<Field> = None;
    let mut output: Option<Field> = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::FUNCTION_KEYWORD | Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::identifier => name = parse_identifier(current.into(), diagnostics),
            Rule::class_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute  => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::field_declaration => match parse_field(
                            &name.as_ref().unwrap().name,
                            "function",
                            item,
                            pending_field_comment.take(),
                            diagnostics,
                        ) {
                            Ok(field) => {
                                match field.name() {
                                    "input" => {
                                        match input {
                                            Some(_) => diagnostics.push_error(DatamodelError::new_duplicate_field_error(
                                                &name.clone().unwrap().name,
                                                field.name(),
                                                "function",
                                                field.identifier().span,
                                            )),
                                            None => input = Some(field),
                                        }
                                    },
                                    "output" => output = Some(field),
                                    _ => {
                                        diagnostics.push_error(DatamodelError::new_validation_error(
                                            "Unsupport field name in function. Only `input` and `output` are allowed.",
                                            Span::from(pair_span),
                                        ))
                                    },
                                }
                            }
                            Err(err) => diagnostics.push_error(err),
                        },
                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => diagnostics.push_error(DatamodelError::new_validation_error(
                            "This line is not a valid field or attribute definition.",
                            item.as_span().into(),
                        )),
                        _ => parsing_catch_all(&item, "model"),
                    }
                }
            }
            _ => parsing_catch_all(&current, "model"),
        }
    }

    match name {
        Some(name) => Function {
            name,
            input: input.unwrap(),
            output: output.unwrap(),
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: Span::from(pair_span),
        },
        _ => panic!("Encountered impossible function declaration during parsing",),
    }
}
