/// Implemented by types that can be converted to and from BAML
/// across the FFI boundary.
///
/// Should only be implemented in the generated SDK and `baml_rs`.
pub trait BamlValue: internal::__BamlValuePrivate {
    // TODO
}

#[doc(hidden)]
pub mod internal {
    /// Public only to allow generated SDK to reference it.
    /// Do NOT implement it yourself.
    pub trait __BamlValuePrivate {}
}
