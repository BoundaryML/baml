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
        Rule::optional_type => Ok((
            FieldArity::Optional,
            parse_base_type(current.into_inner().next().unwrap(), diagnostics),
        )),
        Rule::base_type => Ok((FieldArity::Required, parse_base_type(current, diagnostics))),
        Rule::list_type => Ok((
            FieldArity::List,
            parse_base_type(current.into_inner().next().unwrap(), diagnostics),
        )),
        _ => unreachable!(
            "Encountered impossible field during parsing: {:?}",
            current.tokens()
        ),
    }
}

fn parse_base_type(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> FieldType {
    let current = pair.into_inner().next().unwrap();
    match current.as_rule() {
        // Rule::path => FieldType::Supported(Identifier {
        //     name: current.as_str().to_string(),
        //     span: Span::from(current.as_span()),
        // }),
        Rule::identifier => FieldType::Supported(Identifier {
            name: current.as_str().to_string(),
            span: Span::from(current.as_span()),
        }),
        Rule::primitive_types => FieldType::Supported(Identifier {
            name: current.as_str().to_string(),
            span: Span::from(current.as_span()),
        }),
        _ => unreachable!(
            "Encountered impossible type during parsing: {:?}",
            current.tokens()
        ),
    }
}
