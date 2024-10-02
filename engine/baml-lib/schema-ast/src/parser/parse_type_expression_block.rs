use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_identifier::parse_identifier,
    parse_named_args_list::parse_named_argument_list,
    Rule,
};

use crate::{assert_correct_parser, ast::*};
use crate::{ast::TypeExpressionBlock, parser::parse_field::parse_type_expr}; // Add this line to import DatamodelParser

use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_type_expression_block(
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
    let mut input = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            // There are two identifiers in the children of type_expression_block:
            // class Foo {}  <- identifier "class", identifier "Foo", the rest.
            // So we do different things with an `identifier` as we incrementally
            // build the `TypeExpressionBlock`.
            Rule::identifier => {
                if sub_type.is_none() {
                    // First identifier is the type of block (e.g. class, enum).
                    match current.as_str() {
                        "class" => sub_type = Some(SubType::Class.clone()),
                        "enum" => sub_type = Some(SubType::Enum.clone()),
                        _ => sub_type = Some(SubType::Other(current.as_str().to_string())),
                    }
                } else {
                    // Subsequent identifier is the name of the block.
                    name = Some(parse_identifier(current, diagnostics));
                }
            }

            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::named_argument_list => match parse_named_argument_list(current, diagnostics) {
                Ok(arg) => input = Some(arg),
                Err(err) => diagnostics.push_error(err),
            },
            Rule::type_expression_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, false, diagnostics));
                        }
                        Rule::type_expression =>{
                            let sub_type_is_enum = matches!(sub_type, Some(SubType::Enum));
                            let sub_type_expression = parse_type_expr(
                                &name,
                                sub_type.clone().map(|st| match st {
                                    SubType::Enum => "Enum",
                                    SubType::Class => "Class",
                                    SubType::Other(_) => "Other",
                                }).unwrap_or(""),
                                item,
                                pending_field_comment.take(),
                                diagnostics,
                                sub_type_is_enum,
                            );
                            match sub_type_expression {
                                    Ok(field) => {
                                        fields.push(field);
                                    },
                                    Err(err) => diagnostics.push_error(err),
                                }
                        }
                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                match sub_type {
                                    Some(SubType::Enum) => "This line is not an enum value definition. BAML enums don't have commas, and all values must be all caps.",
                                    _ => "This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description(\"This is a description\")'",
                                },
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(item, "type_expression"),
                    }
                }
            }

            _ => parsing_catch_all(current, "type_expression"),
        }
    }

    match name {
        Some(name) => TypeExpressionBlock {
            name,
            fields,
            input,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{BAMLParser, Rule};
    use internal_baml_diagnostics::{Diagnostics, SourceFile};
    use pest::{Parser, consumes_to, fails_with, parses_to};

    #[test]
    fn keyword_name_mandatory_whitespace() {
        // This is the expected form.
        parses_to! {
            parser: BAMLParser,
            input: "class Foo {}",
            rule: Rule::type_expression_block,
            tokens: [type_expression_block(0, 12, [
                 identifier(0,5,[single_word(0,5)]),
                 identifier(6,9,[single_word(6,9)]),
                 BLOCK_OPEN(10,11),
                 type_expression_contents(11,11),
                 BLOCK_CLOSE(11,12),
            ])]
        }

        // This form passed with a historical version of the
        // grammar that allowed type expression keywords adjacent
        // to type expression identifiers with no mandatory
        // whitespace.
        fails_with! {
            parser: BAMLParser,
            input: "classFoo {}",
            rule: Rule::type_expression_block,
            positives: [Rule::identifier],
            negatives: [],
            pos: 9
        }
    }

    #[test]
    // This test checks that parsing a particular malformed Enum produces
    // a field that is an enum variant with a data payload. This is not
    // a valid BAML enum. But a later validation phase
    // (parser_database::visit_enum) expects this parse result, in order to
    // produce a good error message.
    fn enum_invalid_value_expr() {
        let root_path = "test_file.baml";

        let input = r#"enum Test { A int | string }"#;
        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);
        let parsed = BAMLParser::parse(Rule::type_expression_block, input)
            .unwrap()
            .next()
            .unwrap();
        let result = parse_type_expression_block(parsed, None, &mut diagnostics);
        let TypeExpressionBlock { name, fields, .. } = result;
        assert_eq!(name.to_string(), "Test");
        assert!(fields[0].expr.is_some());
    }
}
