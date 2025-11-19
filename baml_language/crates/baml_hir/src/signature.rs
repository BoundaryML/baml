//! Function signatures (parameters, return types, generics).
//!
//! Separated from `ItemTree` to provide fine-grained incrementality.
//! Signature changes invalidate type checking, but not name resolution.

use crate::{Name, type_ref::TypeRef};
use rowan::ast::AstNode;
use std::sync::Arc;

/// The signature of a function (everything except the body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionSignature {
    /// Function name (duplicated from `ItemTree` for convenience)
    pub name: Name,

    /// Function parameters
    pub params: Vec<Param>,

    /// Return type
    pub return_type: TypeRef,

    /// Attributes/modifiers
    pub attrs: FunctionAttributes,
    // Note: Generic parameters are queried separately via generic_params()
    // for incrementality - changes to generics don't invalidate signatures
}

/// Function parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: Name,
    pub type_ref: TypeRef,
}

/// Function attributes and modifiers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FunctionAttributes {
    /// Whether this is a streaming function
    pub is_streaming: bool,

    /// Whether this is async
    pub is_async: bool,

    /// Custom attributes (@@retry, @@cache, etc.)
    pub custom_attrs: Vec<CustomAttribute>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomAttribute {
    pub name: Name,
    pub args: Vec<String>, // Simplified for now
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
                        .map(|t| lower_type_ref(&t))
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
            .map(|t| lower_type_ref(&t))
            .unwrap_or(TypeRef::Unknown);

        // Extract attributes (future work)
        let attrs = FunctionAttributes::default();

        Arc::new(FunctionSignature {
            name,
            params,
            return_type,
            attrs,
        })
    }
}

/// Lower a type reference from CST.
fn lower_type_ref(node: &baml_syntax::ast::TypeExpr) -> TypeRef {
    let text = node.syntax().text().to_string();
    let text = text.trim();

    match text {
        "int" => TypeRef::Int,
        "float" => TypeRef::Float,
        "string" => TypeRef::String,
        "bool" => TypeRef::Bool,
        "null" => TypeRef::Null,
        "image" => TypeRef::Image,
        "audio" => TypeRef::Audio,
        "video" => TypeRef::Video,
        "pdf" => TypeRef::Pdf,
        _ => {
            if let Some(inner_text) = text.strip_suffix('?') {
                let inner = TypeRef::named(inner_text.into());
                TypeRef::optional(inner)
            } else if let Some(inner_text) = text.strip_suffix("[]") {
                let inner = TypeRef::named(inner_text.into());
                TypeRef::list(inner)
            } else {
                TypeRef::named(text.into())
            }
        }
    }
}
