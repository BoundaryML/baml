use baml_types::{
    expr::{Builtin, Expr, ExprMetadata},
    Arrow, TypeIR,
};
use internal_baml_diagnostics::Span;

use super::repr::{Class, Enum, EnumValue, ExprFunction, Field, Node, NodeAttributes};
use crate::{ir::repr::IntermediateRepr, Configuration};

pub mod functions {
    pub const FETCH_AS: &str = "baml.fetch_as";
}

pub mod classes {
    pub const REQUEST: &str = "baml.HttpRequest";
}

pub mod enums {
    pub const HTTP_METHOD: &str = "baml.HttpMethod";
}

/// Builtins are exposed through a separate IR, which can be combined with
/// the user's IR via `IntermediateRepr::extend`.
pub fn builtin_ir() -> IntermediateRepr {
    IntermediateRepr {
        enums: builtin_enums(),
        classes: builtin_classes(),
        type_aliases: vec![],
        functions: vec![],
        expr_fns: vec![],
        toplevel_assignments: vec![],
        clients: vec![],
        retry_policies: vec![],
        template_strings: vec![],
        finite_recursive_cycles: vec![],
        structural_recursive_alias_cycles: vec![],
        configuration: Configuration::default(),
        pass2_repr: Default::default(),
    }
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
        name: String::from(classes::REQUEST),
        docstring: None,
        static_fields: vec![
            // Node {
            //     attributes: NodeAttributes::default(),
            //     elem: Field {
            //         name: String::from("method"),
            //         r#type: Node {
            //             elem: FieldType::r#enum(enums::HTTP_METHOD),
            //             attributes: NodeAttributes::default(),
            //         },
            //         docstring: None,
            //     },
            // },
            Node {
                attributes: NodeAttributes::default(),
                elem: Field {
                    name: String::from("base_url"),
                    r#type: Node {
                        elem: TypeIR::string(),
                        attributes: NodeAttributes::default(),
                    },
                    docstring: None,
                },
            },
            Node {
                attributes: NodeAttributes::default(),
                elem: Field {
                    name: String::from("headers"),
                    r#type: Node {
                        elem: TypeIR::map(TypeIR::string(), TypeIR::string()),
                        attributes: NodeAttributes::default(),
                    },
                    docstring: None,
                },
            },
            Node {
                attributes: NodeAttributes::default(),
                elem: Field {
                    name: String::from("query_params"),
                    r#type: Node {
                        elem: TypeIR::map(TypeIR::string(), TypeIR::string()),
                        attributes: NodeAttributes::default(),
                    },
                    docstring: None,
                },
            },
        ],
        inputs: vec![],
    }])
}

pub fn builtin_enums() -> Vec<Node<Enum>> {
    builtin([Enum {
        name: String::from(enums::HTTP_METHOD),
        docstring: None,
        values: vec![(
            Node {
                attributes: NodeAttributes::default(),
                elem: EnumValue(String::from("Get")),
            },
            None,
        )],
    }])
}

/// This builds a specialized version of an std generic function.
///
/// For now we only have functions that take in a generic type parameter and
/// return that same type, generics do not appear in function parameters. So
/// managing this is fairly simple, but will require carrying additional data
/// when actual user defined generics are introduced.
pub fn builtin_generic_fn(f: Builtin, return_type: TypeIR) -> Expr<ExprMetadata> {
    let signature = match f {
        // fn fetch_value<T>(request: baml.HttpRequest) -> T
        Builtin::FetchValue => TypeIR::arrow(vec![TypeIR::class(classes::REQUEST)], return_type),
    };

    Expr::Builtin(f, (Span::fake(), Some(signature)))
}

pub fn is_builtin_identifier(identifier: &str) -> bool {
    identifier.starts_with("std::")
}
