use super::{helpers::Pair, Rule};
use crate::{ast::*, parser::parse_expression::parse_expression};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub fn parse_field_type(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<(FieldArity, FieldType), DatamodelError> {
    assert!(pair.as_rule() == Rule::field_type);
    let current = pair.into_inner().next().unwrap();
    match current.as_rule() {
        Rule::field_type_base => parse_field_type_base(current, diagnostics),
        Rule::union_type => Ok((FieldArity::Required, parse_union_type(current, diagnostics))),
        Rule::union_list_type => Ok((
            FieldArity::List,
            parse_union_type(current.into_inner().next().unwrap(), diagnostics),
        )),
        _ => unreachable!(
            "Encountered impossible field during parsing: {:?} {:?}",
            current.as_rule(),
            current.tokens()
        ),
    }
}

fn parse_base_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> FieldType {
    let current = pair.clone().into_inner().next().unwrap();
    match current.as_rule() {
        Rule::primitive_types => FieldType::PrimitiveType(
            match current.as_str() {
                "String" => TypeValue::String,
                "Int" => TypeValue::Int,
                "Float" => TypeValue::Float,
                "Bool" => TypeValue::Bool,
                "Char" => TypeValue::Char,
                _ => unreachable!(
                    "Encountered impossible type during parsing: {:?}\n {:?} {:?}\n {:?}",
                    current.as_rule(),
                    pair.clone().as_str(),
                    current.as_str(),
                    current.tokens()
                ),
            },
            diagnostics.span(current.as_span()),
        ),
        Rule::identifier => FieldType::Supported(Identifier {
            path: None,
            name: current.as_str().to_string(),
            span: diagnostics.span(current.as_span()),
        }),
        _ => unreachable!(
            "Encountered impossible type during parsing:{:?} {:?}",
            current.as_rule(),
            current.tokens()
        ),
    }
}

fn parse_union_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> FieldType {
    assert!(
        pair.as_rule() == Rule::union_type,
        "Encountered impossible type during parsing: {:?}",
        pair.as_rule()
    );

    let span = diagnostics.span(pair.as_span());
    let mut candidates = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::field_type_base => {
                if let Ok(parsed) = parse_field_type_base(current, diagnostics) {
                    candidates.push(parsed);
                }
            }
            _ => unreachable!(
                "Encountered impossible type during parsing: {:?}",
                current.tokens()
            ),
        }
    }

    FieldType::Union(candidates, span)
}

fn parse_field_type_base(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<(FieldArity, FieldType), DatamodelError> {
    assert!(pair.as_rule() == Rule::field_type_base);
    let current = pair.into_inner().next().unwrap();
    match current.as_rule() {
        Rule::optional_type => Ok((
            FieldArity::Optional,
            parse_base_type(current.into_inner().next().unwrap(), diagnostics),
        )),
        Rule::base_type => Ok((FieldArity::Required, parse_base_type(current, diagnostics))),
        Rule::list_type => Ok((
            FieldArity::List,
            parse_base_type(current.into_inner().next().unwrap(), diagnostics),
        )),
        Rule::unsupported_optional_list_type => Err(DatamodelError::new_legacy_parser_error(
            "Optional lists are not supported. Use either `Type[]` or `Type?`.",
            diagnostics.span(current.as_span()),
        )),
        _ => unreachable!(
            "Encountered impossible type during parsing: {:?}",
            current.tokens()
        ),
    }
}
