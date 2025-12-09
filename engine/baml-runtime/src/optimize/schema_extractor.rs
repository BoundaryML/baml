//! Schema Extractor - Extract optimizable types from the IR
//!
//! This module extracts all classes and enums reachable from a function's
//! input/output types, including their @description and @alias annotations.

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use baml_types::TypeIR;

use super::candidate::{
    ClassDefinition, EnumDefinition, OptimizableFunction, SchemaFieldDefinition,
};
use crate::InternalRuntimeInterface;

/// Extract the optimizable function context from the IR
pub fn extract_optimizable_function(
    runtime: &crate::BamlRuntime,
    function_name: &str,
) -> Result<OptimizableFunction> {
    let ir = runtime.ir();

    // Find the function in the IR
    let function = ir
        .walk_functions()
        .find(|f| f.name() == function_name)
        .with_context(|| format!("Function '{}' not found", function_name))?;

    // Get the prompt text from the default config
    let prompt_text = function
        .elem()
        .default_config()
        .map(|c| c.prompt_template.clone())
        .unwrap_or_default();

    // Extract the function source code from the span
    let function_source = extract_source_from_span(function.span());

    // Collect all reachable types from the output type
    let mut classes = Vec::new();
    let mut enums = Vec::new();
    let mut visited_classes = HashSet::new();
    let mut visited_enums = HashSet::new();

    // Extract from output type
    collect_reachable_types(
        ir,
        function.output(),
        &mut classes,
        &mut enums,
        &mut visited_classes,
        &mut visited_enums,
    );

    // Also extract from input types (they might have descriptions too)
    for (_, input_type) in function.inputs() {
        collect_reachable_types(
            ir,
            input_type,
            &mut classes,
            &mut enums,
            &mut visited_classes,
            &mut visited_enums,
        );
    }

    Ok(OptimizableFunction {
        function_name: function_name.to_string(),
        prompt_text,
        classes,
        enums,
        function_source,
    })
}

/// Extract source code text from a span
fn extract_source_from_span(span: Option<&internal_baml_core::ast::Span>) -> Option<String> {
    let span = span?;
    let file_content = span.file.as_str();
    if span.start <= span.end && span.end <= file_content.len() {
        Some(file_content[span.start..span.end].to_string())
    } else {
        None
    }
}

/// Extract the source code for a specific test
pub fn extract_test_source(
    runtime: &crate::BamlRuntime,
    function_name: &str,
    test_name: &str,
) -> Option<String> {
    let ir = runtime.ir();

    // Find the function
    let function = ir.walk_functions().find(|f| f.name() == function_name)?;

    // Find the test within the function
    let test = function
        .walk_tests()
        .find(|t| t.test_case().name == test_name)?;

    extract_source_from_span(test.span())
}

/// Recursively collect all classes and enums reachable from a type
fn collect_reachable_types(
    ir: &internal_baml_core::ir::repr::IntermediateRepr,
    field_type: &TypeIR,
    classes: &mut Vec<ClassDefinition>,
    enums: &mut Vec<EnumDefinition>,
    visited_classes: &mut HashSet<String>,
    visited_enums: &mut HashSet<String>,
) {
    match field_type {
        TypeIR::Class { name, .. } => {
            if visited_classes.contains(name) {
                return;
            }
            visited_classes.insert(name.clone());

            // Find the class in IR
            if let Some(class_walker) = ir.walk_classes().find(|c| c.name() == name) {
                // Extract fields
                let fields: Vec<SchemaFieldDefinition> = class_walker
                    .walk_fields()
                    .map(|field_walker| {
                        // Get field type
                        let field_type_ir = field_walker.r#type();

                        // Recursively process the field type
                        collect_reachable_types(
                            ir,
                            field_type_ir,
                            classes,
                            enums,
                            visited_classes,
                            visited_enums,
                        );

                        SchemaFieldDefinition {
                            field_name: field_walker.name().to_string(),
                            field_type: format_field_type(field_type_ir),
                            description: None, // TODO: Extract from attributes
                            alias: None,       // TODO: Extract from attributes
                        }
                    })
                    .collect();

                classes.push(ClassDefinition {
                    class_name: name.clone(),
                    description: None, // TODO: Extract class description
                    fields,
                });
            }
        }

        TypeIR::Enum { name, .. } => {
            if visited_enums.contains(name) {
                return;
            }
            visited_enums.insert(name.clone());

            // Find the enum in IR
            if let Some(enum_walker) = ir.walk_enums().find(|e| e.name() == name) {
                let values: Vec<String> = enum_walker
                    .walk_values()
                    .map(|v| v.name().to_string())
                    .collect();

                enums.push(EnumDefinition {
                    enum_name: name.clone(),
                    values,
                    value_descriptions: HashMap::new(), // TODO: Extract descriptions
                });
            }
        }

        TypeIR::List(inner, _) => {
            collect_reachable_types(ir, inner, classes, enums, visited_classes, visited_enums);
        }

        TypeIR::Union(variants, _) => {
            for variant in variants.iter_include_null() {
                collect_reachable_types(
                    ir,
                    variant,
                    classes,
                    enums,
                    visited_classes,
                    visited_enums,
                );
            }
        }

        TypeIR::Map(key_type, value_type, _) => {
            collect_reachable_types(ir, key_type, classes, enums, visited_classes, visited_enums);
            collect_reachable_types(
                ir,
                value_type,
                classes,
                enums,
                visited_classes,
                visited_enums,
            );
        }

        TypeIR::Tuple(items, _) => {
            for item in items {
                collect_reachable_types(ir, item, classes, enums, visited_classes, visited_enums);
            }
        }

        // Primitive types don't have nested types
        TypeIR::Primitive(_, _)
        | TypeIR::Literal(_, _)
        | TypeIR::RecursiveTypeAlias { .. }
        | TypeIR::Arrow(_, _)
        | TypeIR::Top(_) => {}
    }
}

/// Format a field type as a string for display
fn format_field_type(field_type: &TypeIR) -> String {
    match field_type {
        TypeIR::Primitive(p, _) => format!("{:?}", p).to_lowercase(),
        TypeIR::Enum { name, .. } | TypeIR::Class { name, .. } => name.clone(),
        TypeIR::List(inner, _) => format!("{}[]", format_field_type(inner)),
        TypeIR::Map(k, v, _) => format!("map<{}, {}>", format_field_type(k), format_field_type(v)),
        TypeIR::Union(variants, _) => {
            let parts: Vec<_> = variants
                .iter_include_null()
                .into_iter()
                .map(format_field_type)
                .collect();
            parts.join(" | ")
        }
        TypeIR::Tuple(items, _) => {
            let parts: Vec<_> = items.iter().map(format_field_type).collect();
            format!("({})", parts.join(", "))
        }
        TypeIR::Literal(lit, _) => format!("{:?}", lit),
        TypeIR::RecursiveTypeAlias { name, .. } => name.clone(),
        TypeIR::Arrow(_, _) => "function".to_string(),
        TypeIR::Top(_) => "any".to_string(),
    }
}

/// Get all functions that have tests associated with them
pub fn functions_with_tests(ir: &internal_baml_core::ir::repr::IntermediateRepr) -> Vec<String> {
    ir.walk_functions()
        .filter(|f| !f.elem().tests.is_empty())
        .map(|f| f.name().to_string())
        .collect()
}

/// Filter functions based on user-provided patterns
pub fn filter_functions(
    ir: &internal_baml_core::ir::repr::IntermediateRepr,
    function_filter: &[String],
) -> Vec<String> {
    let all_with_tests = functions_with_tests(ir);

    if function_filter.is_empty() {
        return all_with_tests;
    }

    all_with_tests
        .into_iter()
        .filter(|name| {
            function_filter.iter().any(|pattern| {
                if pattern.contains('*') {
                    // Simple wildcard matching
                    let parts: Vec<&str> = pattern.split('*').collect();
                    if parts.len() == 2 {
                        name.starts_with(parts[0]) && name.ends_with(parts[1])
                    } else {
                        name == pattern
                    }
                } else {
                    name == pattern
                }
            })
        })
        .collect()
}
