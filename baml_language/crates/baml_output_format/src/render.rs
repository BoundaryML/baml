//! Rendering logic for output format.
//!
//! This module contains the implementation for rendering `OutputFormatContent`
//! to a string suitable for inclusion in LLM prompts.

use baml_base::{LiteralValue, Ty};
use thiserror::Error;

use crate::render_options::{HoistClasses, MapStyle, OutputFormatOptions, RenderSetting};
use crate::types::OutputFormatContent;

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
            RenderSetting::Auto => Some(OutputFormatOptions::DEFAULT_TYPE_PREFIX.to_string()),
            RenderSetting::Always(s) => Some(s.clone()),
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

    if rendered.is_empty() && hoisted.is_empty() {
        return Ok(None);
    }

    let mut result = String::new();

    // Add prefix for complex types if configured
    match &options.prefix {
        RenderSetting::Always(prefix) => {
            result.push_str(prefix);
            result.push('\n');
        }
        RenderSetting::Auto => {
            // Default prefix for complex types
        }
        RenderSetting::Never => {
            // No prefix
        }
    }

    if !hoisted.is_empty() {
        result.push_str(&hoisted);
        if !rendered.is_empty() {
            result.push('\n');
        }
    }

    result.push_str(&rendered);

    Ok(Some(result))
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
        Ty::Image | Ty::Audio | Ty::Video | Ty::Pdf => {
            let media_name = match target {
                Ty::Image => "image",
                Ty::Audio => "audio",
                Ty::Video => "video",
                Ty::Pdf => "pdf",
                _ => unreachable!(),
            };
            Some(format!("Answer with {}", media_name))
        }
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
                    result.push_str(&format!("  {}: {}", field.name.rendered_name(), field_type));
                    if let Some(desc) = &field.description {
                        result.push_str(&format!(" // {}", desc));
                    }
                    result.push('\n');
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
        Ty::Image => Ok("image".to_string()),
        Ty::Audio => Ok("audio".to_string()),
        Ty::Video => Ok("video".to_string()),
        Ty::Pdf => Ok("pdf".to_string()),

        Ty::Literal(lit) => Ok(render_literal(lit)),

        Ty::Optional(inner) => {
            let inner_str = render_type(inner, ctx, false)?;
            Ok(format!("{}?", inner_str))
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

        Ty::Class(name) | Ty::Named(name) => {
            render_class(name.as_str(), ctx, is_top_level)
        }

        Ty::Enum(name) => {
            render_enum(name.as_str(), ctx, is_top_level)
        }

        // Special types - just render as string
        Ty::Unknown | Ty::Error | Ty::Void => Ok("string".to_string()),
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
        Ty::Image => Ok("image".to_string()),
        Ty::Audio => Ok("audio".to_string()),
        Ty::Video => Ok("video".to_string()),
        Ty::Pdf => Ok("pdf".to_string()),

        Ty::Literal(lit) => Ok(render_literal(lit)),

        Ty::Optional(inner) => {
            let inner_str = render_type_inline(inner, ctx)?;
            Ok(format!("{}?", inner_str))
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

        Ty::Class(name) | Ty::Named(name) => {
            // For inline, just use the class name
            Ok(name.to_string())
        }

        Ty::Enum(name) => {
            // For inline enum in a class field, render as choices
            if let Some(enum_def) = ctx.content.find_enum(name.as_str()) {
                let or_splitter = ctx.or_splitter();
                let variants: Vec<String> = enum_def.variants
                    .iter()
                    .map(|v| format!("'{}'", v.name.rendered_name()))
                    .collect();
                Ok(variants.join(or_splitter))
            } else {
                Ok(name.to_string())
            }
        }

        // Special types
        Ty::Unknown | Ty::Error | Ty::Void => Ok("string".to_string()),
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
        let field_type = render_type_inline(&field.field_type, ctx)?;

        let field_name = if ctx.quote_class_fields() {
            format!("\"{}\"", field.name.rendered_name())
        } else {
            field.name.rendered_name().to_string()
        };

        result.push_str(&format!("{}{}: {}", field_indent, field_name, field_type));

        if let Some(desc) = &field.description {
            result.push_str(&format!(" // {}", desc));
        }

        result.push('\n');
    }

    result.push_str(&format!("{}}}", indent));

    Ok(result)
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
        result.push('\n');

        let prefix = ctx.enum_value_prefix();
        for variant in &enum_def.variants {
            result.push_str(&format!("{}{}", prefix, variant.name.rendered_name()));
            if let Some(desc) = &variant.description {
                result.push_str(&format!(" // {}", desc));
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
            .with_target(Ty::Class(BaseName::from("Person")))
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
            .with_target(Ty::Enum(BaseName::from("Color")))
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
            .with_field("status", Ty::Enum(BaseName::from("Status")), None, true);

        let content = OutputFormatBuilder::new()
            .with_enum(status_enum)
            .with_class(task_class)
            .with_target(Ty::Class(BaseName::from("Task")))
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
}
