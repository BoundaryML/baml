use super::{
    helpers::{parsing_catch_all, Pair},
    parse_identifier::parse_identifier,
    Rule,
};
use crate::{ast::*, parser::parse_arguments::parse_arguments_list};

pub(crate) fn parse_attribute(
    pair: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Attribute {
    let span = Span::from(pair.as_span());
    let mut name = None;
    let mut arguments: ArgumentsList = ArgumentsList::default();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::attribute_name => name = parse_identifier(current.into(), diagnostics),
            Rule::arguments_list => parse_arguments_list(current, &mut arguments, diagnostics),
            _ => parsing_catch_all(&current, "attribute"),
        }
    }

    let name = name.unwrap();
    Attribute {
        name,
        arguments,
        span,
    }
}
