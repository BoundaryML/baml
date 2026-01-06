//! Dynamic fallback types for unknown classes, enums, and unions.

use std::collections::HashMap;

use super::{
    baml_value::BamlValue, from_baml_value::FromBamlValue, from_baml_value_ref::FromBamlValueRef,
    known_types::KnownTypes,
};
use crate::error::{BamlError, FullTypeName};

/// A fully dynamic class - all fields accessed via .`get()`
#[derive(Debug, Clone)]
pub struct DynamicClass<T: KnownTypes, S: KnownTypes> {
    pub name: String,
    pub(crate) fields: HashMap<String, BamlValue<T, S>>,
}

impl<T: KnownTypes, S: KnownTypes> DynamicClass<T, S> {
    /// Create a new `DynamicClass`
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

    /// Get the class name (e.g., "`PersonInfo`", "`OrderDetails`").
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get a field and convert it to the specified type.
    /// Clones the value.
    pub fn get<V: FromBamlValue<T, S>>(&self, field_name: &str) -> Result<V, BamlError> {
        let value = self
            .fields
            .get(field_name)
            .ok_or_else(|| BamlError::internal(format!("missing field '{field_name}'")))?
            .clone();
        V::from_baml_value(value)
    }

    /// Get a field by reference (zero-copy for primitives and known types).
    pub fn get_ref<'a, V: FromBamlValueRef<'a, T, S>>(
        &'a self,
        field_name: &str,
    ) -> Result<V, BamlError> {
        let value = self
            .fields
            .get(field_name)
            .ok_or_else(|| BamlError::internal(format!("missing field '{field_name}'")))?;
        V::from_baml_value_ref(value)
    }

    /// Remove a field and convert it (takes ownership, no clone).
    pub fn pop<V: FromBamlValue<T, S>>(&mut self, field_name: &str) -> Result<V, BamlError> {
        let value = self
            .fields
            .remove(field_name)
            .ok_or_else(|| BamlError::internal(format!("missing field '{field_name}'")))?;
        V::from_baml_value(value)
    }

    /// Consume this `DynamicClass` and return the remaining fields.
    /// Useful for `@@dynamic` classes: pop known fields first, then call this
    /// to get remaining dynamic fields.
    pub fn into_fields(self) -> HashMap<String, BamlValue<T, S>> {
        self.fields
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
    pub name: String,                // Union type name (e.g., "FooOrBar")
    pub variant_name: String,        // Which variant matched (e.g., "Foo")
    pub value: Box<BamlValue<T, S>>, // The actual value
}

impl<T: KnownTypes, S: KnownTypes> DynamicUnion<T, S> {
    /// Get the union name (e.g., "`FooOrBar`", "`ResultOrError`").
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

// =============================================================================
// BamlTypeName trait implementations for dynamic types
// =============================================================================

use crate::error::BamlTypeName;

impl<T: KnownTypes, S: KnownTypes> BamlTypeName for DynamicClass<T, S> {
    const BASE_TYPE_NAME: &'static str = "DynamicClass";
}

impl BamlTypeName for DynamicEnum {
    const BASE_TYPE_NAME: &'static str = "DynamicEnum";
}

impl<T: KnownTypes, S: KnownTypes> BamlTypeName for DynamicUnion<T, S> {
    const BASE_TYPE_NAME: &'static str = "DynamicUnion";
}
