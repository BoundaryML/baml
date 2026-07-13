// reexports
pub use indexmap::IndexMap;
// pub use num_bigint::BitInt;

pub mod baml_value;
pub mod runtime;

pub use baml_value::BamlValue;

pub fn get_version() -> &'static str {
    baml_version::CANONICAL_VERSION
}
