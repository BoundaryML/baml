//! Identifier hygiene: BAML names → Rust identifiers.
//!
//! BAML identifiers can collide with Rust keywords. Most keywords are
//! representable as raw identifiers (`r#type`); the handful that are not
//! (`crate`, `self`, `Self`, `super`, plus the non-identifier `_`) get a
//! trailing underscore. The *on-disk* module path segment never carries
//! the `r#` prefix — `mod r#type;` resolves to the directory `type/`.

use proc_macro2::{Ident, Span};

/// The BAML name rendered as a Rust identifier.
///
/// Panics on names that are not lexically identifiers (e.g. `$`-suffixed
/// companions): those must be filtered or mangled upstream, so reaching
/// here with one is a generator bug, never an input-dependent condition.
pub(crate) fn ident(name: &str) -> Ident {
    // A Rust identifier starts with a letter or `_` and continues with
    // letters, digits, or `_`. The leading-character check also rejects the
    // empty string (`next()` is `None`) and digit-leading names like
    // `3d_shape`, which would otherwise fall through to a cryptic
    // proc_macro2/syn panic instead of this generator-bug message.
    assert!(
        name.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
            && name.chars().all(|c| c.is_alphanumeric() || c == '_'),
        "generator bug: `{name}` is not representable as a Rust identifier; \
         symbols with such names must be filtered or mangled before emission"
    );
    if let Some(renamed) = non_raw_able(name) {
        Ident::new(renamed, Span::call_site())
    } else if syn::parse_str::<Ident>(name).is_ok() {
        Ident::new(name, Span::call_site())
    } else {
        // A keyword: lexically a valid identifier, but syn rejects it in
        // identifier position — exactly the raw-identifier cases.
        Ident::new_raw(name, Span::call_site())
    }
}

/// The BAML name as an on-disk module path segment (no `r#` prefix; the
/// underscore renames do appear, since they change the module name).
pub(crate) fn dir_segment(name: &str) -> String {
    match non_raw_able(name) {
        Some(renamed) => renamed.to_string(),
        None => name.to_string(),
    }
}

/// Keywords that cannot be raw identifiers, mapped to their trailing
/// underscore rename.
fn non_raw_able(name: &str) -> Option<&'static str> {
    match name {
        "crate" => Some("crate_"),
        "self" => Some("self_"),
        "Self" => Some("Self_"),
        "super" => Some("super_"),
        "_" => Some("_1"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_pass_through() {
        assert_eq!(ident("hello_world").to_string(), "hello_world");
        assert_eq!(dir_segment("hello_world"), "hello_world");
    }

    #[test]
    fn keywords_become_raw_idents_but_plain_dir_segments() {
        assert_eq!(ident("type").to_string(), "r#type");
        assert_eq!(ident("fn").to_string(), "r#fn");
        assert_eq!(dir_segment("type"), "type");
    }

    #[test]
    fn non_raw_able_keywords_get_trailing_underscore() {
        assert_eq!(ident("self").to_string(), "self_");
        assert_eq!(ident("crate").to_string(), "crate_");
        assert_eq!(ident("Self").to_string(), "Self_");
        assert_eq!(ident("super").to_string(), "super_");
        assert_eq!(dir_segment("self"), "self_");
    }

    #[test]
    #[should_panic(expected = "not representable as a Rust identifier")]
    fn digit_leading_name_is_a_generator_bug() {
        // Not a valid Rust identifier — must fail with the generator-bug
        // message rather than fall through to a proc_macro2/syn panic.
        let _ = ident("3d_shape");
    }
}
