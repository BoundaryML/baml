mod expr;
mod pretty_print;
mod stmt;
mod test_expr;
mod test_stmt;
mod types;

use std::{collections::HashMap, fmt::Debug};

use minijinja::machinery::{
    ast::{self, Expr},
    Span,
};

use pretty_print::pretty_print;

use self::types::{PredefinedTypes, Type};

#[derive(Debug, Clone)]
struct TypeError {
    message: String,
    span: Span,
}

impl TypeError {
    fn new_unresolved_variable(name: &str, span: Span) -> Self {
        Self {
            message: format!("Variable '{}' is not defined", name),
            span,
        }
    }

    fn new_wrong_arg_type(
        func: &str,
        span: Span,
        name: &str,
        arg_span: Span,
        expected: Type,
        got: Type,
    ) -> Self {
        Self {
            message: format!(
                "Function '{}' expects argument '{}' to be of type {}, but got {}",
                func,
                name,
                expected.name(),
                got.name()
            ),
            span,
        }
    }

    fn new_missing_arg(func: &str, span: Span, name: &str) -> Self {
        Self {
            message: format!("Function '{}' expects argument '{}'", func, name),
            span,
        }
    }

    fn new_wrong_arg_count(func: &str, span: Span, expected: usize, got: usize) -> Self {
        Self {
            message: format!(
                "Function '{}' expects {} arguments, but got {}",
                func, expected, got
            ),
            span,
        }
    }

    fn new_unknown_arg(func: &str, span: Span, name: &str) -> Self {
        Self {
            message: format!("Function '{}' does not have an argument '{}'", func, name),
            span,
        }
    }

    fn new_invalid_type(expr: &Expr, expected: &str, span: Span) -> Self {
        Self {
            message: format!(
                "'{}' is not a {}",
                pretty_print::pretty_print(expr),
                expected
            ),
            span,
        }
    }

    fn new_dot_operator_not_supported(
        name: &str,
        r#type: &Type,
        property: &str,
        span: Span,
    ) -> Self {
        Self {
            message: format!(
                "'{}' ({}) does not have a property '{}'",
                name,
                r#type.name(),
                property
            ),
            span,
        }
    }

    fn new_property_not_defined(
        variable_name: &str,
        class_name: &str,
        property: &str,
        span: Span,
    ) -> Self {
        Self {
            message: format!(
                "class {} ({}) does not have a property '{}'",
                class_name, variable_name, property
            ),
            span,
        }
    }

    fn new_class_not_defined(class: &str) -> Self {
        Self {
            message: format!("Class '{}' is not defined", class),
            span: Span::default(),
        }
    }
}

struct ScopeTracker<'a> {
    variable_types: HashMap<&'a str, Type>,
    errors: Vec<TypeError>,
}

impl<'a> ScopeTracker<'a> {
    fn new() -> Self {
        Self {
            variable_types: HashMap::new(),
            errors: Vec::new(),
        }
    }
}
