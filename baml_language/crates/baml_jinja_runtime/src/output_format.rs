//! Output format types for schema representation.
//!
//! These types represent the schema information needed for parsing LLM responses:
//! - `OutputFormatContent` - Container for all type schemas
//! - `Class` - Class/struct schema with fields
//! - `Enum` - Enum schema with variants
//! - `Name` - Name with optional alias

use std::sync::Arc;
use indexmap::{IndexMap, IndexSet};
use baml_types::{Constraint, StreamingMode, TypeIR};

/// A name that may have a different rendered form (alias).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Name {
    /// The actual name in the schema.
    pub name: String,
    /// The rendered name (alias) if different from the actual name.
    pub rendered_name: Option<String>,
}

impl Name {
    /// Create a new name without an alias.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rendered_name: None,
        }
    }

    /// Create a new name with an alias.
    pub fn with_alias(name: impl Into<String>, alias: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            rendered_name: Some(alias.into()),
        }
    }

    /// Get the actual name.
    pub fn real_name(&self) -> &str {
        &self.name
    }

    /// Get the rendered name (alias if set, otherwise the real name).
    pub fn rendered_name(&self) -> &str {
        self.rendered_name.as_deref().unwrap_or(&self.name)
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.rendered_name())
    }
}

/// A variant of an enum.
#[derive(Clone, Debug)]
pub struct EnumVariant {
    /// The variant name.
    pub name: Name,
    /// Optional description.
    pub description: Option<String>,
}

/// An enum schema.
#[derive(Clone, Debug)]
pub struct Enum {
    /// The enum name.
    pub name: Name,
    /// The enum variants.
    pub variants: Vec<EnumVariant>,
    /// Constraints on the enum.
    pub constraints: Vec<Constraint>,
}

impl Enum {
    /// Create a new enum.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Name::new(name),
            variants: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Add a variant to the enum.
    pub fn with_variant(mut self, name: impl Into<String>, description: Option<String>) -> Self {
        self.variants.push(EnumVariant {
            name: Name::new(name),
            description,
        });
        self
    }
}

/// A field of a class.
#[derive(Clone, Debug)]
pub struct ClassField {
    /// The field name.
    pub name: Name,
    /// The field type.
    pub field_type: TypeIR,
    /// Optional description.
    pub description: Option<String>,
    /// Whether the field is required.
    pub required: bool,
}

/// A class schema.
#[derive(Clone, Debug)]
pub struct Class {
    /// The class name.
    pub name: Name,
    /// Optional description.
    pub description: Option<String>,
    /// The streaming mode this class definition is for.
    pub namespace: StreamingMode,
    /// The class fields.
    pub fields: Vec<ClassField>,
    /// Constraints on the class.
    pub constraints: Vec<Constraint>,
}

impl Class {
    /// Create a new class.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Name::new(name),
            description: None,
            namespace: StreamingMode::NonStreaming,
            fields: Vec::new(),
            constraints: Vec::new(),
        }
    }

    /// Set the class description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Add a field to the class.
    pub fn with_field(
        mut self,
        name: impl Into<String>,
        field_type: TypeIR,
        description: Option<String>,
        required: bool,
    ) -> Self {
        self.fields.push(ClassField {
            name: Name::new(name),
            field_type,
            description,
            required,
        });
        self
    }
}

/// Container for all type schemas used in output format.
///
/// This is the primary type used by the JSON-ish parser to understand
/// the expected schema when coercing LLM responses.
#[derive(Clone, Debug)]
pub struct OutputFormatContent {
    /// Enum schemas indexed by name.
    pub enums: Arc<IndexMap<String, Enum>>,
    /// Class schemas indexed by (name, streaming_mode).
    pub classes: Arc<IndexMap<(String, StreamingMode), Class>>,
    /// Names of classes that are recursive.
    pub recursive_classes: Arc<IndexSet<String>>,
    /// Type aliases that are structurally recursive.
    pub structural_recursive_aliases: Arc<IndexMap<String, TypeIR>>,
    /// The target type to parse into.
    pub target: TypeIR,
}

impl OutputFormatContent {
    /// Create an empty OutputFormatContent.
    pub fn empty() -> Self {
        Self {
            enums: Arc::new(IndexMap::new()),
            classes: Arc::new(IndexMap::new()),
            recursive_classes: Arc::new(IndexSet::new()),
            structural_recursive_aliases: Arc::new(IndexMap::new()),
            target: TypeIR::string(),
        }
    }

    /// Find an enum by name.
    pub fn find_enum(&self, name: &str) -> Option<&Enum> {
        self.enums.get(name)
    }

    /// Find a class by name and streaming mode.
    pub fn find_class(&self, name: &str, mode: StreamingMode) -> Option<&Class> {
        self.classes.get(&(name.to_string(), mode))
    }

    /// Find a class by name in non-streaming mode.
    pub fn find_class_non_streaming(&self, name: &str) -> Option<&Class> {
        self.find_class(name, StreamingMode::NonStreaming)
    }
}

/// Builder for OutputFormatContent.
#[derive(Default)]
pub struct OutputFormatBuilder {
    enums: IndexMap<String, Enum>,
    classes: IndexMap<(String, StreamingMode), Class>,
    recursive_classes: IndexSet<String>,
    structural_recursive_aliases: IndexMap<String, TypeIR>,
    target: Option<TypeIR>,
}

impl OutputFormatBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an enum.
    pub fn with_enum(mut self, e: Enum) -> Self {
        self.enums.insert(e.name.real_name().to_string(), e);
        self
    }

    /// Add a class.
    pub fn with_class(mut self, c: Class) -> Self {
        let key = (c.name.real_name().to_string(), c.namespace);
        self.classes.insert(key, c);
        self
    }

    /// Set the target type.
    pub fn with_target(mut self, target: TypeIR) -> Self {
        self.target = Some(target);
        self
    }

    /// Build the OutputFormatContent.
    pub fn build(self) -> OutputFormatContent {
        OutputFormatContent {
            enums: Arc::new(self.enums),
            classes: Arc::new(self.classes),
            recursive_classes: Arc::new(self.recursive_classes),
            structural_recursive_aliases: Arc::new(self.structural_recursive_aliases),
            target: self.target.unwrap_or_else(TypeIR::string),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_without_alias() {
        let name = Name::new("foo");
        assert_eq!(name.real_name(), "foo");
        assert_eq!(name.rendered_name(), "foo");
    }

    #[test]
    fn test_name_with_alias() {
        let name = Name::with_alias("foo", "bar");
        assert_eq!(name.real_name(), "foo");
        assert_eq!(name.rendered_name(), "bar");
    }

    #[test]
    fn test_enum_builder() {
        let e = Enum::new("Status")
            .with_variant("Active", Some("Active status".to_string()))
            .with_variant("Inactive", None);

        assert_eq!(e.variants.len(), 2);
        assert_eq!(e.variants[0].name.real_name(), "Active");
    }

    #[test]
    fn test_class_builder() {
        let c = Class::new("Person")
            .with_description("A person")
            .with_field("name", TypeIR::string(), Some("Name".to_string()), true)
            .with_field("age", TypeIR::int(), None, false);

        assert_eq!(c.fields.len(), 2);
        assert_eq!(c.description, Some("A person".to_string()));
    }

    #[test]
    fn test_output_format_builder() {
        let e = Enum::new("Status").with_variant("Active", None);
        let c = Class::new("Person")
            .with_field("name", TypeIR::string(), None, true);

        let of = OutputFormatBuilder::new()
            .with_enum(e)
            .with_class(c)
            .with_target(TypeIR::class("Person"))
            .build();

        assert!(of.find_enum("Status").is_some());
        assert!(of.find_class_non_streaming("Person").is_some());
    }
}
