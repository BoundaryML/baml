//! Dynamic fallback types for unknown classes, enums, and unions.

use std::collections::HashMap;

use super::baml_value::BamlValue;
use super::known_types::KnownTypes;
use crate::error::FullTypeName;

/// A fully dynamic class - all fields accessed via .get()
#[derive(Debug, Clone)]
pub struct DynamicClass<T: KnownTypes, S: KnownTypes> {
    pub name: String,
    pub(crate) fields: HashMap<String, BamlValue<T, S>>,
}

impl<T: KnownTypes, S: KnownTypes> DynamicClass<T, S> {
    /// Create a new DynamicClass
    pub fn new(name: String) -> Self {
        Self {
            name,
            fields: HashMap::new(),
        }
    }

    /// Create with pre-populated fields
    pub fn with_fields(name: String, fields: HashMap<String, BamlValue<T, S>>) -> Self {
        Self { name, fields }
    }

    /// Iterate over all fields
    pub fn fields(&self) -> impl Iterator<Item = (&str, &BamlValue<T, S>)> {
        self.fields.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Check if a field exists
    pub fn has_field(&self, field_name: &str) -> bool {
        self.fields.contains_key(field_name)
    }

    /// Get the class name (e.g., "PersonInfo", "OrderDetails").
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A dynamic enum - name and value as strings
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynamicEnum {
    pub name: String,
    pub value: String,
}

impl DynamicEnum {
    /// Get the enum name (e.g., "Sentiment", "Status").
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// A dynamic union - wraps a value with union metadata
#[derive(Debug, Clone)]
pub struct DynamicUnion<T: KnownTypes, S: KnownTypes> {
    pub name: String,             // Union type name (e.g., "FooOrBar")
    pub variant_name: String,     // Which variant matched (e.g., "Foo")
    pub value: Box<BamlValue<T, S>>, // The actual value
}

impl<T: KnownTypes, S: KnownTypes> DynamicUnion<T, S> {
    /// Get the union name (e.g., "FooOrBar", "ResultOrError").
    pub fn name(&self) -> &str {
        &self.name
    }
}

// =============================================================================
// FullTypeName trait implementations for dynamic types
// =============================================================================

impl<T: KnownTypes, S: KnownTypes> FullTypeName for DynamicClass<T, S> {
    fn full_type_name(&self) -> String {
        format!("DynamicClass({})", self.name)
    }
}

impl FullTypeName for DynamicEnum {
    fn full_type_name(&self) -> String {
        format!("DynamicEnum({})", self.name)
    }
}

impl<T: KnownTypes, S: KnownTypes> FullTypeName for DynamicUnion<T, S> {
    fn full_type_name(&self) -> String {
        format!("DynamicUnion({})", self.name)
    }
}
