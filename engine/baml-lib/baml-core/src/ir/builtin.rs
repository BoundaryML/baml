use baml_types::FieldType;
use internal_baml_diagnostics::Span;

use super::repr::{Class, ExprFunction, Node, NodeAttributes};

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
        name: String::from("std::request"),
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

pub fn builtin_functions() -> Vec<Node<ExprFunction>> {
    builtin([ExprFunction {
        name: String::from("std::fetch_value"),
        inputs: vec![(String::from("request"), FieldType::class("std::request"))],
        output: FieldType::null(),
        tests: vec![],
        expr: todo!(),
    }])
}
