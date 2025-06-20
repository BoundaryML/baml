use std::collections::HashSet;

use baml_derive::BamlHash;
use baml_types::{BamlMap, UnresolvedValue};
use internal_baml_parser_database::RetryPolicyStrategy;

use super::{super::RetryPolicy, ShallowSignature};

/// Find some way to hash the class

// Shallow hashes are based on the thigns that define the class
// (unordered list of [field name, field type], name of the class, if its dynamic)

#[derive(Debug, BamlHash)]
struct RetryPolicyInterfaceHash<'a> {
    name: &'a str,
}

#[derive(Debug, BamlHash)]
struct RetryPolicyImplementationHash<'a> {
    name: &'a str,
    max_retries: u32,
    strategy: &'a RetryPolicyStrategy,
    // NB: the parser DB has a notion of "empty options" vs "no options"; we collapse
    // those here into an empty vec
    options: Vec<(String, UnresolvedValue<()>)>,
}

impl super::ShallowSignature for RetryPolicy {
    fn shallow_hash_prefix(&self) -> &'static str {
        "retry_policy"
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        let name = self.elem.name.0.as_str();

        RetryPolicyInterfaceHash { name }
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        Some(RetryPolicyImplementationHash {
            name: self.elem.name.0.as_str(),
            max_retries: self.elem.max_retries,
            strategy: &self.elem.strategy,
            options: self
                .elem
                .options
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        })
    }

    // No dependencies for retry policies
    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        Default::default()
    }
}
