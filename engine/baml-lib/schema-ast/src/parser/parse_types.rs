use core::str;

use super::{helpers::Pair, parse_attribute::parse_attribute, Rule};
use crate::{
    assert_correct_parser,
    ast::*,
    parser::{parse_field::parse_field_type_with_attr, parse_identifier::parse_identifier},
    unreachable_rule,
};
use baml_types::{LiteralValue, TypeValue};
use internal_baml_diagnostics::Diagnostics;

pub fn parse_field_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::field_type, Rule::openParan, Rule::closeParan);

    let mut arity = FieldArity::Required;
    let mut ftype = None;
    let mut attributes = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::union => {
                let result = parse_union(current, diagnostics);
                ftype = result;
            }
            Rule::non_union => {
                let result = parse_base_type(current, diagnostics);

                ftype = result;
            }
            Rule::field_attribute => {
                attributes.push(parse_attribute(current, false, diagnostics));
            }
            Rule::optional_token => arity = FieldArity::Optional,
            _ => {
                unreachable_rule!(current, Rule::field_type)
            }
        }
    }

    match ftype {
        Some(ftype) => {
            if arity.is_optional() {
                Some(ftype.to_nullable())
            } else {
                Some(ftype)
            }
        }
        None => {
            unreachable!("Ftype should always be defined")
        }
    }
}

fn parse_union(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::union);

    let span = diagnostics.span(pair.as_span());
    let mut types = Vec::new();
    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::base_type => {
                if let Some(f) = parse_base_type(current, diagnostics) {
                    types.push(f)
                }
            }
            Rule::base_type_with_attr => {
                if let Some(f) = parse_base_type_with_attr(current, diagnostics) {
                    types.push(f)
                }
            }
            Rule::field_operator => {}

            _ => unreachable_rule!(current, Rule::union),
        }
    }

    let mut union = match types.len() {
        0 => unreachable!("A union must have atleast 1 type"),
        1 => Some(types[0].to_owned()),
        _ => Some(FieldType::Union(FieldArity::Required, types, span, None)),
    };
    union.as_mut().map(|ft| reassociate_union_attributes(ft));
    union
}

fn parse_base_type_with_attr(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    let mut attributes = Vec::new();
    let mut base_type = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::base_type => {
                base_type = parse_base_type(current, diagnostics);
            }
            Rule::field_attribute => {
                let att = parse_attribute(current, false, diagnostics);
                attributes.push(att);
            }
            _ => unreachable_rule!(current, Rule::base_type_with_attr),
        }
    }

    match base_type {
        Some(mut ft) => {
            ft.extend_attributes(attributes);
            Some(ft)
        }
        None => None,
    }
}

fn parse_base_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(
        pair,
        Rule::base_type,
        Rule::non_union,
        Rule::base_type_without_array
    );

    if let Some(current) = pair.into_inner().next() {
        return match current.as_rule() {
            Rule::identifier => {
                let identifier = parse_identifier(current.clone(), diagnostics);
                let field_type = match current.as_str() {
                    "string" | "int" | "float" | "bool" | "image" | "audio" => {
                        FieldType::Primitive(
                            FieldArity::Required,
                            TypeValue::from_str(identifier.name()).expect("Invalid type value"),
                            diagnostics.span(current.as_span()),
                            None,
                        )
                    }
                    "null" => FieldType::Primitive(
                        FieldArity::Optional,
                        TypeValue::Null,
                        diagnostics.span(current.as_span()),
                        None,
                    ),
                    _ => FieldType::Symbol(
                        FieldArity::Required,
                        Identifier::Local(
                            identifier.name().to_string(),
                            diagnostics.span(current.as_span()),
                        ),
                        None,
                    ),
                };
                Some(field_type)
            }
            Rule::array_notation => parse_array(current, diagnostics),
            Rule::map => parse_map(current, diagnostics),
            Rule::group => parse_group(current, diagnostics),
            Rule::tuple => parse_tuple(current, diagnostics),
            Rule::parenthesized_type => parse_parenthesized_type(current, diagnostics),
            Rule::literal_type => parse_literal_type(current, diagnostics),
            _ => unreachable_rule!(current, Rule::base_type),
        };
    }

    unreachable!("A base type must be one of the above");
}

fn parse_parenthesized_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::parenthesized_type);

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParan | Rule::closeParan => continue,
            Rule::field_type_with_attr => {
                return parse_field_type_with_attr(current, true, diagnostics);
            }
            _ => unreachable_rule!(current, Rule::parenthesized_type),
        }
    }

    unreachable!("impossible parenthesized parsing");
}

fn parse_literal_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::literal_type);

    let span = diagnostics.span(pair.as_span());

    let Some(literal_type) = pair.into_inner().next() else {
        unreachable!("impossible literal parsing");
    };

    let literal_value = match literal_type.as_rule() {
        Rule::quoted_string_literal => match literal_type.into_inner().next() {
            Some(string_content) => LiteralValue::String(string_content.as_str().into()),
            None => unreachable!("quoted string literal has no string content"),
        },
        Rule::numeric_literal => LiteralValue::Int(literal_type.as_str().parse().unwrap()), // TODO: Floats
        _ => unreachable_rule!(literal_type, Rule::literal_type),
    };

    Some(FieldType::Literal(
        FieldArity::Required,
        literal_value,
        span,
        None,
    ))
}

fn parse_array(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::array_notation);

    let mut dims = 0_u32;
    let mut field = None;
    let span = diagnostics.span(pair.as_span());
    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::base_type_without_array => field = parse_base_type(current, diagnostics),
            Rule::array_suffix => dims += 1,
            _ => unreachable_rule!(current, Rule::map),
        }
    }

    match field {
        Some(field) => Some(FieldType::List(
            FieldArity::Required,
            Box::new(field),
            dims,
            span,
            None,
        )),
        _ => unreachable!("Field must have been defined"),
    }
}

fn parse_map(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::map);

    let mut fields = Vec::new();
    let span = diagnostics.span(pair.as_span());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::field_type => {
                if let Some(f) = parse_field_type(current, diagnostics) {
                    fields.push(f)
                }
            }
            _ => unreachable_rule!(current, Rule::map),
        }
    }

    match fields.len() {
        0 => None,
        1 => None,
        2 => Some(FieldType::Map(
            FieldArity::Required,
            Box::new((fields[0].to_owned(), fields[1].to_owned())),
            span,
            None,
        )),
        _ => unreachable!("Maps must specify a key type and value type"),
    }
}

fn parse_group(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::group);
    let mut attributes = Vec::new();
    let mut field_type = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParan | Rule::closeParan => continue,
            Rule::field_type => {
                field_type = parse_field_type(current, diagnostics);
            }
            Rule::field_attribute => {
                let attr = parse_attribute(current, true, diagnostics);
                attributes.push(attr);
            }
            _ => unreachable_rule!(current, Rule::group),
        }
    }

    field_type
        .as_mut()
        .map(|ft| ft.extend_attributes(attributes));
    field_type
}

fn parse_tuple(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::tuple);

    let span = diagnostics.span(pair.as_span());

    let mut fields = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParan | Rule::closeParan => continue,

            Rule::field_type_with_attr => {
                if let Some(f) = parse_field_type_with_attr(current, false, diagnostics) {
                    fields.push(f)
                }
            }
            Rule::field_type => {
                if let Some(f) = parse_field_type(current, diagnostics) {
                    fields.push(f)
                }
            }
            _ => unreachable_rule!(current, Rule::tuple),
        }
    }

    match fields.len() {
        0 => None,
        1 => Some(fields[0].to_owned()),
        _ => Some(FieldType::Tuple(FieldArity::Required, fields, span, None)),
    }
}

/// For the last variant of a Union, remove the attributes from that variant
/// and attach them to the union, unless the attribute was tagged with the
/// `parenthesized` field.
///
/// This is done because `field_foo int | string @description("d")`
/// is naturally parsed as a field with a union whose secord variant has
/// a description. But the correct BAML interpretation is a union with a
/// description.
pub fn reassociate_union_attributes(field_type: &mut FieldType) {
    match field_type {
        FieldType::Union(_arity, ref mut variants, _, _) => {
            if let Some(last_variant) = variants.last_mut() {
                let last_variant_attributes = last_variant.attributes().to_owned();
                let (attrs_for_variant, attrs_for_union): (Vec<Attribute>, Vec<Attribute>) =
                    last_variant_attributes
                        .into_iter()
                        .partition(|attr| attr.parenthesized);
                last_variant.set_attributes(attrs_for_variant);
                field_type.extend_attributes(attrs_for_union);
            }
        }
        _ => {
            panic!("Unexpected: `reassociate_union_attributes` should only be called when parsing a union.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{BAMLParser, Rule};
    use pest::{consumes_to, parses_to};

    #[test]
    fn type_attributes() {
        parses_to! {
            parser: BAMLParser,
            input: r#"int @description("hi")"#,
            rule: Rule::type_expression,
            tokens: [type_expression(0,22,[
                identifier(0,3, [
                    single_word(0, 3)
                ]),
                field_attribute(4,22,[
                    identifier(5,16,[
                        single_word(5,16)
                    ]),
                    arguments_list(16, 22, [
                        expression(17,21, [
                            string_literal(17,21,[
                                quoted_string_literal(17,21,[
                                  quoted_string_content(18,20)
                                ])
                            ])
                        ])
                    ])
                ])
              ])
            ]
        }
    }
}
