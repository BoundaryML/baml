use baml_types::ConstraintLevel;
use internal_baml_diagnostics::DatamodelError;

use super::{
    helpers::{assert_correct_parser, parsing_catch_all, Pair},
    parse_identifier::{parse_identifier, parse_path_identifier},
    Rule,
};
use crate::{ast::*, parser::parse_arguments::parse_arguments_list};

pub(crate) fn parse_attribute(
    pair: Pair<'_>,
    parenthesized: bool,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Attribute {
    assert_correct_parser(
        &pair,
        &[Rule::block_attribute, Rule::field_attribute],
        diagnostics,
    );

    let span = diagnostics.span(pair.as_span());
    let mut name = None;
    let mut arguments: ArgumentsList = ArgumentsList::default();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::identifier => name = parse_identifier(current, diagnostics).into(),
            Rule::path_identifier => name = parse_path_identifier(current, diagnostics).into(),
            Rule::arguments_list => {
                parse_arguments_list(current, &mut arguments, &name, diagnostics)
            }
            _ => parsing_catch_all(current, "attribute", diagnostics),
        }
    }

    match name {
        Some(name) => Attribute {
            name,
            arguments,
            parenthesized,
            span,
        },
        // This is suspicious, can probably cause a panic
        None => unreachable!("Name should always be defined for attribute."),
    }
}
