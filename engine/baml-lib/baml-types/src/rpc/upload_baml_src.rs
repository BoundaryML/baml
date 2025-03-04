use serde::{Deserialize, Serialize};

// TODO: version handling should be non-exhaustive for all of these
// clients need to say "i can only handle v1 responses"

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BamlSrcUploadStatus {
    None,
    Pending,
    Completed,
    Failed,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlTypeId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub function_id: String,
    pub inputs: Vec<BamlFunctionInput>,
    pub output: BamlTypeReference,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlFunctionInput {
    pub name: String,
    pub value: BamlTypeReference,
}
