use super::{
    helpers::{parsing_catch_all, Pair},
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_field::parse_field,
    Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_config_block(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Top {
    let pair_span = pair.as_span();
    let mut template_args = None;
    let mut name: Option<Identifier> = None;
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut fields: Vec<Field> = Vec::new();
    let mut kw = None;

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::template_args => {
                // TODO: Correctly parse template args.
                template_args = Some(current.as_str());
            }
            Rule::class_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::field_declaration => match parse_field(
                            &name.as_ref().unwrap().name,
                            "client",
                            item,
                            pending_field_comment.take(),
                            diagnostics,
                        ) {
                            Ok(field) => fields.push(field),
                            Err(err) => diagnostics.push_error(err),
                        },
                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not a valid field or attribute definition.",
                                item.as_span().into(),
                            ))
                        }
                        _ => parsing_catch_all(&item, "model"),
                    }
                }
            }
            Rule::identifier => name = Some(current.into()),
            Rule::GENERATOR_KEYWORD | Rule::CLIENT_KEYWORD | Rule::VARIANT_KEYWORD => {
                kw = Some(current.as_str())
            }
            _ => parsing_catch_all(&current, "client"),
        }
    }

    match kw {
        Some("client") => Top::Client(Client {
            name: name.unwrap(),
            fields,
            attributes,
            documentation: doc_comment.and_then(parse_comment_block),
            span: Span::from(pair_span),
            client_type: template_args.unwrap_or("").to_string(),
        }),
        Some("generator") => {
            if !template_args.is_none() {
                diagnostics.push_error(DatamodelError::new_validation_error(
                    "Template arguments are not allowed for generators.",
                    Span::from(pair_span),
                ));
            }
            Top::Generator(GeneratorConfig {
                name: name.unwrap(),
                fields,
                attributes,
                documentation: doc_comment.and_then(parse_comment_block),
                span: Span::from(pair_span),
            })
        }
        _ => unreachable!("Encountered impossible model declaration during parsing",),
    }
}
