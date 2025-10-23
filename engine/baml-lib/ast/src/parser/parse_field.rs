use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_expression::parse_expression,
    parse_identifier::parse_identifier,
    parse_types::{parse_field_type, reassociate_union_attributes},
    Rule,
};
use crate::{ast::*, parser::parse_expression::parse_config_expression};

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
    let mut comment: Option<Comment> =
        block_comment.and_then(|c| parse_comment_block(c, diagnostics));

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::field_attribute => {
                attributes.push(parse_attribute(current, false, diagnostics));
            }
            Rule::trailing_comment => {
                comment = match (comment, parse_trailing_comment(current, diagnostics)) {
                    (c, None) | (None, c) => c,
                    (Some(existing), Some(new)) => Some(Comment {
                        text: [existing.text, new.text].join("\n"),
                    }),
                };
            }
            Rule::expression => field_type = Some(parse_expression(current, diagnostics)),
            Rule::config_expression => {
                field_type = Some(parse_config_expression(current, diagnostics))
            }

            _ => parsing_catch_all(current, "field", diagnostics),
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
            model_name.as_ref().map_or("<unknown>", Identifier::name),
            diagnostics.span(pair_span),
        )),
    }
}

/// Sort all attributes on a field into either field attributes or type attributes.
/// The name of the attribute fully determines whether it will be associated with
/// the field, or with the type.
fn reassociate_type_attributes(field_attributes: &mut Vec<Attribute>, field_type: &mut FieldType) {
    let mut all_attrs = field_type.attributes().to_owned();
    all_attrs.append(field_attributes);
    let (attrs_for_type, attrs_for_field): (Vec<_>, Vec<_>) = all_attrs
        .into_iter()
        .partition(|attr| TYPE_ATTRIBUTE_NAMES.contains(&attr.name()));
    field_type.set_attributes(attrs_for_type.clone());
    *field_attributes = attrs_for_field;
}

const TYPE_ATTRIBUTE_NAMES: [&str; 5] = [
    "assert",
    "check",
    "stream.done",
    "stream.with_state",
    "stream.not_null",
];

pub(crate) fn parse_type_expr(
    model_name: &Option<Identifier>,
    container_type: &'static str,
    pair: Pair<'_>,
    block_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
    _is_enum: bool,
) -> Result<Field<FieldType>, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let mut field_attributes = Vec::<Attribute>::new();
    let mut field_type = None;
    let mut comment: Option<Comment> =
        block_comment.and_then(|c| parse_comment_block(c, diagnostics));

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => {
                name = Some(parse_identifier(current, diagnostics));
            }
            Rule::trailing_comment => {
                comment = merge_comments(comment, parse_trailing_comment(current, diagnostics));
            }
            Rule::field_type_chain => {
                field_type = parse_field_type_chain(current, diagnostics);
            }
            Rule::field_attribute => {
                let attribute = parse_attribute(current, false, diagnostics);
                field_attributes.push(attribute);
            }
            _ => parsing_catch_all(current, "field", diagnostics),
        }
    }

    // Strip certain attributes from the field and attach them to the type.
    match field_type.as_mut() {
        None => {}
        Some(ft) => reassociate_type_attributes(&mut field_attributes, ft),
    }

    match (name, &field_type) {
        // Class field.
        (Some(name), Some(field_type)) => Ok(Field {
            expr: Some(field_type.clone()),
            name,
            attributes: field_attributes,
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
            _ => {
                diagnostics.push_error(DatamodelError::new_model_validation_error(
                    "Unexpected token in field type chain",
                    "field_type_chain",
                    "<unknown>",
                    diagnostics.span(current.as_span()),
                ));
            }
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
                parsing_catch_all(current, "field_type_with_attr!", diagnostics);
            }
        }
    }

    match field_type {
        Some(mut ft) => {
            // ft.set_attributes(field_attributes);
            if let FieldType::Union(_arity, ref mut _variants, _, _) = &mut ft {
                reassociate_union_attributes(&mut ft);

                // if let Some(attributes) = attributes.as_ref() {
                //     ft.set_attributes(attributes.clone()); // Clone the borrowed `Vec<Attribute>`
                // }
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

    // In a union, use the attributes associated with the last type as the
    // attributes of the union. Example:
    //
    // field: string? | int @alias("hello")
    //
    // The alias is part of the union.
    let last_field_attrs = types.last().map(|t| t.attributes().to_vec());

    let mut earliest_start = combined_type.span().start;
    let mut latest_end = combined_type.span().end;

    for next_type in types.into_iter().skip(1) {
        let span = next_type.span().to_owned();
        seen_types.push(next_type);

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

    // We know it's a union because it was assigned above in the for loop.
    if let FieldType::Union(_, _, _, attrs) = &mut combined_type {
        *attrs = last_field_attrs;
    }

    Some(combined_type)
}

#[cfg(test)]
mod tests {

    use baml_types::TypeValue;
    use internal_baml_diagnostics::{Diagnostics, SourceFile};
    use pest::Parser;

    use super::{
        super::{BAMLParser, Rule},
        *,
    };
    use crate::test_parse_baml_type;

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

    #[test]
    fn streaming_attributes() {
        test_parse_baml_type! {
            source: r#"int @stream.done @stream.not_null @stream.with_state"#,
            target: FieldType::Primitive(
                FieldArity::Required,
                TypeValue::Int,
                Span::fake(),
                Some(vec![mk_bare_attribute("stream.done"), mk_bare_attribute("stream.not_null"), mk_bare_attribute("stream.with_state")])
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
    #[allow(dead_code)]
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

    fn mk_bare_attribute(value: &'static str) -> Attribute {
        Attribute {
            name: (value, Span::fake()).into(),
            parenthesized: false,
            arguments: ArgumentsList {
                arguments: Vec::new(),
            },
            span: Span::fake(),
        }
    }
}
