use std::fmt::Write as _;

use proc_macro2::{Ident, Span};

/// Prefix reserved for BAML field names that cannot be represented by a Rust
/// identifier, even as a raw identifier.
///
/// Names that already start with this prefix are encoded too, which keeps the
/// mapping injective: a user field can never collide with an escaped field.
const ESCAPED_FIELD_PREFIX: &str = "__baml_field_";

/// Map a BAML field name to a legal, collision-free Rust identifier.
///
/// Ordinary names are preserved for readable generated APIs. Rust keywords
/// that support raw identifiers use `r#name`. The few names Rust forbids even
/// in raw form (`self`, `Self`, `super`, `crate`, and `_`), plus BAML names
/// containing Rust-illegal punctuation, use a reversible hex encoding.
pub(crate) fn rust_field_ident(name: &str) -> Ident {
    if !name.starts_with(ESCAPED_FIELD_PREFIX) {
        if let Ok(ident) = syn::parse_str::<Ident>(name) {
            return ident;
        }
        if let Ok(ident) = syn::parse_str::<Ident>(&format!("r#{name}")) {
            return ident;
        }
    }

    let encoded = encoded_ident(ESCAPED_FIELD_PREFIX, name);
    Ident::new(&encoded, Span::call_site())
}

/// Internal local used while converting a generated field back to a VM value.
/// Always encode these bindings: they are implementation details, so readability
/// is less important than avoiding another identifier-concatenation edge case.
pub(crate) fn rust_field_value_ident(name: &str) -> Ident {
    Ident::new(
        &encoded_ident("__baml_field_value_", name),
        Span::call_site(),
    )
}

fn encoded_ident(prefix: &str, name: &str) -> String {
    let mut encoded = String::with_capacity(prefix.len() + name.len() * 2);
    encoded.push_str(prefix);
    for byte in name.as_bytes() {
        write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use proc_macro2::Ident;

    use super::{rust_field_ident, rust_field_value_ident};

    #[test]
    fn preserves_plain_names_and_uses_raw_rust_keywords() {
        assert_eq!(rust_field_ident("ordinary").to_string(), "ordinary");
        for keyword in ["type", "match", "move"] {
            assert_eq!(
                rust_field_ident(keyword).to_string(),
                format!("r#{keyword}")
            );
        }
    }

    #[test]
    fn escaping_is_legal_and_collision_free() {
        let names = [
            "self",
            "Self",
            "super",
            "crate",
            "_",
            "dash-name",
            "$data",
            "__baml_field_73656c66",
            "self_",
        ];
        let idents = names
            .iter()
            .map(|name| rust_field_ident(name).to_string())
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(idents.len(), names.len());
        assert!(
            idents
                .iter()
                .all(|ident| syn::parse_str::<Ident>(ident).is_ok())
        );
        assert_eq!(rust_field_ident("self_").to_string(), "self_");
        assert_ne!(
            rust_field_ident("self").to_string(),
            rust_field_ident("__baml_field_73656c66").to_string()
        );
        assert_ne!(
            rust_field_value_ident("dash-name"),
            rust_field_value_ident("dash_name")
        );
    }
}
