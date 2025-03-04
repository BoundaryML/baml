use serde::{Deserialize, Serialize};

use crate::{FieldType, LiteralValue, TypeValue};

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
    pub baml_src_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetBamlSrcUploadStatusResponse {
    pub project_id: String,
    pub baml_src_id: String,
    pub status: BamlSrcUploadStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBamlSrcRequest {
    pub project_id: String,
    pub baml_src_id: String,
    pub function_definitions: Vec<BamlFunctionDefinition>,
    pub type_definitions: Vec<BamlTypeDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadBamlSrcResponse {
    pub project_id: String,
    pub baml_src_id: String,
}

// ------------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BamlTypeId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BamlTypeReference {
    Null,
    Int,
    Bool,
    Float,
    String,
    Class {
        type_id: BamlTypeId,
    },
    Enum {
        type_id: BamlTypeId,
    },
    TypeAlias {
        type_id: BamlTypeId,
    },
    Array {
        items: Box<BamlTypeReference>,
    },
    Map {
        key: Box<BamlTypeReference>,
        value: Box<BamlTypeReference>,
    },
    Union {
        #[serde(rename = "anyOf")]
        any_of: Vec<BamlTypeReference>,
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
            FieldType::Class(class_name) => BamlTypeReference::Class {
                type_id: BamlTypeId(class_name.to_string()),
            },
            FieldType::Enum(enum_name) => BamlTypeReference::Enum {
                type_id: BamlTypeId(enum_name.to_string()),
            },
            FieldType::List(inner) => BamlTypeReference::Array {
                items: Box::new((*inner).into()),
            },
            FieldType::Map(key, value) => BamlTypeReference::Map {
                key: Box::new((*key).into()),
                value: Box::new((*value).into()),
            },
            // TODO: union flattening
            FieldType::Union(union) => BamlTypeReference::Union {
                any_of: union.into_iter().map(|t| t.into()).collect(),
            },
            FieldType::Literal(literal) => BamlTypeReference::Literal(literal.into()),
            FieldType::Optional(inner) => BamlTypeReference::Union {
                any_of: vec![BamlTypeReference::Null, (*inner).into()],
            },
            _ => unimplemented!("from(FieldType) not implemented for {:?}", value),
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
    pub function_id: String,
    pub inputs: Vec<BamlFunctionInput>,
    pub output: BamlTypeReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlFunctionInput {
    pub name: String,
    pub value: BamlTypeReference,
}
