use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use bstd::ProjectFqn;
use internal_baml_ast::ast::{self, WithName, WithSpan};
use internal_baml_diagnostics::DatamodelError;
use semver::Version;
use strum::VariantNames;

use crate::configuration::{
    CloudProject, CloudProjectBuilder, CodegenGeneratorBuilder, Generator,
    GeneratorDefaultClientMode, GeneratorOutputType, ModuleFormat,
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
                &format!("The `{key}` argument is required for a generator."),
                generator_span.clone(),
            ))
        }
    };

    match expr.as_string_value() {
        Some((name, _)) => Ok((name, expr.span())),
        None => Err(DatamodelError::new_validation_error(
            &format!("`{key}` must be a string."),
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
            &format!("`{key}` must be a string."),
            expr.span().clone(),
        )),
    }
}

pub(crate) fn parse_generator(
    ast_generator: &ast::ValueExprBlock,
    baml_src: &Path,
) -> Result<Generator, Vec<DatamodelError>> {
    let generator_name = ast_generator.name();

    let mut builder = CodegenGeneratorBuilder::default();

    builder
        .name(generator_name.into())
        .baml_src(baml_src.to_path_buf())
        .span(ast_generator.span().clone());

    let args = check_property_allowlist(generator_name, ast_generator)?;
    let mut errors = vec![];

    match parse_required_key(&args, "output_type", ast_generator.span()) {
        Ok((name, name_span)) => match GeneratorOutputType::from_str(name) {
            Ok(lang) => {
                builder.output_type(lang);
            }
            Err(_) => {
                const BOUNDARY_CLOUD_OUTPUT_TYPE: &str = "boundary-cloud";

                if name == BOUNDARY_CLOUD_OUTPUT_TYPE {
                    let mut cloud_builder = CloudProjectBuilder::default();
                    cloud_builder
                        .name(generator_name.to_string())
                        .baml_src(baml_src.to_path_buf())
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
                    log::error!("Unknown output type: {name}");
                    errors.push(DatamodelError::not_found_error(
                        "output_type",
                        name,
                        name_span.clone(),
                        GeneratorOutputType::VARIANTS
                            .iter()
                            .chain([BOUNDARY_CLOUD_OUTPUT_TYPE].iter())
                            .map(|s| s.to_string())
                            .collect(),
                        false,
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
                    &format!("Invalid semver version string: '{version_str}'"),
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
                &format!("'{name}' is not supported. Use one of: 'async' or 'sync'"),
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

    match parse_optional_key(&args, "client_package_name") {
        Ok(Some(name)) => {
            builder.client_package_name(Some(name.to_string()));
        }
        Ok(None) => {
            builder.client_package_name(None);
        }
        Err(err) => {
            errors.push(err);
        }
    }

    match parse_optional_key(&args, "module_format") {
        Ok(Some("cjs")) => {
            builder.module_format(Some(ModuleFormat::Cjs));
        }
        Ok(Some("esm")) => {
            builder.module_format(Some(ModuleFormat::Esm));
        }
        Ok(Some(name)) => {
            errors.push(DatamodelError::new_validation_error(
                &format!("'{name}' is not supported. Use one of: 'cjs' or 'esm'"),
                args.get("module_format")
                    .map(|arg| arg.span().clone())
                    .unwrap_or_else(|| ast_generator.span().clone()),
            ));
        }
        Ok(None) => {
            // TODO: add a warning if not set?
            builder.module_format(None);
        }
        Err(err) => {
            errors.push(err);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    match builder.build() {
        Ok(generator) => {
            if matches!(generator.output_type, GeneratorOutputType::Go) {
                // check that the client_package_name is a valid go package name
                if generator.client_package_name.is_none() {
                    return Err(vec![DatamodelError::new_validation_error(
                        "client_package_name is required for a go generator",
                        ast_generator.span().clone(),
                    )]);
                }
            }
            Ok(Generator::Codegen(generator))
        }
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
    match parse_optional_key(args, "version") {
        Ok(Some(version_str)) => match Version::parse(version_str) {
            Ok(version) => {
                builder.version(version.to_string());
            }
            Err(_) => {
                errors.push(DatamodelError::new_validation_error(
                    &format!("Invalid semver version string: '{version_str}'"),
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

    match parse_optional_key(args, "project") {
        Ok(Some(project_fqn_str)) => match ProjectFqn::parse(project_fqn_str) {
            Ok(project_fqn) => {
                builder.project_fqn(project_fqn);
            }
            Err(e) => {
                errors.push(DatamodelError::new_validation_error(
                    "`project` must be a fully-qualified project ID, i.e. @boundaryml/baml",
                    args.get("project")
                        .map(|arg| arg.span().clone())
                        .unwrap_or_else(|| ast_generator.span().clone()),
                ));
            }
        },
        Ok(None) => {
            errors.push(DatamodelError::new_validation_error(
                "`project` is required for a boundary-cloud generator.",
                args.get("project")
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
        "project",
        "client_package_name",
        "module_format",
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
