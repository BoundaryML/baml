//! Rendering logic for output format.
//!
//! This module contains the implementation for rendering `OutputFormatContent`
//! to a string suitable for inclusion in LLM prompts.

use baml_compiler_tir::{LiteralValue, Ty};
use thiserror::Error;

use super::render_options::{HoistClasses, MapStyle, OutputFormatOptions, RenderSetting};
use super::types::OutputFormatContent;

/// Error during output format rendering.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("Render error: {0}")]
    Other(String),
}

/// Rendering context passed through recursive calls.
struct RenderContext<'a> {
    content: &'a OutputFormatContent,
    options: &'a OutputFormatOptions,
    /// Track which classes have been rendered to avoid infinite recursion.
    rendered_classes: std::collections::HashSet<String>,
    /// Indent level for nested structures.
    indent: usize,
}

impl<'a> RenderContext<'a> {
    fn new(content: &'a OutputFormatContent, options: &'a OutputFormatOptions) -> Self {
        Self {
            content,
            options,
            rendered_classes: std::collections::HashSet::new(),
            indent: 0,
        }
    }

    fn indent_str(&self) -> String {
        "  ".repeat(self.indent)
    }

    fn or_splitter(&self) -> &str {
        &self.options.or_splitter
    }

    fn map_style(&self) -> MapStyle {
        self.options.map_style
    }

    fn enum_value_prefix(&self) -> String {
        match &self.options.enum_value_prefix {
            RenderSetting::Auto => "- ".to_string(),
            RenderSetting::Always(s) => s.clone(),
            RenderSetting::Never => String::new(),
        }
    }

    fn hoisted_class_prefix(&self) -> Option<String> {
        match &self.options.hoisted_class_prefix {
            // Default: no prefix for hoisted classes (matches engine behavior)
            RenderSetting::Auto => None,
            RenderSetting::Always(s) if !s.is_empty() => Some(s.clone()),
            RenderSetting::Always(_) => None, // Empty string means no prefix
            RenderSetting::Never => None,
        }
    }

    fn quote_class_fields(&self) -> bool {
        self.options.quote_class_fields
    }
}

/// Render the output format content to a string.
pub fn render(
    content: &OutputFormatContent,
    options: &OutputFormatOptions,
) -> Result<Option<String>, RenderError> {
    let mut ctx = RenderContext::new(content, options);

    // Check if target is a simple primitive type
    if let Some(simple) = render_simple_target(&content.target, options) {
        return Ok(Some(simple));
    }

    // For complex types, render the full schema
    let rendered = render_type(&content.target, &mut ctx, true)?;

    // Add hoisted definitions if needed
    let hoisted = render_hoisted_definitions(&mut ctx)?;

    // Add structural recursive alias definitions
    let alias_definitions = render_structural_recursive_aliases(&mut ctx)?;

    if rendered.is_empty() && hoisted.is_empty() && alias_definitions.is_empty() {
        return Ok(None);
    }

    let mut result = String::new();

    // Add hoisted class and enum definitions first
    if !hoisted.is_empty() {
        result.push_str(&hoisted);
        result.push('\n');
    }

    // Add type alias definitions
    if !alias_definitions.is_empty() {
        result.push_str(&alias_definitions);
        result.push('\n');
    }

    // Add prefix for complex types if configured
    let prefix = get_auto_prefix(&content.target, options);
    if let Some(p) = prefix {
        result.push_str(&p);
    }

    result.push_str(&rendered);

    Ok(Some(result))
}

/// Get the auto-generated prefix based on target type.
fn get_auto_prefix(target: &Ty, options: &OutputFormatOptions) -> Option<String> {
    match &options.prefix {
        RenderSetting::Always(prefix) => {
            if prefix.is_empty() {
                None
            } else {
                Some(format!("{}\n", prefix))
            }
        }
        RenderSetting::Never => None,
        RenderSetting::Auto => {
            // Generate appropriate prefix based on target type
            match target {
                Ty::Class(_) | Ty::TypeAlias(_) => {
                    Some("Answer in JSON using this schema:\n".to_string())
                }
                Ty::List(_) => Some("Answer with a JSON Array using this schema:\n".to_string()),
                Ty::Union(_) => Some("Answer in JSON using any of these schemas:\n".to_string()),
                Ty::Map { .. } => Some("Answer in JSON using this schema:\n".to_string()),
                Ty::Enum(_) => Some("Answer with any of the categories:\n".to_string()),
                Ty::Optional(_) => Some("Answer in JSON using this schema:\n".to_string()),
                _ => None,
            }
        }
    }
}

/// Render structural recursive type alias definitions.
fn render_structural_recursive_aliases(ctx: &mut RenderContext) -> Result<String, RenderError> {
    let mut result = String::new();

    for (alias_name, target_type) in ctx.content.structural_recursive_aliases.iter() {
        let rendered_target = render_type_inline(target_type, ctx)?;
        result.push_str(&format!("{} = {}\n", alias_name, rendered_target));
    }

    Ok(result)
}

/// Render simple targets (primitives) with a descriptive prefix.
fn render_simple_target(target: &Ty, options: &OutputFormatOptions) -> Option<String> {
    match target {
        Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Null => {
            let type_name = match target {
                Ty::String => "string",
                Ty::Int => "int",
                Ty::Float => "float",
                Ty::Bool => "bool",
                Ty::Null => "null",
                _ => unreachable!(),
            };

            // "Answer as an int" vs "Answer as a string"
            let article = match type_name {
                "int" => "an",
                _ => "a",
            };

            match &options.prefix {
                RenderSetting::Always(prefix) => {
                    if prefix.is_empty() {
                        Some(type_name.to_string())
                    } else {
                        Some(format!("{} {}", prefix, type_name))
                    }
                }
                RenderSetting::Never => Some(type_name.to_string()),
                RenderSetting::Auto => Some(format!("Answer as {} {}", article, type_name)),
            }
        }
        Ty::Media(kind) => Some(format!("Answer with {}", kind)),
        Ty::Literal(lit) => {
            let value = match lit {
                LiteralValue::Int(i) => format!("{}", i),
                LiteralValue::Bool(b) => format!("{}", b),
                LiteralValue::String(s) => format!("\"{}\"", s),
                LiteralValue::Float(f) => f.clone(),
            };
            Some(format!("Answer with exactly: {}", value))
        }
        _ => None,
    }
}

/// Render hoisted class/enum definitions.
fn render_hoisted_definitions(ctx: &mut RenderContext) -> Result<String, RenderError> {
    let mut result = String::new();
    let should_hoist = match &ctx.options.hoist_classes {
        HoistClasses::All => true,
        HoistClasses::Auto => !ctx.content.recursive_classes.is_empty(),
        HoistClasses::Subset(names) => !names.is_empty(),
    };

    if !should_hoist {
        return Ok(result);
    }

    // Hoist classes that need it
    for name in ctx.content.recursive_classes.iter() {
        if let Some(class) = ctx.content.find_class(name) {
            if !ctx.rendered_classes.contains(name) {
                ctx.rendered_classes.insert(name.clone());

                if let Some(prefix) = ctx.hoisted_class_prefix() {
                    result.push_str(&prefix);
                    result.push(' ');
                }
                result.push_str(&class.name.rendered_name());
                result.push_str(" {\n");

                for field in &class.fields {
                    let field_type = render_type_inline(&field.field_type, ctx)?;
                    if let Some(desc) = &field.description {
                        // Block comment before field (matches old implementation)
                        result.push_str(&format!("  // {}\n", desc.replace("\n", "\n  // ")));
                    }
                    result.push_str(&format!("  {}: {},\n", field.name.rendered_name(), field_type));
                }

                result.push_str("}\n");
            }
        }
    }

    Ok(result)
}

/// Render a type, potentially with full schema expansion.
fn render_type(ty: &Ty, ctx: &mut RenderContext, is_top_level: bool) -> Result<String, RenderError> {
    match ty {
        Ty::Int => Ok("int".to_string()),
        Ty::Float => Ok("float".to_string()),
        Ty::String => Ok("string".to_string()),
        Ty::Bool => Ok("bool".to_string()),
        Ty::Null => Ok("null".to_string()),
        Ty::Media(kind) => Ok(kind.to_string()),

        Ty::Literal(lit) => Ok(render_literal(lit)),

        Ty::Optional(inner) => {
            let inner_str = render_type(inner, ctx, false)?;
            let or_splitter = ctx.or_splitter();
            Ok(format!("{}{}null", inner_str, or_splitter))
        }

        Ty::List(inner) => {
            let inner_str = render_type(inner, ctx, false)?;
            Ok(format!("{}[]", inner_str))
        }

        Ty::Map { key, value } => {
            let key_str = render_type(key, ctx, false)?;
            let value_str = render_type(value, ctx, false)?;
            match ctx.map_style() {
                MapStyle::TypeParameters => Ok(format!("map<{}, {}>", key_str, value_str)),
                MapStyle::ObjectLiteral => Ok(format!("{{ [key: {}]: {} }}", key_str, value_str)),
            }
        }

        Ty::Union(variants) => {
            let or_splitter = ctx.or_splitter().to_string();
            let rendered: Vec<String> = variants
                .iter()
                .map(|v| render_type(v, ctx, false))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rendered.join(&or_splitter))
        }

        Ty::Class(name) | Ty::TypeAlias(name) => {
            render_class(name.name.as_str(), ctx, is_top_level)
        }

        Ty::Enum(name) => {
            render_enum(name.name.as_str(), ctx, is_top_level)
        }

        // Special types - just render as string
        Ty::Unknown | Ty::Error | Ty::Void | Ty::Builtin(_) => Ok("string".to_string()),
        Ty::Function { .. } => Ok("string".to_string()),
        Ty::WatchAccessor(inner) => render_type(inner, ctx, is_top_level),
    }
}

/// Render a type inline (without full expansion).
fn render_type_inline(ty: &Ty, ctx: &RenderContext) -> Result<String, RenderError> {
    match ty {
        Ty::Int => Ok("int".to_string()),
        Ty::Float => Ok("float".to_string()),
        Ty::String => Ok("string".to_string()),
        Ty::Bool => Ok("bool".to_string()),
        Ty::Null => Ok("null".to_string()),
        Ty::Media(kind) => Ok(kind.to_string()),

        Ty::Literal(lit) => Ok(render_literal(lit)),

        Ty::Optional(inner) => {
            let inner_str = render_type_inline(inner, ctx)?;
            let or_splitter = ctx.or_splitter();
            Ok(format!("{}{}null", inner_str, or_splitter))
        }

        Ty::List(inner) => {
            let inner_str = render_type_inline(inner, ctx)?;
            Ok(format!("{}[]", inner_str))
        }

        Ty::Map { key, value } => {
            let key_str = render_type_inline(key, ctx)?;
            let value_str = render_type_inline(value, ctx)?;
            match ctx.map_style() {
                MapStyle::TypeParameters => Ok(format!("map<{}, {}>", key_str, value_str)),
                MapStyle::ObjectLiteral => Ok(format!("{{ [key: {}]: {} }}", key_str, value_str)),
            }
        }

        Ty::Union(variants) => {
            let or_splitter = ctx.or_splitter();
            let rendered: Vec<String> = variants
                .iter()
                .map(|v| render_type_inline(v, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rendered.join(or_splitter))
        }

        Ty::Class(name) | Ty::TypeAlias(name) => {
            // For inline classes that are not recursive, we should reference by name
            // (The old implementation does this for hoisted classes)
            Ok(name.name.to_string())
        }

        Ty::Enum(name) => {
            // For inline enum in a class field, render as choices
            if let Some(enum_def) = ctx.content.find_enum(name.name.as_str()) {
                let or_splitter = ctx.or_splitter();
                let variants: Vec<String> = enum_def.variants
                    .iter()
                    .map(|v| format!("'{}'", v.name.rendered_name()))
                    .collect();
                Ok(variants.join(or_splitter))
            } else {
                Ok(name.name.to_string())
            }
        }

        // Special types
        Ty::Unknown | Ty::Error | Ty::Void | Ty::Builtin(_) => Ok("string".to_string()),
        Ty::Function { .. } => Ok("string".to_string()),
        Ty::WatchAccessor(inner) => render_type_inline(inner, ctx),
    }
}

/// Render a literal value.
fn render_literal(lit: &LiteralValue) -> String {
    match lit {
        LiteralValue::Int(i) => format!("{}", i),
        LiteralValue::Bool(b) => format!("{}", b),
        LiteralValue::String(s) => format!("\"{}\"", s),
        LiteralValue::Float(f) => f.clone(),
    }
}

/// Render a class definition.
fn render_class(name: &str, ctx: &mut RenderContext, _is_top_level: bool) -> Result<String, RenderError> {
    // Check if this class is recursive and already rendered
    if ctx.content.recursive_classes.contains(name) {
        if ctx.rendered_classes.contains(name) {
            return Ok(name.to_string());
        }
        // Mark as rendered to prevent infinite recursion
        ctx.rendered_classes.insert(name.to_string());
    }

    let class = match ctx.content.find_class(name) {
        Some(c) => c,
        None => return Ok(name.to_string()), // Unknown class, just return name
    };

    let mut result = String::new();
    let indent = ctx.indent_str();

    result.push_str("{\n");

    for field in &class.fields {
        let field_indent = format!("{}  ", indent);
        let field_type = render_field_type(&field.field_type, ctx)?;

        let field_name = if ctx.quote_class_fields() {
            format!("\"{}\"", field.name.rendered_name())
        } else {
            field.name.rendered_name().to_string()
        };

        // Block comment before field (matches old implementation)
        if let Some(desc) = &field.description {
            result.push_str(&format!("{}// {}\n", field_indent, desc.replace("\n", &format!("\n{}// ", field_indent))));
        }
        result.push_str(&format!("{}{}: {},\n", field_indent, field_name, field_type));
    }

    result.push_str(&format!("{}}}", indent));

    Ok(result)
}

/// Render a field type, expanding nested classes inline when appropriate.
fn render_field_type(ty: &Ty, ctx: &RenderContext) -> Result<String, RenderError> {
    match ty {
        Ty::Class(name) | Ty::TypeAlias(name) => {
            // Check if this class is recursive - if so, just use the name
            if ctx.content.recursive_classes.contains(name.name.as_str()) {
                return Ok(name.name.to_string());
            }

            // For non-recursive classes, expand inline
            if let Some(class) = ctx.content.find_class(name.name.as_str()) {
                let mut result = String::new();
                result.push_str("{\n");

                for field in &class.fields {
                    let field_type = render_field_type(&field.field_type, ctx)?;
                    let field_name = if ctx.quote_class_fields() {
                        format!("\"{}\"", field.name.rendered_name())
                    } else {
                        field.name.rendered_name().to_string()
                    };

                    // Replace newlines in nested types for proper indentation
                    let field_type = field_type.replace('\n', "\n    ");

                    // Block comment before field (matches old implementation)
                    if let Some(desc) = &field.description {
                        result.push_str(&format!("    // {}\n", desc.replace("\n", "\n    // ")));
                    }
                    result.push_str(&format!("    {}: {},\n", field_name, field_type));
                }

                result.push_str("  }");
                Ok(result)
            } else {
                // Unknown class, just use name
                Ok(name.name.to_string())
            }
        }

        Ty::Optional(inner) => {
            let inner_str = render_field_type(inner, ctx)?;
            let or_splitter = ctx.or_splitter();
            Ok(format!("{}{}null", inner_str, or_splitter))
        }

        Ty::List(inner) => {
            let inner_str = render_field_type(inner, ctx)?;
            // Always use postfix notation, parenthesize unions for clarity
            if matches!(inner.as_ref(), Ty::Union(_)) {
                Ok(format!("({})[]", inner_str))
            } else {
                Ok(format!("{}[]", inner_str))
            }
        }

        Ty::Union(variants) => {
            let or_splitter = ctx.or_splitter();
            let rendered: Vec<String> = variants
                .iter()
                .map(|v| render_field_type(v, ctx))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rendered.join(or_splitter))
        }

        // For other types, delegate to render_type_inline
        _ => render_type_inline(ty, ctx),
    }
}

/// Render an enum definition.
fn render_enum(name: &str, ctx: &mut RenderContext, is_top_level: bool) -> Result<String, RenderError> {
    let enum_def = match ctx.content.find_enum(name) {
        Some(e) => e,
        None => return Ok(name.to_string()), // Unknown enum, just return name
    };

    let mut result = String::new();

    if is_top_level {
        // Full format with name and variants on separate lines
        result.push_str(&enum_def.name.rendered_name());
        result.push_str("\n----\n"); // Add separator after enum name

        let prefix = ctx.enum_value_prefix();
        for variant in &enum_def.variants {
            let rendered = variant.name.rendered_name();

            result.push_str(&prefix);
            if let Some(desc) = &variant.description {
                // Has description: show "alias: description" (matches old implementation)
                result.push_str(&format!("{}: {}", rendered, desc.replace("\n", "\n  ")));
            } else {
                // No description: show just the rendered name (alias or actual)
                result.push_str(rendered);
            }
            result.push('\n');
        }
    } else {
        // Inline format: 'variant1' or 'variant2' or ...
        let or_splitter = ctx.or_splitter();
        let variants: Vec<String> = enum_def.variants
            .iter()
            .map(|v| format!("'{}'", v.name.rendered_name()))
            .collect();
        result.push_str(&variants.join(or_splitter));
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Class, Enum, OutputFormatBuilder};
    use baml_base::Name as BaseName;
    use baml_compiler_hir::FullyQualifiedName;

    #[test]
    fn test_render_int() {
        let content = OutputFormatBuilder::new()
            .with_target(Ty::Int)
            .build();

        let result = render(&content, &OutputFormatOptions::default()).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Answer as an int"));
    }

    #[test]
    fn test_render_string() {
        let content = OutputFormatBuilder::new()
            .with_target(Ty::String)
            .build();

        let result = render(&content, &OutputFormatOptions::default()).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Answer as a string"));
    }

    #[test]
    fn test_render_class() {
        let person_class = Class::new("Person")
            .with_field("name", Ty::String, Some("The person's name".to_string()), true)
            .with_field("age", Ty::Int, None, true);

        let content = OutputFormatBuilder::new()
            .with_class(person_class)
            .with_target(Ty::Class(FullyQualifiedName::local(BaseName::from("Person"))))
            .build();

        let result = render(&content, &OutputFormatOptions::default()).unwrap();
        assert!(result.is_some());
        let rendered = result.unwrap();
        assert!(rendered.contains("name: string"), "Expected 'name: string' but got: {}", rendered);
        assert!(rendered.contains("age: int"), "Expected 'age: int' but got: {}", rendered);
        assert!(rendered.contains("The person's name"), "Expected description but got: {}", rendered);
    }

    #[test]
    fn test_render_enum_top_level() {
        let color_enum = Enum::new("Color")
            .with_variant("red", None)
            .with_variant("green", None)
            .with_variant("blue", None);

        let content = OutputFormatBuilder::new()
            .with_enum(color_enum)
            .with_target(Ty::Enum(FullyQualifiedName::local(BaseName::from("Color"))))
            .build();

        let options = OutputFormatOptions::new(
            Some(None), // prefix = null (suppress)
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        let result = render(&content, &options).unwrap();
        assert!(result.is_some());
        let rendered = result.unwrap();
        assert!(rendered.contains("Color"), "Expected 'Color' but got: {}", rendered);
        assert!(rendered.contains("- red"), "Expected '- red' but got: {}", rendered);
    }

    #[test]
    fn test_render_enum_inline_in_class() {
        let status_enum = Enum::new("Status")
            .with_variant("pending", None)
            .with_variant("done", None);

        let task_class = Class::new("Task")
            .with_field(
                "status",
                Ty::Enum(FullyQualifiedName::local(BaseName::from("Status"))),
                None,
                true,
            );

        let content = OutputFormatBuilder::new()
            .with_enum(status_enum)
            .with_class(task_class)
            .with_target(Ty::Class(FullyQualifiedName::local(BaseName::from("Task"))))
            .build();

        let options = OutputFormatOptions::new(
            None,
            Some(" | ".to_string()), // Custom or_splitter
            None,
            None,
            None,
            None,
            None,
            None,
        );

        let result = render(&content, &options).unwrap();
        assert!(result.is_some());
        let rendered = result.unwrap();
        assert!(rendered.contains("'pending' | 'done'"), "Expected custom or_splitter but got: {}", rendered);
    }

    #[test]
    fn test_union_in_list_uses_or_splitter() {
        // Test: (float | bool)[] should render with "or" not "|"
        let test_class = Class::new("TestClass")
            .with_field("prop2", Ty::List(Box::new(
                Ty::Union(vec![Ty::Float, Ty::Bool])
            )), None, true);

        let content = OutputFormatBuilder::new()
            .with_class(test_class)
            .with_target(Ty::Class(FullyQualifiedName::local(BaseName::from("TestClass"))))
            .build();

        let result = render(&content, &OutputFormatOptions::default()).unwrap();
        let rendered = result.unwrap();
        eprintln!("Rendered: {}", rendered);

        // Should use "or" not "|"
        assert!(rendered.contains("float or bool"), "Expected 'float or bool' but got: {}", rendered);
        assert!(!rendered.contains(" | "), "Should not contain ' | ': {}", rendered);
    }

    #[test]
    fn test_union_of_lists_uses_or_splitter() {
        // Test: bool[] | int[] should render with "or" not "|"
        let test_class = Class::new("TestClass")
            .with_field("prop3", Ty::Union(vec![
                Ty::List(Box::new(Ty::Bool)),
                Ty::List(Box::new(Ty::Int)),
            ]), None, true);

        let content = OutputFormatBuilder::new()
            .with_class(test_class)
            .with_target(Ty::Class(FullyQualifiedName::local(BaseName::from("TestClass"))))
            .build();

        let result = render(&content, &OutputFormatOptions::default()).unwrap();
        let rendered = result.unwrap();
        eprintln!("Rendered: {}", rendered);

        // Should use "or" not "|"
        assert!(rendered.contains("bool[] or int[]"), "Expected 'bool[] or int[]' but got: {}", rendered);
        assert!(!rendered.contains(" | "), "Should not contain ' | ': {}", rendered);
    }
}
