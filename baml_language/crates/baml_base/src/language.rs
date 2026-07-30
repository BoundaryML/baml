//! Compiler-owned registry for built-in language constructs.
//!
//! This module contains semantic metadata shared by parsing, validation, and
//! editor tooling. Human-readable documentation lives in `baml_builtins2` and
//! is keyed by the stable names exposed here.

/// The argument shape accepted by a built-in schema attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaAttributeArguments {
    None,
    String { placeholder: &'static str },
}

/// Semantic metadata for a built-in schema attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchemaAttributeSpec {
    pub name: &'static str,
    pub arguments: SchemaAttributeArguments,
    pub repeatable: bool,
}

impl SchemaAttributeSpec {
    /// Render the attribute without its contextual `@`/`@@` prefix.
    pub fn signature(self) -> String {
        match self.arguments {
            SchemaAttributeArguments::None => self.name.to_string(),
            SchemaAttributeArguments::String { placeholder } => {
                format!(r#"{}("{placeholder}")"#, self.name)
            }
        }
    }
}

pub const SCHEMA_ATTRIBUTE_SPECS: &[SchemaAttributeSpec] = &[
    SchemaAttributeSpec {
        name: "description",
        arguments: SchemaAttributeArguments::String {
            placeholder: "text",
        },
        repeatable: false,
    },
    SchemaAttributeSpec {
        name: "alias",
        arguments: SchemaAttributeArguments::String {
            placeholder: "name",
        },
        repeatable: false,
    },
    SchemaAttributeSpec {
        name: "skip",
        arguments: SchemaAttributeArguments::None,
        repeatable: false,
    },
];

pub fn schema_attribute_spec(name: &str) -> Option<&'static SchemaAttributeSpec> {
    SCHEMA_ATTRIBUTE_SPECS.iter().find(|spec| spec.name == name)
}

/// Presentation-neutral identity and signature for a well-known client
/// configuration key. Provider-specific validation remains owned by the
/// client-options/type schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientConfigKeySpec {
    pub name: &'static str,
    pub signature: &'static str,
}

pub const CLIENT_CONFIG_KEY_SPECS: &[ClientConfigKeySpec] = &[
    ClientConfigKeySpec {
        name: "provider",
        signature: "provider <name>",
    },
    ClientConfigKeySpec {
        name: "options",
        signature: "options { ... }",
    },
    ClientConfigKeySpec {
        name: "model",
        signature: "model <name>",
    },
    ClientConfigKeySpec {
        name: "http",
        signature: "http { ... }",
    },
    ClientConfigKeySpec {
        name: "request_timeout_ms",
        signature: "request_timeout_ms <milliseconds>",
    },
    ClientConfigKeySpec {
        name: "retry_policy",
        signature: "retry_policy <name>",
    },
];

pub fn client_config_key_spec(name: &str) -> Option<&'static ClientConfigKeySpec> {
    CLIENT_CONFIG_KEY_SPECS
        .iter()
        .find(|spec| spec.name == name)
}

/// Words that BAML holds back for future language surface.
///
/// None of these are keywords today — they still lex as ordinary identifier
/// words, so member *access* (`record.const`, `x.try`) keeps parsing. What is
/// rejected is *declaring* a name equal to one of them, so that turning any of
/// these into real syntax later cannot break existing programs.
///
/// Must stay sorted; `is_reserved_keyword` binary-searches it.
pub const RESERVED_KEYWORDS: &[&str] = &[
    "async", "const", "do", "gen", "import", "macro", "priv", "private", "pub", "public",
    "readonly", "try", "unsafe", "virtual", "yield",
];

// `final` and `union` belong on this list but are held back: both are declared
// as method names today — `Stream.final()` in
// `baml_builtins2/baml_std/baml/ns_llm/llm_types.baml` (called from the Python,
// TypeScript and Rust SDK suites) and `union` in
// `sdk_tests/fixtures/function_calls/baml_src/ns_method_self_edges/types.baml`.
// Rename those, then add the words here.

/// Whether `name` is held back for future language surface and so may not be
/// declared as an identifier. Case-sensitive: only the exact lowercase spelling
/// is reserved.
pub fn is_reserved_keyword(name: &str) -> bool {
    RESERVED_KEYWORDS.binary_search(&name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_attribute_lookup_and_signatures() {
        assert_eq!(
            schema_attribute_spec("alias").map(|spec| spec.signature()),
            Some(r#"alias("name")"#.to_string())
        );
        assert_eq!(
            schema_attribute_spec("skip").map(|spec| spec.signature()),
            Some("skip".to_string())
        );
        assert!(schema_attribute_spec("stream.done").is_none());
    }

    #[test]
    fn client_config_key_lookup() {
        assert_eq!(
            client_config_key_spec("request_timeout_ms").map(|spec| spec.signature),
            Some("request_timeout_ms <milliseconds>")
        );
        assert!(client_config_key_spec("temperature").is_none());
    }

    #[test]
    fn reserved_keywords_are_sorted_and_unique() {
        let mut sorted = RESERVED_KEYWORDS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted, RESERVED_KEYWORDS,
            "RESERVED_KEYWORDS must stay sorted and duplicate-free for `is_reserved_keyword`"
        );
    }

    #[test]
    fn reserved_keyword_lookup_is_exact() {
        assert!(is_reserved_keyword("async"));
        assert!(is_reserved_keyword("yield"));
        assert!(is_reserved_keyword("priv"));
        // Held back until the stdlib/SDK method names are renamed.
        assert!(!is_reserved_keyword("final"));
        assert!(!is_reserved_keyword("union"));
        // Only the exact lowercase spelling is reserved.
        assert!(!is_reserved_keyword("Async"));
        assert!(!is_reserved_keyword("async_"));
        assert!(!is_reserved_keyword("asyn"));
    }
}
