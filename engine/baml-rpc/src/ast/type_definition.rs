/*
 * EvaluationContext is used to evaluate a function call with context-specific information.
 *
 * For example, client_registry and type_builder
 */

use serde::{Deserialize, Serialize};

use super::type_reference::TypeReference;
use crate::ast::ast_node_id::AstNodeId;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone)]
pub struct BamlTypeId(pub AstNodeId);

/// FieldType represents the type of either a class field or a function arg.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TypeDefinition {
    // User-defined types
    Enum {
        name: BamlTypeId,
        // Order matters!
        values: Vec<String>,
        source: TypeDefinitionSource,
        dependencies: Vec<AstNodeId>,
    },
    Class {
        name: BamlTypeId,
        // Order matters!
        fields: Vec<NamedType>,
        source: TypeDefinitionSource,
        dependencies: Vec<AstNodeId>,
    },
    Alias {
        name: BamlTypeId,
        rhs: TypeReference,
    },
}

impl TypeDefinition {
    pub fn id(&self) -> &BamlTypeId {
        match self {
            TypeDefinition::Enum { name, .. } => name,
            TypeDefinition::Class { name, .. } => name,
            TypeDefinition::Alias { name, .. } => name,
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub struct NamedType {
    pub name: String,
    pub r#type: TypeReference,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
/// Whether the type definition is buildable or pure dynamic.
pub enum TypeDefinitionSource {
    /// Defined statically, and cannot be extended.
    CompileTime,
    /// Defined statically, but modifiable via TypeBuilder.
    /// (add/remove fields, change field types, alias, description, etc.)
    Buildable,
    /// Defined ONLY within a TypeBuilder.
    PureBuildable,
}
