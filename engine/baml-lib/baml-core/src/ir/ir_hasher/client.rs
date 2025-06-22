use std::collections::HashSet;

use baml_types::BamlMap;
use internal_llm_client::{ClientProvider, UnresolvedClientProperty};

use super::{super::Client, ShallowSignature};

/// Find some way to hash the class

// Shallow hashes are based on the thigns that define the class
// (unordered list of [field name, field type], name of the class, if its dynamic)

#[derive(Debug)]
struct ClientInterfaceHash<'a> {
    name: &'a str,
}

impl<'a> std::hash::Hash for ClientInterfaceHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

#[derive(Debug)]
struct ClientImplementationHash<'a> {
    name: &'a str,
    provider: &'a ClientProvider,
    retry_policy: Option<&'a str>,
    options: &'a UnresolvedClientProperty<()>,
}

impl<'a> std::hash::Hash for ClientImplementationHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.provider.hash(state);
        self.retry_policy.hash(state);
        self.options.hash(state);
    }
}

impl super::ShallowSignature for Client {
    fn shallow_hash_prefix(&self) -> &'static str {
        "client"
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        let name = self.elem.name.as_str();

        ClientInterfaceHash { name }
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        Some(ClientImplementationHash {
            name: self.elem.name.as_str(),
            provider: &self.elem.provider,
            retry_policy: self.elem.retry_policy_id.as_deref(),
            options: &self.elem.options,
        })
    }

    fn unsorted_implementation_dependencies(&self) -> HashSet<String> {
        let mut deps = self.elem.options.dependencies();
        if let Some(id) = &self.elem.retry_policy_id {
            deps.insert(id.to_string());
        }
        deps
    }

    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        // No dependencies for the interface because it's just the name
        Default::default()
    }
}
