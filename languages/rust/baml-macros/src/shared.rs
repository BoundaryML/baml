//! Shared utilities for parsing `#[baml(...)]` attributes.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Result};

/// Returns the path to the baml crate, handling the case where we're
/// being used inside the baml crate itself.
pub fn baml_crate_path() -> TokenStream {
    // Check if we're being used inside the baml crate's lib target.
    // CARGO_CRATE_NAME is "baml" only for the lib target, not for test targets
    // (test targets have names like "codec", "derive", "ffi", etc.)
    if std::env::var("CARGO_CRATE_NAME").as_deref() == Ok("baml") {
        quote!(crate)
    } else {
        quote!(::baml)
    }
}

/// Container-level attributes (on struct/enum)
#[derive(Default)]
pub struct ContainerAttrs {
    /// The BAML type name (defaults to Rust type name)
    pub name: Option<String>,
}

/// Field-level attributes
#[derive(Default)]
pub struct FieldAttrs {
    /// The BAML field name (defaults to Rust field name)
    pub name: Option<String>,
    /// Whether to skip this field during encoding
    pub skip: bool,
}

/// Variant-level attributes (for enums)
#[derive(Default)]
pub struct VariantAttrs {
    /// The BAML variant name (defaults to Rust variant name)
    pub name: Option<String>,
}

impl ContainerAttrs {
    /// Parse container attributes from a list of attributes
    pub fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("baml") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value: LitStr = meta.value()?.parse()?;
                    result.name = Some(value.value());
                    Ok(())
                } else {
                    Err(meta.error("unrecognized baml attribute"))
                }
            })?;
        }

        Ok(result)
    }
}

impl FieldAttrs {
    /// Parse field attributes from a list of attributes
    pub fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("baml") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value: LitStr = meta.value()?.parse()?;
                    result.name = Some(value.value());
                    Ok(())
                } else if meta.path.is_ident("skip") {
                    result.skip = true;
                    Ok(())
                } else {
                    Err(meta.error("unrecognized baml attribute"))
                }
            })?;
        }

        Ok(result)
    }
}

impl VariantAttrs {
    /// Parse variant attributes from a list of attributes
    pub fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
        let mut result = Self::default();

        for attr in attrs {
            if !attr.path().is_ident("baml") {
                continue;
            }

            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("name") {
                    let value: LitStr = meta.value()?.parse()?;
                    result.name = Some(value.value());
                    Ok(())
                } else {
                    Err(meta.error("unrecognized baml attribute"))
                }
            })?;
        }

        Ok(result)
    }
}

use syn::LitStr;
