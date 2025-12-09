//! Embedded default GEPA BAML files
//!
//! These files are written to .baml_optimize/gepa/baml_src/ on first run
//! and can be customized by users.

/// The default GEPA functions for reflection and proposal
pub const GEPA_BAML: &str = include_str!("gepa.baml");

/// The default client configuration for the reflection model
pub const CLIENTS_BAML: &str = include_str!("clients.baml");

/// Current version of the GEPA implementation
pub const GEPA_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Compute a hash of the default GEPA files for change detection
pub fn default_gepa_hash() -> String {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };

    let mut hasher = DefaultHasher::new();
    GEPA_BAML.hash(&mut hasher);
    CLIENTS_BAML.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
