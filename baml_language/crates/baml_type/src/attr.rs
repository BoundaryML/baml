//! SAP types re-exported from `baml_base`.
//!
//! The canonical definitions live in `baml_base::attr` so that both
//! `baml_type` and `baml_compiler_tir` can use them without a cyclic dependency.

pub use baml_base::{FieldAttr, SapAttrValue, SapConstValue, TyAttr};
