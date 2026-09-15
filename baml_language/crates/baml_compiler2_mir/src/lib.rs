mod builder;
mod inference_provider;
mod ir;
mod lower;
mod optimize;
pub mod pretty;

pub use baml_type::ResolvedAliases;
pub use ir::*;
pub use lower::{
    RuntimeLowering, definition_link_name, enum_link_name, function_is_interface_body,
    function_link_name, interface_body_link_bounds_suffix, lower_function, lower_let_body,
    native_key_for, resolved_aliases_for_package, tir2_to_template,
};

/// Database trait for compiler2 MIR queries.
#[salsa::db]
pub trait Db: baml_compiler2_ppir::Db {}
