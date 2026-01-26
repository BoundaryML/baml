//! Derive macros for BAML types.
//!
//! This crate provides `#[derive(BamlEncode)]` and `#[derive(BamlDecode)]`
//! macros for automatically implementing serialization to/from BAML's protobuf
//! format.
//!
//! # Usage
//!
//! ```ignore
//! use baml::{BamlEncode, BamlDecode};
//!
//! #[derive(BamlEncode, BamlDecode)]
//! #[baml(name = "Person")]  // Optional: specify BAML type name if different from Rust name
//! struct Person {
//!     name: String,
//!     #[baml(name = "years_old")]  // Optional: rename field
//!     age: i64,
//!     #[baml(skip)]  // Optional: skip field (only for encode)
//!     internal_id: String,
//! }
//!
//! #[derive(BamlEncode, BamlDecode)]
//! #[baml(name = "Color")]
//! enum Color {
//!     Red,
//!     #[baml(name = "GREEN")]  // Optional: rename variant
//!     Green,
//!     Blue,
//! }
//! ```

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

mod decode;
mod encode;
mod shared;

/// Derive macro for `BamlEncode` trait.
///
/// Generates code to encode Rust structs/enums to BAML's `HostValue` format.
///
/// # Attributes
///
/// - `#[baml(name = "...")]` on the type: Specify the BAML type name
/// - `#[baml(name = "...")]` on a field/variant: Specify the BAML field/variant
///   name
/// - `#[baml(skip)]` on a field: Skip this field during encoding
///
/// # Example
///
/// ```ignore
/// #[derive(BamlEncode)]
/// #[baml(name = "UserInfo")]
/// struct User {
///     name: String,
///     #[baml(name = "email_address")]
///     email: String,
/// }
/// ```
#[proc_macro_derive(BamlEncode, attributes(baml))]
pub fn derive_baml_encode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    encode::derive_encode(&input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}

/// Derive macro for `BamlDecode` trait.
///
/// Generates code to decode BAML's `CffiValueHolder` format to Rust
/// structs/enums.
///
/// # Attributes
///
/// - `#[baml(name = "...")]` on the type: Specify the BAML type name
/// - `#[baml(name = "...")]` on a field/variant: Specify the BAML field/variant
///   name
///
/// # Example
///
/// ```ignore
/// #[derive(BamlDecode)]
/// #[baml(name = "UserInfo")]
/// struct User {
///     name: String,
///     #[baml(name = "email_address")]
///     email: Option<String>,
/// }
/// ```
#[proc_macro_derive(BamlDecode, attributes(baml))]
pub fn derive_baml_decode(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    decode::derive_decode(&input)
        .unwrap_or_else(|e| e.to_compile_error())
        .into()
}
