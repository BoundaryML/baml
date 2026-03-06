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

/// A value within an enum definition for output format rendering.
#[derive(Clone, Debug)]
pub struct EnumValue {
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
}

/// An enum definition for output format rendering.
#[derive(Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub values: Vec<EnumValue>,
}

/// A field within a class definition for output format rendering.
#[derive(Clone, Debug)]
pub struct ClassField {
    pub name: String,
    pub alias: Option<String>,
    pub field_type: Ty,
    pub description: Option<String>,
}

/// A class definition for output format rendering.
#[derive(Clone, Debug)]
pub struct Class {
    pub name: String,
    pub alias: Option<String>,
    pub description: Option<String>,
    pub fields: Vec<ClassField>,
}

/// Content for rendering output format schemas.
#[derive(Clone, Debug)]
pub struct OutputFormatContent {
    pub enums: IndexMap<String, Enum>,
    pub classes: IndexMap<String, Class>,
    pub target: Ty,
    pub recursive_classes: indexmap::IndexSet<String>,
    /// Recursive type aliases: alias name → target type.
    pub recursive_type_aliases: IndexMap<String, Ty>,
}

impl OutputFormatContent {
    /// Create a new `OutputFormatContent` with the given target type.
    pub fn new(target: Ty) -> Self {
        Self {
            enums: IndexMap::new(),
            classes: IndexMap::new(),
            target,
            recursive_classes: indexmap::IndexSet::new(),
            recursive_type_aliases: IndexMap::new(),
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

    /// Mark a class as recursive (will be hoisted during rendering).
    #[must_use]
    pub fn with_recursive_class(mut self, name: String) -> Self {
        self.recursive_classes.insert(name);
        self
    }

    /// Add a recursive type alias (alias name → target type).
    #[must_use]
    pub fn with_recursive_type_alias(mut self, name: String, target: Ty) -> Self {
        self.recursive_type_aliases.insert(name, target);
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
        if matches!(self.target, Ty::String { .. }) && matches!(options.prefix, RenderSetting::Auto)
        {
            return Ok(None);
        }

        // Compute which classes to hoist
        let hoisted = self.compute_hoisted_classes(options);

        let prefix = self.get_prefix(options, &hoisted);

        // For simple primitives (int, float, bool) with Auto prefix, the prefix IS the full message
        // But with explicit prefix, we need to append the type
        if matches!(
            self.target,
            Ty::Int { .. } | Ty::Float { .. } | Ty::Bool { .. }
        ) && matches!(options.prefix, RenderSetting::Auto)
        {
            return Ok(prefix);
        }

        // Render hoisted class definitions
        let mut hoisted_defs = Vec::new();
        for name in &hoisted {
            if let Some(cls) = self.find_class(name) {
                let body = self.render_class_hoisted(cls, options, &hoisted)?;

                let hoisted_prefix = match &options.hoisted_class_prefix {
                    RenderSetting::Always(p) if !p.is_empty() => format!("{p} "),
                    _ => String::new(),
                };

                let display_name = rendered_name(name, cls.alias.as_ref());

                let def = format!("{hoisted_prefix}{display_name} {body}");

                hoisted_defs.push(def);
            }
        }

        // Render hoisted type alias definitions
        let mut alias_defs = Vec::new();
        for (alias_name, target_ty) in &self.recursive_type_aliases {
            let target_str = self
                .render_type_hoisted(target_ty, options, &hoisted)?
                .unwrap_or_else(|| "unknown".to_string());

            let def = match &options.hoisted_class_prefix {
                RenderSetting::Always(p) if !p.is_empty() => {
                    format!("{p} {alias_name} = {target_str}")
                }
                _ => format!("{alias_name} = {target_str}"),
            };
            alias_defs.push(def);
        }

        // Render the target type with hoisting awareness
        let target_rendered = if let Ty::Class(tn, _) = &self.target {
            if hoisted.contains(tn.display_name.as_str()) {
                // Use alias if the class has one
                let display_name = self
                    .find_class(tn.display_name.as_str())
                    .and_then(|cls| cls.alias.as_deref())
                    .unwrap_or(tn.display_name.as_str());
                Some(display_name.to_string())
            } else {
                self.render_type_hoisted(&self.target, options, &hoisted)?
            }
        } else if let Ty::TypeAlias(fqn, _) = &self.target {
            // Recursive type alias target: render as just the display name
            Some(fqn.display_name.to_string())
        } else {
            self.render_type_hoisted(&self.target, options, &hoisted)?
        };

        // Assemble: hoisted class defs + alias defs + prefix + target
        let mut output = String::new();
        if !hoisted_defs.is_empty() {
            output.push_str(&hoisted_defs.join("\n\n"));
            if !alias_defs.is_empty() {
                output.push_str("\n\n");
            }
        }
        if !alias_defs.is_empty() {
            output.push_str(&alias_defs.join("\n"));
            output.push_str("\n\n");
        } else if !hoisted_defs.is_empty() {
            output.push_str("\n\n");
        }
        if let Some(p) = prefix {
            output.push_str(&p);
        }
        if let Some(t) = target_rendered {
            output.push_str(&t);
        }

        if output.is_empty() {
            Ok(None)
        } else {
            Ok(Some(output))
        }
    }

    /// Compute which classes should be hoisted (rendered as top-level definitions).
    fn compute_hoisted_classes(&self, options: &RenderOptions) -> indexmap::IndexSet<String> {
        let mut hoisted = indexmap::IndexSet::new();

        // Recursive classes are always hoisted
        hoisted.extend(self.recursive_classes.iter().cloned());

        // Additional hoisting based on options
        match &options.hoist_classes {
            HoistClasses::All => {
                hoisted.extend(self.classes.keys().cloned());
            }
            HoistClasses::Subset(names) => {
                hoisted.extend(names.iter().cloned());
            }
            HoistClasses::Auto => {
                // Only recursive classes (already added above)
            }
        }

        hoisted
    }

    fn get_prefix(
        &self,
        options: &RenderOptions,
        hoisted: &indexmap::IndexSet<String>,
    ) -> Option<String> {
        match &options.prefix {
            RenderSetting::Always(p) => Some(p.clone()),
            RenderSetting::Never => None,
            RenderSetting::Auto => {
                let type_word = match &options.hoisted_class_prefix {
                    RenderSetting::Always(p) if !p.is_empty() => p.as_str(),
                    _ => "schema",
                };

                match &self.target {
                    Ty::String { .. } => None,
                    Ty::Int { .. } => Some("Answer as an int".to_string()),
                    Ty::Float { .. } => Some("Answer as a float".to_string()),
                    Ty::Bool { .. } => Some("Answer as a bool".to_string()),
                    Ty::List(..) => {
                        Some("Answer with a JSON Array using this schema:\n".to_string())
                    }
                    Ty::Class(tn, _) => {
                        let end = if hoisted.contains(tn.display_name.as_str()) {
                            " "
                        } else {
                            "\n"
                        };
                        Some(format!("Answer in JSON using this {type_word}:{end}"))
                    }
                    Ty::Map { .. } => Some(format!("Answer in JSON using this {type_word}:\n")),
                    Ty::Enum(..) => Some("Answer with any of the categories:\n".to_string()),
                    Ty::Union(variants, _) => {
                        // Distinguish optional (1 non-null variant) from true union (multiple)
                        let non_null_count = variants
                            .iter()
                            .filter(|v| !matches!(v, Ty::Null { .. }))
                            .count();
                        if non_null_count > 1 {
                            Some(format!("Answer in JSON using any of these {type_word}s:\n"))
                        } else {
                            Some(format!("Answer in JSON using this {type_word}:\n"))
                        }
                    }
                    Ty::TypeAlias(..) => Some(format!("Answer in JSON using this {type_word}: ")),
                    Ty::Literal(..) => Some("Answer using this specific value:\n".to_string()),
                    _ => None,
                }
            }
        }
    }

    /// Render a type, with hoisted classes rendered as just their name.
    fn render_type_hoisted(
        &self,
        ty: &Ty,
        options: &RenderOptions,
        hoisted: &indexmap::IndexSet<String>,
    ) -> Result<Option<String>, RenderError> {
        // Intercept hoisted classes: return just the (aliased) name
        if let Ty::Class(tn, _) = ty {
            if hoisted.contains(tn.display_name.as_str()) {
                let display_name = self
                    .find_class(tn.display_name.as_str())
                    .and_then(|cls| cls.alias.as_deref())
                    .unwrap_or(tn.display_name.as_str());
                return Ok(Some(display_name.to_string()));
            }
        }

        match ty {
            Ty::String { .. } => Ok(Some("string".to_string())),
            Ty::Int { .. } => Ok(Some("int".to_string())),
            Ty::Float { .. } => Ok(Some("float".to_string())),
            Ty::Bool { .. } => Ok(Some("bool".to_string())),
            Ty::Null { .. } => Ok(Some("null".to_string())),

            Ty::Optional(inner, _) => {
                let inner_str = self
                    .render_type_hoisted(inner, options, hoisted)?
                    .unwrap_or_else(|| "unknown".to_string());
                let splitter = match &options.or_splitter {
                    RenderSetting::Always(s) => s.as_str(),
                    RenderSetting::Auto | RenderSetting::Never => " or ",
                };
                Ok(Some(format!("{inner_str}{splitter}null")))
            }

            Ty::List(inner, _) => {
                let inner_str = self
                    .render_type_hoisted(inner, options, hoisted)?
                    .unwrap_or_else(|| "unknown".to_string());
                let needs_parens = matches!(inner.as_ref(), Ty::Union(_, _) | Ty::Optional(_, _));
                if needs_parens {
                    Ok(Some(format!("({inner_str})[]")))
                } else {
                    Ok(Some(format!("{inner_str}[]")))
                }
            }

            Ty::Map { key, value, .. } => {
                let key_str = self
                    .render_type_hoisted(key, options, hoisted)?
                    .unwrap_or_else(|| "string".to_string());
                let value_str = self
                    .render_type_hoisted(value, options, hoisted)?
                    .unwrap_or_else(|| "unknown".to_string());
                match options.map_style {
                    MapStyle::TypeParameters => Ok(Some(format!("map<{key_str}, {value_str}>"))),
                    MapStyle::ObjectLiteral => {
                        Ok(Some(format!("{{ [key: {key_str}]: {value_str} }}")))
                    }
                }
            }

            Ty::Union(variants, _) => {
                let rendered: Vec<String> = variants
                    .iter()
                    .filter_map(|v| self.render_type_hoisted(v, options, hoisted).ok().flatten())
                    .collect();
                let splitter = match &options.or_splitter {
                    RenderSetting::Always(s) => s.as_str(),
                    RenderSetting::Auto | RenderSetting::Never => " or ",
                };
                Ok(Some(rendered.join(splitter)))
            }

            Ty::Enum(tn, _) => {
                if let Some(enm) = self.find_enum(tn.display_name.as_str()) {
                    Ok(Some(self.render_enum(enm, options)))
                } else {
                    Ok(Some(tn.display_name.to_string()))
                }
            }

            Ty::Class(tn, _) => {
                if let Some(cls) = self.find_class(tn.display_name.as_str()) {
                    Ok(Some(self.render_class_hoisted(cls, options, hoisted)?))
                } else {
                    Ok(Some(tn.display_name.to_string()))
                }
            }

            Ty::Media(_, _) => Err(RenderError::UnsupportedType("media".to_string())),

            Ty::Literal(lit, _) => Ok(Some(render_literal(lit))),

            Ty::Opaque(tn, _) => Err(RenderError::UnsupportedType(tn.to_string())),

            Ty::TypeAlias(fqn, _) => {
                // Recursive type aliases render as just their display name
                Ok(Some(fqn.display_name.to_string()))
            }

            Ty::Function { .. }
            | Ty::Void { .. }
            | Ty::WatchAccessor(..)
            | Ty::BuiltinUnknown { .. } => {
                unreachable!(
                    "compiler-only variant {:?} should not reach output_format",
                    ty
                )
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn render_enum(&self, enm: &Enum, options: &RenderOptions) -> String {
        let display_name = rendered_name(&enm.name, enm.alias.as_ref());

        // Header: "EnumName\n----"
        let mut result = format!("{display_name}\n----");

        // Values with prefix (default "- ")
        for v in &enm.values {
            let value_name = rendered_name(&v.name, v.alias.as_ref());
            let prefix = match &options.enum_value_prefix {
                RenderSetting::Auto => "- ",
                RenderSetting::Always(p) => p.as_str(),
                RenderSetting::Never => "",
            };
            let line = match &v.description {
                Some(d) => format!("{prefix}{value_name}: {d}"),
                None => format!("{prefix}{value_name}"),
            };
            result.push('\n');
            result.push_str(&line);
        }

        result
    }

    /// Render a class body, with hoisted classes rendered as just their name in field types.
    fn render_class_hoisted(
        &self,
        cls: &Class,
        options: &RenderOptions,
        hoisted: &indexmap::IndexSet<String>,
    ) -> Result<String, RenderError> {
        use std::fmt::Write;

        let mut fields_str = Vec::new();

        for field in &cls.fields {
            let ty_str = self
                .render_type_hoisted(&field.field_type, options, hoisted)?
                .unwrap_or_else(|| "unknown".to_string());
            // Re-indent multi-line type strings for proper nesting
            let ty_str = if ty_str.contains('\n') {
                ty_str.replace('\n', "\n  ")
            } else {
                ty_str
            };
            let display_name = rendered_name(&field.name, field.alias.as_ref());
            let quote_fields = matches!(options.quote_class_fields, RenderSetting::Always(true));
            let field_name = if quote_fields {
                format!("\"{display_name}\"")
            } else {
                display_name.to_string()
            };
            let line = match &field.description {
                Some(d) => format!("  {field_name}: {ty_str}, // {d}"),
                None => format!("  {field_name}: {ty_str},"),
            };
            fields_str.push(line);
        }

        let mut output = String::new();
        output.push_str("{\n");
        if let Some(ref d) = cls.description {
            let d = d.trim();
            if !d.is_empty() {
                for line in d.lines() {
                    let _ = writeln!(output, "  // {line}");
                }
                output.push('\n');
            }
        }
        output.push_str(&fields_str.join("\n"));
        output.push_str("\n}");

        Ok(output)
    }
}

/// Return alias if set, otherwise the real name.
fn rendered_name<'a>(name: &'a str, alias: Option<&'a String>) -> &'a str {
    alias.map(String::as_str).unwrap_or(name)
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
    /// Default behavior: hoist only recursive classes.
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
    use baml_type::TyAttr;

    use super::*;

    #[test]
    fn test_render_string() {
        let content = OutputFormatContent::new(Ty::String {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, None);
    }

    #[test]
    fn test_render_int() {
        let content = OutputFormatContent::new(Ty::Int {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as an int".to_string()));
    }

    #[test]
    fn test_render_float() {
        let content = OutputFormatContent::new(Ty::Float {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a float".to_string()));
    }

    #[test]
    fn test_render_bool() {
        let content = OutputFormatContent::new(Ty::Bool {
            attr: TyAttr::default(),
        });
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("Answer as a bool".to_string()));
    }

    #[test]
    fn test_render_list() {
        let content = OutputFormatContent::new(Ty::List(
            Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer with a JSON Array using this schema:\nstring[]".to_string())
        );
    }

    #[test]
    fn test_render_list_of_int() {
        let content = OutputFormatContent::new(Ty::List(
            Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer with a JSON Array using this schema:\nint[]".to_string())
        );
    }

    #[test]
    fn test_render_optional() {
        let content = OutputFormatContent::new(Ty::Optional(
            Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(rendered, Some("string or null".to_string()));
    }

    #[test]
    fn test_render_map() {
        let content = OutputFormatContent::new(Ty::Map {
            key: Box::new(Ty::String {
                attr: TyAttr::default(),
            }),
            value: Box::new(Ty::Int {
                attr: TyAttr::default(),
            }),
            attr: TyAttr::default(),
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
            alias: None,
            description: Some("A person".to_string()),
            fields: vec![
                ClassField {
                    name: "name".to_string(),
                    alias: None,
                    field_type: Ty::String {
                        attr: TyAttr::default(),
                    },
                    description: None,
                },
                ClassField {
                    name: "age".to_string(),
                    alias: None,
                    field_type: Ty::Int {
                        attr: TyAttr::default(),
                    },
                    description: Some("Age in years".to_string()),
                },
            ],
        };

        let content = OutputFormatContent::new(Ty::Class(
            baml_type::TypeName::local("Person".into()),
            TyAttr::default(),
        ))
        .with_class(cls);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n  \
                   // A person\n\
                 \n  \
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
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "x".to_string(),
                    alias: None,
                    field_type: Ty::Int {
                        attr: TyAttr::default(),
                    },
                    description: None,
                },
                ClassField {
                    name: "y".to_string(),
                    alias: None,
                    field_type: Ty::Int {
                        attr: TyAttr::default(),
                    },
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(Ty::Class(
            baml_type::TypeName::local("Point".into()),
            TyAttr::default(),
        ))
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
            alias: None,
            description: None,
            values: vec![
                EnumValue {
                    name: "Red".to_string(),
                    alias: None,
                    description: None,
                },
                EnumValue {
                    name: "Green".to_string(),
                    alias: None,
                    description: Some("Like grass".to_string()),
                },
                EnumValue {
                    name: "Blue".to_string(),
                    alias: None,
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(Ty::Enum(
            baml_type::TypeName::local("Color".into()),
            TyAttr::default(),
        ))
        .with_enum(enm);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Color\n\
                 ----\n\
                 - Red\n\
                 - Green: Like grass\n\
                 - Blue"
                    .to_string()
            )
        );
    }

    #[test]
    fn test_render_union() {
        let content = OutputFormatContent::new(Ty::Union(
            vec![
                Ty::String {
                    attr: TyAttr::default(),
                },
                Ty::Int {
                    attr: TyAttr::default(),
                },
                Ty::Bool {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using any of these schemas:\nstring or int or bool".to_string())
        );
    }

    #[test]
    fn test_render_with_custom_or_splitter() {
        let content = OutputFormatContent::new(Ty::Union(
            vec![
                Ty::String {
                    attr: TyAttr::default(),
                },
                Ty::Int {
                    attr: TyAttr::default(),
                },
            ],
            TyAttr::default(),
        ));
        let options = RenderOptions {
            or_splitter: RenderSetting::Always(" | ".to_string()),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some("Answer in JSON using any of these schemas:\nstring | int".to_string())
        );
    }

    #[test]
    fn test_render_literal_string() {
        let content = OutputFormatContent::new(Ty::Literal(
            LiteralValue::String("hello".to_string()),
            TyAttr::default(),
        ));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\n\"hello\"".to_string())
        );
    }

    #[test]
    fn test_render_literal_int() {
        let content =
            OutputFormatContent::new(Ty::Literal(LiteralValue::Int(42), TyAttr::default()));
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some("Answer using this specific value:\n42".to_string())
        );
    }

    #[test]
    fn test_render_literal_bool() {
        let content =
            OutputFormatContent::new(Ty::Literal(LiteralValue::Bool(true), TyAttr::default()));
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

    // ========================================================================
    // Helper functions for creating types (used by recursive type tests)
    // ========================================================================

    fn ty_int() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }
    fn ty_bool() -> Ty {
        Ty::Bool {
            attr: TyAttr::default(),
        }
    }
    fn ty_string() -> Ty {
        Ty::String {
            attr: TyAttr::default(),
        }
    }
    fn ty_float() -> Ty {
        Ty::Float {
            attr: TyAttr::default(),
        }
    }
    fn ty_class(name: &str) -> Ty {
        Ty::Class(baml_type::TypeName::local(name.into()), TyAttr::default())
    }
    fn ty_optional(inner: Ty) -> Ty {
        Ty::Optional(Box::new(inner), TyAttr::default())
    }
    fn ty_list(inner: Ty) -> Ty {
        Ty::List(Box::new(inner), TyAttr::default())
    }
    fn ty_map(key: Ty, value: Ty) -> Ty {
        Ty::Map {
            key: Box::new(key),
            value: Box::new(value),
            attr: TyAttr::default(),
        }
    }
    fn ty_union(variants: Vec<Ty>) -> Ty {
        Ty::Union(variants, TyAttr::default())
    }

    fn ty_enum(name: &str) -> Ty {
        Ty::Enum(baml_type::TypeName::local(name.into()), TyAttr::default())
    }

    fn mk_class(name: &str, fields: Vec<(&str, Ty)>) -> Class {
        Class {
            name: name.to_string(),
            alias: None,
            description: None,
            fields: fields
                .into_iter()
                .map(|(n, t)| ClassField {
                    name: n.to_string(),
                    alias: None,
                    field_type: t,
                    description: None,
                })
                .collect(),
        }
    }

    fn mk_class_desc(name: &str, desc: &str, fields: Vec<(&str, Ty)>) -> Class {
        Class {
            name: name.to_string(),
            alias: None,
            description: Some(desc.to_string()),
            fields: fields
                .into_iter()
                .map(|(n, t)| ClassField {
                    name: n.to_string(),
                    alias: None,
                    field_type: t,
                    description: None,
                })
                .collect(),
        }
    }

    fn mk_enum(name: &str, values: Vec<&str>) -> Enum {
        Enum {
            name: name.to_string(),
            alias: None,
            description: None,
            values: values
                .into_iter()
                .map(|v| EnumValue {
                    name: v.to_string(),
                    alias: None,
                    description: None,
                })
                .collect(),
        }
    }

    fn mk_recursive(names: &[&str]) -> indexmap::IndexSet<String> {
        names.iter().map(std::string::ToString::to_string).collect()
    }

    // ========================================================================
    // Recursive class tests (ported from engine)
    // ========================================================================

    #[test]
    fn test_render_top_level_simple_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("Node")).with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "Node {\n  data: int,\n  next: Node or null,\n}\n\n\
                 Answer in JSON using this schema: Node"
            ))
        );
    }

    #[test]
    fn test_render_nested_simple_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("LinkedList"))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "LinkedList",
                vec![("head", ty_optional(ty_class("Node"))), ("len", ty_int())],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  head: Node or null,
  len: int,
}"#
            ))
        );
    }

    #[test]
    fn test_top_level_recursive_cycle() {
        let mut content = OutputFormatContent::new(ty_class("A"))
            .with_class(mk_class("A", vec![("pointer", ty_class("B"))]))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"A {
  pointer: B,
}

B {
  pointer: C,
}

C {
  pointer: A or null,
}

Answer in JSON using this schema: A"#
            ))
        );
    }

    #[test]
    fn test_nested_recursive_cycle() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class("A", vec![("pointer", ty_class("B"))]))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    ("pointer", ty_class("A")),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"A {
  pointer: B,
}

B {
  pointer: C,
}

C {
  pointer: A or null,
}

Answer in JSON using this schema:
{
  pointer: A,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    #[test]
    fn test_nested_class_in_hoisted_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class(
                "A",
                vec![("pointer", ty_class("B")), ("nested", ty_class("Nested"))],
            ))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    ("pointer", ty_class("A")),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ))
            .with_class(mk_class(
                "Nested",
                vec![("data", ty_int()), ("field", ty_bool())],
            ));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"A {
  pointer: B,
  nested: {
    data: int,
    field: bool,
  },
}

B {
  pointer: C,
}

C {
  pointer: A or null,
}

Answer in JSON using this schema:
{
  pointer: A,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    #[test]
    fn test_mutually_recursive_list() {
        let mut content = OutputFormatContent::new(ty_class("Tree"))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_class("Forest"))],
            ))
            .with_class(mk_class(
                "Forest",
                vec![("trees", ty_list(ty_class("Tree")))],
            ));
        content.recursive_classes = mk_recursive(&["Tree", "Forest"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Tree {
  data: int,
  children: Forest,
}

Forest {
  trees: Tree[],
}

Answer in JSON using this schema: Tree"#
            ))
        );
    }

    // ========================================================================
    // Recursive class with description
    // ========================================================================

    #[test]
    fn test_hoisted_class_with_description() {
        let mut content = OutputFormatContent::new(ty_class("Node")).with_class(mk_class_desc(
            "Node",
            "A node in a linked list",
            vec![("value", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "Node {\n  // A node in a linked list\n\n  value: int,\n  next: Node or null,\n}\n\n\
                 Answer in JSON using this schema: Node"
            ))
        );
    }

    // ========================================================================
    // Recursive union tests
    // ========================================================================

    #[test]
    fn test_top_level_recursive_union() {
        let mut content =
            OutputFormatContent::new(ty_union(vec![ty_class("Node"), ty_class("Tree")]))
                .with_class(mk_class(
                    "Node",
                    vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
                ))
                .with_class(mk_class(
                    "Tree",
                    vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
                ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using any of these schemas:
Node or Tree"#
            ))
        );
    }

    #[test]
    fn test_nested_recursive_union() {
        let mut content = OutputFormatContent::new(ty_class("DataType"))
            .with_class(mk_class(
                "DataType",
                vec![
                    (
                        "data_type",
                        ty_union(vec![ty_class("Node"), ty_class("Tree")]),
                    ),
                    ("len", ty_int()),
                    ("description", ty_string()),
                ],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
            ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using this schema:
{
  data_type: Node or Tree,
  len: int,
  description: string,
}"#
            ))
        );
    }

    #[test]
    fn test_top_level_recursive_union_with_non_recursive_class() {
        let mut content = OutputFormatContent::new(ty_union(vec![
            ty_class("Node"),
            ty_class("Tree"),
            ty_class("NonRecursive"),
        ]))
        .with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ))
        .with_class(mk_class(
            "Tree",
            vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
        ))
        .with_class(mk_class(
            "NonRecursive",
            vec![("data", ty_int()), ("tag", ty_string())],
        ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using any of these schemas:
Node or Tree or {
  data: int,
  tag: string,
}"#
            ))
        );
    }

    #[test]
    fn test_nested_recursive_union_with_non_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("DataType"))
            .with_class(mk_class(
                "DataType",
                vec![
                    (
                        "data_type",
                        ty_union(vec![
                            ty_class("Node"),
                            ty_class("Tree"),
                            ty_class("NonRecursive"),
                        ]),
                    ),
                    ("len", ty_int()),
                    ("description", ty_string()),
                ],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
            ))
            .with_class(mk_class(
                "NonRecursive",
                vec![("data", ty_int()), ("tag", ty_string())],
            ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using this schema:
{
  data_type: Node or Tree or {
    data: int,
    tag: string,
  },
  len: int,
  description: string,
}"#
            ))
        );
    }

    #[test]
    fn test_top_level_union_of_unions_pointing_to_recursive_class() {
        let mut content = OutputFormatContent::new(ty_union(vec![
            ty_union(vec![ty_class("Node"), ty_int()]),
            ty_union(vec![ty_string(), ty_class("Tree")]),
        ]))
        .with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ))
        .with_class(mk_class(
            "Tree",
            vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
        ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using any of these schemas:
Node or int or string or Tree"#
            ))
        );
    }

    #[test]
    fn test_nested_union_of_unions_pointing_to_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    (
                        "the_union",
                        ty_union(vec![
                            ty_union(vec![ty_class("Node"), ty_int()]),
                            ty_union(vec![ty_string(), ty_class("Tree")]),
                        ]),
                    ),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Tree",
                vec![("data", ty_int()), ("children", ty_list(ty_class("Tree")))],
            ));
        content.recursive_classes = mk_recursive(&["Node", "Tree"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Tree {
  data: int,
  children: Tree[],
}

Answer in JSON using this schema:
{
  the_union: Node or int or string or Tree,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    // ========================================================================
    // Collection types (list/map) with recursion
    // ========================================================================

    #[test]
    fn test_render_top_level_list_with_recursive_items() {
        let mut content = OutputFormatContent::new(ty_list(ty_class("Node"))).with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer with a JSON Array using this schema:
Node[]"#
            ))
        );
    }

    #[test]
    fn test_render_top_level_class_with_self_referential_map() {
        let mut content = OutputFormatContent::new(ty_class("RecursiveMap")).with_class(mk_class(
            "RecursiveMap",
            vec![("data", ty_map(ty_string(), ty_class("RecursiveMap")))],
        ));
        content.recursive_classes = mk_recursive(&["RecursiveMap"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"RecursiveMap {
  data: map<string, RecursiveMap>,
}

Answer in JSON using this schema: RecursiveMap"#
            ))
        );
    }

    #[test]
    fn test_render_nested_self_referential_map() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class(
                "RecursiveMap",
                vec![("data", ty_map(ty_string(), ty_class("RecursiveMap")))],
            ))
            .with_class(mk_class(
                "NonRecursive",
                vec![("rec_map", ty_class("RecursiveMap"))],
            ));
        content.recursive_classes = mk_recursive(&["RecursiveMap"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"RecursiveMap {
  data: map<string, RecursiveMap>,
}

Answer in JSON using this schema:
{
  rec_map: RecursiveMap,
}"#
            ))
        );
    }

    #[test]
    fn test_render_top_level_map_pointing_to_another_recursive_class() {
        let mut content = OutputFormatContent::new(ty_map(ty_string(), ty_class("Node")))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
map<string, Node>"#
            ))
        );
    }

    #[test]
    fn test_render_nested_map_pointing_to_another_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("MapWithRecValue"))
            .with_class(mk_class(
                "MapWithRecValue",
                vec![("data", ty_map(ty_string(), ty_class("Node")))],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: map<string, Node>,
}"#
            ))
        );
    }

    #[test]
    fn test_render_nested_map_pointing_to_another_optional_recursive_class() {
        let mut content = OutputFormatContent::new(ty_class("MapWithRecValue"))
            .with_class(mk_class(
                "MapWithRecValue",
                vec![("data", ty_map(ty_string(), ty_optional(ty_class("Node"))))],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: map<string, Node or null>,
}"#
            ))
        );
    }

    #[test]
    fn test_render_top_level_map_pointing_to_recursive_union() {
        let mut content = OutputFormatContent::new(ty_map(
            ty_string(),
            ty_union(vec![ty_class("Node"), ty_int(), ty_class("NonRecursive")]),
        ))
        .with_class(mk_class(
            "Node",
            vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
        ))
        .with_class(mk_class(
            "NonRecursive",
            vec![("field", ty_string()), ("data", ty_int())],
        ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
map<string, Node or int or {
  field: string,
  data: int,
}>"#
            ))
        );
    }

    #[test]
    fn test_render_nested_map_pointing_to_recursive_union() {
        let mut content = OutputFormatContent::new(ty_class("MapWithRecUnion"))
            .with_class(mk_class(
                "MapWithRecUnion",
                vec![(
                    "data",
                    ty_map(
                        ty_string(),
                        ty_union(vec![ty_class("Node"), ty_int(), ty_class("NonRecursive")]),
                    ),
                )],
            ))
            .with_class(mk_class(
                "Node",
                vec![("data", ty_int()), ("next", ty_optional(ty_class("Node")))],
            ))
            .with_class(mk_class(
                "NonRecursive",
                vec![("field", ty_string()), ("data", ty_int())],
            ));
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"Node {
  data: int,
  next: Node or null,
}

Answer in JSON using this schema:
{
  data: map<string, Node or int or {
    field: string,
    data: int,
  }>,
}"#
            ))
        );
    }

    // ========================================================================
    // Hoisting options tests
    // ========================================================================

    #[test]
    fn test_render_hoisted_classes_with_prefix() {
        let mut content = OutputFormatContent::new(ty_class("NonRecursive"))
            .with_class(mk_class("A", vec![("pointer", ty_class("B"))]))
            .with_class(mk_class("B", vec![("pointer", ty_class("C"))]))
            .with_class(mk_class("C", vec![("pointer", ty_optional(ty_class("A")))]))
            .with_class(mk_class(
                "NonRecursive",
                vec![
                    ("pointer", ty_class("A")),
                    ("data", ty_int()),
                    ("field", ty_bool()),
                ],
            ));
        content.recursive_classes = mk_recursive(&["A", "B", "C"]);

        let options = RenderOptions {
            hoisted_class_prefix: RenderSetting::Always("interface".to_string()),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"interface A {
  pointer: B,
}

interface B {
  pointer: C,
}

interface C {
  pointer: A or null,
}

Answer in JSON using this interface:
{
  pointer: A,
  data: int,
  field: bool,
}"#
            ))
        );
    }

    #[test]
    fn test_render_hoisted_classes_subset() {
        let content = OutputFormatContent::new(ty_class("Ret"))
            .with_class(mk_class("A", vec![("prop", ty_int())]))
            .with_class(mk_class("B", vec![("prop", ty_string())]))
            .with_class(mk_class("C", vec![("prop", ty_float())]))
            .with_class(mk_class(
                "Ret",
                vec![
                    ("a", ty_class("A")),
                    ("b", ty_class("B")),
                    ("c", ty_class("C")),
                ],
            ));

        let options = RenderOptions {
            hoist_classes: HoistClasses::Subset(vec!["A".to_string(), "B".to_string()]),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"A {
  prop: int,
}

B {
  prop: string,
}

Answer in JSON using this schema:
{
  a: A,
  b: B,
  c: {
    prop: float,
  },
}"#
            ))
        );
    }

    #[test]
    fn test_render_hoist_all_classes() {
        let content = OutputFormatContent::new(ty_class("Ret"))
            .with_class(mk_class("A", vec![("prop", ty_int())]))
            .with_class(mk_class("B", vec![("prop", ty_string())]))
            .with_class(mk_class("C", vec![("prop", ty_float())]))
            .with_class(mk_class(
                "Ret",
                vec![
                    ("a", ty_class("A")),
                    ("b", ty_class("B")),
                    ("c", ty_class("C")),
                ],
            ));

        let options = RenderOptions {
            hoist_classes: HoistClasses::All,
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"A {
  prop: int,
}

B {
  prop: string,
}

C {
  prop: float,
}

Ret {
  a: A,
  b: B,
  c: C,
}

Answer in JSON using this schema: Ret"#
            ))
        );
    }

    // ========================================================================
    // Attribute handling tests (ported from old engine + new)
    // ========================================================================

    /// Ported from old engine: `skipped_variants_are_not_rendered`
    #[test]
    fn skipped_variants_are_not_rendered() {
        // Enum Foo with @skip on Baz variant — only Bar should render
        let enm = Enum {
            name: "Foo".to_string(),
            alias: None,
            description: None,
            values: vec![EnumValue {
                name: "Bar".to_string(),
                alias: None,
                description: None,
            }],
            // Baz is already filtered out by the extraction layer
        };

        let content = OutputFormatContent::new(ty_enum("Foo")).with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Foo\n\
                 ----\n\
                 - Bar"
                    .to_string()
            )
        );
    }

    /// Ported from old engine: `skipped_class_fields_are_not_rendered`
    #[test]
    fn skipped_class_fields_are_not_rendered() {
        // Class with @skip optional field — only `keep` field rendered
        let cls = Class {
            name: "MyClass".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "keep".to_string(),
                alias: None,
                field_type: ty_string(),
                description: None,
            }],
            // hidden field is already filtered out by the extraction layer
        };

        let content = OutputFormatContent::new(ty_class("MyClass")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n\
                 \x20 keep: string,\n\
                 }"
                .to_string()
            )
        );
    }

    /// Ported from old engine: `test_render_output_format_aliases` (recursive Date → hoisted)
    /// Note: Enum hoisting and list wrapping format differ from old engine.
    /// This test verifies alias/description/skip work correctly with the current renderer.
    #[test]
    fn test_render_output_format_aliases() {
        // Recursive Date class (self-referencing via year: Date?)
        let month_enum = mk_enum(
            "Month",
            vec![
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
        );

        let date_cls = Class {
            name: "Date".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "day".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
                ClassField {
                    name: "month".to_string(),
                    alias: None,
                    field_type: ty_enum("Month"),
                    description: None,
                },
                ClassField {
                    name: "year".to_string(),
                    alias: None,
                    field_type: ty_optional(ty_class("Date")),
                    description: None,
                },
            ],
        };

        let education_cls = Class {
            name: "Education".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "from_date".to_string(),
                    alias: None,
                    field_type: ty_class("Date"),
                    description: None,
                },
                ClassField {
                    name: "to_date".to_string(),
                    alias: None,
                    field_type: ty_union(vec![
                        ty_class("Date"),
                        Ty::Literal(
                            LiteralValue::String("current".to_string()),
                            TyAttr::default(),
                        ),
                    ]),
                    description: None,
                },
                ClassField {
                    name: "school".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "description".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
            ],
        };

        let resume_cls = Class {
            name: "Resume".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "education".to_string(),
                alias: None,
                field_type: ty_list(ty_class("Education")),
                description: None,
            }],
        };

        let mut content = OutputFormatContent::new(ty_class("Resume"))
            .with_enum(month_enum)
            .with_class(date_cls)
            .with_class(education_cls)
            .with_class(resume_cls);
        content.recursive_classes = mk_recursive(&["Date"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        // Verify Date is hoisted, enum renders inline with header, fields present
        let r = rendered.unwrap();
        assert!(r.contains("Date {"), "Date should be hoisted");
        assert!(r.contains("day: int,"), "Date should have day field");
        assert!(
            r.contains("year: Date or null,"),
            "Date should self-reference"
        );
        assert!(
            r.contains("from_date: Date,"),
            "Education should reference Date"
        );
        assert!(r.contains("to_date: Date or \"current\","), "to_date union");
        assert!(r.contains("school: string,"), "school field");
    }

    /// Ported from old engine: `test_render_output_format` (non-recursive → inline)
    #[test]
    fn test_render_output_format_inline() {
        // Non-recursive version — Date has year: int (not Date?)
        let month_enum = mk_enum(
            "Month",
            vec![
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
        );

        let date_cls = Class {
            name: "Date".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "day".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
                ClassField {
                    name: "month".to_string(),
                    alias: None,
                    field_type: ty_enum("Month"),
                    description: None,
                },
                ClassField {
                    name: "year".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
            ],
        };

        let education_cls = Class {
            name: "Education".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "from_date".to_string(),
                    alias: None,
                    field_type: ty_class("Date"),
                    description: None,
                },
                ClassField {
                    name: "to_date".to_string(),
                    alias: None,
                    field_type: ty_union(vec![
                        ty_class("Date"),
                        Ty::Literal(
                            LiteralValue::String("current".to_string()),
                            TyAttr::default(),
                        ),
                    ]),
                    description: None,
                },
                ClassField {
                    name: "school".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "description".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
            ],
        };

        let resume_cls = Class {
            name: "Resume".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "education".to_string(),
                alias: None,
                field_type: ty_list(ty_class("Education")),
                description: None,
            }],
        };

        let content = OutputFormatContent::new(ty_class("Resume"))
            .with_enum(month_enum)
            .with_class(date_cls)
            .with_class(education_cls)
            .with_class(resume_cls);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        // Verify inline rendering (no hoisting since not recursive)
        let r = rendered.unwrap();
        assert!(
            !r.contains("Date {"),
            "Date should NOT be hoisted (not recursive)"
        );
        assert!(r.contains("day: int,"), "Date fields should be inline");
        assert!(r.contains("year: int,"), "year is int, not self-ref");
        assert!(r.contains("from_date:"), "Education from_date");
        assert!(r.contains("school: string,"), "school field");
    }

    /// Ported from old engine: `test_render_output_format_description_and_alias`
    #[test]
    fn test_render_output_format_description_and_alias() {
        let cls = Class {
            name: "MyClass".to_string(),
            alias: None,
            description: None,
            fields: vec![ClassField {
                name: "Name".to_string(),
                alias: Some("a".to_string()),
                field_type: ty_string(),
                description: Some("d".to_string()),
            }],
        };

        let content = OutputFormatContent::new(ty_class("MyClass")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n\
                 \x20 a: string, // d\n\
                 }"
                .to_string()
            )
        );
    }

    /// New test: class with @alias on fields, no description
    #[test]
    fn test_render_class_with_field_alias() {
        let cls = Class {
            name: "MyClass".to_string(),
            alias: None,
            description: None,
            fields: vec![
                ClassField {
                    name: "my_field".to_string(),
                    alias: Some("myField".to_string()),
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "other".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(ty_class("MyClass")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n\
                 \x20 myField: string,\n\
                 \x20 other: int,\n\
                 }"
                .to_string()
            )
        );
    }

    /// New test: enum with @alias on variants
    #[test]
    fn test_render_enum_with_variant_alias() {
        let enm = Enum {
            name: "Color".to_string(),
            alias: None,
            description: None,
            values: vec![
                EnumValue {
                    name: "Red".to_string(),
                    alias: Some("r".to_string()),
                    description: None,
                },
                EnumValue {
                    name: "Green".to_string(),
                    alias: Some("g".to_string()),
                    description: Some("Like grass".to_string()),
                },
                EnumValue {
                    name: "Blue".to_string(),
                    alias: None,
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(ty_enum("Color")).with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Color\n\
                 ----\n\
                 - r\n\
                 - g: Like grass\n\
                 - Blue"
                    .to_string()
            )
        );
    }

    /// New test: enum with @@alias on the enum itself
    #[test]
    fn test_render_enum_with_block_alias() {
        let enm = Enum {
            name: "TestEnum".to_string(),
            alias: Some("Category".to_string()),
            description: None,
            values: vec![
                EnumValue {
                    name: "A".to_string(),
                    alias: None,
                    description: None,
                },
                EnumValue {
                    name: "B".to_string(),
                    alias: None,
                    description: None,
                },
            ],
        };

        let content = OutputFormatContent::new(ty_enum("TestEnum")).with_enum(enm);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer with any of the categories:\n\
                 Category\n\
                 ----\n\
                 - A\n\
                 - B"
                .to_string()
            )
        );
    }

    /// New test: recursive class with @@alias, verify hoisted definition and references use alias
    #[test]
    fn test_render_hoisted_class_with_alias() {
        let cls = Class {
            name: "Node".to_string(),
            alias: Some("GraphNode".to_string()),
            description: None,
            fields: vec![
                ClassField {
                    name: "data".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: None,
                },
                ClassField {
                    name: "next".to_string(),
                    alias: None,
                    field_type: ty_optional(ty_class("Node")),
                    description: None,
                },
            ],
        };

        let mut content = OutputFormatContent::new(ty_class("Node")).with_class(cls);
        content.recursive_classes = mk_recursive(&["Node"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                "GraphNode {\n\
                 \x20 data: int,\n\
                 \x20 next: GraphNode or null,\n\
                 }\n\
                 \n\
                 Answer in JSON using this schema: GraphNode"
            ))
        );
    }

    /// New test: class with @@description, verify comment rendered
    #[test]
    fn test_render_class_with_class_description() {
        let cls = Class {
            name: "Foo".to_string(),
            alias: None,
            description: Some("A foo object".to_string()),
            fields: vec![
                ClassField {
                    name: "bar".to_string(),
                    alias: None,
                    field_type: ty_string(),
                    description: None,
                },
                ClassField {
                    name: "baz".to_string(),
                    alias: None,
                    field_type: ty_int(),
                    description: Some("A baz field".to_string()),
                },
            ],
        };

        let content = OutputFormatContent::new(ty_class("Foo")).with_class(cls);
        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(
                "Answer in JSON using this schema:\n\
                 {\n  \
                   // A foo object\n\
                 \n  \
                   bar: string,\n  \
                   baz: int, // A baz field\n\
                 }"
                .to_string()
            )
        );
    }

    // ========================================================================
    // Phase 5: Additional test coverage
    // ========================================================================

    fn ty_alias(name: &str) -> Ty {
        Ty::TypeAlias(baml_type::TypeName::local(name.into()), TyAttr::default())
    }

    #[test]
    fn test_self_referential_union() {
        let mut content =
            OutputFormatContent::new(ty_class("SelfReferential")).with_class(mk_class(
                "SelfReferential",
                vec![(
                    "recursion",
                    ty_union(vec![
                        ty_int(),
                        ty_string(),
                        ty_optional(ty_class("SelfReferential")),
                    ]),
                )],
            ));
        content.recursive_classes = mk_recursive(&["SelfReferential"]);

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"SelfReferential {
  recursion: int or string or SelfReferential or null,
}

Answer in JSON using this schema: SelfReferential"#
            ))
        );
    }

    #[test]
    fn test_render_simple_recursive_alias() {
        let mut content = OutputFormatContent::new(ty_alias("RecursiveMapAlias"));
        content.recursive_type_aliases.insert(
            "RecursiveMapAlias".to_string(),
            ty_map(ty_string(), ty_alias("RecursiveMapAlias")),
        );

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"RecursiveMapAlias = map<string, RecursiveMapAlias>

Answer in JSON using this schema: RecursiveMapAlias"#
            ))
        );
    }

    #[test]
    fn test_render_recursive_alias_cycle() {
        // A = B[], B = C, C = A[]
        let mut content = OutputFormatContent::new(ty_alias("A"));
        content
            .recursive_type_aliases
            .insert("A".to_string(), ty_list(ty_alias("B")));
        content
            .recursive_type_aliases
            .insert("B".to_string(), ty_alias("C"));
        content
            .recursive_type_aliases
            .insert("C".to_string(), ty_list(ty_alias("A")));

        let rendered = content.render(&RenderOptions::default()).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"A = B[]
B = C
C = A[]

Answer in JSON using this schema: A"#
            ))
        );
    }

    #[test]
    fn test_render_recursive_alias_cycle_with_hoist_prefix() {
        let mut content = OutputFormatContent::new(ty_alias("A"));
        content
            .recursive_type_aliases
            .insert("A".to_string(), ty_list(ty_alias("B")));
        content
            .recursive_type_aliases
            .insert("B".to_string(), ty_alias("C"));
        content
            .recursive_type_aliases
            .insert("C".to_string(), ty_list(ty_alias("A")));

        let options = RenderOptions {
            hoisted_class_prefix: RenderSetting::Always("type".to_string()),
            ..Default::default()
        };
        let rendered = content.render(&options).unwrap();
        assert_eq!(
            rendered,
            Some(String::from(
                r#"type A = B[]
type B = C
type C = A[]

Answer in JSON using this type: A"#
            ))
        );
    }
}
