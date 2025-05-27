use std::collections::HashSet;

use baml_types::{FieldType, StringOr};
use internal_llm_client::ClientSpec;

use super::{super::FunctionNode, ShallowSignature};

/// Find some way to hash the class

// Shallow hashes are based on the thigns that define the class
// (unordered list of [field name, field type], name of the class, if its dynamic)

#[derive(Debug)]
struct FunctionInterfaceHash<'a> {
    name: &'a str,
    args: Vec<(&'a str, &'a FieldType)>,
    return_type: &'a FieldType,
}

impl<'a> std::hash::Hash for FunctionInterfaceHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        for (name, field_type) in &self.args {
            name.hash(state);
            field_type.shallow_interface_hash().hash(state);
        }
        self.return_type.shallow_interface_hash().hash(state);
    }
}

#[derive(Debug)]
struct FunctionImplementationHash<'a> {
    client: &'a ClientSpec,
    prompt: &'a str,
    args: Vec<(&'a str, &'a FieldType)>,
    return_type: &'a FieldType,
}

impl<'a> std::hash::Hash for FunctionImplementationHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.client.hash(state);
        self.prompt.hash(state);
        for (name, field_type) in &self.args {
            name.hash(state);
            field_type.shallow_implementation_hash().hash(state);
        }
        self.return_type.shallow_implementation_hash().hash(state);
    }
}

impl super::ShallowSignature for FunctionNode {
    fn shallow_hash_prefix(&self) -> &'static str {
        "function"
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        let name = self.elem.name.as_str();

        let args = self
            .elem
            .inputs
            .iter()
            .map(|(name, r#type)| (name.as_str(), r#type))
            .collect::<Vec<_>>();

        FunctionInterfaceHash {
            name,
            args,
            return_type: &self.elem.output,
        }
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        self.elem
            .default_config()
            .map(|c| FunctionImplementationHash {
                client: &c.client,
                prompt: c.prompt_template.as_str(),
                args: self
                    .elem
                    .inputs
                    .iter()
                    .map(|(name, r#type)| (name.as_str(), r#type))
                    .collect(),
                return_type: &self.elem.output,
            })
    }

    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        self.elem
            .inputs
            .iter()
            .flat_map(|(_, r#type)| r#type.dependencies())
            .chain(self.elem.output.dependencies())
            .collect()
    }

    fn unsorted_implementation_dependencies(&self) -> HashSet<String> {
        let type_dependencies = self
            .elem
            .inputs
            .iter()
            .flat_map(|(_, r#type)| r#type.dependencies())
            .chain(self.elem.output.dependencies());

        let client_dependencies = self.elem.default_config().map(|c| match &c.client {
            ClientSpec::Named(name) => Some(name.clone()),
            ClientSpec::Shorthand(provider, model) => None,
        });

        // TODO: Add prompt dependencies (i.e. template_strings)

        match client_dependencies {
            Some(client_dependencies) => type_dependencies.chain(client_dependencies).collect(),
            None => type_dependencies.collect(),
        }
    }
}
