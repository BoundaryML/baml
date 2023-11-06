use super::{
    helpers::{parsing_catch_all, Pair},
    Rule,
};
use crate::{assert_correct_parser, ast::*, parser::parse_expression::parse_expression};

pub(crate) fn parse_template_args(
    token: Pair<'_>,
    diagnostics: &mut internal_baml_diagnostics::Diagnostics,
) -> Option<Vec<Expression>> {
    assert_correct_parser!(token, Rule::template_args);

    let mut template_args = Vec::new();
    for current in token.into_inner() {
        match current.as_rule() {
            Rule::empty_template_args => {
                return None;
            }
            Rule::expression => {
                template_args.push(parse_expression(current, diagnostics));
            }
            _ => parsing_catch_all(&current, "template args"),
        }
    }

    if template_args.is_empty() {
        return None;
    }

    Some(template_args)
}
