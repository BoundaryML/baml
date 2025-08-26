use baml_types::ir_type::TypeIR;
use internal_baml_diagnostics::Span;

use crate::hir::{Class, Enum, EnumVariant, Field};

pub mod functions {
    pub const FETCH_VALUE: &str = "std::fetch_value";
}

pub mod classes {
    pub const REQUEST: &str = "std::Request";
}

pub mod enums {
    pub const HTTP_METHOD: &str = "std::HttpMethod";
}

pub fn builtin_classes() -> Vec<Class> {
    vec![Class {
        name: String::from(classes::REQUEST),
        methods: vec![],
        fields: vec![
            Field {
                name: String::from("base_url"),
                r#type: TypeIR::string(),
                span: Span::fake(),
            },
            Field {
                name: String::from("headers"),
                r#type: TypeIR::map(TypeIR::string(), TypeIR::string()),
                span: Span::fake(),
            },
            Field {
                name: String::from("query_params"),
                r#type: TypeIR::map(TypeIR::string(), TypeIR::string()),
                span: Span::fake(),
            },
        ],
        span: Span::fake(),
    }]
}

pub fn builtin_enums() -> Vec<Enum> {
    vec![Enum {
        name: String::from(enums::HTTP_METHOD),
        variants: vec![EnumVariant {
            name: String::from("Get"),
            span: Span::fake(),
        }],
        span: Span::fake(),
    }]
}

/// Create a type for the std::Request class
pub fn std_request_type() -> TypeIR {
    TypeIR::class(classes::REQUEST)
}

/// Create a function signature for std::fetch_value<T>
pub fn std_fetch_value_signature(return_type: TypeIR) -> TypeIR {
    TypeIR::arrow(vec![std_request_type()], return_type)
}

pub fn is_builtin_identifier(identifier: &str) -> bool {
    identifier.starts_with("std::") || identifier == "true" || identifier == "false"
}
