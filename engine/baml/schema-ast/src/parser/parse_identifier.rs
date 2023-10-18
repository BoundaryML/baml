use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use crate::{ast::Identifier, parser::Rule};

use super::helpers::Pair;

pub fn parse_identifier_string(
    pair: Pair<'_>,
    diagnostics: &mut Diagnostics,
) -> Result<(String, bool), ()> {
    assert!(pair.as_rule() == Rule::identifier);
    let current = pair.into_inner().next().unwrap();
    match current.as_rule() {
        Rule::valid_identifier => Ok((current.to_string(), true)),
        Rule::single_word => Ok((current.as_str().to_string(), false)),
        _ => unreachable!(
            "Encountered impossible field during parsing: {:?}",
            current.tokens()
        ),
    }
}

pub fn parse_identifier(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Identifier {
    match pair.as_rule() {
        Rule::identifier => {
            let current = pair.into_inner().next().unwrap();
            match current.as_rule() {
                Rule::valid_identifier => Identifier {
                    path: None,
                    name: current.as_str().to_string(),
                    span: diagnostics.span(current.as_span()),
                },
                Rule::single_word => {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "Identifiers must be capitalized.",
                        diagnostics.span(current.as_span()),
                    ));
                    Identifier {
                        path: None,
                        name: current.as_str().to_string(),
                        span: diagnostics.span(current.as_span()),
                    }
                }
                _ => unreachable!(
                    "Encountered impossible field during parsing: {:?}",
                    current.tokens()
                ),
            }
        }
        Rule::field_key | Rule::attribute_name | Rule::single_word => Identifier {
            path: None,
            name: pair.as_str().to_string(),
            span: diagnostics.span(pair.as_span()),
        },
        _ => unreachable!(
            "Encountered impossible field during parsing: {:?} {:?}",
            pair.as_rule(),
            pair.tokens()
        ),
    }
}
