use std::{
    borrow::Cow,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use super::{
    ast_node_id::AstNodeId,
    type_definition::{NamedType, TypeDefinition},
    type_reference::TypeReference,
};

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub struct BamlFunctionId(pub AstNodeId);

// Delegate serialization and deserialization to AstNodeId
impl Serialize for BamlFunctionId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BamlFunctionId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let ast_node_id = AstNodeId::deserialize(deserializer)?;
        Ok(BamlFunctionId(ast_node_id))
    }
}

// Only includes baml llm functions, not user-defined functions
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub struct FunctionDefinition {
    pub function_id: Arc<BamlFunctionId>,
    pub inputs: Vec<NamedType>,
    pub output: TypeReference,
    pub dependencies: Vec<AstNodeId>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub struct AST {
    pub functions: Vec<FunctionDefinition>,
    pub types: Vec<TypeDefinition>,
    pub source_code: Vec<SourceCode>,
}

impl AST {
    pub fn id(&self) -> ASTId<'_> {
        // Get all top level ids
        let top_ids = self
            .functions
            .iter()
            .map(|function| Cow::Borrowed(&function.function_id.0))
            .chain(
                self.types
                    .iter()
                    .map(|type_definition| Cow::Borrowed(&type_definition.id().0)),
            )
            .collect::<Vec<_>>();

        let top_ast_hash = AstNodeId::new_ast(
            top_ids.iter().fold(0, |acc, id| {
                let mut hasher = DefaultHasher::new();
                acc.hash(&mut hasher);
                id.interface_hash().hash(&mut hasher);
                hasher.finish()
            }),
            top_ids.iter().fold(None, |acc, id| {
                if let Some(impl_hash) = id.impl_hash() {
                    let mut hasher = DefaultHasher::new();
                    acc.hash(&mut hasher);
                    impl_hash.hash(&mut hasher);
                    Some(hasher.finish())
                } else {
                    acc
                }
            }),
        );

        ASTId {
            top_id: Cow::Owned(top_ast_hash),
            top_ids,
            raw_src_id: Cow::Borrowed(&self.source_code[0].file_name),
            raw_src_ids: self
                .source_code
                .iter()
                .map(|src| {
                    (
                        Cow::Borrowed(src.file_name.as_str()),
                        Cow::Borrowed(src.content_hash.as_str()),
                    )
                })
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub struct ASTId<'a> {
    pub top_id: Cow<'a, AstNodeId>,
    pub top_ids: Vec<Cow<'a, AstNodeId>>,
    pub raw_src_id: Cow<'a, str>,
    /// (relative_file_name, content_hash)
    pub raw_src_ids: Vec<(Cow<'a, str>, Cow<'a, str>)>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
#[serde(rename_all = "snake_case")]
pub struct SourceCode {
    pub file_name: String,
    pub content: String,
    pub content_hash: String,
}
