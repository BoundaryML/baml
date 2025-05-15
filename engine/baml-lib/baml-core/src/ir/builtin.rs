use baml_types::{
    expr::{Builtin, Expr, ExprMetadata},
    Arrow, FieldType,
};
use internal_baml_diagnostics::Span;

use super::repr::{Class, ExprFunction, Node, NodeAttributes};

pub mod functions {
    pub const FETCH_VALUE: &str = "std::fetch_value";
}

pub mod classes {
    pub const REQUEST: &str = "std::Request";
}

fn builtin<T, const N: usize>(elems: [T; N]) -> Vec<Node<T>> {
    let mut attributes = NodeAttributes::default();
    attributes.span = Some(Span::fake());

    Vec::from_iter(elems.into_iter().map(|e| Node {
        attributes: NodeAttributes::default(),
        elem: e,
    }))
}

pub fn builtin_classes() -> Vec<Node<Class>> {
    builtin([Class {
        name: String::from(functions::FETCH_VALUE),
        docstring: None,
        static_fields: vec![],
        inputs: vec![
            (String::from("base_url"), FieldType::string()),
            (
                String::from("headers"),
                FieldType::map(FieldType::string(), FieldType::string()),
            ),
            (
                String::from("query_params"),
                FieldType::map(FieldType::string(), FieldType::string()),
            ),
        ],
    }])
}

/// This builds a specialized version of an std generic function.
///
/// For now we only have functions that take in a generic type parameter and
/// return that same type, generics to not appear in function parameters. So
/// managing this is fairly simple, but will require carrying additional data
/// when actual user defined generics are introduced.
pub fn builtin_generic_fn(f: Builtin, return_type: FieldType) -> Expr<ExprMetadata> {
    let signature = match f {
        // fn fetch_value<T>(request: std::Request) -> T
        Builtin::FetchValue => Arrow {
            param_types: vec![FieldType::class(classes::REQUEST)],
            return_type,
        },
    };

    Expr::Builtin(f, (Span::fake(), Some(FieldType::arrow(signature))))
}

pub fn is_builtin_identifier(identifier: &str) -> bool {
    identifier.starts_with("std::")
}
