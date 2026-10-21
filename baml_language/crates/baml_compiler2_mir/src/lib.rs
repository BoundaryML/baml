mod builder;
mod inference_provider;
mod ir;
mod lower;
pub mod memory;
mod optimize;
pub mod pretty;

pub use baml_type::ResolvedAliases;
pub use ir::*;
pub use lower::{
    LoweredLetBody, RuntimeLowering, class_display_name, definition_display_name,
    enum_display_name, function_display_name, function_is_interface_body, global_display_name,
    interface_body_link_bounds_suffix, lower_function, lower_let_body, native_key_for,
    resolved_aliases_for_package, tir2_to_template,
};

/// Database trait for compiler2 MIR queries.
#[salsa::db]
pub trait Db: baml_compiler2_hir::Db {}
