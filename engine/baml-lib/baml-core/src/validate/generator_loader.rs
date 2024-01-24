use crate::{ast::WithSpan, configuration::Generator, internal_baml_diagnostics::*};
use internal_baml_parser_database::{
    ast::{self, WithDocumentation, WithName},
    coerce,
};
use internal_baml_schema_ast::ast::WithIdentifier;
use std::{collections::HashMap, path::PathBuf};

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
    let args = ast_generator
        .fields()
        .iter()
        .map(|arg| match &arg.value {
            Some(expr) => {
                if FIRST_CLASS_PROPERTIES.iter().any(|k| *k == arg.name()) {
                    Some((arg.name(), expr))
                } else {
                    diagnostics.push_error(DatamodelError::new_property_not_known_error(
                        arg.name(),
                        arg.span.clone(),
                        FIRST_CLASS_PROPERTIES.to_vec(),
                    ));
                    None
                }
            }
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
        Some(val) => match coerce::string(val, diagnostics) {
            Some(v) => Some(v),
            None => None,
        },
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

    let command_prefix = match args.get(LANGUAGE_SETUP_PREFIX) {
        Some(val) => match coerce::string(val, diagnostics) {
            Some(v) => Some(v),
            None => None,
        },
        None => match pkg_manager {
            Some("poetry") => Some("poetry run"),
            _ => None,
        },
    };

    let output = args
        .get(OUTPUT_KEY)
        .and_then(|v| coerce::path(v, diagnostics))
        .and_then(|v| Some(PathBuf::from(v)))
        .unwrap_or(PathBuf::from("../"))
        .join("baml_client");

    if diagnostics.has_errors() {
        return None;
    }

    let client_version = match language {
        "python" => get_python_client_version(command_prefix, diagnostics),
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
        config: Default::default(),
        documentation: ast_generator.documentation().map(String::from),
        client_version,
        span: Some(ast_generator.identifier().span().clone()),
        shell_setup: command_prefix.map(String::from),
    })
}

fn get_python_client_version(
    shell_setup: Option<&str>,
    _diagnostics: &mut Diagnostics,
) -> Option<String> {
    let cmd = match shell_setup {
        Some(setup) => format!("{} python -m baml_version", setup),
        None => String::from("python -m baml_version"),
    };

    match std::process::Command::new(cmd)
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
        }) {
        Some(v) => Some(v),
        None => None,
    }
}
