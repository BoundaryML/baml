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
    pub const REQUEST: &str = "std::request";
}

fn builtin<T, const N: usize>(elems: [T; N]) -> Vec<Node<T>> {
    let mut attributes = NodeAttributes::default();
    attributes.span = Some(Span::fake()); // TODO: Make spans optional in ExprMetadata

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
            // TODO: How do we add JSON type here? Builtin JSON type alias?
            // (String::from(
            //     "query_params",
            //     FieldType::map(FieldType::string(), FieldType::null()),
            // )),
        ],
    }])
}

pub fn builtin_generic_fn(f: Builtin, return_type: FieldType) -> Expr<ExprMetadata> {
    match f {
        Builtin::FetchValue => Expr::Builtin(
            Builtin::FetchValue,
            (
                Span::fake(),
                Some(FieldType::arrow(Arrow {
                    param_types: vec![FieldType::class(classes::REQUEST)],
                    return_type,
                })),
            ),
        ),
    }
}

pub fn is_builtin_identifier(name: &str) -> bool {
    name.starts_with("std::")
}
