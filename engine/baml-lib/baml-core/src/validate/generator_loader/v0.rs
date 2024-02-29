use std::{collections::HashMap, path::PathBuf};

use internal_baml_diagnostics::DatamodelError;
use internal_baml_schema_ast::ast::{self, WithName, WithSpan};

use crate::configuration::{Generator, GeneratorLanguage};

const LANGUAGE_KEY: &str = "language";
const OUTPUT_KEY: &str = "output";
const PROJECT_ROOT_KEY: &str = "project_root";
const PKG_MANAGER_KEY: &str = "pkg_manager";
const LANGUAGE_SETUP_PREFIX: &str = "python_setup_prefix";

const FIRST_CLASS_PROPERTIES: &[&str] = &[
    LANGUAGE_KEY,
    OUTPUT_KEY,
    PROJECT_ROOT_KEY,
    PKG_MANAGER_KEY,
    LANGUAGE_SETUP_PREFIX,
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
    baml_src_path: &PathBuf,
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

    let _ = match parse_required_key(&args, "language", ast_generator.span()) {
        Ok("python") => Some(GeneratorLanguage::Python),
        Ok("typescript") => {
            errors.push(DatamodelError::new_validation_error(
                "TypeScript is not supported yet.",
                ast_generator.span().clone(),
            ));
            None
        }
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

    let pkg_manager = match parse_optional_key(&args, PKG_MANAGER_KEY) {
        Ok(Some(val)) => Some(val),
        Ok(None) => {
            // Check if there's a pyproject.toml
            let pyproject_toml = baml_src_path.join("pyproject.toml");
            if pyproject_toml.exists() {
                Some("poetry")
            } else {
                // check if pip3 command exists
                match std::process::Command::new("pip3").arg("--version").output() {
                    Ok(output) if output.status.success() => Some("pip3"),
                    _ => Some("pip"),
                }
            }
        }
        Err(err) => {
            errors.push(err);
            None
        }
    };

    let command_prefix = match parse_optional_key(&args, LANGUAGE_SETUP_PREFIX) {
        Ok(Some(val)) => Some(val),
        Ok(None) => match pkg_manager {
            Some("poetry") => Some("poetry run"),
            _ => None,
        },
        Err(err) => {
            errors.push(err);
            None
        }
    };

    let project_root = match parse_optional_key(&args, OUTPUT_KEY) {
        Ok(Some(v)) => Some(PathBuf::from(v)),
        Ok(None) => Some(PathBuf::from("../")),
        Err(err) => {
            errors.push(err);
            None
        }
    };

    if !errors.is_empty() {
        return Err(errors);
    }

    let test_command = match command_prefix {
        Some(prefix) => format!("{} python -m pytest", prefix),
        None => "python -m pytest".into(),
    };

    let install_command = match pkg_manager {
        Some("poetry") => "poetry add baml@latest".into(),
        Some("pip3") => "pip3 install --upgrade baml".into(),
        Some("pip") => "pip install --upgrade baml".into(),
        _ => {
            errors.push(DatamodelError::new_validation_error(
                "No package manager specified.",
                ast_generator.span().clone(),
            ));
            return Err(errors);
        }
    };

    let package_version_command = match pkg_manager {
        Some("poetry") => "poetry show baml".into(),
        Some("pip3") => "pip3 show baml".into(),
        Some("pip") => "pip show baml".into(),
        _ => {
            errors.push(DatamodelError::new_validation_error(
                "No package manager specified.",
                ast_generator.span().clone(),
            ));
            return Err(errors);
        }
    };

    let project_root = project_root.unwrap();

    Generator::new(
        generator_name.to_string(),
        ast_generator
            .span()
            .file
            .path_buf()
            .parent()
            .unwrap()
            .join(project_root),
        GeneratorLanguage::Python,
        test_command,
        install_command,
        package_version_command,
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
