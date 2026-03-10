//! SAP types re-exported from `baml_base`.
//!
//! The canonical definitions live in `baml_base::attr` so that both
//! `baml_type` and `baml_compiler_tir` can use them without a cyclic dependency.
//!
//! `TyAttr` and `FieldAttr` are type aliases that fix the generic name
//! parameter to `TypeName`, matching the rest of `baml_type::Ty`.

pub use baml_base::{SapAttrValue, SapConstValue};
pub type TyAttr = baml_base::TyAttr<super::TypeName>;
pub type FieldAttr = baml_base::FieldAttr<super::TypeName>;
