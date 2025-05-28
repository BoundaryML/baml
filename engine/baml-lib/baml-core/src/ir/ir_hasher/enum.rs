use std::collections::HashSet;

use baml_types::StringOr;

use super::super::Enum;

/// Find some way to hash the class

// Shallow hashes are based on the thigns that define the class
// (unordered list of [field name, field type], name of the class, if its dynamic)

#[derive(Debug)]
struct EnumInterfaceHash<'a> {
    name: &'a str,
    values: Vec<&'a str>,
    is_dynamic: bool,
}

impl<'a> std::hash::Hash for EnumInterfaceHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.is_dynamic.hash(state);
        self.values.hash(state);
    }
}

#[derive(Debug)]
pub(super) struct NameForLLM<'a> {
    pub name: &'a str,
    pub alias: Option<&'a StringOr>,
    pub description: Option<&'a StringOr>,
    pub skip: bool,
}

impl<'a> std::hash::Hash for NameForLLM<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        if let Some(alias) = self.alias {
            alias.hash(state);
        }
        if let Some(description) = self.description {
            description.hash(state);
        }
        self.skip.hash(state);
    }
}

#[derive(Debug)]
struct EnumImplementationHash<'a> {
    values: Vec<NameForLLM<'a>>,
}

impl<'a> std::hash::Hash for EnumImplementationHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.values.hash(state);
    }
}

impl super::ShallowSignature for Enum {
    fn shallow_hash_prefix(&self) -> &'static str {
        "enum"
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        let name = self.elem.name.as_str();

        let mut values = self
            .elem
            .values
            .iter()
            .map(|f| f.0.elem.0.as_str())
            .collect::<Vec<_>>();
        // sort by field name
        values.sort();

        let is_dynamic = self.attributes.dynamic();

        EnumInterfaceHash {
            name,
            values,
            is_dynamic,
        }
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        let values = self
            .elem
            .values
            .iter()
            .map(|(value, docstring)| NameForLLM {
                name: value.elem.0.as_str(),
                alias: value.attributes.alias(),
                description: value.attributes.description(),
                skip: value.attributes.skip(),
            })
            .collect::<Vec<_>>();
        Some(EnumImplementationHash { values })
    }

    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        HashSet::new()
    }
}
