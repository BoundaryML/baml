use super::{helpers::Pair, Rule};
use crate::{
    assert_correct_parser, ast::*, parser::parse_identifier::parse_identifier, unreachable_rule,
};
use internal_baml_diagnostics::Diagnostics;

pub fn parse_field_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::field_type);

    let mut arity = FieldArity::Required;
    let mut ftype = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::union => ftype = parse_union(current, diagnostics),
            Rule::non_union => ftype = parse_base_type(current, diagnostics),
            Rule::optional_token => arity = FieldArity::Optional,
            _ => unreachable_rule!(current, Rule::field_type),
        }
    }

    match ftype {
        Some(ftype) => {
            if arity.is_optional() {
                match ftype.to_nullable() {
                    Ok(ftype) => return Some(ftype),
                    Err(e) => {
                        diagnostics.push_error(e);
                        return None;
                    }
                }
            }
            return Some(ftype);
        }
        _ => unreachable!("Ftype should aways be defiened"),
    }
}

fn parse_union(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::union);

    let span = diagnostics.span(pair.as_span());
    let mut types = Vec::new();
    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::base_type => {
                parse_base_type(current, diagnostics).map(|f| types.push(f));
            }
            _ => unreachable_rule!(current, Rule::union),
        }
    }

    match types.len() {
        0 => unreachable!("A union must have atleast 1 type"),
        1 => Some(types[0].to_owned()),
        _ => Some(FieldType::Union(FieldArity::Required, types, span)),
    }
}

fn parse_base_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(
        pair,
        Rule::base_type,
        Rule::non_union,
        Rule::base_type_without_array
    );

    for current in pair.into_inner() {
        return match current.as_rule() {
            Rule::identifier => Some(FieldType::Identifier(
                FieldArity::Required,
                parse_identifier(current, diagnostics),
            )),
            Rule::array_notation => parse_array(current, diagnostics),
            Rule::dict => parse_dict(current, diagnostics),
            Rule::group => parse_group(current, diagnostics),
            Rule::tuple => parse_tuple(current, diagnostics),
            _ => unreachable_rule!(current, Rule::base_type),
        };
    }

    unreachable!("A base type must be one of the above");
}

fn parse_array(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::array_notation);

    let mut dims = 0 as u32;
    let mut field = None;
    let span = diagnostics.span(pair.as_span());
    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::base_type_without_array => field = parse_base_type(current, diagnostics),
            Rule::array_suffix => dims += 1,
            _ => unreachable_rule!(current, Rule::dict),
        }
    }

    match field {
        Some(field) => Some(FieldType::List(Box::new(field), dims, span)),
        _ => unreachable!("Field must have been defined"),
    }
}

fn parse_dict(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::dict);

    let mut fields = Vec::new();
    let span = diagnostics.span(pair.as_span());

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::field_type => {
                parse_field_type(current, diagnostics).map(|f| fields.push(f));
            }
            _ => unreachable_rule!(current, Rule::dict),
        }
    }

    match fields.len() {
        0 => None,
        1 => None,
        2 => Some(FieldType::Dictionary(
            Box::new((fields[0].to_owned(), fields[1].to_owned())),
            span,
        )),
        _ => unreachable!("Impossible to have more than 2 field types in dictionary"),
    }
}

fn parse_group(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::group);
    for current in pair.into_inner() {
        return parse_field_type(current, diagnostics);
    }

    unreachable!("impossible group parsing");
}

fn parse_tuple(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Option<FieldType> {
    assert_correct_parser!(pair, Rule::tuple);

    let span = diagnostics.span(pair.as_span());

    let mut fields = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::field_type => {
                parse_field_type(current, diagnostics).map(|f| fields.push(f));
            }
            _ => unreachable_rule!(current, Rule::tuple),
        }
    }

    match fields.len() {
        0 => None,
        1 => Some(fields[0].to_owned()),
        _ => Some(FieldType::Tuple(FieldArity::Required, fields, span)),
    }
}
