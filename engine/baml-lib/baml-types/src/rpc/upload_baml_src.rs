use serde::{Deserialize, Serialize};

use crate::{FieldType, LiteralValue, TypeValue};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(into = "String", from = "String")]
pub struct UniqueId {
    pub type_name: String,
    pub name: String,
    pub interface_hash: Option<u64>,
    pub impl_hash: Option<u64>,
}

impl std::fmt::Display for UniqueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}##{}##{}##{}",
            self.type_name,
            self.name,
            self.interface_hash.unwrap_or(0),
            self.impl_hash.unwrap_or(0)
        )
    }
}

impl std::str::FromStr for UniqueId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split("##").collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(anyhow::anyhow!("Invalid unique id: {}", s));
        }
        Ok(UniqueId {
            type_name: parts[0].to_string(),
            name: parts[1].to_string(),
            interface_hash: parts[2].parse().ok(),
            impl_hash: parts[3].parse().ok(),
        })
    }
}

impl From<UniqueId> for String {
    fn from(value: UniqueId) -> Self {
        value.to_string()
    }
}

impl From<String> for UniqueId {
    fn from(value: String) -> Self {
        value.parse().expect("Failed to parse UniqueId from string")
    }
}

// TODO: version handling should be non-exhaustive for all of these
// clients need to say "i can only handle v1 responses"

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BamlSrcUploadStatus {
    DoesNotExist,
    Exists,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBamlSrcUploadStatusRequest {
    pub project_id: String,
    pub baml_src_id: UniqueId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBamlSrcUploadStatusResponse {
    pub project_id: String,
    pub baml_src_id: UniqueId,
    pub status: BamlSrcUploadStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBamlSrcRequest {
    pub project_id: String,
    pub baml_src_id: UniqueId,
    pub function_definitions: Vec<BamlFunctionDefinition>,
    pub type_definitions: Vec<BamlTypeDefinition>,
}

impl UploadBamlSrcRequest {
    pub fn to_get_baml_src_upload_status_request(&self) -> GetBamlSrcUploadStatusRequest {
        GetBamlSrcUploadStatusRequest {
            project_id: self.project_id.clone(),
            baml_src_id: self.baml_src_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBamlSrcResponse {
    pub project_id: String,
    pub baml_src_id: UniqueId,
}

// ------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BamlTypeId(pub UniqueId);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BamlFunctionId(pub UniqueId);

impl From<UniqueId> for BamlFunctionId {
    fn from(value: UniqueId) -> Self {
        BamlFunctionId(value)
    }
}

impl From<UniqueId> for BamlTypeId {
    fn from(value: UniqueId) -> Self {
        BamlTypeId(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BamlMediaType {
    Image,
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BamlTypeReference {
    Null,
    Int,
    Bool,
    Float,
    String,
    Media(BamlMediaType),
    Class {
        type_id: String,
    },
    Enum {
        type_id: String,
    },
    TypeAlias {
        type_id: String,
    },
    Array {
        items: Box<BamlTypeReference>,
    },
    Map {
        key: Box<BamlTypeReference>,
        value: Box<BamlTypeReference>,
    },
    // Optionals are unions
    Union {
        #[serde(rename = "anyOf")]
        any_of: Vec<BamlTypeReference>,
    },
    Tuple {
        items: Vec<BamlTypeReference>,
    },
    Literal(BamlLiteralTypeReference),
}

impl From<FieldType> for BamlTypeReference {
    fn from(value: FieldType) -> Self {
        match value {
            FieldType::Primitive(TypeValue::Null) => BamlTypeReference::Null,
            FieldType::Primitive(TypeValue::String) => BamlTypeReference::String,
            FieldType::Primitive(TypeValue::Int) => BamlTypeReference::Int,
            FieldType::Primitive(TypeValue::Float) => BamlTypeReference::Float,
            FieldType::Primitive(TypeValue::Bool) => BamlTypeReference::Bool,
            FieldType::Primitive(TypeValue::Media(media_type)) => {
                BamlTypeReference::Media(match media_type {
                    crate::BamlMediaType::Image => BamlMediaType::Image,
                    crate::BamlMediaType::Audio => BamlMediaType::Audio,
                })
            }
            FieldType::Class(class_name) => BamlTypeReference::Class {
                type_id: class_name.to_string(),
            },
            FieldType::Enum(enum_name) => BamlTypeReference::Enum {
                type_id: enum_name.to_string(),
            },
            FieldType::List(inner) => BamlTypeReference::Array {
                items: Box::new((*inner).into()),
            },
            FieldType::Map(key, value) => BamlTypeReference::Map {
                key: Box::new((*key).into()),
                value: Box::new((*value).into()),
            },
            FieldType::Union(union) => BamlTypeReference::Union {
                any_of: union.into_iter().map(|t| t.into()).collect(),
            },
            FieldType::Literal(literal) => BamlTypeReference::Literal(literal.into()),
            FieldType::Optional(inner) => BamlTypeReference::Union {
                any_of: vec![BamlTypeReference::Null, (*inner).into()],
            },
            FieldType::Tuple(field_types) => BamlTypeReference::Tuple {
                items: field_types.into_iter().map(|t| t.into()).collect(),
            },
            FieldType::RecursiveTypeAlias(name) => BamlTypeReference::TypeAlias {
                type_id: name.to_string(),
            },
            FieldType::WithMetadata { base, .. } => (*base).into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "literal_type", content = "literal", rename_all = "snake_case")]
pub enum BamlLiteralTypeReference {
    String(String),
    Int(i64),
    Bool(bool),
}

impl From<LiteralValue> for BamlLiteralTypeReference {
    fn from(value: LiteralValue) -> Self {
        match value {
            LiteralValue::String(s) => BamlLiteralTypeReference::String(s),
            LiteralValue::Int(i) => BamlLiteralTypeReference::Int(i),
            LiteralValue::Bool(b) => BamlLiteralTypeReference::Bool(b),
        }
    }
}

impl From<&LiteralValue> for BamlLiteralTypeReference {
    fn from(value: &LiteralValue) -> Self {
        value.clone().into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BamlTypeDefinition {
    Class(BamlClassDefinition),
    Enum(BamlEnumDefinition),
    TypeAlias(BamlTypeAliasDefinition),
}

impl BamlTypeDefinition {
    pub fn type_id(&self) -> &BamlTypeId {
        match self {
            BamlTypeDefinition::Class(definition) => &definition.type_id,
            BamlTypeDefinition::Enum(definition) => &definition.type_id,
            BamlTypeDefinition::TypeAlias(definition) => &definition.type_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlClassDefinition {
    pub type_id: BamlTypeId,
    pub fields: Vec<BamlClassField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlClassField {
    pub name: String,
    pub r#type: BamlTypeReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlEnumDefinition {
    pub type_id: BamlTypeId,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlTypeAliasDefinition {
    pub type_id: BamlTypeId,
    pub type_reference: BamlTypeReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlFunctionDefinition {
    pub function_id: BamlFunctionId,
    pub inputs: Vec<BamlFunctionInput>,
    pub output: BamlTypeReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlFunctionInput {
    pub name: String,
    pub value: BamlTypeReference,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unique_id() {
        let id = UniqueId {
            type_name: "test".to_string(),
            name: "test".to_string(),
            interface_hash: Some(1),
            impl_hash: Some(2),
        };
        let id_str = serde_json::to_string(&id).unwrap();
        let id_from_str = serde_json::from_str::<UniqueId>(&id_str).unwrap();
        assert_eq!(id, id_from_str);
    }

    #[test]
    fn test_baml_type_id() {
        let id = BamlTypeId(UniqueId {
            type_name: "test".to_string(),
            name: "test".to_string(),
            interface_hash: Some(1),
            impl_hash: Some(2),
        });
        let id_as_string = serde_json::to_string(&id).unwrap();
        println!("id_as_string: {}", id_as_string);
        let id_str = serde_json::to_string(&id.0).unwrap();
        assert_eq!(id_as_string, id_str);
        let id_from_str = serde_json::from_str::<BamlTypeId>(&id_as_string).unwrap();
        assert_eq!(id.0, id_from_str.0);
    }
}
