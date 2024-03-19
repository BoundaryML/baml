use std::ops::Index;

use super::{
    helpers::{parsing_catch_all, Pair},
    parse_adapter::parse_adapter,
    parse_attribute::parse_attribute,
    parse_comments::*,
    parse_config::parse_key_value,
    parse_identifier::parse_identifier,
    parse_serializer::parse_serializer,
    parse_template_args::parse_template_args,
    Rule,
};
use crate::ast::*;
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

pub(crate) fn parse_variant_block(
    pair: Pair<'_>,
    doc_comment: Option<Pair<'_>>,
    diagnostics: &mut Diagnostics,
) -> Result<Variant, DatamodelError> {
    let pair_span = pair.as_span();
    let mut template_args = None;
    let mut name: Option<Identifier> = None;
    let mut serializers: Vec<Serializer> = Vec::new();
    let mut attributes: Vec<Attribute> = Vec::new();
    let mut fields: Vec<ConfigBlockProperty> = Vec::new();
    let mut adapters: Vec<Adapter> = Vec::new();

    for current in pair.into_inner() {
        match current.as_rule() {
            Rule::BLOCK_OPEN | Rule::BLOCK_CLOSE => {}
            Rule::template_args => template_args = parse_template_args(current, diagnostics),
            Rule::variant_contents => {
                let mut pending_field_comment: Option<Pair<'_>> = None;

                for item in current.into_inner() {
                    match item.as_rule() {
                        Rule::block_attribute => {
                            attributes.push(parse_attribute(item, diagnostics));
                        }
                        Rule::key_value => {
                            fields.push(parse_key_value(
                                item,
                                pending_field_comment.take(),
                                diagnostics,
                            ));
                        }
                        Rule::adapter_block => {
                            if let Some(f) = parse_adapter(item, None, diagnostics) {
                                adapters.push(f)
                            }
                        }
                        Rule::serializer_block => {
                            serializers.push(parse_serializer(item, None, diagnostics))
                        }
                        Rule::comment_block => pending_field_comment = Some(item),
                        Rule::BLOCK_LEVEL_CATCH_ALL => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                "This line is not a valid field or attribute definition.",
                                diagnostics.span(item.as_span()),
                            ))
                        }
                        _ => parsing_catch_all(&item, "model"),
                    }
                }
            }
            Rule::identifier => name = Some(parse_identifier(current, diagnostics)),
            Rule::VARIANT_KEYWORD => {}
            _ => parsing_catch_all(&current, "client"),
        }
    }

    match (name, template_args) {
        (_, None) => Err(DatamodelError::new_validation_error(
            "Missing template for impl. (did you forget <llm, FunctionName>)",
            diagnostics.span(pair_span.get(0..4).unwrap()),
        )),
        (Some(name), Some(args)) => match args.len() {
            2 => {
                let variant_type = args.index(0).as_constant_value().map(|f| f.0);
                if variant_type.is_none() {
                    return Err(DatamodelError::new_validation_error(
                        "impl's first template arg should be an executor. (did you forget <llm, FunctionName>).",
                        args[0].span().clone(),
                    ));
                }

                let identifier = args.index(1).as_identifer();
                if identifier.is_none() {
                    return Err(DatamodelError::new_validation_error(
                        "impl's second template arg should be a function name. (did you forget <llm, FunctionName>).",
                        args[1].span().clone(),
                    ));
                }
                Ok(Variant {
                    name,
                    fields,
                    serializers,
                    attributes,
                    adapters,
                    documentation: doc_comment.and_then(parse_comment_block),
                    span: diagnostics.span(pair_span),
                    variant_type: variant_type.unwrap().to_string(),
                    function_name: identifier.unwrap().to_owned(),
                })
            }
            _ => Err(DatamodelError::new_validation_error(
                "impl requires 2 template args. (did you forget <llm, FunctionName>)",
                diagnostics.span(pair_span.get(0..("impl".len())).unwrap()),
            )),
        },
        _ => unreachable!("Encountered impossible impl declaration during parsing",),
    }
}
