use crate::internal_baml_parser_database::{ast, coerce};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};
use serde::Serialize;

/// Either an env var or a string literal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StringFromEnvVar {
    /// Contains the name of env var if the value was read from one.
    pub from_env_var: Option<String>,
    /// Contains the string literal, when it was directly in the parsed schema.
    pub value: Option<String>,
}

impl StringFromEnvVar {
    pub(crate) fn coerce(expr: &ast::Expression, diagnostics: &mut Diagnostics) -> Option<Self> {
        match expr {
            ast::Expression::ConstantValue(value, _) => {
                if value.starts_with("#ENV.") {
                    Some(StringFromEnvVar::new_from_env_var(
                        value.trim_start_matches("#ENV.").to_owned(),
                    ))
                } else {
                    Some(StringFromEnvVar::new_literal(value.clone()))
                }
            }
            ast::Expression::StringValue(value, _) => {
                Some(StringFromEnvVar::new_literal(value.clone()))
            }
            _ => {
                diagnostics.push_error(DatamodelError::new_type_mismatch_error(
                    "String",
                    expr.describe_value_type(),
                    &expr.to_string(),
                    expr.span().clone(),
                ));
                None
            }
        }
    }

    pub fn new_from_env_var(env_var_name: String) -> StringFromEnvVar {
        StringFromEnvVar {
            from_env_var: Some(env_var_name),
            value: None,
        }
    }

    pub fn new_literal(value: String) -> StringFromEnvVar {
        StringFromEnvVar {
            from_env_var: None,
            value: Some(value),
        }
    }

    /// Returns the name of the env var, if env var.
    pub fn as_env_var(&self) -> Option<&str> {
        self.from_env_var.as_deref()
    }

    /// Returns the contents of the string literal, if applicable.
    pub fn as_literal(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

struct EnvFunction {
    var_name: String,
}

impl EnvFunction {
    fn from_ast(expr: &ast::Expression, diagnostics: &mut Diagnostics) -> Option<EnvFunction> {
        let args = if let ast::Expression::ConstantValue(name, _) = &expr {
            if name.starts_with("#ENV.") {
                let parsed = name.trim_start_matches("#ENV.");
                parsed
            } else {
                diagnostics.push_error(DatamodelError::new_functional_evaluation_error(
                    "Expected this to be an env function.",
                    expr.span().clone(),
                ));
                return None;
            }
        } else {
            diagnostics.push_error(DatamodelError::new_functional_evaluation_error(
                "This is not a function expression but expected it to be one.",
                expr.span().clone(),
            ));
            return None;
        };

        Some(Self {
            var_name: args.to_string(),
        })
    }

    fn var_name(&self) -> &str {
        &self.var_name
    }
}
