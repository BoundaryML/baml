use crate::{
    ast::WithSpan,
    configuration::{Generator, GeneratorConfigValue},
    internal_baml_diagnostics::*,
};
use internal_baml_parser_database::{
    ast::{self, Expression, WithDocumentation, WithName},
    coerce,
};
use internal_baml_schema_ast::ast::WithIdentifier;
use std::{collections::HashMap, path::PathBuf};

const LANGUAGE_KEY: &str = "language";
const OUTPUT_KEY: &str = "output";
const PKG_MANAGER_KEY: &str = "pkg_manager";

const FIRST_CLASS_PROPERTIES: &[&str] = &[LANGUAGE_KEY, OUTPUT_KEY, PKG_MANAGER_KEY];

fn convert_python_version_to_rust_semver(py_version: &str) -> String {
    // Replace '.pre' with '-pre.' for Rust's semver compatibility
    py_version.replacen(".dev", "-dev.", 1)
}

/// Load and validate Generators defined in an AST.
pub(crate) fn load_generators_from_ast<'i>(
    ast_schema: &'i ast::SchemaAst,
    diagnostics: &'i mut Diagnostics,
) -> Vec<Generator> {
    let mut generators: Vec<Generator> = Vec::new();

    for gen in ast_schema.generators() {
        if let Some(generator) = lift_generator(gen, diagnostics) {
            generators.push(generator)
        }
    }

    generators
}

fn lift_generator(
    ast_generator: &ast::GeneratorConfig,
    diagnostics: &mut Diagnostics,
) -> Option<Generator> {
    let generator_name = ast_generator.name();
    let args: HashMap<_, &Expression> = ast_generator
        .fields()
        .iter()
        .map(|arg| match &arg.value {
            Some(expr) => Some((arg.name(), expr)),
            None => {
                diagnostics.push_error(DatamodelError::new_config_property_missing_value_error(
                    arg.name(),
                    generator_name,
                    "generator",
                    ast_generator.span().clone(),
                ));

                None
            }
        })
        .collect::<Option<HashMap<_, _>>>()?;

    if let Some(expr) = args.get(LANGUAGE_KEY) {
        if !expr.is_string() {
            diagnostics.push_error(DatamodelError::new_type_mismatch_error(
                "string",
                expr.describe_value_type(),
                &expr.to_string(),
                expr.span().clone(),
            ))
        }
    }

    let language = match args.get(LANGUAGE_KEY) {
        Some(val) => coerce::string(val, diagnostics)?,
        None => {
            diagnostics.push_error(DatamodelError::new_generator_argument_not_found_error(
                LANGUAGE_KEY,
                &ast_generator.name(),
                ast_generator.span().clone(),
            ));
            return None;
        }
    };

    let pkg_manager = match args.get(PKG_MANAGER_KEY) {
        Some(val) => Some(coerce::string(val, diagnostics)?),
        None => match language {
            "python" => {
                // Check if there's a pyproject.toml
                let pyproject_toml = diagnostics.root_path.join("../pyproject.toml");
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
            _ => None,
        },
    };

    let output = args
        .get(OUTPUT_KEY)
        .and_then(|v| coerce::path(v, diagnostics))
        .and_then(|v| Some(PathBuf::from(v)))
        .unwrap_or(PathBuf::from("../"))
        .join("baml_client");

    let mut properties = HashMap::new();
    for prop in ast_generator.fields() {
        let is_first_class_prop = FIRST_CLASS_PROPERTIES.iter().any(|k| *k == prop.name());
        if is_first_class_prop {
            continue;
        }

        let value = match &prop.value {
            Some(val) => GeneratorConfigValue::from(val),
            None => {
                diagnostics.push_error(DatamodelError::new_config_property_missing_value_error(
                    prop.name(),
                    generator_name,
                    "generator",
                    prop.span.clone(),
                ));
                continue;
            }
        };

        properties.insert(prop.name().to_string(), value);
    }

    // Call python -m baml_client to get the version
    let client_version = match (language, pkg_manager) {
        ("python", Some("poetry")) => std::process::Command::new("poetry")
            .arg("run")
            .arg("python")
            .arg("-m")
            .arg("baml_version")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    match String::from_utf8(output.stdout).ok() {
                        Some(v) => Some(convert_python_version_to_rust_semver(&v.trim())),
                        None => None,
                    }
                } else {
                    None
                }
            })
            .map(|v| v.trim().to_string()),
        ("python", Some("pip") | None) => std::process::Command::new("python")
            .arg("-m")
            .arg("baml_version")
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    match String::from_utf8(output.stdout).ok() {
                        Some(v) => Some(convert_python_version_to_rust_semver(&v.trim())),
                        None => None,
                    }
                } else {
                    None
                }
            })
            .map(|v| v.trim().to_string()),
        _ => None,
    };

    Some(Generator {
        name: String::from(ast_generator.name()),
        language: String::from(language),
        pkg_manager: pkg_manager.map(String::from),
        source_path: diagnostics.root_path.clone(),
        output: match output.is_absolute() {
            true => output,
            false => diagnostics.root_path.join(output),
        },
        config: properties,
        documentation: ast_generator.documentation().map(String::from),
        client_version,
        span: Some(ast_generator.identifier().span().clone()),
    })
}
