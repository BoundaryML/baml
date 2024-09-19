use internal_baml_diagnostics::Diagnostics;

use crate::{
    assert_correct_parser,
    ast::{Identifier, RefIdentifier},
    parser::Rule,
    unreachable_rule,
};

use super::helpers::Pair;

pub fn parse_identifier(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Identifier {
    assert_correct_parser!(pair, Rule::identifier);

    if let Some(inner) = pair.into_inner().next() {
        return match inner.as_rule() {
            Rule::path_identifier => parse_path_identifier(inner, diagnostics),
            Rule::single_word => parse_single_word(inner, diagnostics),
            _ => unreachable_rule!(inner, Rule::identifier),
        };
    }
    unreachable!("Encountered impossible identifier during parsing.")
}

fn parse_single_word(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Identifier {
    assert_correct_parser!(pair, Rule::single_word);
    let span = diagnostics.span(pair.as_span());

    Identifier::from((pair.as_str(), span))
}

fn parse_path_identifier(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Identifier {
    assert_correct_parser!(pair, Rule::path_identifier);

    let span = diagnostics.span(pair.as_span());
    let raw_str = pair.as_str();
    let mut vec = vec![];
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::single_word => vec.push(inner.as_str()),
            _ => unreachable_rule!(inner, Rule::path_identifier),
        }
    }

    // TODO: THIS IS SUSPECT
    assert!(
        vec.len() > 1,
        "Path identifier must have at least 2 elements. Path({}) Raw({})",
        vec.join("."),
        raw_str
    );

    if vec[0] == "env" {
        let env_name = vec[1..].join(".");
        return Identifier::ENV(env_name, span);
    }

    return Identifier::Ref(
        RefIdentifier {
            path: vec[..vec.len() - 1].iter().map(|s| s.to_string()).collect(),
            name: vec[vec.len() - 1].to_string(),
            full_name: vec.join("."),
        },
        span,
    );
}
