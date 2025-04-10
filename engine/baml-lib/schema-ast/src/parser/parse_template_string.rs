use super::{
    helpers::{parsing_catch_all, Pair},
    parse_comments::*,
    parse_expression::parse_raw_string,
    parse_identifier::parse_identifier,
    parse_named_args_list::parse_named_argument_list,
    Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_template_string(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<TemplateString, DatamodelError> {
    let pair_span = pair.as_span();
    let mut name: Option<Identifier> = None;
    let attributes: Vec<Attribute> = Vec::new();
    let mut input = None;
    let mut value = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::TEMPLATE_KEYWORD => {}
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::assignment => {}
            Rule::named_argument_list => match parse_named_argument_list(current, diagnostics) {
                Ok(arg) => input = Some(arg),
                Err(err) => diagnostics.push_error(err),
            },
            Rule::raw_string_literal => {
                value = Some(Expression::RawStringValue(parse_raw_string(
                    current,
                    diagnostics,
                )))
            }
            _ => parsing_catch_all(current, "function"),
        }
    }

    let response = match name {
        Some(name) => {
            let msg = match value {
                Some(prompt) => {
                    return Ok(TemplateString {
                        name,
                        input,
                        value: prompt,
                        attributes,
                        documentation: doc_comment.and_then(parse_comment_block),
                        span: diagnostics.span(pair_span),
                    });
                }
                None => "Must have a prompt string.",
            };
            (msg, Some(name.name().to_string()))
        }
        None => ("Invalid template_string syntax.", None),
    };

    Err(DatamodelError::new_model_validation_error(
        format!(
            r##"{} Valid template_string syntax is
```
template_string {}(param1: String, param2: String) #"
    your template string here
"#
```"##,
            response.0,
            response.1.as_deref().unwrap_or("MyTemplateString")
        )
        .as_str(),
        "template_string",
        response.1.as_deref().unwrap_or("<unknown>"),
        diagnostics.span(pair_span),
    ))
}
