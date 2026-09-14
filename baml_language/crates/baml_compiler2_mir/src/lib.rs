mod builder;
mod inference_provider;
mod ir;
mod lower;
mod optimize;
pub mod pretty;

pub use baml_type::ResolvedAliases;
pub use ir::*;
pub use lower::{
    RuntimeLowering, def_to_item_ref, function_is_interface_body,
    interface_body_link_bounds_suffix, lower_function, lower_let_body, native_key_for,
    resolved_aliases_for_package, tir2_to_template,
};

/// Database trait for compiler2 MIR queries.
#[salsa::db]
pub trait Db: baml_compiler2_ppir::Db {}
