/*
 * EvaluationContext is used to evaluate a function call with context-specific information.
 *
 * For example, client_registry and type_builder
 */

use serde::{Deserialize, Serialize};

use super::type_definition::TypeDefinition;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TypeBuilderValue {
    pub types: Vec<TypeDefinition>,
}
