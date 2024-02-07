use std::{collections::HashMap, path::PathBuf};

use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{self, WithName, WithSpan};

use crate::configuration::{Generator, GeneratorLanguage};

const FIRST_CLASS_PROPERTIES: &[&str] = &[
    "language",
    "project_root",
    "test_command",
    "install_command",
    "package_version_command",
];

fn parse_required_key<'a>(
    map: &'a HashMap<&str, &ast::Expression>,
    key: &str,
    generator_span: &ast::Span,
) -> Result<&'a str, DatamodelError> {
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
        Some((name, _)) => Ok(name),
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

    let language = match parse_required_key(&args, "language", ast_generator.span()) {
        Ok("python") => Some(GeneratorLanguage::Python),
        Ok("typescript") => Some(GeneratorLanguage::TypeScript),
        Ok(name) => {
            errors.push(DatamodelError::new_validation_error(
                &format!("The language '{}' is not supported.", name),
                ast_generator.span().clone(),
            ));
            None
        }
        Err(err) => {
            errors.push(err);
            None
        }
    };

    let project_root = match parse_optional_key(&args, "project_root") {
        Ok(Some(name)) => Some(name),
        Ok(None) => "../".into(),
        Err(err) => {
            errors.push(err);
            None
        }
    };

    let test_command = match parse_required_key(&args, "test_command", ast_generator.span()) {
        Ok(name) => Some(name),
        Err(err) => {
            errors.push(err);
            None
        }
    };

    let install_command = match parse_required_key(&args, "install_command", ast_generator.span()) {
        Ok(name) => Some(name),
        Err(err) => {
            errors.push(err);
            None
        }
    };

    let package_version_command =
        match parse_required_key(&args, "package_version_command", ast_generator.span()) {
            Ok(name) => Some(name),
            Err(err) => {
                errors.push(err);
                None
            }
        };

    if !errors.is_empty() {
        return Err(errors);
    }
    let language = language.unwrap();

    // This is relative from ../main.baml
    let project_root = project_root.unwrap();
    let test_command = test_command.unwrap();
    let install_command = install_command.unwrap();
    let package_version_command = package_version_command.unwrap();

    Generator::new(
        generator_name.to_string(),
        ast_generator
            .span()
            .file
            .path_buf()
            .parent()
            .unwrap()
            .join(project_root),
        language,
        test_command.into(),
        install_command.into(),
        package_version_command.into(),
        None,
        None,
        ast_generator.span().clone(),
    )
    .map_err(|err| {
        vec![DatamodelError::new_validation_error(
            &format!("Failed to create generator: {}", err),
            ast_generator.span().clone(),
        )]
    })
}
