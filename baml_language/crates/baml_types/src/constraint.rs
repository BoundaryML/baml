//! Constraint types for @assert/@check validation.

use crate::JinjaExpression;
use serde::Serialize;

/// A constraint definition (either @assert or @check).
#[derive(Clone, Debug, Serialize, PartialEq, Eq, Hash)]
pub struct Constraint {
    pub level: ConstraintLevel,
    pub expression: JinjaExpression,
    pub label: Option<String>,
}

impl Constraint {
    pub fn new_check(label: &str, expr: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            level: ConstraintLevel::Check,
            expression: JinjaExpression::new(expr),
        }
    }

    pub fn new_assert(label: &str, expr: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            level: ConstraintLevel::Assert,
            expression: JinjaExpression::new(expr),
        }
    }

    pub fn as_check(self) -> Option<(String, JinjaExpression)> {
        match self.level {
            ConstraintLevel::Check => Some((
                self.label
                    .expect("Checks are guaranteed to have a label"),
                self.expression,
            )),
            ConstraintLevel::Assert => None,
        }
    }
}

/// The level of a constraint - Check (soft) or Assert (hard).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize)]
pub enum ConstraintLevel {
    Check,
    Assert,
}

/// The result of evaluating a check constraint.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct ResponseCheck {
    pub name: String,
    pub expression: String,
    pub status: String,
}

impl ResponseCheck {
    /// Create a ResponseCheck from a Constraint and its evaluation result.
    pub fn from_check_result(constraint: Constraint, succeeded: bool) -> Option<Self> {
        match (constraint.level, constraint.label) {
            (ConstraintLevel::Check, Some(label)) => {
                let status = if succeeded { "succeeded" } else { "failed" }.to_string();
                Some(ResponseCheck {
                    name: label,
                    expression: constraint.expression.0,
                    status,
                })
            }
            _ => None,
        }
    }
}
