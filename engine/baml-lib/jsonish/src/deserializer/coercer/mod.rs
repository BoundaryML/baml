mod array_helper;
mod coerce_array;
mod coerce_map;
mod coerce_optional;
mod coerce_primitive;
mod coerce_union;
mod field_type;
mod ir_ref;
use anyhow::Result;
use baml_types::{BamlValue, Constraint, ConstraintFailure, ConstraintLevel, ConstraintsResult};
use internal_baml_jinja::{evaluate_predicate, types::OutputFormatContent};

use internal_baml_core::ir::{Expression, FieldType};

use crate::deserializer::deserialize_flags::Flag;

use super::types::BamlValueWithFlags;

pub struct ParsingContext<'a> {
    scope: Vec<String>,
    of: &'a OutputFormatContent,
    allow_partials: bool,
}

impl ParsingContext<'_> {
    pub fn display_scope(&self) -> String {
        if self.scope.is_empty() {
            return "<root>".to_string();
        }
        self.scope.join(".")
    }

    pub(crate) fn new<'a>(of: &'a OutputFormatContent, allow_partials: bool) -> ParsingContext<'a> {
        ParsingContext {
            scope: Vec::new(),
            of,
            allow_partials,
        }
    }

    pub(crate) fn enter_scope(&self, scope: &str) -> ParsingContext {
        let mut new_scope = self.scope.clone();
        new_scope.push(scope.to_string());
        ParsingContext {
            scope: new_scope,
            of: self.of,
            allow_partials: self.allow_partials,
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

    pub(crate) fn error_audio_not_supported(&self) -> ParsingError {
        ParsingError {
            reason: "Audio type is not supported here".to_string(),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_map_must_have_string_key(&self, key_type: &FieldType) -> ParsingError {
        ParsingError {
            reason: format!("Maps may only have strings for keys, but got {}", key_type),
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_missing_required_field<T: AsRef<str>>(
        &self,
        unparsed_fields: &[(T, T)],
        missing_fields: &[T],
        item: Option<&crate::jsonish::Value>,
    ) -> ParsingError {
        let fields = missing_fields
            .iter()
            .map(|c| c.as_ref())
            .collect::<Vec<_>>()
            .join(", ");
        let missing_error = match missing_fields.len() {
            0 => None,
            1 => Some(format!("Missing required field: {}", fields)),
            _ => Some(format!("Missing required fields: {}", fields)),
        };

        let unparsed = unparsed_fields
            .iter()
            .map(|(k, v)| format!("{}: {}", k.as_ref(), v.as_ref().replace("\n", "\n  ")))
            .collect::<Vec<_>>()
            .join("\n");
        let unparsed_error = match unparsed_fields.len() {
            0 => None,
            1 => Some(format!(
                "Unparsed field: {}\n  {}",
                unparsed_fields[0].0.as_ref(),
                unparsed_fields[0].1.as_ref().replace("\n", "\n  ")
            )),
            _ => Some(format!(
                "Unparsed fields:\n{}\n  {}",
                unparsed_fields
                    .iter()
                    .map(|(k, _)| k.as_ref())
                    .collect::<Vec<_>>()
                    .join(", "),
                unparsed.replace("\n", "\n  ")
            )),
        };

        ParsingError {
            reason: match (missing_error, unparsed_error) {
                (Some(m), Some(u)) => format!("{}\n{}", m, u),
                (Some(m), None) => m,
                (None, Some(u)) => u,
                (None, None) => "Unexpected error".to_string(),
            },
            scope: self.scope.clone(),
        }
    }

    pub(crate) fn error_unexpected_type<T: std::fmt::Display + std::fmt::Debug>(
        &self,
        target: &FieldType,
        got: &T,
    ) -> ParsingError {
        ParsingError {
            reason: format!("Expected {}, got {}.\n{:#?}", target, got, got),
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

    /// Coorce a value to a target type, and penalize the conversion according
    /// to failed constraints. Failing a constraint will not necessarily fail
    /// the coorsion; it will only flag the coersion result with the fact of
    /// the constraint failures.
    fn coerce_and_validate<T: TypeCoercer>(
        unvalidated_value: &T,
        ctx: &ParsingContext,
        target: &FieldType,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags, ParsingError> {
        let mut coerced_value = unvalidated_value.coerce(ctx, target, value)?;
        let constraints_result = run_user_checks_shallow(&coerced_value.clone().into(), target)
            .map_err(|e| ParsingError {
                reason: format!("Failed to evaluate constraints: {:?}", e),
                scope: ctx.scope.clone(),
            })?;
        match constraints_result {
            ConstraintsResult::Success => {}
            ConstraintsResult::AssertFailure(f) => coerced_value.add_flag(Flag::AssertFailure(f)),
            ConstraintsResult::CheckFailures(fs) => coerced_value.add_flag(Flag::CheckFailures(fs)),
        }
        Ok(coerced_value)
    }
}

pub trait DefaultValue {
    fn default_value(&self, error: Option<&ParsingError>) -> Option<BamlValueWithFlags>;
}

/// Run all checks and asserts for every field, recursing into fields
/// that contain classes with further asserts and checks.
pub fn run_user_checks_shallow(
    baml_value: &BamlValue,
    type_: &FieldType,
) -> Result<ConstraintsResult> {
    match type_ {
        FieldType::Constrained { constraints, .. } => {
            let mut check_failures: Vec<ConstraintFailure> = vec![];
            for Constraint {
                level,
                expression,
                label,
            } in constraints.0.iter()
            {
                let constraint_succeeded =
                    evaluate_predicate(baml_value, expression).map_err(|e| {
                        anyhow::anyhow!(format!("Error evaluating constraint: {:?}", e))
                    })?;
                if !constraint_succeeded {
                    let constraint_failure = ConstraintFailure {
                        constraint_name: label.to_string(),
                    };
                    match level {
                        ConstraintLevel::Check => {
                            check_failures.push(constraint_failure);
                        }
                        ConstraintLevel::Assert => {
                            return Ok(ConstraintsResult::AssertFailure(constraint_failure));
                        }
                    }
                }
            }
            if check_failures.len() == 0 {
                Ok(ConstraintsResult::Success)
            } else {
                Ok(ConstraintsResult::CheckFailures(check_failures))
            }
        }
        _ => Ok(ConstraintsResult::Success),
    }
}
