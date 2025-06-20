/*
 * EvaluationContext is used to evaluate a function call with context-specific information.
 *
 * For example, client_registry and type_builder
 */

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::type_reference::TypeReference;
use crate::ast::ast_node_id::AstNodeId;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, Clone, TS)]
#[ts(export)]
pub struct BamlTypeId(pub AstNodeId);

/// FieldType represents the type of either a class field or a function arg.
#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Hash)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TypeDefinition {
    // User-defined types
    Enum {
        type_id: BamlTypeId,
        // Order matters!
        values: Vec<String>,
        source: TypeDefinitionSource,
        dependencies: Vec<AstNodeId>,
    },
    Class {
        type_id: BamlTypeId,
        // Order matters!
        fields: Vec<NamedType>,
        source: TypeDefinitionSource,
        dependencies: Vec<AstNodeId>,
    },
    Alias {
        type_id: BamlTypeId,
        rhs: TypeReference,
    },
}

impl TypeDefinition {
    pub fn id(&self) -> &BamlTypeId {
        match self {
            TypeDefinition::Enum { type_id: name, .. } => name,
            TypeDefinition::Class { type_id: name, .. } => name,
            TypeDefinition::Alias { type_id: name, .. } => name,
        }
    }
}
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash, TS)]
#[serde(rename_all = "snake_case")]
pub struct NamedType {
    pub name: String,
    pub type_ref: TypeReference,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, PartialEq, Eq, Hash, TS)]
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
