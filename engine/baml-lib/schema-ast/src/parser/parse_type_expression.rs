use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_field::parse_field,
    parse_identifier::parse_identifier,
    parse_named_argument_list::parse_named_argument_list,
    Rule,
};
use crate::ast::TypeExpression;
use crate::{assert_correct_parser, ast::*};

use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_type_expression(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> TypeExpression {
    assert_correct_parser!(pair, Rule::type_expression_block);

    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut fields: Vec<Field> = Vec::new();
    let mut sub_type: Option<SubType> = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::CLASS_KEYWORD => sub_type = Some(SubType::Class),
            Rule::ENUM_KEYWORD => sub_type = Some(SubType::Enum),
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::named_argument_list => match parse_named_arguement_list(current, diagnostics) {
                Ok(BlockArgs::Named(arg)) => input = Some(BlockArgs::Named(arg)),
                Ok(BlockArgs::Unnamed(arg)) => {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "Unnamed arguments are not supported for function input. Define a new class instead.",
                        arg.span,
                    ))
                }
                Err(err) => diagnostics.push_error(err),
            },
            Rule::type_expression_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::type_expression =>{

                            match parse_field(
                                    &name,
                                    sub_type.as_ref().map(|st| match st {
                                        SubType::Enum => "Enum",
                                        SubType::Class => "Class",
                                        SubType::Other(s) => s,
                                    }).unwrap_or(""),
                                    item,
                                    pending_field_comment.take(),
                                    diagnostics,
                                ) {
                                    Ok(field) => fields.push(field),
                                    Err(err) => diagnostics.push_error(err),
                                }
                        }
                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description(\"This is a description\")'",
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(&item, "type_expression"),
                    }
                }
            }
            _ => parsing_catch_all(&current, "type_expression"),
        }
    }

    match name {
        Some(name) => TypeExpression {
            name,
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: diagnostics.span(pair_span),
            sub_type: None,
        },
        _ => panic!("Encountered impossible type_expression declaration during parsing",),
    }
}
