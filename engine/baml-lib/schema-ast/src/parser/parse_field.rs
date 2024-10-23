use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_expression::parse_expression,
    parse_identifier::parse_identifier,
    parse_types::{parse_field_type, reassociate_union_attributes},
    Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_value_expr(
    model_name: &Option<Identifier>,
    container_type: &'static str,
    pair: Pair<'_>,
    block_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<Field<Expression>, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut field_type = None;
    let mut comment: Option<Comment> = block_comment.and_then(parse_comment_block);

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::field_attribute => {
                attributes.push(parse_attribute(current, false, diagnostics));
            }
            Rule::trailing_comment => {
                comment = match (comment, parse_trailing_comment(current)) {
                    (c, None) | (None, c) => c,
                    (Some(existing), Some(new)) => Some(Comment {
                        text: [existing.text, new.text].join("\n"),
                    }),
                };
            }
            Rule::expression => field_type = Some(parse_expression(current, diagnostics)),

            _ => parsing_catch_all(current, "field"),
        }
    }

    match (name, field_type) {
        (Some(name), Some(field_type)) => Ok(Field {
            expr: field_type,
            name,
            attributes,
            documentation: comment,
            span: diagnostics.span(pair_span),
        }),
        _ => Err(DatamodelError::new_model_validation_error(
            "This field declaration is invalid. It is either missing a name or a type.",
            container_type,
            model_name.as_ref().map_or("<unknown>", |f| f.name()),
            diagnostics.span(pair_span),
        )),
    }
}

pub(crate) fn parse_type_expr(
    model_name: &Option<Identifier>,
    container_type: &'static str,
    pair: Pair<'_>,
    block_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
    is_enum: bool,
) -> Result<Field<FieldType>, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut field_attributes = Vec::<Attribute>::new();
    let mut field_type = None;
    let mut comment: Option<Comment> = block_comment.and_then(parse_comment_block);

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => {
                name = Some(parse_identifier(current, diagnostics));
            }
            Rule::trailing_comment => {
                comment = merge_comments(comment, parse_trailing_comment(current));
            }
            Rule::field_type_chain => {
                field_type = parse_field_type_chain(current, diagnostics);
            }
            Rule::field_attribute => {
                field_attributes.push(parse_attribute(current, false, diagnostics))
            }
            _ => parsing_catch_all(current, "field"),
        }
    }

    match (name, &field_type) {
        (Some(name), Some(field_type)) => Ok(Field {
            expr: Some(field_type.clone()),
            name,
            attributes: field_type.clone().attributes().to_vec(),
            documentation: comment,
            span: diagnostics.span(pair_span),
        }),
        (Some(name), None) => Ok(Field {
            expr: None,
            name,
            attributes: field_attributes,
            documentation: comment,
            span: diagnostics.span(pair_span),
        }),
        _ => Err(DatamodelError::new_model_validation_error(
            "This field declaration is invalid. It is either missing a name or a type.",
            container_type,
            model_name.as_ref().map_or("<unknown>", |f| f.name()),
            diagnostics.span(pair_span),
        )),
    }
}

fn merge_comments(existing: Option<Comment>, new: Option<Comment>) -> Option<Comment> {
    match (existing, new) {
        (Some(existing), Some(new)) => Some(Comment {
            text: format!("{}\n{}", existing.text, new.text),
        }),
        (existing, None) | (None, existing) => existing,
    }
}

pub fn parse_field_type_chain(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    let mut types = Vec::new();
    let mut operators = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::field_type_with_attr => {
                if let Some(field_type) = parse_field_type_with_attr(current, false, diagnostics) {
                    types.push(field_type);
                }
            }
            Rule::field_operator => operators.push(current.as_str().to_string()),
            _ => parsing_catch_all(current, "field_type_chain"),
        }
    }

    //do not need to pass in operators, as the only operator we can have is of union (|) type, so we handle this implicitly in the combine_field_types function
    combine_field_types(types)
}

pub(crate) fn parse_field_type_with_attr(
    pair: Pair<'_>,
    parenthesized: bool,
    diagnostics: &mut Diagnostics,
) -> Option<FieldType> {
    let mut field_type = None;
    let mut field_attributes = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::field_type => {
                field_type = parse_field_type(current, diagnostics);
            }
            Rule::field_type_with_attr => {}
            Rule::field_attribute => {
                field_attributes.push(parse_attribute(current, parenthesized, diagnostics));
            }
            Rule::trailing_comment => {}
            _ => {
                parsing_catch_all(current, "field_type_with_attr!");
            }
        }
    }

    match field_type {
        Some(mut ft) => {
            // ft.set_attributes(field_attributes);
            match &mut ft {
                FieldType::Union(_arity, ref mut _variants, _, _) => {
                    reassociate_union_attributes(&mut ft);

                    // if let Some(attributes) = attributes.as_ref() {
                    //     ft.set_attributes(attributes.clone()); // Clone the borrowed `Vec<Attribute>`
                    // }
                }
                _ => {}
            }
            ft.extend_attributes(field_attributes);

            Some(ft) // Return the field type with attributes
        }
        None => None,
    }
}

fn combine_field_types(types: Vec<FieldType>) -> Option<FieldType> {
    if types.is_empty() {
        return None;
    }

    let mut combined_type = types[0].clone();

    let mut seen_types = vec![combined_type.clone()];

    let mut earliest_start = combined_type.span().start;
    let mut latest_end = combined_type.span().end;

    for next_type in types.into_iter().skip(1) {
        seen_types.push(next_type.clone());

        let span = next_type.span();
        if span.start < earliest_start {
            earliest_start = span.start;
        }
        if span.end > latest_end {
            latest_end = span.end;
        }

        combined_type = FieldType::Union(
            FieldArity::Required,
            seen_types.clone(),
            Span {
                file: combined_type.span().file.clone(),
                start: earliest_start,
                end: latest_end,
            },
            None,
        );
    }

    Some(combined_type)
}

#[cfg(test)]
mod tests {

    use super::super::{BAMLParser, Rule};
    use super::*;
    use crate::test_parse_baml_type;
    use baml_types::TypeValue;
    use internal_baml_diagnostics::{Diagnostics, SourceFile};
    use pest::Parser;

    #[test]
    fn type_union_association() {
        let root_path = "test_file.baml";

        let input = r#"int | (string @description("hi"))"#;
        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);
        let parsed = BAMLParser::parse(Rule::field_type_chain, input)
            .unwrap()
            .next()
            .unwrap();
        let result = parse_field_type_chain(parsed, &mut diagnostics).unwrap();
        if let FieldType::Union(_, types, _, _) = &result {
            assert_eq!(types[1].clone().attributes().len(), 1);
            assert_eq!(
                types[1].clone().attributes()[0].name.to_string().as_str(),
                "description"
            );
        } else {
            panic!("Expected union");
        }
    }

    #[test]
    fn field_union_association() {
        let root_path = "test_file.baml";

        let input = r#"bar int | (string @description("hi")) @description("hi")"#;
        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);
        let parsed = BAMLParser::parse(Rule::type_expression, input)
            .unwrap()
            .next()
            .unwrap();
        let result =
            parse_type_expr(&None, "class", parsed, None, &mut diagnostics, false).unwrap();
        assert_eq!(result.name.to_string().as_str(), "bar");
        assert_eq!(result.attributes().len(), 1);
        assert_eq!(
            result.attributes()[0].name.to_string().as_str(),
            "description"
        );
    }

    #[test]
    fn test_primitive() {
        test_parse_baml_type! {
            source: r#"int"#,
            target: FieldType::Primitive(
                FieldArity::Required,
                TypeValue::Int,
                Span::fake(),
                Some(vec![])
            ),
        }
    }

    #[test]
    fn int_with_attribute() {
        test_parse_baml_type! {
            source: r#"int @description("hi")"#,
            target: mk_int(Some(vec![mk_description("hi", false)])),
        }
    }

    #[test]
    fn parenthesized_int_with_attribute() {
        test_parse_baml_type! {
            source: r#"(int @description("hi")) | string @description("there")"#,
            target: FieldType::Union(
                FieldArity::Required,
                vec![
                    mk_int(Some(vec![mk_description("hi", true)])),
                    mk_string(Some(vec![])),
                ],
                Span::fake(),
                Some(vec![mk_description("there", false)]),
            ),
        }
    }

    #[test]
    fn parenthesized_int_or_string_with_attribute() {
        test_parse_baml_type! {
            source: r#"(int @description("hi")) | (string @description("there")) @description("everyone")"#,
            target: FieldType::Union(
                FieldArity::Required,
                vec![
                    mk_int(Some(vec![mk_description("hi", true)])),
                    mk_string(Some(vec![mk_description("there", true)])),
                ],
                Span::fake(),
                Some(vec![mk_description("everyone", false)]),
            ),
        }
    }

    #[test]
    fn nested_parentheses() {
        test_parse_baml_type! {
            source: r#"(int | (bool | string)) @description("hi")"#,
            target: FieldType::Union(
                FieldArity::Required,
                vec![
                    mk_int(Some(vec![])),
                    FieldType::Union(
                        FieldArity::Required,
                        vec![
                            mk_bool(Some(vec![])),
                            mk_string(Some(vec![])),
                        ],
                        Span::fake(),
                        Some(vec![])
                    )
                ],
                Span::fake(),
                Some(vec![mk_description("hi", false)])
            ),
        }
    }

    #[test]
    fn union_array() {
        test_parse_baml_type! {
            source: r#"(int | string)[] @description("hi")"#,
            target: FieldType::List(
                FieldArity::Required,
                Box::new(FieldType::Union(
                    FieldArity::Required,
                    vec![
                        mk_int(Some(vec![])),
                        mk_string(Some(vec![]))
                    ],
                    Span::fake(),
                    Some(vec![])
                )),
                1,
                Span::fake(),
                Some(vec![mk_description("hi", false)])
            ),
        }
    }

    #[test]
    fn optional_union() {
        test_parse_baml_type! {
            source: r#"(int | string)? @description("hi")"#,
            target: FieldType::Union(
                    FieldArity::Optional,
                    vec![
                        mk_int(Some(vec![])),
                        mk_string(Some(vec![])),
                    ],
                    Span::fake(),
                    Some(vec![mk_description("hi", false)])
                ),
        }
    }

    #[test]
    fn optional_union_inner_attribute() {
        test_parse_baml_type! {
            source: r#"(int | (string @description("stringdesc")))? @description("hi")"#,
            target: FieldType::Union(
                    FieldArity::Optional,
                    vec![
                        mk_int(Some(vec![])),
                        mk_string(Some(vec![mk_description("stringdesc", true)])),
                    ],
                    Span::fake(),
                    Some(vec![mk_description("hi", false)])
                ),
        }
    }

    #[test]
    fn union_list_inner_attribute() {
        test_parse_baml_type! {
            source: r#"(int | (string @description("stringdesc")))[] @description("hi")"#,
            target: FieldType::List(
                    FieldArity::Required,
                    Box::new(
                        FieldType::Union(
                            FieldArity::Required,
                            vec![
                                mk_int(Some(vec![])),
                                mk_string(Some(vec![mk_description("stringdesc", true)])),
                            ],
                            Span::fake(),
                            Some(vec![])
                        )
                    ),
                    1,
                    Span::fake(),
                    Some(vec![mk_description("hi", false)])
                ),
        }
    }

    #[test]
    fn union_list_inner_attribute_union_descr() {
        test_parse_baml_type! {
            source: r#"(int | (string @description("stringdesc")) @description("union"))[] @description("hi")"#,
            target: FieldType::List(
                FieldArity::Required,
                Box::new(
                    FieldType::Union(
                        FieldArity::Required,
                        vec![
                            mk_int(Some(vec![])),
                            mk_string(Some(vec![mk_description("stringdesc", true)])),
                        ],
                        Span::fake(),
                        Some(vec![mk_description("union", false)])
                    )
                ),
                1,
                Span::fake(),
                Some(vec![mk_description("hi", false)])
            ),
        }
    }

    // Convenience functions.

    fn mk_int(attrs: Option<Vec<Attribute>>) -> FieldType {
        FieldType::Primitive(FieldArity::Required, TypeValue::Int, Span::fake(), attrs)
    }
    fn mk_bool(attrs: Option<Vec<Attribute>>) -> FieldType {
        FieldType::Primitive(FieldArity::Required, TypeValue::Bool, Span::fake(), attrs)
    }
    fn mk_string(attrs: Option<Vec<Attribute>>) -> FieldType {
        FieldType::Primitive(FieldArity::Required, TypeValue::String, Span::fake(), attrs)
    }
    fn mk_null(attrs: Option<Vec<Attribute>>) -> FieldType {
        FieldType::Primitive(FieldArity::Required, TypeValue::Null, Span::fake(), attrs)
    }

    fn mk_description(value: &'static str, parenthesized: bool) -> Attribute {
        Attribute {
            name: ("description", Span::fake()).into(),
            parenthesized,
            arguments: ArgumentsList {
                arguments: vec![Argument {
                    value: Expression::StringValue(value.to_string(), Span::fake()),
                    span: Span::fake(),
                }],
            },
            span: Span::fake(),
        }
    }
}
