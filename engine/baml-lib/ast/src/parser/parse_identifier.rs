use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use super::helpers::Pair;
use crate::{
    assert_correct_parser,
    ast::{Identifier, RefIdentifier, WithName},
    parser::Rule,
    unreachable_rule,
};

pub fn parse_identifier(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Identifier {
    assert_correct_parser!(pair, Rule::identifier);

    if let Some(inner) = pair.into_inner().next() {
        return match inner.as_rule() {
            Rule::path_identifier => parse_path_identifier(inner, diagnostics),
            Rule::namespaced_identifier => parse_namespaced_identifier(inner, diagnostics),
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

pub fn parse_path_identifier(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Identifier {
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

    Identifier::Ref(
        RefIdentifier {
            path: vec[..vec.len() - 1].iter().map(|s| s.to_string()).collect(),
            name: vec[vec.len() - 1].to_string(),
            full_name: vec.join("."),
        },
        span,
    )
}

/// Parse an identifier of the form `word::word::word` directly into a that string.
/// TODO: `Identifier` should eventually store the namespace components
/// individually.
fn parse_namespaced_identifier(pair: Pair<'_>, diagnostics: &mut Diagnostics) -> Identifier {
    assert_correct_parser!(pair, Rule::namespaced_identifier);

    let raw_str = pair.as_str();
    let span = diagnostics.span(pair.as_span());
    let mut name_parts = Vec::new();
    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::single_word => name_parts.push(inner.as_str()),
            _ => unreachable_rule!(inner, Rule::namespaced_identifier),
        }
    }

    assert!(
        name_parts.len() > 1,
        "Namespaced identifier must have at least 2 elements. Parts({}) Raw({})",
        name_parts.join("::"),
        raw_str
    );

    Identifier::Local(name_parts.join("::"), span)
}
