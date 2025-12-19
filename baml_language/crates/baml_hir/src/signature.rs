//! Function signatures (parameters, return types, generics).
//!
//! Separated from `ItemTree` to provide fine-grained incrementality.
//! Signature changes invalidate type checking, but not name resolution.

use std::sync::Arc;

use crate::{Name, type_ref::TypeRef};

/// The signature of a function (everything except the body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    /// Function name (duplicated from `ItemTree` for convenience)
    pub name: Name,

    /// Function parameters
    pub params: Vec<Param>,

    /// Return type
    pub return_type: TypeRef,
}

/// Function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub type_ref: TypeRef,
}

impl FunctionSignature {
    /// Lower a function signature from CST.
    pub fn lower(func_node: &baml_syntax::ast::FunctionDef) -> Arc<FunctionSignature> {
        let name = func_node
            .name()
            .map(|n| Name::new(n.text()))
            .unwrap_or_else(|| Name::new("UnnamedFunction"));

        // Extract parameters
        let mut params = Vec::new();
        if let Some(param_list) = func_node.param_list() {
            for param_node in param_list.params() {
                if let Some(name_token) = param_node.name() {
                    let type_ref = param_node
                        .ty()
                        .map(|t| TypeRef::from_ast(&t))
                        .unwrap_or(TypeRef::Unknown);

                    params.push(Param {
                        name: Name::new(name_token.text()),
                        type_ref,
                    });
                }
            }
        }

        // Extract return type
        let return_type = func_node
            .return_type()
            .map(|t| TypeRef::from_ast(&t))
            .unwrap_or(TypeRef::Unknown);

        Arc::new(FunctionSignature {
            name,
            params,
            return_type,
        })
    }
}
