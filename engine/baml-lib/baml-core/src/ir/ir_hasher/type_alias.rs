use std::collections::HashSet;

use baml_types::FieldType;

use super::{super::TypeAlias, ShallowSignature};

/// Find some way to hash the class

// Shallow hashes are based on the thigns that define the class
// (unordered list of [field name, field type], name of the class, if its dynamic)

#[derive(Debug)]
struct TypeAliasInterfaceHash<'a> {
    name: &'a str,
    expr: &'a FieldType,
}

impl<'a> std::hash::Hash for TypeAliasInterfaceHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
        let expr_hash = self.expr.shallow_interface_hash();
        expr_hash.hash(state);
    }
}

#[derive(Debug)]
struct TypeAliasImplementationHash<'a> {
    expr: &'a FieldType,
}

impl<'a> std::hash::Hash for TypeAliasImplementationHash<'a> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let expr_hash = self.expr.shallow_implementation_hash();
        expr_hash.hash(state);
    }
}

impl super::ShallowSignature for TypeAlias {
    fn shallow_hash_prefix(&self) -> &'static str {
        "type_alias"
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        TypeAliasInterfaceHash {
            name: self.elem.name.as_str(),
            expr: &self.elem.r#type.elem,
        }
    }

    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        self.elem.r#type.elem.dependencies()
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        Some(TypeAliasImplementationHash {
            expr: &self.elem.r#type.elem,
        })
    }
}
