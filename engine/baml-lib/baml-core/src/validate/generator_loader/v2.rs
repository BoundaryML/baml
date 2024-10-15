use std::{collections::HashMap, path::PathBuf, str::FromStr};

use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{self, WithName, WithSpan};
use semver::Version;
use strum::VariantNames;

use crate::configuration::{
    CloudProject, CloudProjectBuilder, CodegenGeneratorBuilder, Generator,
    GeneratorDefaultClientMode, GeneratorOutputType,
};

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
    ast_generator: &ast::ValueExprBlock,
    baml_src: &PathBuf,
) -> Result<Generator, Vec<DatamodelError>> {
    let generator_name = ast_generator.name();

    let mut builder = CodegenGeneratorBuilder::default();

    builder
        .name(generator_name.into())
        .baml_src(baml_src.clone())
        .span(ast_generator.span().clone());

    let args = check_property_allowlist(generator_name, ast_generator)?;
    let mut errors = vec![];

    match parse_required_key(&args, "output_type", ast_generator.span()) {
        Ok((name, name_span)) => match GeneratorOutputType::from_str(name) {
            Ok(lang) => {
                builder.output_type(lang);
            }
            Err(_) => {
                if name == "cloud" {
                    let mut cloud_builder = CloudProjectBuilder::default();
                    cloud_builder
                        .name(generator_name.to_string())
                        .baml_src(baml_src.clone())
                        .span(ast_generator.span().clone());
                    parse_cloud_project(ast_generator, &args, &mut cloud_builder)?;
                    return match cloud_builder.build() {
                        Ok(generator) => Ok(Generator::BoundaryCloud(generator)),
                        Err(e) => Err(vec![DatamodelError::new_anyhow_error(
                            anyhow::Error::from(e).context("Error parsing generator"),
                            ast_generator.span().clone(),
                        )]),
                    };
                } else {
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

    match parse_optional_key(&args, "version") {
        Ok(Some(version_str)) => match Version::parse(version_str) {
            Ok(version) => {
                builder.version(version.to_string());
            }
            Err(_) => {
                errors.push(DatamodelError::new_validation_error(
                    &format!("Invalid semver version string: '{}'", version_str),
                    args.get("version")
                        .map(|arg| arg.span().clone())
                        .unwrap_or_else(|| ast_generator.span().clone()),
                ));
            }
        },
        Ok(None) => {
            builder.version("0.0.0".to_string());
        }
        Err(err) => {
            errors.push(err);
        }
    }

    match parse_optional_key(&args, "default_client_mode") {
        Ok(Some("sync")) => {
            builder.default_client_mode(Some(GeneratorDefaultClientMode::Sync));
        }
        Ok(Some("async")) => {
            builder.default_client_mode(Some(GeneratorDefaultClientMode::Async));
        }
        Ok(Some(name)) => {
            errors.push(DatamodelError::new_validation_error(
                &format!("'{}' is not supported. Use one of: 'async' or 'sync'", name),
                args.get("default_client_mode")
                    .map(|arg| arg.span())
                    .unwrap_or_else(|| ast_generator.span())
                    .clone(),
            ));
        }
        Ok(None) => {
            builder.default_client_mode(None);
        }
        Err(err) => {
            errors.push(err);
        }
    }

    match parse_optional_key(&args, "on_generate") {
        Ok(Some(cmd)) => {
            builder.on_generate(vec![cmd.to_string()]);
        }
        Ok(None) => {
            builder.on_generate(vec![]);
        }
        Err(err) => {
            errors.push(err);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    match builder.build() {
        Ok(generator) => Ok(Generator::Codegen(generator)),
        Err(e) => Err(vec![DatamodelError::new_anyhow_error(
            anyhow::Error::from(e).context("Error parsing generator"),
            ast_generator.span().clone(),
        )]),
    }
}

fn parse_cloud_project(
    ast_generator: &ast::ValueExprBlock,
    args: &HashMap<&str, &ast::Expression>,
    builder: &mut CloudProjectBuilder,
) -> Result<(), Vec<DatamodelError>> {
    let mut errors = vec![];
    match parse_optional_key(&args, "version") {
        Ok(Some(version_str)) => match Version::parse(version_str) {
            Ok(version) => {
                builder.version(version.to_string());
            }
            Err(_) => {
                errors.push(DatamodelError::new_validation_error(
                    &format!("Invalid semver version string: '{}'", version_str),
                    args.get("version")
                        .map(|arg| arg.span().clone())
                        .unwrap_or_else(|| ast_generator.span().clone()),
                ));
            }
        },
        Ok(None) => {
            builder.version("0.0.0".to_string());
        }
        Err(err) => {
            errors.push(err);
        }
    }

    match parse_optional_key(&args, "project_id") {
        Ok(Some(project_id)) => {
            builder.project_id(project_id.to_string());
        }
        Ok(None) => {
            errors.push(DatamodelError::new_validation_error(
                "`project_id` is required for a boundary-cloud generator.",
                args.get("project_id")
                    .map(|arg| arg.span().clone())
                    .unwrap_or_else(|| ast_generator.span().clone()),
            ));
        }
        Err(err) => {
            errors.push(err);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}

fn check_property_allowlist<'ir>(
    generator_name: &str,
    ast_generator: &'ir ast::ValueExprBlock,
) -> Result<HashMap<&'ir str, &'ir ast::Expression>, Vec<DatamodelError>> {
    const FIRST_CLASS_PROPERTIES: &[&str] = &[
        "output_type",
        "output_dir",
        "version",
        "default_client_mode",
        "on_generate",
        "project_id",
    ];

    let mut errors = vec![];
    let args = ast_generator
        .fields()
        .iter()
        .map(|arg| match &arg.expr {
            Some(expr) => {
                if FIRST_CLASS_PROPERTIES.iter().any(|k| *k == arg.name()) {
                    Ok((arg.name(), expr))
                } else {
                    Err(DatamodelError::new_property_not_known_error(
                        arg.name(),
                        arg.span().clone(),
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

    Ok(args)
}
