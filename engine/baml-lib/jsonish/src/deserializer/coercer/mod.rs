mod array_helper;
mod coerce_array;
mod coerce_optional;
mod coerce_primitive;
mod coerce_union;
mod field_type;
mod ir_ref;
use anyhow::Result;
use std::collections::HashMap;

use internal_baml_core::ir::{repr::IntermediateRepr, FieldType};

use super::types::BamlValueWithFlags;

pub struct ParsingContext<'a> {
    scope: Vec<String>,
    env: &'a HashMap<String, String>,
    ir: &'a IntermediateRepr,
}

impl ParsingContext<'_> {
    pub(crate) fn new<'a>(
        ir: &'a IntermediateRepr,
        env: &'a HashMap<String, String>,
    ) -> ParsingContext<'a> {
        ParsingContext {
            scope: Vec::new(),
            env,
            ir,
        }
    }

    pub(crate) fn enter_scope(&self, scope: &str) -> ParsingContext {
        let mut new_scope = self.scope.clone();
        new_scope.push(scope.to_string());
        ParsingContext {
            scope: new_scope,
            env: self.env,
            ir: self.ir,
        }
    }

    pub(crate) fn error_too_many_matches<T: std::fmt::Display>(
        &self,
        target: &FieldType,
        options: impl IntoIterator<Item = T>,
    ) -> ParsingError {
        ParsingError {
            reason: format!(
                "Too many matches for {}. Got: {}",
                target,
                options.into_iter().fold("".to_string(), |acc, f| {
                    if acc.is_empty() {
                        return f.to_string();
                    }
                    return format!("{}, {}", acc, f);
                })
            ),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_merge_multiple<'a>(
        &self,
        summary: &str,
        error: impl IntoIterator<Item = &'a ParsingError>,
    ) -> ParsingError {
        let reasons = error
            .into_iter()
            .map(|e| {
                // Strip all shared prefixes (assume the same unless different length)
                let remaining =
                    e.scope
                        .iter()
                        .skip(self.scope.len())
                        .fold("".to_string(), |acc, f| {
                            if acc.is_empty() {
                                return f.clone();
                            }
                            return format!("{}.{}", acc, f);
                        });

                if remaining.is_empty() {
                    return e.reason.clone();
                } else {
                    // Prefix each new lines in e.reason with "  "
                    return format!("{}: {}", remaining, e.reason.replace("\n", "\n  "));
                }
            })
            .collect::<Vec<_>>();

        ParsingError {
            reason: format!("{}:\n{}", summary, reasons.join("\n").replace("\n", "\n  ")),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_unexpected_empty_array(&self, target: &FieldType) -> ParsingError {
        ParsingError {
            reason: format!("Expected {}, got empty array", target.to_string()),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_unexpected_null(&self, target: &FieldType) -> ParsingError {
        ParsingError {
            reason: format!("Expected {}, got null", target),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_image_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Image type is not supported here".to_string(),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_missing_required_field<T: AsRef<str>>(
        &self,
        fields: &[T],
        item: Option<&crate::jsonish::Value>,
    ) -> ParsingError {
        ParsingError {
            reason: format!(
                "Missing required fields: {}\nGot: {:#?}",
                fields
                    .iter()
                    .map(|c| c.as_ref())
                    .collect::<Vec<_>>()
                    .join(", "),
                item
            ),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_unexpected_type(
        &self,
        target: &FieldType,
        got: &crate::jsonish::Value,
    ) -> ParsingError {
        let type_of = got.r#type();

        ParsingError {
            reason: format!("Expected {}, got {:#?}.\n{}", target, type_of, got),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_internal<T: std::fmt::Display>(&self, error: T) -> ParsingError {
        ParsingError {
            reason: format!("Internal error: {}", error),
            scope: self.scope.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsingError {
    reason: String,
    scope: Vec<String>,
}

impl std::fmt::Display for ParsingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.scope.is_empty() {
            return write!(f, "Error parsing '<root>': {}", self.reason);
        }
        write!(
            f,
            "Error parsing '{}': {}",
            self.scope.join("."),
            self.reason
        )
    }
}

impl std::error::Error for ParsingError {}

pub trait TypeCoercer {
    fn coerce(
        &self,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError>;
}
