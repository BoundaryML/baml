use baml_types::ir_type::{TypeIR, UnionConstructor};
use internal_baml_diagnostics::Span;

use crate::hir::{Class, Enum, EnumVariant, Field};

pub mod functions {
    pub const FETCH_AS: &str = "baml.fetch_as";
}

pub mod classes {
    pub const HTTP_REQUEST: &str = "baml.HttpRequest";
}

pub mod enums {
    pub const HTTP_METHOD: &str = "baml.HttpMethod";
}

pub fn builtin_classes() -> Vec<Class> {
    vec![Class {
        name: String::from(classes::HTTP_REQUEST),
        methods: vec![],
        fields: vec![
            Field {
                name: String::from("url"),
                r#type: TypeIR::string(),
                span: Span::fake(),
            },
            Field {
                name: String::from("method"),
                r#type: TypeIR::r#enum(enums::HTTP_METHOD),
                span: Span::fake(),
            },
            Field {
                name: String::from("headers"),
                r#type: TypeIR::optional(TypeIR::map(TypeIR::string(), TypeIR::string())),
                span: Span::fake(),
            },
            Field {
                name: String::from("query_params"),
                r#type: TypeIR::optional(TypeIR::map(TypeIR::string(), TypeIR::string())),
                span: Span::fake(),
            },
            Field {
                name: String::from("json"),
                r#type: TypeIR::optional(TypeIR::Top(Default::default())), // generic T
                span: Span::fake(),
            },
        ],
        span: Span::fake(),
    }]
}

pub fn builtin_enums() -> Vec<Enum> {
    vec![Enum {
        name: String::from(enums::HTTP_METHOD),
        variants: vec![
            EnumVariant {
                name: String::from("Get"),
                span: Span::fake(),
            },
            EnumVariant {
                name: String::from("Post"),
                span: Span::fake(),
            },
            EnumVariant {
                name: String::from("Put"),
                span: Span::fake(),
            },
            EnumVariant {
                name: String::from("Delete"),
                span: Span::fake(),
            },
            EnumVariant {
                name: String::from("Patch"),
                span: Span::fake(),
            },
        ],
        span: Span::fake(),
    }]
}

/// Create a type for the std::Request class
pub fn baml_request_type() -> TypeIR {
    TypeIR::class(classes::HTTP_REQUEST)
}

pub fn baml_http_method_type() -> TypeIR {
    TypeIR::r#enum(enums::HTTP_METHOD)
}

/// Create a function signature for std::fetch_value<T>
pub fn baml_fetch_as_signature(return_type: TypeIR) -> TypeIR {
    TypeIR::arrow(
        vec![TypeIR::union(vec![
            TypeIR::string(),
            TypeIR::class(classes::HTTP_REQUEST),
        ])],
        return_type,
    )
}
