use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_identifier::parse_identifier,
    parse_named_args_list::parse_named_argument_list,
    Rule,
};

use crate::{assert_correct_parser, ast::*};
use crate::{ast::TypeExpressionBlock, parser::parse_field::parse_expr_as_type}; // Add this line to import DatamodelParser

use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_type_expression(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> TypeExpressionBlock {
    assert_correct_parser!(pair, Rule::type_expression_block);

    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut fields: Vec<Field<FieldType>> = Vec::new();
    let mut sub_type: Option<SubType> = None;

    for current in pair.into_inner() {
        let mut input: Option<BlockArgs> = None;
        println!("Parsing type_expression block: {:?}", current.as_rule());

        match current.as_rule() {
            Rule::type_expression_keyword => {
                match current.as_str() {
                    "class" => sub_type = Some(SubType::Class.clone()),
                    "enum" => sub_type = Some(SubType::Enum.clone()),
                    _ => sub_type = Some(SubType::Other(current.as_str().to_string())),
                }
                // Rule::CLASS_KEYWORD => sub_type = Some(SubType::Class.clone());
                // Rule::ENUM_KEYWORD => sub_type = Some(SubType::Enum.clone());
            }

            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::named_argument_list => match parse_named_argument_list(current, diagnostics) {
                Ok(arg) => input = Some(arg),
                Err(err) => diagnostics.push_error(err),
            },
            Rule::type_expression_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    println!("Parsing type_expression_contents: {:?}", item.as_rule());

                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::type_expression =>{
                            
                            match parse_expr_as_type(
                                    &name,
                                    sub_type.clone().map(|st| match st {
                                        SubType::Enum => "Enum",
                                        SubType::Class => "Class",
                                        SubType::Other(_) => "Other",
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
                        _ => parsing_catch_all(item, "type_expression"),
                    }
                }
            }

            _ => {
                println!("Encountered rule: {:?}", current.as_rule());
                parsing_catch_all(current, "type_expression")
            }
        }
    }
    if let Some(ref name) = name {
        println!("Populated name: {:?}", name);
    }
    if !fields.is_empty() {
        println!("Populated fields: {:?}", fields);
    }
    if !attributes.is_empty() {
        println!("Populated attributes: {:?}", attributes);
    }
    if let Some(ref doc_comment) = doc_comment {
        println!("Populated documentation comment: {:?}", doc_comment);
    }
    if let Some(ref sub_type) = sub_type {
        println!("Populated sub_type: {:?}", sub_type);
    }
    match name {
        Some(name) => TypeExpressionBlock {
            name,
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: diagnostics.span(pair_span),
            sub_type: sub_type
                .clone()
                .unwrap_or(SubType::Other("Subtype not found".to_string())),
        },
        _ => panic!("Encountered impossible type_expression declaration during parsing",),
    }
}
