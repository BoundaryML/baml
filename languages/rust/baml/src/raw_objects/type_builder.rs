//! TypeBuilder and related types
//!
//! These wrap FFI pointers to type builder objects managed by the BAML runtime.
//! TypeBuilder enables dynamic type construction at runtime.

use std::ffi::c_void;

use crate::baml_unreachable;
use crate::error::BamlError;
use crate::proto::baml_cffi_v1::BamlObjectType;

use super::{define_raw_object_wrapper, RawObject, RawObjectTrait};

// =============================================================================
// TypeDef - A dynamically constructed BAML type
// =============================================================================

define_raw_object_wrapper! {
    /// A dynamically constructed BAML type
    TypeDef => ObjectType
}

impl TypeDef {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract TypeDef handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectType),
        }
    }

    /// Get string representation (never fails)
    pub fn print(&self) -> String {
        self.raw.call_method("__display__", ())
    }

    /// Wrap this type in a list (infallible)
    pub fn list(&self) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("list", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to create list type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Make this type optional (infallible)
    pub fn optional(&self) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("optional", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to create optional type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }
}

// =============================================================================
// EnumValueBuilder - Builder for enum values
// =============================================================================

define_raw_object_wrapper! {
    /// Builder for enum values
    ///
    /// **Note**: All methods are fallible because the underlying enum value could be deleted
    /// while you hold a reference to the builder.
    EnumValueBuilder => ObjectEnumValueBuilder
}

impl EnumValueBuilder {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract EnumValueBuilder handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectEnumValueBuilder),
        }
    }

    /// Get the value name
    pub fn name(&self) -> Result<String, BamlError> {
        self.raw.try_call_method("name", ())
    }

    /// Set description for LLM
    pub fn set_description(&self, description: &str) -> Result<(), BamlError> {
        self.raw
            .try_call_method("set_description", ("description", description))
    }

    /// Get description (if set)
    pub fn description(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("description", ())
    }

    /// Set alias for LLM
    pub fn set_alias(&self, alias: &str) -> Result<(), BamlError> {
        self.raw.try_call_method("set_alias", ("alias", alias))
    }

    /// Get alias (if set)
    pub fn alias(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("alias", ())
    }

    /// Set skip flag
    pub fn set_skip(&self, skip: bool) -> Result<(), BamlError> {
        self.raw.try_call_method("set_skip", ("skip", skip))
    }

    /// Get skip flag
    pub fn skip(&self) -> Result<bool, BamlError> {
        self.raw.try_call_method("skip", ())
    }
}

// =============================================================================
// EnumBuilder - Builder for enum types
// =============================================================================

define_raw_object_wrapper! {
    /// Builder for enum types
    ///
    /// **Note**: All methods are fallible because the underlying enum could be deleted
    /// while you hold a reference to the builder.
    EnumBuilder => ObjectEnumBuilder
}

impl EnumBuilder {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract EnumBuilder handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectEnumBuilder),
        }
    }

    /// Add a value to this enum
    pub fn add_value(&self, value: &str) -> Result<EnumValueBuilder, BamlError> {
        let handle = self
            .raw
            .call_method_for_object("add_value", ("value", value))?;
        Ok(EnumValueBuilder::from_handle(handle, self.raw.runtime()))
    }

    /// Get a value by name (if it exists)
    pub fn get_value(&self, name: &str) -> Result<Option<EnumValueBuilder>, BamlError> {
        match self.raw.call_method_for_object("value", ("name", name)) {
            Ok(handle) => Ok(Some(EnumValueBuilder::from_handle(
                handle,
                self.raw.runtime(),
            ))),
            Err(_) => Ok(None), // Not found
        }
    }

    /// List all values
    pub fn list_values(&self) -> Result<Vec<EnumValueBuilder>, BamlError> {
        let handles = self.raw.call_method_for_objects("list_values", ())?;
        Ok(handles
            .into_iter()
            .map(|h| EnumValueBuilder::from_handle(h, self.raw.runtime()))
            .collect())
    }

    /// Get this enum as a Type
    pub fn as_type(&self) -> Result<TypeDef, BamlError> {
        let handle = self.raw.call_method_for_object("type_", ())?;
        Ok(TypeDef::from_handle(handle, self.raw.runtime()))
    }

    /// Get the enum name
    pub fn name(&self) -> Result<String, BamlError> {
        self.raw.try_call_method("name", ())
    }

    /// Set description for LLM
    pub fn set_description(&self, description: &str) -> Result<(), BamlError> {
        self.raw
            .try_call_method("set_description", ("description", description))
    }

    /// Get description (if set)
    pub fn description(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("description", ())
    }

    /// Set alias for LLM
    pub fn set_alias(&self, alias: &str) -> Result<(), BamlError> {
        self.raw.try_call_method("set_alias", ("alias", alias))
    }

    /// Get alias (if set)
    pub fn alias(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("alias", ())
    }
}

// =============================================================================
// ClassPropertyBuilder - Builder for class properties
// =============================================================================

define_raw_object_wrapper! {
    /// Builder for class properties
    ///
    /// **Note**: All methods are fallible because the underlying property could be deleted
    /// while you hold a reference to the builder.
    ClassPropertyBuilder => ObjectClassPropertyBuilder
}

impl ClassPropertyBuilder {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle).unwrap_or_else(|e| {
            baml_unreachable!("Failed to extract ClassPropertyBuilder handle: {e}")
        });
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectClassPropertyBuilder),
        }
    }

    /// Get the property name
    pub fn name(&self) -> Result<String, BamlError> {
        self.raw.try_call_method("name", ())
    }

    /// Set the property type
    pub fn set_type(&self, field_type: &TypeDef) -> Result<(), BamlError> {
        self.raw
            .try_call_method("set_type", ("field_type", field_type))
    }

    /// Get the property type
    pub fn get_type(&self) -> Result<TypeDef, BamlError> {
        let handle = self.raw.call_method_for_object("type_", ())?;
        Ok(TypeDef::from_handle(handle, self.raw.runtime()))
    }

    /// Set description for LLM
    pub fn set_description(&self, description: &str) -> Result<(), BamlError> {
        self.raw
            .try_call_method("set_description", ("description", description))
    }

    /// Get description (if set)
    pub fn description(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("description", ())
    }

    /// Set alias for LLM
    pub fn set_alias(&self, alias: &str) -> Result<(), BamlError> {
        self.raw.try_call_method("set_alias", ("alias", alias))
    }

    /// Get alias (if set)
    pub fn alias(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("alias", ())
    }
}

// =============================================================================
// ClassBuilder - Builder for class types
// =============================================================================

define_raw_object_wrapper! {
    /// Builder for class types
    ///
    /// **Note**: All methods are fallible because the underlying class could be deleted
    /// while you hold a reference to the builder.
    ClassBuilder => ObjectClassBuilder
}

impl ClassBuilder {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract ClassBuilder handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectClassBuilder),
        }
    }

    /// Add a property to this class
    pub fn add_property(
        &self,
        name: &str,
        field_type: &TypeDef,
    ) -> Result<ClassPropertyBuilder, BamlError> {
        let handle = self
            .raw
            .call_method_for_object("add_property", (("name", name), ("field_type", field_type)))?;
        Ok(ClassPropertyBuilder::from_handle(
            handle,
            self.raw.runtime(),
        ))
    }

    /// Get a property by name (if it exists)
    pub fn get_property(&self, name: &str) -> Result<Option<ClassPropertyBuilder>, BamlError> {
        match self
            .raw
            .call_method_for_object("property", ("name", name))
        {
            Ok(handle) => Ok(Some(ClassPropertyBuilder::from_handle(
                handle,
                self.raw.runtime(),
            ))),
            Err(_) => Ok(None), // Not found
        }
    }

    /// List all properties
    pub fn list_properties(&self) -> Result<Vec<ClassPropertyBuilder>, BamlError> {
        let handles = self.raw.call_method_for_objects("list_properties", ())?;
        Ok(handles
            .into_iter()
            .map(|h| ClassPropertyBuilder::from_handle(h, self.raw.runtime()))
            .collect())
    }

    /// Get this class as a Type
    pub fn as_type(&self) -> Result<TypeDef, BamlError> {
        let handle = self.raw.call_method_for_object("type_", ())?;
        Ok(TypeDef::from_handle(handle, self.raw.runtime()))
    }

    /// Get the class name
    pub fn name(&self) -> Result<String, BamlError> {
        self.raw.try_call_method("name", ())
    }

    /// Set description for LLM
    pub fn set_description(&self, description: &str) -> Result<(), BamlError> {
        self.raw
            .try_call_method("set_description", ("description", description))
    }

    /// Get description (if set)
    pub fn description(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("description", ())
    }

    /// Set alias for LLM
    pub fn set_alias(&self, alias: &str) -> Result<(), BamlError> {
        self.raw.try_call_method("set_alias", ("alias", alias))
    }

    /// Get alias (if set)
    pub fn alias(&self) -> Result<Option<String>, BamlError> {
        self.raw.try_call_method("alias", ())
    }
}

// =============================================================================
// TypeBuilder - Builder for constructing BAML types at runtime
// =============================================================================

define_raw_object_wrapper! {
    /// Builder for constructing BAML types at runtime
    TypeBuilder => ObjectTypeBuilder
}

impl TypeBuilder {
    /// Create a new TypeBuilder (infallible)
    pub fn new(runtime: *const c_void) -> Self {
        let raw = RawObject::new(runtime, BamlObjectType::ObjectTypeBuilder, ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to create TypeBuilder: {}", e));
        Self { raw }
    }

    // =========================================================================
    // Primitive types (infallible)
    // =========================================================================

    /// Get string type
    pub fn string(&self) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("string", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get string type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get int type
    pub fn int(&self) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("int", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get int type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get float type
    pub fn float(&self) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("float", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get float type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get bool type
    pub fn bool(&self) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("bool", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get bool type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get null type
    pub fn null(&self) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("null", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get null type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    // =========================================================================
    // Literal types (infallible)
    // =========================================================================

    /// Get a literal string type
    pub fn literal_string(&self, value: &str) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("literal_string", ("value", value))
            .unwrap_or_else(|e| baml_unreachable!("Failed to get literal string type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get a literal int type
    pub fn literal_int(&self, value: i64) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("literal_int", ("value", value))
            .unwrap_or_else(|e| baml_unreachable!("Failed to get literal int type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get a literal bool type
    pub fn literal_bool(&self, value: bool) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("literal_bool", ("value", value))
            .unwrap_or_else(|e| baml_unreachable!("Failed to get literal bool type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    // =========================================================================
    // Composite types (infallible)
    // =========================================================================

    /// Get a list type containing the given inner type
    pub fn list(&self, inner: &TypeDef) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("list", ("inner", inner))
            .unwrap_or_else(|e| baml_unreachable!("Failed to get list type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get an optional type containing the given inner type
    pub fn optional(&self, inner: &TypeDef) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("optional", ("inner", inner))
            .unwrap_or_else(|e| baml_unreachable!("Failed to get optional type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get a map type with the given key and value types
    pub fn map(&self, key: &TypeDef, value: &TypeDef) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("map", (("key", key), ("value", value)))
            .unwrap_or_else(|e| baml_unreachable!("Failed to get map type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    /// Get a union type of the given types
    pub fn union(&self, types: &[&TypeDef]) -> TypeDef {
        let handle = self
            .raw
            .call_method_for_object("union", ("types", types))
            .unwrap_or_else(|e| baml_unreachable!("Failed to get union type: {}", e));
        TypeDef::from_handle(handle, self.raw.runtime())
    }

    // =========================================================================
    // Schema operations (fallible - BAML parsing can fail)
    // =========================================================================

    /// Parse and add BAML schema definitions
    pub fn add_baml(&self, baml: &str) -> Result<(), BamlError> {
        self.raw.try_call_method("add_baml", ("baml", baml))
    }

    /// Get string representation (never fails)
    pub fn print(&self) -> String {
        self.raw.call_method("__display__", ())
    }

    // =========================================================================
    // Enum operations
    // =========================================================================

    /// Add a new enum (fallible - name conflicts, invalid names)
    pub fn add_enum(&self, name: &str) -> Result<EnumBuilder, BamlError> {
        let handle = self
            .raw
            .call_method_for_object("add_enum", ("name", name))?;
        Ok(EnumBuilder::from_handle(handle, self.raw.runtime()))
    }

    /// Get an enum by name (nullable - may not exist)
    pub fn get_enum(&self, name: &str) -> Option<EnumBuilder> {
        let handle = self
            .raw
            .call_method_for_object("enum_", ("name", name))
            .ok()?;
        Some(EnumBuilder::from_handle(handle, self.raw.runtime()))
    }

    /// List all enums (infallible)
    pub fn list_enums(&self) -> Vec<EnumBuilder> {
        let handles = self
            .raw
            .call_method_for_objects("list_enums", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to list enums: {}", e));
        handles
            .into_iter()
            .map(|h| EnumBuilder::from_handle(h, self.raw.runtime()))
            .collect()
    }

    // =========================================================================
    // Class operations
    // =========================================================================

    /// Add a new class (fallible - name conflicts, invalid names)
    pub fn add_class(&self, name: &str) -> Result<ClassBuilder, BamlError> {
        let handle = self
            .raw
            .call_method_for_object("add_class", ("name", name))?;
        Ok(ClassBuilder::from_handle(handle, self.raw.runtime()))
    }

    /// Get a class by name (nullable - may not exist)
    pub fn get_class(&self, name: &str) -> Option<ClassBuilder> {
        let handle = self
            .raw
            .call_method_for_object("class", ("name", name))
            .ok()?;
        Some(ClassBuilder::from_handle(handle, self.raw.runtime()))
    }

    /// List all classes (infallible)
    pub fn list_classes(&self) -> Vec<ClassBuilder> {
        let handles = self
            .raw
            .call_method_for_objects("list_classes", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to list classes: {}", e));
        handles
            .into_iter()
            .map(|h| ClassBuilder::from_handle(h, self.raw.runtime()))
            .collect()
    }
}
