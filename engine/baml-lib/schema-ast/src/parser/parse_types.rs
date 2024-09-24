use super::{helpers::Pair, parse_attribute::parse_attribute, Rule};
use crate::{
    assert_correct_parser,
    ast::*,
    parser::{parse_field::parse_field_type_with_attr, parse_identifier::parse_identifier},
    unreachable_rule,
};
use baml_types::TypeValue;
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
                let attribute = parse_attribute(current, diagnostics);
                attributes.push(attribute);
            }
            Rule::optional_token => arity = FieldArity::Optional,
            _ => {
                unreachable_rule!(current, Rule::field_type)
            }
        }
    }

    match ftype {
        Some(mut ftype) => {
            ftype.set_attributes(attributes);
            if arity.is_optional() {
                match ftype.to_nullable() {
                    Ok(ftype) => Some(ftype),
                    Err(_) => None,
                }
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

    match types.len() {
        0 => unreachable!("A union must have atleast 1 type"),
        1 => Some(types[0].to_owned()),
        _ => Some(FieldType::Union(FieldArity::Required, types, span, None)),
    }
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
                let att = parse_attribute(current, diagnostics);
                attributes.push(att);
            }
            _ => unreachable_rule!(current, Rule::base_type_with_attr),
        }
    }

    match base_type {
        Some(mut ft) => {
            ft.set_attributes(attributes);
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
                return parse_field_type_with_attr(current, diagnostics);
            }
            _ => unreachable_rule!(current, Rule::parenthesized_type),
        }
    }

    unreachable!("impossible parenthesized parsing");
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
        Some(field) => Some(FieldType::List(Box::new(field), dims, span, None)),
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
            Box::new((fields[0].to_owned(), fields[1].to_owned())),
            span,
            None,
        )),
        _ => unreachable!("Maps must specify a key type and value type"),
    }
}

fn parse_group(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::group);

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParan | Rule::closeParan => continue,
            Rule::field_type => {
                return parse_field_type(current, diagnostics);
            }
            _ => unreachable_rule!(current, Rule::group),
        }
    }

    unreachable!("impossible group parsing");
}

fn parse_tuple(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::tuple);

    let span = diagnostics.span(pair.as_span());

    let mut fields = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::openParan | Rule::closeParan => continue,

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

#[cfg(test)]
mod tests {
    use pest::parses_to;

    #[test]
    fn type_attributes() {
        parses_to!{
            parser: BAMLParser,
            rule: Rule::type_expression,
            input: r#"int @description("hi")"#,
            tokens: [],
        }
    }
}
