mod expr;
mod pretty_print;
mod stmt;
mod test_expr;
mod test_stmt;
mod types;

use std::{collections::HashMap, fmt::Debug};

use minijinja::machinery::{ast::Expr, Span};

pub use self::types::Type;

pub use self::types::PredefinedTypes;

pub use self::stmt::get_variable_types;

#[derive(Debug, Clone)]
pub struct TypeError {
    message: String,
    span: Span,
}

// Implementing the Display trait for TypeError.
impl std::fmt::Display for TypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{} at {:?}", self.message, self.span)
    }
}

// Implementing the Error trait for TypeError.
impl std::error::Error for TypeError {}

impl TypeError {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn span(&self) -> Span {
        self.span
    }

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

    fn new_invalid_type(expr: &Expr, got: &Type, expected: &str, span: Span) -> Self {
        Self {
            message: format!(
                "'{}' is a {}, expected {}",
                pretty_print::pretty_print(expr),
                got.name(),
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
