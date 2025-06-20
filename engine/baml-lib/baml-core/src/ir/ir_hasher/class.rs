use std::collections::HashSet;

use baml_types::{Constraint, FieldType, StringOr};

use super::{super::Class, ShallowSignature};

/// Find some way to hash the class

// Shallow hashes are based on the thigns that define the class
// (unordered list of [field name, field type], name of the class, if its dynamic)

#[derive(Debug)]
struct ClassInterfaceHash<'a> {
    name: &'a str,
    is_dynamic: bool,
    fields: Vec<(&'a str, &'a FieldType)>,
}

impl<'a> std::hash::Hash for ClassInterfaceHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        self.is_dynamic.hash(state);
        for (name, field_type) in &self.fields {
            name.hash(state);
            field_type.shallow_interface_hash().hash(state);
        }
    }
}

#[derive(Debug)]
pub(super) struct NameForLLM<'a> {
    pub name: &'a str,
    pub alias: Option<&'a StringOr>,
    pub description: Option<&'a StringOr>,
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
    }
}

#[derive(Debug)]
struct ClassImplementationHash<'a> {
    fields: Vec<(NameForLLM<'a>, &'a FieldType)>,
    constraints: Vec<&'a Constraint>,
}

impl<'a> std::hash::Hash for ClassImplementationHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (name, field_type) in &self.fields {
            name.hash(state);
            field_type.shallow_implementation_hash().hash(state);
        }
        self.constraints.hash(state);
    }
}

impl super::ShallowSignature for Class {
    fn shallow_hash_prefix(&self) -> &'static str {
        "class"
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        let name = self.elem.name.as_str();

        let mut fields = self
            .elem
            .static_fields
            .iter()
            .map(|f| (f.elem.name.as_str(), &f.elem.r#type.elem))
            .collect::<Vec<_>>();
        // sort by field name
        fields.sort_by_key(|(name, _)| name.to_string());

        // Only asserts are used
        let mut constraints = self
            .attributes
            .constraints
            .iter()
            .filter(|c| matches!(c.level, baml_types::ConstraintLevel::Assert))
            .collect::<Vec<_>>();
        constraints.sort_by_key(|c| (c.level.clone(), c.label.clone()));

        let is_dynamic = self.attributes.dynamic();

        ClassInterfaceHash {
            name,
            fields,
            is_dynamic,
        }
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        // Only asserts are used
        let mut constraints = self.attributes.constraints.iter().collect::<Vec<_>>();
        constraints.sort_by_key(|c| (c.level.clone(), c.label.clone()));

        let fields = self
            .elem
            .static_fields
            .iter()
            .map(|f| {
                (
                    NameForLLM {
                        name: f.elem.name.as_str(),
                        alias: f.attributes.alias(),
                        description: f.attributes.description(),
                    },
                    &f.elem.r#type.elem,
                )
            })
            .collect::<Vec<_>>();

        Some(ClassImplementationHash {
            fields,
            constraints,
        })
    }

    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        // Depends on the fields
        self.elem
            .static_fields
            .iter()
            .flat_map(|f| f.elem.r#type.elem.dependencies())
            .collect()
    }
}
