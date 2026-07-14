//! `baml_rs` — the runtime layer generated Rust SDKs link against.
//!
//! A generated `baml_sdk` crate embeds compiled BAML bytecode, boots the
//! process-global BEX engine lazily on first call, and converts values
//! across the boundary with the [`BamlValue`] trait. This crate carries
//! everything the generated code composes: the conversion traits and
//! provided impls ([`baml_value`]), the inbound wire builders
//! ([`encode`]), the result-envelope decoding ([`decode`]), the typed
//! error surface ([`error`]), and the call machinery ([`runtime`]).

pub mod baml_value;
pub mod decode;
pub mod encode;
pub mod error;
pub mod runtime;

pub use baml_value::{BamlValue, OptionalArg};
pub use error::{DecodeError, Error, SdkError};
// reexports
pub use indexmap::IndexMap;
pub use num_bigint::BigInt;

/// Order-preserving map type of BAML `map` values (the engine's own
/// representation is insertion-ordered).
pub type Map<K, V> = indexmap::IndexMap<K, V>;

/// The protobuf wire types generated code touches. An implementation
/// detail of the generated-SDK ↔ runtime boundary, not a public API.
#[doc(hidden)]
pub mod wire {
    pub use bridge_ctypes::baml_bridge::cffi::*;
}

pub fn get_version() -> &'static str {
    baml_version::CANONICAL_VERSION
}
