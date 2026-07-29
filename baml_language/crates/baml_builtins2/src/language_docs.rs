//! Shared, embedded language-reference documentation.
//!
//! The CLI and editor tooling consume these typed registries instead of
//! maintaining presentation-specific keyword and attribute tables.

use std::{collections::HashMap, sync::LazyLock};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LanguageTopic {
    pub summary: String,
    #[serde(default)]
    pub syntax: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct TypescriptCrosswalkTopic {
    pub message: String,
    #[serde(default)]
    pub see: Option<String>,
}

static LANGUAGE_TOPICS: LazyLock<HashMap<String, LanguageTopic>> = LazyLock::new(|| {
    serde_yaml::from_str(crate::BAML_KEYWORDS_YAML)
        .expect("failed to parse embedded BAML language topics")
});

static TYPESCRIPT_CROSSWALK_TOPICS: LazyLock<HashMap<String, TypescriptCrosswalkTopic>> =
    LazyLock::new(|| {
        serde_yaml::from_str(crate::TS_KEYWORDS_YAML)
            .expect("failed to parse embedded TypeScript crosswalk topics")
    });

pub fn language_topic(name: &str) -> Option<&'static LanguageTopic> {
    LANGUAGE_TOPICS.get(name)
}

pub fn language_topics() -> &'static HashMap<String, LanguageTopic> {
    &LANGUAGE_TOPICS
}

pub fn typescript_crosswalk_topic(name: &str) -> Option<&'static TypescriptCrosswalkTopic> {
    TYPESCRIPT_CROSSWALK_TOPICS.get(name)
}

pub fn typescript_crosswalk_topics() -> &'static HashMap<String, TypescriptCrosswalkTopic> {
    &TYPESCRIPT_CROSSWALK_TOPICS
}

pub fn has_describe_topic(name: &str) -> bool {
    language_topic(name).is_some() || typescript_crosswalk_topic(name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_attributes_and_intrinsic_types_have_topics() {
        for spec in baml_base::SCHEMA_ATTRIBUTE_SPECS {
            assert!(
                language_topic(spec.name).is_some(),
                "missing topic for schema attribute `{}`",
                spec.name
            );
        }
        for spec in baml_base::CLIENT_CONFIG_KEY_SPECS {
            assert!(
                language_topic(spec.name).is_some(),
                "missing topic for client config key `{}`",
                spec.name
            );
        }
        for name in ["void", "never", "unknown"] {
            assert!(language_topic(name).is_some(), "missing topic for `{name}`");
        }
    }

    #[test]
    fn language_and_crosswalk_topics_share_one_lookup_boundary() {
        assert!(has_describe_topic("class"));
        assert!(has_describe_topic("instanceof"));
        assert!(!has_describe_topic("definitely_not_a_language_topic"));
    }

    #[test]
    fn test_topic_uses_expression_body_syntax() {
        let topic = language_topic("test").expect("missing test topic");
        let syntax = topic
            .syntax
            .as_deref()
            .expect("test topic should show syntax");
        assert!(syntax.contains("test \""));
        assert!(!syntax.contains("functions ["));
        assert!(!syntax.contains("args {"));
        assert!(
            !topic
                .details
                .as_deref()
                .unwrap_or_default()
                .contains("functions [")
        );
    }
}
