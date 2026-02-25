use baml_base::Literal as LiteralValue;
use baml_type::Ty;
use indexmap::IndexMap;
use thiserror::Error;

/// Error type for output format rendering.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("Enum '{0}' not found")]
    EnumNotFound(String),
    #[error("Class '{0}' not found")]
    ClassNotFound(String),
    #[error("Type '{0}' is not supported in outputs")]
    UnsupportedType(String),
}

/// An enum definition for output format rendering.
#[derive(Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub values: Vec<(String, Option<String>)>, // (value_name, description)
}

/// A class definition for output format rendering.
#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<(String, Ty, Option<String>)>, // (name, type, description)
}

/// Content for rendering output format schemas.
#[derive(Clone, Debug)]
pub struct OutputFormatContent {
    pub enums: IndexMap<String, Enum>,
    pub classes: IndexMap<String, Class>,
    pub target: Ty,
}

impl OutputFormatContent {
    /// Create a new `OutputFormatContent` with the given target type.
    pub fn new(target: Ty) -> Self {
        Self {
            enums: IndexMap::new(),
            classes: IndexMap::new(),
            target,
        }
    }

    /// Add an enum definition.
    #[must_use]
    pub fn with_enum(mut self, enm: Enum) -> Self {
        self.enums.insert(enm.name.clone(), enm);
        self
    }

    /// Add a class definition.
    #[must_use]
    pub fn with_class(mut self, cls: Class) -> Self {
        self.classes.insert(cls.name.clone(), cls);
        self
    }

    /// Find an enum by name.
    pub fn find_enum(&self, name: &str) -> Option<&Enum> {
        self.enums.get(name)
    }

    /// Find a class by name.
    pub fn find_class(&self, name: &str) -> Option<&Class> {
        self.classes.get(name)
    }

    /// Render the output format schema as a string.
    pub fn render(&self, options: &RenderOptions) -> Result<Option<String>, RenderError> {
        self.render_impl(options)
    }

    fn render_impl(&self, options: &RenderOptions) -> Result<Option<String>, RenderError> {
        // For string target with no explicit prefix, return None
        if matches!(self.target, Ty::String) && matches!(options.prefix, RenderSetting::Auto) {
            return Ok(None);
        }

        let prefix = self.get_prefix(options);

        // For simple primitives (int, float, bool) with Auto prefix, the prefix IS the full message
        // But with explicit prefix, we need to append the type
        if matches!(self.target, Ty::Int | Ty::Float | Ty::Bool)
            && matches!(options.prefix, RenderSetting::Auto)
        {
            return Ok(prefix);
        }

        let rendered_type = self.render_type(&self.target, options)?;

        match (prefix, rendered_type) {
            (Some(p), Some(t)) => Ok(Some(format!("{p}{t}"))),
            (Some(p), None) => Ok(Some(p)),
            (None, Some(t)) => Ok(Some(t)),
            (None, None) => Ok(None),
        }
    }

    fn get_prefix(&self, options: &RenderOptions) -> Option<String> {
        match &options.prefix {
            RenderSetting::Always(p) => Some(p.clone()),
            RenderSetting::Never => None,
            RenderSetting::Auto => match &self.target {
                Ty::String => None,
                Ty::Int => Some("Answer as an int".to_string()),
                Ty::Float => Some("Answer as a float".to_string()),
                Ty::Bool => Some("Answer as a bool".to_string()),
                Ty::List(_) => Some("Answer with a JSON Array using this schema:\n".to_string()),
                Ty::Class(_) | Ty::Map { .. } => {
                    Some("Answer in JSON using this schema:\n".to_string())
                }
                Ty::Enum(_) => None, // Enum prefix handled differently
                Ty::Union(_) => Some("Answer in JSON using this schema:\n".to_string()),
                Ty::Literal(_) => Some("Answer using this specific value:\n".to_string()),
                _ => None,
            },
        }
    }

    fn render_type(&self, ty: &Ty, options: &RenderOptions) -> Result<Option<String>, RenderError> {
        match ty {
            Ty::String => Ok(Some("string".to_string())),
            Ty::Int => Ok(Some("int".to_string())),
            Ty::Float => Ok(Some("float".to_string())),
            Ty::Bool => Ok(Some("bool".to_string())),
            Ty::Null => Ok(Some("null".to_string())),

            Ty::Optional(inner) => {
                let inner_str = self
                    .render_type(inner, options)?
                    .unwrap_or_else(|| "unknown".to_string());
                Ok(Some(format!("{inner_str} | null")))
            }

            Ty::List(inner) => {
                let inner_str = self
                    .render_type(inner, options)?
                    .unwrap_or_else(|| "unknown".to_string());
                Ok(Some(format!("{inner_str}[]")))
            }

            Ty::Map { key, value } => {
                let key_str = self
                    .render_type(key, options)?
                    .unwrap_or_else(|| "string".to_string());
                let value_str = self
                    .render_type(value, options)?
                    .unwrap_or_else(|| "unknown".to_string());
                match options.map_style {
                    MapStyle::TypeParameters => Ok(Some(format!("map<{key_str}, {value_str}>"))),
                    MapStyle::ObjectLiteral => {
                        Ok(Some(format!("{{ [key: {key_str}]: {value_str} }}")))
                    }
                }
            }

            Ty::Union(variants) => {
                let rendered: Vec<String> = variants
                    .iter()
                    .filter_map(|v| self.render_type(v, options).ok().flatten())
                    .collect();
                let splitter = match &options.or_splitter {
                    RenderSetting::Always(s) => s.as_str(),
                    RenderSetting::Auto | RenderSetting::Never => " or ",
                };
                Ok(Some(rendered.join(splitter)))
            }

            Ty::Enum(tn) => {
                if let Some(enm) = self.find_enum(tn.display_name.as_str()) {
                    Ok(Some(self.render_enum(enm, options)))
                } else {
                    Ok(Some(tn.display_name.to_string()))
                }
            }

            Ty::Class(tn) => {
                if let Some(cls) = self.find_class(tn.display_name.as_str()) {
                    Ok(Some(self.render_class(cls, options)?))
                } else {
                    Ok(Some(tn.display_name.to_string()))
                }
            }

            Ty::Media(_) => Err(RenderError::UnsupportedType("media".to_string())),

            // Literal rendering follows LiteralValue::Display from engine:
            // - String: "value" (quoted with double quotes)
            // - Int: 42 (plain number)
            // - Bool: true/false
            Ty::Literal(lit) => Ok(Some(render_literal(lit))),

            // Never is uninhabited — no value to render in the output format.
            Ty::Never => Ok(None),

            // Runtime-only variants that shouldn't appear in LLM prompts
            Ty::Opaque(tn) => Err(RenderError::UnsupportedType(tn.to_string())),

            // Compiler-only variants should never reach runtime
            Ty::TypeAlias(_)
            | Ty::Function { .. }
            | Ty::Void
            | Ty::WatchAccessor(_)
            | Ty::BuiltinUnknown => {
                unreachable!(
                    "compiler-only variant {:?} should not reach output_format",
                    ty
                )
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn render_enum(&self, enm: &Enum, _options: &RenderOptions) -> String {
        let values: Vec<String> = enm
            .values
            .iter()
            .map(|(name, desc)| match desc {
                Some(d) => format!("{name} // {d}"),
                None => name.clone(),
            })
            .collect();
        values.join("\n")
    }

    fn render_class(&self, cls: &Class, options: &RenderOptions) -> Result<String, RenderError> {
        use std::fmt::Write;

        let mut fields_str = Vec::new();

        for (name, ty, desc) in &cls.fields {
            let ty_str = self
                .render_type(ty, options)?
                .unwrap_or_else(|| "unknown".to_string());
            let quote_fields = matches!(options.quote_class_fields, RenderSetting::Always(true));
            let field_name = if quote_fields {
                format!("\"{name}\"")
            } else {
                name.clone()
            };
            let field = match desc {
                Some(d) => format!("  {field_name}: {ty_str}, // {d}"),
                None => format!("  {field_name}: {ty_str},"),
            };
            fields_str.push(field);
        }

        let mut output = String::new();
        if let Some(ref d) = cls.description {
            let _ = writeln!(output, "// {d}");
        }
        output.push_str("{\n");
        output.push_str(&fields_str.join("\n"));
        output.push_str("\n}");

        Ok(output)
    }
}

/// Render a literal value following engine's `LiteralValue::Display` convention.
fn render_literal(lit: &LiteralValue) -> String {
    match lit {
        LiteralValue::String(s) => format!("\"{s}\""),
        LiteralValue::Int(n) => n.to_string(),
        LiteralValue::Float(f) => f.clone(),
        LiteralValue::Bool(b) => b.to_string(),
    }
}

/// Tri-state setting: Auto (default behavior), Always(value), or Never.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:193-199
#[derive(Clone, Debug, Default)]
pub enum RenderSetting<T> {
    #[default]
    Auto,
    Always(T),
    Never,
}

/// Map rendering style.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:201-208
#[derive(Clone, Debug, Default)]
pub enum MapStyle {
    /// Render as `map<K, V>` (angle bracket style)
    #[default]
    TypeParameters,
    /// Render as `{ [key: K]: V }` (object literal style)
    ObjectLiteral,
}

/// Hoist classes setting.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:213-221
#[derive(Clone, Debug, Default)]
pub enum HoistClasses {
    /// Hoist all classes.
    All,
    /// Hoist only the specified subset.
    Subset(Vec<String>),
    /// Default behavior (for now: don't hoist, since we don't track recursive classes).
    #[default]
    Auto,
}

/// Options for rendering output format.
/// Ported from engine/baml-lib/jinja-runtime/src/output_format/types.rs:226-235
#[derive(Clone, Debug)]
pub struct RenderOptions {
    /// Prefix for the output format (e.g., "Answer in JSON using this schema:")
    pub prefix: RenderSetting<String>,
    /// Separator for union/or types (default: " or ")
    pub or_splitter: RenderSetting<String>,
    /// Prefix for enum values
    pub enum_value_prefix: RenderSetting<String>,
    /// Prefix for hoisted class definitions
    pub hoisted_class_prefix: RenderSetting<String>,
    /// Which classes to hoist
    pub hoist_classes: HoistClasses,
    /// Whether to always hoist enums
    pub always_hoist_enums: RenderSetting<bool>,
    /// Map rendering style
    pub map_style: MapStyle,
    /// Whether to quote class field names
    pub quote_class_fields: RenderSetting<bool>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            prefix: RenderSetting::Auto,
            or_splitter: RenderSetting::Auto,
            enum_value_prefix: RenderSetting::Auto,
            hoisted_class_prefix: RenderSetting::Auto,
            hoist_classes: HoistClasses::Auto,
            always_hoist_enums: RenderSetting::Auto,
            map_style: MapStyle::TypeParameters,
            quote_class_fields: RenderSetting::Auto,
        }
    }
}

impl RenderOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_string() {
        let content = OutputFormatContent::new(Ty::String);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, None);
    }

    #[test]
    fn test_render_int() {
        let content = OutputFormatContent::new(Ty::Int);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as an int".to_string()));
    }

    #[test]
    fn test_render_float() {
        let content = OutputFormatContent::new(Ty::Float);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a float".to_string()));
    }

    #[test]
    fn test_render_bool() {
        let content = OutputFormatContent::new(Ty::Bool);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a bool".to_string()));
    }

    #[test]
    fn test_render_list() {
        let content = OutputFormatContent::new(Ty::List(Box::new(Ty::String)));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer with a JSON Array using this schema:\nstring[]".to_string())
        );
    }

    #[test]
    fn test_render_list_of_int() {
        let content = OutputFormatContent::new(Ty::List(Box::new(Ty::Int)));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer with a JSON Array using this schema:\nint[]".to_string())
        );
    }

    #[test]
    fn test_render_optional() {
        let content = OutputFormatContent::new(Ty::Optional(Box::new(Ty::String)));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("string | null".to_string()));
    }

    #[test]
    fn test_render_map() {
        let content = OutputFormatContent::new(Ty::Map {
            key: Box::new(Ty::String),
            value: Box::new(Ty::Int),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using this schema:\nmap<string, int>".to_string())
        );
    }

    #[test]
    fn test_render_class() {
        let cls = Class {
            name: "Person".to_string(),
            description: Some("A person".to_string()),
            fields: vec![
                ("name".to_string(), Ty::String, None),
                ("age".to_string(), Ty::Int, Some("Age in years".to_string())),
            ],
        };

        let content =
            OutputFormatContent::new(Ty::Class(baml_type::TypeName::local("Person".into())))
                .with_class(cls);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 // A person\n\
                 {\n  \
                   name: string,\n  \
                   age: int, // Age in years\n\
                 }"
                .to_string()
            )
        );
    }

    #[test]
    fn test_render_class_no_description() {
        let cls = Class {
            name: "Point".to_string(),
            description: None,
            fields: vec![
                ("x".to_string(), Ty::Int, None),
                ("y".to_string(), Ty::Int, None),
            ],
        };

        let content =
            OutputFormatContent::new(Ty::Class(baml_type::TypeName::local("Point".into())))
                .with_class(cls);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n  \
                   x: int,\n  \
                   y: int,\n\
                 }"
                .to_string()
            )
        );
    }

    #[test]
    fn test_render_enum() {
        let enm = Enum {
            name: "Color".to_string(),
            values: vec![
                ("Red".to_string(), None),
                ("Green".to_string(), Some("Like grass".to_string())),
                ("Blue".to_string(), None),
            ],
        };

        let content =
            OutputFormatContent::new(Ty::Enum(baml_type::TypeName::local("Color".into())))
                .with_enum(enm);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Red\n\
                 Green // Like grass\n\
                 Blue"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_union() {
        let content = OutputFormatContent::new(Ty::Union(vec![Ty::String, Ty::Int, Ty::Bool]));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using this schema:\nstring or int or bool".to_string())
        );
    }

    #[test]
    fn test_render_with_custom_or_splitter() {
        let content = OutputFormatContent::new(Ty::Union(vec![Ty::String, Ty::Int]));
        let options = RenderOptions {
            or_splitter: RenderSetting::Always(" | ".to_string()),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using this schema:\nstring | int".to_string())
        );
    }

    #[test]
    fn test_render_literal_string() {
        let content =
            OutputFormatContent::new(Ty::Literal(LiteralValue::String("hello".to_string())));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\n\"hello\"".to_string())
        );
    }

    #[test]
    fn test_render_literal_int() {
        let content = OutputFormatContent::new(Ty::Literal(LiteralValue::Int(42)));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\n42".to_string())
        );
    }

    #[test]
    fn test_render_literal_bool() {
        let content = OutputFormatContent::new(Ty::Literal(LiteralValue::Bool(true)));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\ntrue".to_string())
        );
    }

    #[test]
    fn test_render_opaque_unsupported() {
        let content = OutputFormatContent::new(Ty::type_type());
        let err = content.render(&RenderOptions::default()).unwrap_err();
        assert!(matches!(err, RenderError::UnsupportedType(s) if s == "type"));
    }
}
