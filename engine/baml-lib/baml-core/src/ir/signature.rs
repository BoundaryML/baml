use baml_types::{Resolvable, StringOr};

mod r#enum;
mod class;
mod template_string;
mod test_case;
mod client;
mod function;
mod type_alias;
#[cfg(test)]
mod test;

/// Used to identify unique signatures of types.

pub(super) trait Signature {
    fn type_name(&self) -> &'static str;
    fn interface(&self) -> Option<String>;
    fn impl_(&self) -> Option<String> {
        None
    }
}

pub struct BamlHash {
    type_name: &'static str,
    interface_hash: Option<u64>,
    impl_hash: Option<u64>,
}

pub trait ProvideBamlHash {
    fn to_baml_hash(&self) -> BamlHash;
}

fn hash_string(type_name: &str, s: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(&(type_name, s), &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

impl<T: Signature> ProvideBamlHash for T {
    fn to_baml_hash(&self) -> BamlHash {
        BamlHash {
            type_name: self.type_name(),
            interface_hash: self.interface().map(|s| hash_string(self.type_name(), &s)),
            impl_hash: self.impl_().map(|s| hash_string(self.type_name(), &s)),
        }
    }
}

impl<Meta> Signature for Resolvable<StringOr, Meta> {
    fn type_name(&self) -> &'static str {
        match self {
            Resolvable::String(s, _) => s.type_name(),
            Resolvable::Numeric(n, _) => "numeric",
            Resolvable::Bool(b, _) => "bool",
            Resolvable::Array(_, _) => "array",
            Resolvable::Map(_, _) => "map",
            Resolvable::Null(_) => "null",
        }
    }

    fn interface(&self) -> Option<String> {
        match self {
            Resolvable::String(value, _) => value.interface(),
            Resolvable::Numeric(value, _) => Some(value.to_string()),
            Resolvable::Bool(value, _) => Some(value.to_string()),
            Resolvable::Array(resolvables, _) => Some(resolvables
                .iter()
                .filter_map(|r| r.interface())
                .collect::<Vec<_>>()
                .join("_BAML_ARRAY_")),
            Resolvable::Map(index_map, _) => Some(index_map
                .iter()
                .filter_map(|(k, (_, v))| v.interface().map(|s| format!("{}: {}", k, s)))
                .collect::<Vec<_>>()
                .join("_BAML_MAP_")),
            Resolvable::Null(_) => Some("null".to_string()),
        }
    }
}

impl Signature for StringOr {
    fn type_name(&self) -> &'static str {
        match self {
            StringOr::EnvVar(_) => "env_var",
            StringOr::Value(_) => "value",
            StringOr::JinjaExpression(j) => j.type_name(),
        }
    }

    fn interface(&self) -> Option<String> {
        match self {
            StringOr::EnvVar(name) => Some(name.clone()),
            StringOr::Value(value) => Some(value.clone()),
            StringOr::JinjaExpression(expression) => expression.interface(),
        }
    }
}

impl Signature for baml_types::JinjaExpression {
    fn type_name(&self) -> &'static str {
        "jinja_expression"
    }

    fn interface(&self) -> Option<String> {
        Some(self.to_string())
    }
}
