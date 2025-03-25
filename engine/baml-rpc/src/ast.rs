use serde::{Deserialize, Serialize};

use crate::ast_node_id::AstNodeId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BamlTypeId(pub AstNodeId);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BamlFunctionId(pub AstNodeId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BamlMediaType {
    Image,
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum TypeValue {
    String,
    Int,
    Float,
    Bool,
    Null,
    Media(BamlMediaType),
}

/// Subset of [`crate::BamlValue`] allowed for literal type definitions.
#[derive(serde::Serialize, Debug, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
}
/// FieldType represents the type of either a class field or a function arg.
#[derive(serde::Serialize, Debug, Clone, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum FieldType {
    Primitive(TypeValue),
    Enum(String),
    Literal(LiteralValue),
    Class(String),
    List(Box<FieldType>),
    Map(Box<FieldType>, Box<FieldType>),
    Union(Vec<FieldType>),
    Tuple(Vec<FieldType>),
    Optional(Box<FieldType>),
    RecursiveTypeAlias(String),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "literal_type", content = "literal", rename_all = "snake_case")]
pub enum BamlLiteralTypeReference {
    String(String),
    Int(i64),
    Bool(bool),
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
