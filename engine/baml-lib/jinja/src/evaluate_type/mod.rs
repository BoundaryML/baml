mod expr;
mod pretty_print;
mod stmt;
mod test_expr;
mod test_stmt;
mod types;

use std::fmt::Debug;
use std::ops::Index;

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

fn sort_by_match<'a, I, T>(name: &str, options: &'a I, max_return: Option<usize>) -> Vec<&'a str>
where
    I: Index<usize, Output = T> + 'a,
    &'a I: IntoIterator<Item = &'a T>,
    T: AsRef<str> + 'a,
{
    // The maximum allowed distance for a string to be considered similar.
    const THRESHOLD: usize = 20;

    // Calculate distances and sort names by distance
    let mut name_distances = options
        .into_iter()
        .enumerate()
        .map(|(idx, n)| {
            (
                // Case insensitive comparison
                strsim::osa_distance(&n.as_ref().to_lowercase(), &name.to_lowercase()),
                idx,
            )
        })
        .collect::<Vec<_>>();

    name_distances.sort_by_key(|k| k.0);

    // Filter names based on the threshold
    let filtered_names = name_distances
        .iter()
        .filter(|&&(dist, _)| dist <= THRESHOLD)
        .map(|&(_, idx)| options.index(idx).as_ref());

    // Return either a limited or full set of filtered names
    match max_return {
        Some(max) => filtered_names.take(max).collect(),
        None => filtered_names.collect(),
    }
}

impl TypeError {
    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn span(&self) -> Span {
        self.span
    }

    fn new_unresolved_variable(name: &str, span: Span, options: Vec<String>) -> Self {
        let mut close_names = sort_by_match(name, &options, Some(3));
        close_names.sort();
        let close_names = close_names;

        let message = if close_names.is_empty() {
            // If no names are close enough, suggest nothing or provide a generic message
            format!("Variable `{}` does not exist.", name)
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(
                "Variable `{}` does not exist. Did you mean `{}`?",
                name, close_names[0]
            )
        } else {
            // If there are multiple close names, suggest them all
            let suggestions = close_names.join("`, `");
            format!(
                "Variable `{}` does not exist. Did you mean one of these: `{}`?",
                name, suggestions
            )
        };

        Self { message, span }
    }

    fn new_wrong_arg_type(
        func: &str,
        span: Span,
        name: &str,
        _arg_span: Span,
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
                "'{}' is {}, expected {}",
                pretty_print::pretty_print(expr),
                if *got == Type::Undefined {
                    "undefined".to_string()
                } else {
                    format!("a {}", got.name())
                },
                expected
            ),
            span,
        }
    }

    #[allow(dead_code)]
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

struct ScopeTracker {
    errors: Vec<TypeError>,
}

impl ScopeTracker {
    fn new() -> Self {
        Self { errors: Vec::new() }
    }
}
