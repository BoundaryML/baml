use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_expression::parse_expression,
    parse_identifier::parse_identifier,
    parse_types::parse_field_type,
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
                attributes.push(parse_attribute(current, diagnostics));
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
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::trailing_comment => {
                comment = merge_comments(comment, parse_trailing_comment(current));
            }
            Rule::field_type_chain => {
                if !is_enum {
                    field_type = parse_field_type_chain(current, diagnostics);
                }
            }
            Rule::field_attribute => field_attributes.push(parse_attribute(current, diagnostics)),
            _ => parsing_catch_all(current, "field"),
        }
    }

    // dbg!(&field_attributes);
    // panic!("stop");

    // // Attributes that should be associated with a type may have been parsed along
    // // with the field and vice versa. This function associates attributes with
    // // the correct entity.
    // let (new_field_attributes, new_type_attributes) =
    //     partition_attributes(field_attributes, field_type.clone());
    // field_attributes = new_field_attributes;
    // field_type
    //     .as_mut()
    //     .map(|ft| ft.set_attributes(new_type_attributes));

    match (name, &field_type) {
        (Some(name), Some(field_type)) => Ok(Field {
            expr: Some(field_type.clone()),
            name,
            attributes: field_type.attributes().to_vec(),
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
                eprintln!("About to parse_field_type_with_attr from field_type_chain");
                if let Some(field_type) = parse_field_type_with_attr(current, diagnostics) {
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
    diagnostics: &mut Diagnostics,
) -> Option<FieldType> {
    eprintln!("Starting parse_field_type_with_attr");
    dbg!(&pair);
    let mut field_type = None;
    let mut field_attributes = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::field_type => field_type = parse_field_type(current, diagnostics),
            Rule::field_type_with_attr => {} // TODO: (Greg) Why do we need this match?
            Rule::field_attribute => field_attributes.push(parse_attribute(current, diagnostics)),
            Rule::trailing_comment => {}
            _ => {
                parsing_catch_all(current, "field_type_with_attr!");
            }
        }
    }

    match field_type {
        Some(mut ft) => {
            eprintln!("{:?}", ft);
            // ft.set_attributes(field_attributes);
            match &mut ft {
                FieldType::Union(_, ref mut types, _, _) => {
                    if let Some(last_type) = types.last_mut() {
                        // last_type.reset_attributes();
                        // log::info!("last_type: {:#?}", last_type);
                        let last_type_attributes = last_type.attributes().to_owned();
                        let mut new_attributes = last_type_attributes.clone();
                        new_attributes.extend(field_attributes);
                        ft.set_attributes(new_attributes);
                    }

                    // if let Some(attributes) = attributes.as_ref() {
                    //     ft.set_attributes(attributes.clone()); // Clone the borrowed `Vec<Attribute>`
                    // }
                }
                _ => {
                    ft.set_attributes(field_attributes);
                }
            }

            Some(ft) // Return the field type with attributes
        }
        None => {
            eprintln!("NO FIELD TYPE");
            None
        }
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

// fn partition_attributes(
//     input_field_attributes: Vec<Attribute>,
//     field_type: Option<FieldType>,
// ) -> (Vec<Attribute>, Vec<Attribute>) {
//     dbg!(&input_field_attributes);
//     dbg!(&field_type);
//     fn is_for_field(attr: &&Attribute) -> bool {
//         ["assert", "check"].contains(&attr.name())
//     }
//
//     let mut all_attrs = input_field_attributes.clone();
//     if let Some(ft) = field_type.as_ref() {
//         all_attrs.extend(ft.attributes().iter().cloned());
//     }
//
//     let (field_attrs, non_field_attrs): (Vec<&Attribute>, Vec<&Attribute>) =
//         all_attrs.iter().partition(is_for_field);
//
//     (
//         field_attrs.into_iter().cloned().collect(),
//         non_field_attrs.into_iter().cloned().collect(),
//     )
// }

#[cfg(test)]
mod tests {

    use super::super::{BAMLParser, Rule};
    use super::*;
    use internal_baml_diagnostics::{Diagnostics, SourceFile};
    use pest::Parser;

    #[test]
    fn union_association() {
        let root_path = "test_file.baml";

        // let input = r#"int | string @check({{true}}, "true")"#;
        // let source = SourceFile::new_static(root_path.into(), input);
        // let mut diagnostics = Diagnostics::new(root_path.into());
        // diagnostics.set_source(&source);
        // let parsed = BAMLParser::parse(Rule::field_type_chain, input)
        //     .unwrap()
        //     .next()
        //     .unwrap();
        // let result = parse_field_type_chain(parsed, &mut diagnostics).unwrap();
        // assert_eq!(result.attributes()[0].name.to_string().as_str(), "check");

        let input = r#"int | (string @check({{true}}, "true"))"#;
        let source = SourceFile::new_static(root_path.into(), input);
        let mut diagnostics = Diagnostics::new(root_path.into());
        diagnostics.set_source(&source);
        let parsed = BAMLParser::parse(Rule::field_type_chain, input)
            .unwrap()
            .next()
            .unwrap();
        let result = parse_field_type_chain(parsed, &mut diagnostics).unwrap();
        panic!("STOP");
        // dbg!(&result);
        // if let FieldType::Union(_, types, _, _) = &result {
        //     assert_eq!(types[1].attributes().len(), 1);
        // }
        // assert_eq!(result.attributes()[0].name.to_string().as_str(), "check");
    }
}
