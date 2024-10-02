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
    pub name: Option<String>,
    pub expression: String,
    pub status: String,
}

impl ResponseCheck {
    pub fn from_constraint_result((Constraint{ level, expression, label }, succeeded): (Constraint, bool)) -> Option<Self> {
        match level {
            ConstraintLevel::Check => {
                let status = if succeeded { "succeeded".to_string() } else { "failed".to_string() };
                Some( ResponseCheck {
                    name: label,
                    expression: expression.0,
                    status
                })
            },
            _ => None,
        }
    }
}
