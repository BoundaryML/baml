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
                    name: current.as_str().to_string(),
                    span: current.as_span().into(),
                },
                Rule::single_word => {
                    diagnostics.push_error(DatamodelError::new_validation_error(
                        "Identifiers must be capitalized.",
                        current.as_span().into(),
                    ));
                    Identifier {
                        name: current.as_str().to_string(),
                        span: current.as_span().into(),
                    }
                }
                _ => unreachable!(
                    "Encountered impossible field during parsing: {:?}",
                    current.tokens()
                ),
            }
        }
        Rule::attribute_name | Rule::single_word => Identifier {
            name: pair.as_str().to_string(),
            span: pair.as_span().into(),
        },
        _ => unreachable!(
            "Encountered impossible field during parsing: {:?} {:?}",
            pair.as_rule(),
            pair.tokens()
        ),
    }
}
