//! Schema items for VIR.
//!
//! VIR schema captures classes, enums, functions, and type aliases with resolved
//! types and propagated HIR attributes. Types are re-exported from `baml_type::defs`.

pub use baml_type::{
    ClassDef as VirClass, EnumDef as VirEnum, EnumVariantDef as VirEnumVariant,
    FieldDef as VirField, FunctionBodyKind as VirFunctionBodyKind, FunctionDef as VirFunction,
    ParamDef as VirParam, SchemaDefs as VirSchema, TypeAliasDef as VirTypeAlias,
};
