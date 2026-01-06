//! Shared utilities for parsing `#[baml(...)]` attributes.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, LitBool, LitInt, Result};

/// Returns the path to the baml crate, handling the case where we're
/// being used inside the baml crate itself.
pub(crate) fn baml_crate_path() -> TokenStream {
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
pub(crate) struct ContainerAttrs {
    /// The BAML type name (defaults to Rust type name)
    pub name: Option<String>,
    /// Whether this enum represents a BAML union type
    pub union: bool,
    /// Whether this type supports dynamic fields/variants
    pub dynamic: bool,
}

/// Field-level attributes
#[derive(Default)]
pub(crate) struct FieldAttrs {
    /// The BAML field name (defaults to Rust field name)
    pub name: Option<String>,
    /// Whether to skip this field during encoding
    pub skip: bool,
    /// Whether this field holds dynamic properties (`HashMap`<String,
    /// `BamlValue`>)
    pub dynamic_fields: bool,
}

/// Variant-level attributes (for enums)
#[derive(Default)]
pub(crate) struct VariantAttrs {
    /// The BAML variant name (defaults to Rust variant name)
    pub name: Option<String>,
    /// Whether this variant is the dynamic catch-all
    pub dynamic_variant: bool,
    /// If the variant is a literal
    pub literal_string: Option<String>,
    /// If the variant is a literal int
    pub literal_int: Option<i64>,
    /// If the variant is a literal bool
    pub literal_bool: Option<bool>,
}

impl ContainerAttrs {
    /// Parse container attributes from a list of attributes
    pub(crate) fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
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
                } else if meta.path.is_ident("union") {
                    result.union = true;
                    Ok(())
                } else if meta.path.is_ident("dynamic") {
                    result.dynamic = true;
                    Ok(())
                } else {
                    Err(meta.error("unrecognized baml attribute on container"))
                }
            })?;
        }

        Ok(result)
    }
}

impl FieldAttrs {
    /// Parse field attributes from a list of attributes
    pub(crate) fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
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
                } else if meta.path.is_ident("dynamic_fields") {
                    result.dynamic_fields = true;
                    Ok(())
                } else {
                    Err(meta.error("unrecognized baml attribute on field"))
                }
            })?;
        }

        Ok(result)
    }
}

impl VariantAttrs {
    pub(crate) fn is_literal(&self) -> bool {
        self.literal_string.is_some() || self.literal_int.is_some() || self.literal_bool.is_some()
    }

    /// Parse variant attributes from a list of attributes
    pub(crate) fn from_attrs(attrs: &[Attribute]) -> Result<Self> {
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
                } else if meta.path.is_ident("dynamic_variant") {
                    result.dynamic_variant = true;
                    Ok(())
                } else if meta.path.is_ident("literal_string") {
                    let value: LitStr = meta.value()?.parse()?;
                    result.literal_string = Some(value.value());
                    Ok(())
                } else if meta.path.is_ident("literal_int") {
                    let value: LitInt = meta.value()?.parse()?;
                    result.literal_int = Some(value.base10_parse()?);
                    Ok(())
                } else if meta.path.is_ident("literal_bool") {
                    let value: LitBool = meta.value()?.parse()?;
                    result.literal_bool = Some(value.value());
                    Ok(())
                } else {
                    Err(meta.error("unrecognized baml attribute on enum variant"))
                }
            })?;
        }

        Ok(result)
    }
}

use syn::LitStr;
