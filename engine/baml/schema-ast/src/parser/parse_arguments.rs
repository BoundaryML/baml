use super::{
    helpers::{parsing_catch_all, Pair},
    parse_expression::parse_expression,
    Rule,
};
use crate::ast;
use internal_baml_diagnostics::Diagnostics;

pub(crate) fn parse_arguments_list(
    token: Pair<'_>,
    arguments: &mut ast::ArgumentsList,
    diagnostics: &mut Diagnostics,
) {
    debug_assert_eq!(token.as_rule(), Rule::arguments_list);
    for current in token.into_inner() {
        let current_span = current.as_span();
        match current.as_rule() {
            // At the top level only unnamed args are supported.
            // For multiple args, pass in a dictionary.
            Rule::expression => arguments.arguments.push(ast::Argument {
                name: None,
                value: parse_expression(current, diagnostics),
                span: ast::Span::from(current_span),
            }),
            _ => parsing_catch_all(&current, "attribute arguments"),
        }
    }
}
