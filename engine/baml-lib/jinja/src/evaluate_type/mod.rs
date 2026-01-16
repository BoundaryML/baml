mod expr;
mod pretty_print;
mod stmt;
#[cfg(test)]
mod test_expr;
#[cfg(test)]
mod test_stmt;
mod types;

use std::{collections::HashSet, fmt::Debug, ops::Index};

use indexmap::{IndexMap, IndexSet};
use minijinja::machinery::{ast::Expr, Span};

pub use self::{
    expr::evaluate_type,
    stmt::get_variable_types,
    types::{EnumDefinition, EnumValueDefinition, JinjaContext, PredefinedTypes, Type},
};

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
            format!("Variable `{name}` does not exist.")
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(
                "Variable `{}` does not exist. Did you mean `{}`?",
                name, close_names[0]
            )
        } else {
            // If there are multiple close names, suggest them all
            let suggestions = close_names.join("`, `");
            format!("Variable `{name}` does not exist. Did you mean one of these: `{suggestions}`?")
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
            message: format!("Function '{func}' expects argument '{name}'"),
            span,
        }
    }

    fn new_wrong_arg_count(func: &str, span: Span, expected: usize, got: usize) -> Self {
        Self {
            message: format!("Function '{func}' expects {expected} arguments, but got {got}"),
            span,
        }
    }

    // TODO: There's a bug with the suggestions, they are not consistent due to
    // either some ordering issue or closest match algorithm does weird stuff
    // and returns results non-deterministically. See commented test in
    // baml-lib/jinja/src/evaluate_type/test_expr.rs
    // NB(sam): this is probably because valid_args is a HashSet, not an IndexSet
    fn new_unknown_arg(func: &str, span: Span, name: &str, valid_args: HashSet<&String>) -> Self {
        let names = valid_args.into_iter().collect::<Vec<_>>();
        let mut close_names = sort_by_match(name, &names, Some(3));
        close_names.sort();
        let close_names = close_names;

        let message = if close_names.is_empty() {
            // If no names are close enough, suggest nothing or provide a generic message
            format!("Function '{func}' does not have an argument '{name}'.")
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(
                "Function '{}' does not have an argument '{}'. Did you mean '{}'?",
                func, name, close_names[0]
            )
        } else {
            // If there are multiple close names, suggest them all
            let suggestions = close_names.join("', '");
            format!(
                "Function '{func}' does not have an argument '{name}'. Did you mean one of these: '{suggestions}'?"
            )
        };

        Self { message, span }
    }

    fn new_invalid_filter(name: &str, span: Span, valid_filters: &Vec<&str>) -> Self {
        let mut close_names = sort_by_match(name, valid_filters, Some(5));
        close_names.sort();
        let close_names = close_names;

        let message = if close_names.is_empty() {
            // If no names are close enough, suggest nothing or provide a generic message
            format!("Filter '{name}' does not exist")
        } else if close_names.len() == 1 {
            // If there's only one close name, suggest it
            format!(
                "Filter '{}' does not exist. Did you mean '{}'?",
                name, close_names[0]
            )
        } else {
            // If there are multiple close names, suggest them all
            let suggestions = close_names.join("', '");
            format!("Filter '{name}' does not exist. Did you mean one of these: '{suggestions}'?")
        };

        Self { message: format!("{message}\n\nSee: https://docs.rs/minijinja/latest/minijinja/filters/index.html#functions for the compelete list"), span }
    }

    pub fn new_function_reference_without_call(func: &str, span: Span) -> Self {
        Self {
            message: format!(
                "Function '{func}' referenced without parentheses. Did you mean '{func}()'?"
            ),
            span,
        }
    }

    fn new_enum_literal_suggestion(
        expr: &Expr,
        enum_name: &str,
        literal_value: &str,
        types: &types::PredefinedTypes,
        span: Span,
    ) -> Self {
        let enum_def = match types.as_enum(enum_name) {
            Some(def) => def,
            None => return Self::new_enum_string_cmp_deprecated(expr, enum_name, span),
        };

        // 1. EXACT VALUE NAME MATCH
        if enum_def.values.iter().any(|v| v.name == literal_value) {
            return Self {
                message: format!(
                    "Use `{enum_name}.{literal_value}` instead of \"{literal_value}\" - comparing enums with strings will soon be deprecated."
                ),
                span,
            };
        }

        // 2. CASE-INSENSITIVE VALUE NAME MATCH
        if let Some(correct_case) = enum_def
            .values
            .iter()
            .find(|v| v.name.to_lowercase() == literal_value.to_lowercase())
        {
            return Self {
                message: format!(
                    "Use `{}.{}` instead of \"{}\" - comparing enums with strings will soon be deprecated.",
                    enum_name, correct_case.name, literal_value
                ),
                span,
            };
        }

        // 3. EXACT ALIAS MATCH
        if let Some(value_for_alias) = enum_def
            .values
            .iter()
            .find(|v| v.alias.as_ref() == Some(&literal_value.to_string()))
        {
            return Self {
                message: format!(
                    "Did you mean `{}.{}` instead of \"{}\" (alias)? Enums are not equal to their alias values.",
                    enum_name, value_for_alias.name, literal_value
                ),
                span,
            };
        }

        // 4. CASE-INSENSITIVE ALIAS MATCH
        if let Some(value_for_alias) = enum_def.values.iter().find(|v| {
            v.alias.as_ref().map(|a| a.to_lowercase()) == Some(literal_value.to_lowercase())
        }) {
            return Self {
                message: format!(
                    "Did you mean `{}.{}` instead of \"{}\" (alias)? Enums are not equal to their alias values.",
                    enum_name, value_for_alias.name, literal_value
                ),
                span,
            };
        }

        // 5. FUZZY MATCH using existing sort_by_match function
        let mut all_searchable_terms = Vec::new();
        let mut term_to_value_name = IndexMap::new();
        for value in &enum_def.values {
            all_searchable_terms.push(value.name.clone());
            term_to_value_name.insert(value.name.clone(), value.name.clone());

            if let Some(alias) = &value.alias {
                all_searchable_terms.push(alias.clone());
                term_to_value_name.insert(alias.clone(), value.name.clone());
            }
        }

        let close_matches = sort_by_match(literal_value, &all_searchable_terms, Some(3));
        if !close_matches.is_empty() {
            let unique_values: IndexSet<_> = close_matches
                .iter()
                .filter_map(|term| term_to_value_name.get(*term))
                .collect();
            let suggestions: Vec<_> = unique_values
                .iter()
                .map(|v| format!("{enum_name}.{v}"))
                .collect();

            return Self {
                message: if suggestions.len() == 1 {
                    format!(
                        "Use `{}` instead of \"{}\" - comparing enums with strings will soon be deprecated.",
                        suggestions[0], literal_value
                    )
                } else {
                    format!(
                        "Use one of: {} - comparing enums with strings will soon be deprecated.",
                        suggestions.join(", ")
                    )
                },
                span,
            };
        }

        // 6. FALLBACK: Show all available values
        Self {
            message: format!(
                "Use one of: {} - comparing enums with strings will soon be deprecated.",
                enum_def
                    .values
                    .iter()
                    .map(|v| format!("{}.{}", enum_name, v.name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            span,
        }
    }

    fn new_enum_string_cmp_deprecated(_expr: &Expr, enum_name: &str, span: Span) -> Self {
        Self {
            message: format!(
                "Comparing enum {enum_name} to string variable - enum-string comparisons will soon be deprecated. Please see https://github.com/BoundaryML/baml/issues/2339."
            ),
            span,
        }
    }

    fn new_enum_value_property_error(
        variable_name: &str,
        enum_value: &str,
        property: &str,
        span: Span,
    ) -> Self {
        Self {
            message: format!(
                "enum value {enum_value} ({variable_name}) does not have a property '{property}'"
            ),
            span,
        }
    }

    fn new_invalid_type(expr: &Expr, got: &Type, expected: &str, span: Span) -> Self {
        Self {
            message: format!(
                "'{}' is {}, expected {}",
                pretty_print::pretty_print(expr),
                if got.is_subtype_of(&Type::Undefined) {
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
                "class {class_name} ({variable_name}) does not have a property '{property}'"
            ),
            span,
        }
    }

    fn new_class_not_defined(class: &str) -> Self {
        Self {
            message: format!("Class '{class}' is not defined"),
            span: Span::default(),
        }
    }

    fn new_enum_not_defined(class: &str) -> Self {
        Self {
            message: format!("Class '{class}' is not defined"),
            span: Span::default(),
        }
    }

    fn new_property_not_found_in_union(
        _variable_name: &str,
        property: &str,
        missing_on_classes: &[&str],
        union_name: Option<&str>,
        span: Span,
    ) -> Self {
        let classes_str = missing_on_classes.join(", ");
        let message = match union_name {
            Some(name) => format!(
                "property '{property}' does not exist on {classes_str} in type alias {name}"
            ),
            None => format!("property '{property}' does not exist on {classes_str}"),
        };
        Self { message, span }
    }

    fn new_property_type_mismatch_in_union(
        _variable_name: &str,
        property: &str,
        union_name: Option<&str>,
        span: Span,
    ) -> Self {
        let message = match union_name {
            Some(name) => format!(
                "property '{property}' has inconsistent types across classes in type alias {name}"
            ),
            None => format!("property '{property}' has inconsistent types across union members"),
        };
        Self { message, span }
    }

    fn new_non_class_in_union(
        variable_name: &str,
        property: &str,
        non_class_type: &str,
        span: Span,
    ) -> Self {
        Self {
            message: format!(
                "cannot access property '{property}' on '{variable_name}': union contains non-class type {non_class_type}"
            ),
            span,
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
