//! Function signatures (parameters, return types, generics).
//!
//! Separated from `ItemTree` to provide fine-grained incrementality.
//! Signature changes invalidate type checking, but not name resolution.
//! Spans are stored separately in `SignatureSourceMap` for incrementality.

use std::sync::Arc;

use rowan::ast::AstNode;

use crate::{Name, SignatureSourceMap, type_ref::TypeRef};

/// The signature of a function (everything except the body).
///
/// Position-independent: spans are stored in `SignatureSourceMap`.
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
    ///
    /// Returns both the signature (position-independent) and its source map (spans).
    pub fn lower(
        func_node: &baml_compiler_syntax::ast::FunctionDef,
    ) -> (Arc<FunctionSignature>, SignatureSourceMap) {
        let name = func_node
            .name()
            .map(|n| Name::new(n.text()))
            .unwrap_or_else(|| Name::new("UnnamedFunction"));

        let mut source_map = SignatureSourceMap::new();

        // Extract parameters
        let mut params = Vec::new();
        if let Some(param_list) = func_node.param_list() {
            for param_node in param_list.params() {
                if let Some(name_token) = param_node.name() {
                    let type_ref = param_node
                        .ty()
                        .map(|t| TypeRef::from_ast(&t))
                        .unwrap_or(TypeRef::Unknown);

                    // Store the span in the source map
                    source_map.push_param_span(Some(param_node.syntax().text_range()));

                    params.push(Param {
                        name: Name::new(name_token.text()),
                        type_ref,
                    });
                }
            }
        }

        // Extract return type and its span
        let return_type_node = func_node.return_type();
        let return_type = return_type_node
            .as_ref()
            .map(TypeRef::from_ast)
            .unwrap_or(TypeRef::Unknown);

        // Store return type span in source map
        if let Some(span) = return_type_node.map(|t| t.text_range()) {
            source_map.set_return_type_span(span);
        }

        (
            Arc::new(FunctionSignature {
                name,
                params,
                return_type,
            }),
            source_map,
        )
    }
}
