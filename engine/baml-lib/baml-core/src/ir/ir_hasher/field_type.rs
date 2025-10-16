use std::collections::HashSet;

use baml_types::{BamlMediaType, LiteralValue, TypeIR};

#[derive(Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum InterfaceFieldType<'a> {
    Unknown,
    Null,
    String,
    Int,
    Bool,
    Float,
    Media(&'a BamlMediaType),
    Literal(&'a LiteralValue),
    Enum(&'a str),
    Class(&'a str),
    List(Box<InterfaceFieldType<'a>>),
    Map(Box<InterfaceFieldType<'a>>, Box<InterfaceFieldType<'a>>),
    Union(Vec<InterfaceFieldType<'a>>),
    Tuple(Vec<InterfaceFieldType<'a>>),
    RecursiveTypeAlias(&'a str),
}

impl<'a> InterfaceFieldType<'a> {
    pub fn from_field_type(field_type: &'a TypeIR) -> Self {
        Self::from(field_type).simplify()
    }

    fn flatten(self) -> Vec<InterfaceFieldType<'a>> {
        match self {
            InterfaceFieldType::Union(field_types) => field_types
                .into_iter()
                .flat_map(|ft| ft.flatten())
                .collect(),
            _ => vec![self],
        }
    }

    fn simplify(self) -> InterfaceFieldType<'a> {
        let flattened = self.flatten();
        match flattened.len() {
            0 => InterfaceFieldType::Unknown,
            1 => flattened.into_iter().next().unwrap(),
            _ => {
                let mut simplified: Vec<_> =
                    flattened.into_iter().map(|ft| ft.simplify()).collect();
                simplified.sort();
                simplified.dedup();
                if simplified.len() == 1 {
                    simplified.into_iter().next().unwrap()
                } else {
                    InterfaceFieldType::Union(simplified)
                }
            }
        }
    }
}

impl<'a> InterfaceFieldType<'a> {
    fn from(field_type: &'a TypeIR) -> Self {
        match field_type {
            TypeIR::Primitive(type_value, _) => match type_value {
                baml_types::TypeValue::String => InterfaceFieldType::String,
                baml_types::TypeValue::Int => InterfaceFieldType::Int,
                baml_types::TypeValue::Float => InterfaceFieldType::Float,
                baml_types::TypeValue::Bool => InterfaceFieldType::Bool,
                baml_types::TypeValue::Null => InterfaceFieldType::Null,
                baml_types::TypeValue::Media(baml_media_type) => {
                    InterfaceFieldType::Media(baml_media_type)
                }
            },
            TypeIR::Enum { name, .. } => InterfaceFieldType::Enum(name.as_str()),
            TypeIR::Literal(literal_value, _) => InterfaceFieldType::Literal(literal_value),
            TypeIR::Class { name, .. } => InterfaceFieldType::Class(name.as_str()),
            TypeIR::List(field_type, _) => {
                InterfaceFieldType::List(Box::new(Self::from(field_type)))
            }
            TypeIR::Map(field_type, field_type1, _) => InterfaceFieldType::Map(
                Box::new(Self::from(field_type)),
                Box::new(Self::from(field_type1)),
            ),
            TypeIR::Union(field_types, _) => InterfaceFieldType::Union(
                field_types
                    .iter_include_null()
                    .iter()
                    .map(|ft| Self::from(ft))
                    .collect(),
            ),
            TypeIR::Tuple(field_types, _) => {
                InterfaceFieldType::Tuple(field_types.iter().map(Self::from).collect())
            }
            TypeIR::RecursiveTypeAlias { name, .. } => {
                InterfaceFieldType::RecursiveTypeAlias(name.as_str())
            }
            TypeIR::Arrow(_, _) => InterfaceFieldType::Unknown,
            TypeIR::Top(_) => InterfaceFieldType::Unknown,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
struct ImplementationFieldType {}

impl super::ShallowSignature for TypeIR {
    fn shallow_hash_prefix(&self) -> &'static str {
        "type_alias"
    }

    fn shallow_interface_hash(&self) -> impl std::hash::Hash {
        InterfaceFieldType::from_field_type(self)
    }

    fn unsorted_interface_dependencies(&self) -> HashSet<String> {
        self.dependencies()
    }

    fn shallow_implementation_hash(&self) -> Option<impl std::hash::Hash> {
        None as Option<ImplementationFieldType>
    }
}
