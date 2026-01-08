//! Generator validation logic for HIR lowering.
//!
//! This module contains validation for generator definitions, including:
//! - Property name validation
//! - Property value validation
//! - Required property checking

use baml_base::Name;
use baml_diagnostics::HirDiagnostic;
use baml_syntax::SyntaxNode;
use rowan::ast::AstNode;

use crate::{LoweringContext, item_tree::Generator};

/// Valid generator properties.
pub(crate) const VALID_GENERATOR_PROPERTIES: &[&str] = &[
    "output_type",
    "output_dir",
    "version",
    "default_client_mode",
    "on_generate",
    "project",
    "client_package_name",
    "module_format",
];

/// Valid output types for generators.
pub(crate) const VALID_OUTPUT_TYPES: &[&str] = &[
    "python/pydantic",
    "python/pydantic/v1",
    "typescript",
    "typescript/react",
    "ruby/sorbet",
    "go",
    "rest/openapi",
    "boundary-cloud",
];

/// Valid values for `default_client_mode`.
pub(crate) const VALID_CLIENT_MODES: &[&str] = &["sync", "async"];

/// Valid values for `module_format`.
pub(crate) const VALID_MODULE_FORMATS: &[&str] = &["cjs", "esm"];

/// Extract generator definition from CST with validation.
pub(crate) fn lower_generator(node: &SyntaxNode, ctx: &mut LoweringContext) -> Option<Generator> {
    use baml_syntax::ast::GeneratorDef;

    let generator = GeneratorDef::cast(node.clone())?;

    // Extract name using AST accessor
    let name_token = generator.name()?;
    let name = Name::new(name_token.text());
    let generator_name = name.to_string();

    // Track for required property check
    let mut output_type: Option<String> = None;
    let mut output_dir: Option<String> = None;
    let mut version: Option<String> = None;
    let mut default_client_mode: Option<String> = None;
    let mut on_generate: Option<String> = None;
    let mut project: Option<String> = None;
    let mut client_package_name: Option<String> = None;
    let mut module_format: Option<String> = None;

    // Process config block if present
    if let Some(config_block) = generator.config_block() {
        for item in config_block.items() {
            let Some(key_token) = item.key() else {
                continue;
            };
            let key = key_token.text();
            let key_span = ctx.span(key_token.text_range());

            // Validate property name
            if !VALID_GENERATOR_PROPERTIES.contains(&key) {
                ctx.push_diagnostic(HirDiagnostic::UnknownGeneratorProperty {
                    generator_name: generator_name.clone(),
                    property_name: key.to_string(),
                    span: key_span,
                    valid_properties: VALID_GENERATOR_PROPERTIES.to_vec(),
                });
                continue;
            }

            // Get value - use value_str() to handle compound values like "python/pydantic"
            let value = item.value_str();

            match key {
                "output_type" => {
                    if let Some(ref v) = value {
                        if !VALID_OUTPUT_TYPES.contains(&v.as_str()) {
                            if let Some(value_range) = item.value_text_range() {
                                ctx.push_diagnostic(HirDiagnostic::InvalidGeneratorPropertyValue {
                                    generator_name: generator_name.clone(),
                                    property_name: key.to_string(),
                                    value: v.clone(),
                                    span: ctx.span(value_range),
                                    valid_values: Some(
                                        VALID_OUTPUT_TYPES
                                            .iter()
                                            .map(|s| (*s).to_string())
                                            .collect(),
                                    ),
                                    help: None,
                                });
                            }
                        }
                    }
                    output_type = value;
                }
                "output_dir" => {
                    output_dir = value;
                }
                "version" => {
                    version = value;
                }
                "default_client_mode" => {
                    if let Some(ref v) = value {
                        if !VALID_CLIENT_MODES.contains(&v.as_str()) {
                            if let Some(value_range) = item.value_text_range() {
                                ctx.push_diagnostic(HirDiagnostic::InvalidGeneratorPropertyValue {
                                    generator_name: generator_name.clone(),
                                    property_name: key.to_string(),
                                    value: v.clone(),
                                    span: ctx.span(value_range),
                                    valid_values: Some(
                                        VALID_CLIENT_MODES
                                            .iter()
                                            .map(|s| (*s).to_string())
                                            .collect(),
                                    ),
                                    help: Some("Use \"sync\" or \"async\"".to_string()),
                                });
                            }
                        }
                    }
                    default_client_mode = value;
                }
                "on_generate" => {
                    on_generate = value;
                }
                "project" => {
                    project = value;
                }
                "client_package_name" => {
                    client_package_name = value;
                }
                "module_format" => {
                    if let Some(ref v) = value {
                        if !VALID_MODULE_FORMATS.contains(&v.as_str()) {
                            if let Some(value_range) = item.value_text_range() {
                                ctx.push_diagnostic(HirDiagnostic::InvalidGeneratorPropertyValue {
                                    generator_name: generator_name.clone(),
                                    property_name: key.to_string(),
                                    value: v.clone(),
                                    span: ctx.span(value_range),
                                    valid_values: Some(
                                        VALID_MODULE_FORMATS
                                            .iter()
                                            .map(|s| (*s).to_string())
                                            .collect(),
                                    ),
                                    help: Some("Use \"cjs\" or \"esm\"".to_string()),
                                });
                            }
                        }
                    }
                    module_format = value;
                }
                _ => {}
            }
        }
    }

    // Check required property: output_type
    if output_type.is_none() {
        ctx.push_diagnostic(HirDiagnostic::MissingGeneratorProperty {
            generator_name: generator_name.clone(),
            property_name: "output_type",
            span: ctx.span(name_token.text_range()),
        });
    }

    // Check boundary-cloud specific requirement
    if output_type.as_deref() == Some("boundary-cloud") && project.is_none() {
        ctx.push_diagnostic(HirDiagnostic::MissingGeneratorProperty {
            generator_name,
            property_name: "project",
            span: ctx.span(name_token.text_range()),
        });
    }

    Some(Generator {
        name,
        output_type,
        output_dir,
        version,
        default_client_mode,
        on_generate,
        project,
        client_package_name,
        module_format,
    })
}
