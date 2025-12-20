//! TypeBuilder and related types
//!
//! These wrap FFI pointers to type builder objects managed by the BAML runtime.
//! Full implementation in Phase 13.

use crate::proto::baml_cffi_v1::BamlObjectType;

use super::{define_raw_object_wrapper, RawObject, RawObjectTrait};

define_raw_object_wrapper! {
    /// Builder for creating dynamic types at runtime
    TypeBuilder => ObjectTypeBuilder
}

define_raw_object_wrapper! {
    /// A type definition handle
    TypeDef => ObjectType
}

define_raw_object_wrapper! {
    /// Builder for creating enum types
    EnumBuilder => ObjectEnumBuilder
}

define_raw_object_wrapper! {
    /// Builder for enum values
    EnumValueBuilder => ObjectEnumValueBuilder
}

define_raw_object_wrapper! {
    /// Builder for creating class types
    ClassBuilder => ObjectClassBuilder
}

define_raw_object_wrapper! {
    /// Builder for class properties
    ClassPropertyBuilder => ObjectClassPropertyBuilder
}
