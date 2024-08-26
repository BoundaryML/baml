use super::{
    helpers::{parsing_catch_all, Pair},
    parse_expression::parse_expression,
    Rule,
};
use crate::ast::{self, Identifier};
use internal_baml_diagnostics::Diagnostics;

pub(crate) fn parse_arguments_list(
    token: Pair<'_>,
    arguments: &mut ast::ArgumentsList,
    _args_for: &Option<Identifier>,
    diagnostics: &mut Diagnostics,
) {
    debug_assert_eq!(token.as_rule(), Rule::arguments_list);
    for current in token.into_inner() {
        let current_span = current.as_span();
        match current.as_rule() {
            // At the top level only unnamed args are supported.
            // For multiple args, pass in a dictionary.
            Rule::expression => {
                if let Some(parsed_value) = parse_expression(current, diagnostics) {
                    arguments.arguments.push(ast::Argument {
                        value: parsed_value,
                        span: diagnostics.span(current_span),
                    });
                }
            }
            _ => parsing_catch_all(current, "attribute arguments"),
        }
    }
}
