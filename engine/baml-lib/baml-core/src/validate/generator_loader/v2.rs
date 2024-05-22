use std::{collections::HashMap, path::PathBuf, str::FromStr};

use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{self, WithName, WithSpan};
use strum::{VariantArray, VariantNames};

use crate::configuration::{Generator, GeneratorBuilder, GeneratorOutputType};

const FIRST_CLASS_PROPERTIES: &[&str] = &["output_type", "output_dir"];

fn parse_required_key<'a>(
    map: &'a HashMap<&str, &ast::Expression>,
    key: &str,
    generator_span: &ast::Span,
) -> Result<(&'a str, &'a ast::Span), DatamodelError> {
    let expr = match map.get(key) {
        Some(expr) => expr,
        None => {
            return Err(DatamodelError::new_validation_error(
                &format!("The `{}` argument is required for a generator.", key),
                generator_span.clone(),
            ))
        }
    };

    match expr.as_string_value() {
        Some((name, _)) => Ok((name, expr.span())),
        None => Err(DatamodelError::new_validation_error(
            &format!("`{}` must be a string.", key),
            expr.span().clone(),
        )),
    }
}

fn parse_optional_key<'a>(
    map: &'a HashMap<&str, &ast::Expression>,
    key: &str,
) -> Result<Option<&'a str>, DatamodelError> {
    let expr = match map.get(key) {
        Some(expr) => expr,
        None => {
            return Ok(None);
        }
    };

    match expr.as_string_value() {
        Some((name, _)) => Ok(Some(name)),
        None => Err(DatamodelError::new_validation_error(
            &format!("`{}` must be a string.", key),
            expr.span().clone(),
        )),
    }
}

pub(crate) fn parse_generator(
    ast_generator: &ast::GeneratorConfig,
    _baml_src_path: &PathBuf,
) -> Result<Generator, Vec<DatamodelError>> {
    let generator_name = ast_generator.name();

    let mut builder = GeneratorBuilder::default();

    builder
        .name(generator_name.into())
        .span(ast_generator.span().clone());

    let mut errors = vec![];
    let args = ast_generator
        .fields()
        .iter()
        .map(|arg| match &arg.value {
            Some(expr) => {
                if FIRST_CLASS_PROPERTIES.iter().any(|k| *k == arg.name()) {
                    Ok((arg.name(), expr))
                } else {
                    Err(DatamodelError::new_property_not_known_error(
                        arg.name(),
                        arg.span.clone(),
                        FIRST_CLASS_PROPERTIES.to_vec(),
                    ))
                }
            }
            None => Err(DatamodelError::new_config_property_missing_value_error(
                arg.name(),
                generator_name,
                "generator",
                arg.span().clone(),
            )),
        })
        .filter_map(|res| match res {
            Ok(val) => Some(val),
            Err(err) => {
                errors.push(err);
                None
            }
        })
        .collect::<HashMap<_, _>>();

    if !errors.is_empty() {
        return Err(errors);
    }

    match parse_required_key(&args, "output_type", ast_generator.span()) {
        Ok((name, name_span)) => match GeneratorOutputType::from_str(name) {
            Ok(lang) => {
                builder.output_type(lang);
            }
            Err(_) => {
                errors.push(DatamodelError::not_found_error(
                    "output_type",
                    name,
                    name_span.clone(),
                    GeneratorOutputType::VARIANTS
                        .iter()
                        .map(|s| s.to_string())
                        .collect(),
                ));
            }
        },
        Err(err) => {
            errors.push(err);
        }
    };

    match parse_optional_key(&args, "output_dir") {
        Ok(Some(name)) => {
            builder.output_dir(name.into());
        }
        Ok(None) => {
            builder.output_dir("../".into());
        }
        Err(err) => {
            errors.push(err);
        }
    };

    if !errors.is_empty() {
        return Err(errors);
    }

    builder.build().map_err(|e| {
        vec![DatamodelError::new_internal_error(
            anyhow::Error::from(e).context("Internal error while parsing generator (v1 syntax)"),
            ast_generator.span().clone(),
        )]
    })
}
