use crate::JinjaExpression;

#[derive(Clone, Debug, serde::Serialize, PartialEq)]
pub struct Constraint {
    pub level: ConstraintLevel,
    pub expression: JinjaExpression,
    pub label: Option<String>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub enum ConstraintLevel {
    Check,
    Assert,
}

/// The user-visible schema for a failed check.
#[derive(Clone, Debug, serde::Serialize)]
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
