//! Shared, embedded language-reference documentation.
//!
//! The CLI and editor tooling consume these typed registries instead of
//! maintaining presentation-specific keyword and attribute tables.

use std::{collections::HashMap, sync::LazyLock};

use yaml_rust2::{Yaml, YamlLoader};

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

static LANGUAGE_TOPICS: LazyLock<HashMap<String, LanguageTopic>> =
    LazyLock::new(|| load_topics(crate::BAML_KEYWORDS_YAML, language_topic_from_yaml));

static TYPESCRIPT_CROSSWALK_TOPICS: LazyLock<HashMap<String, TypescriptCrosswalkTopic>> =
    LazyLock::new(|| {
        load_topics(crate::TS_KEYWORDS_YAML, |topic| TypescriptCrosswalkTopic {
            message: required_string(topic, "message"),
            see: optional_string(topic, "see"),
        })
    });

// These embedded registries contain string fields only. Read their schema
// directly so all YAML consumers can share the same parser dependency.
fn load_topics<T>(source: &str, parse_topic: impl Fn(&Yaml) -> T) -> HashMap<String, T> {
    let documents = YamlLoader::load_from_str(source).expect("invalid embedded topic YAML");
    let [document] = documents.as_slice() else {
        panic!("embedded topics must contain exactly one YAML document");
    };
    document
        .as_hash()
        .expect("embedded topics must be a YAML mapping")
        .iter()
        .map(|(name, topic)| {
            let name = name.as_str().expect("embedded topic names must be strings");
            assert!(
                topic.as_hash().is_some(),
                "topic `{name}` must be a mapping"
            );
            (name.to_owned(), parse_topic(topic))
        })
        .collect()
}

fn language_topic_from_yaml(topic: &Yaml) -> LanguageTopic {
    LanguageTopic {
        summary: required_string(topic, "summary"),
        syntax: optional_string(topic, "syntax"),
        details: optional_string(topic, "details"),
    }
}

fn required_string(topic: &Yaml, field: &str) -> String {
    topic[field]
        .as_str()
        .unwrap_or_else(|| panic!("embedded topic field `{field}` must be a string"))
        .to_owned()
}

fn optional_string(topic: &Yaml, field: &str) -> Option<String> {
    match &topic[field] {
        Yaml::BadValue | Yaml::Null => None,
        Yaml::String(value) => Some(value.clone()),
        _ => panic!("embedded topic field `{field}` must be a string or null"),
    }
}

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
    fn topic_yaml_preserves_block_strings_and_optional_fields() {
        let topics = load_topics(
            "test:\n  summary: |\n    First line.\n    Second line.\n  syntax: null\n",
            language_topic_from_yaml,
        );
        let topic = &topics["test"];
        assert_eq!(topic.summary, "First line.\nSecond line.\n");
        assert_eq!(topic.syntax, None);
        assert_eq!(topic.details, None);
    }

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
