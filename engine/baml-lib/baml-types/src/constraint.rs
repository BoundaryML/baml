use crate::JinjaExpression;

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq, Hash)]
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
            expression: JinjaExpression(expr.to_string()),
        }
    }

    pub fn new_assert(label: &str, expr: &str) -> Self {
        Self {
            label: Some(label.to_string()),
            level: ConstraintLevel::Assert,
            expression: JinjaExpression(expr.to_string()),
        }
    }

    pub fn as_check(self) -> Option<(String, JinjaExpression)> {
        match self.level {
            ConstraintLevel::Check => Some((
                self.label
                    .expect("Checks are guaranteed by the pest grammar to have a label."),
                self.expression,
            )),
            ConstraintLevel::Assert => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, Eq, Hash, Ord, PartialOrd)]
pub enum ConstraintLevel {
    Check,
    Assert,
}

/// The user-visible schema for a failed check.
#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct ResponseCheck {
    pub name: String,
    pub expression: String,
    pub status: String,
}

impl ResponseCheck {
    /// Convert a Constraint and its status to a ResponseCheck.
    /// Returns `None` if the Constraint is not a check (i.e.,
    /// if it doesn't meet the invariants that level==Check and
    /// label==Some).
    pub fn from_check_result(
        (
            Constraint {
                level,
                expression,
                label,
            },
            succeeded,
        ): (Constraint, bool),
    ) -> Option<Self> {
        match (level, label) {
            (ConstraintLevel::Check, Some(label)) => {
                let status = if succeeded {
                    "succeeded".to_string()
                } else {
                    "failed".to_string()
                };
                Some(ResponseCheck {
                    name: label,
                    expression: expression.0,
                    status,
                })
            }
            _ => None,
        }
    }
}
