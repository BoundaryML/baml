use std::collections::HashMap;

use crate::codec::BamlDecode;
use crate::error::BamlError;
use crate::proto::baml_cffi_v1::{cffi_value_holder, CffiStreamState, CffiValueHolder};

/// Result of a @check constraint
#[derive(Debug, Clone)]
pub struct Checked<T> {
    pub value: T,
    pub checks: HashMap<String, Check>,
}

/// Individual check result
#[derive(Debug, Clone)]
pub struct Check {
    pub name: String,
    pub expression: String,
    pub status: CheckStatus,
}

/// Status of a check constraint
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckStatus {
    Passed,
    Failed,
}

impl<T: BamlDecode> BamlDecode for Checked<T> {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::CheckedValue(checked)) => {
                let inner = checked
                    .value
                    .as_ref()
                    .ok_or_else(|| BamlError::internal("missing checked value"))?;
                let value = T::baml_decode(inner)?;

                let checks = checked
                    .checks
                    .iter()
                    .map(|c| {
                        (
                            c.name.clone(),
                            Check {
                                name: c.name.clone(),
                                expression: c.expression.clone(),
                                status: match c.status.as_str() {
                                    "passed" | "PASSED" => CheckStatus::Passed,
                                    _ => CheckStatus::Failed,
                                },
                            },
                        )
                    })
                    .collect();

                Ok(Checked { value, checks })
            }
            other => Err(BamlError::internal(format!(
                "expected checked value, got {:?}",
                other.is_some()
            ))),
        }
    }
}

impl<T> Checked<T> {
    /// Returns true if all checks passed
    pub fn all_passed(&self) -> bool {
        self.checks.values().all(|c| c.status == CheckStatus::Passed)
    }

    /// Returns true if any check failed
    pub fn any_failed(&self) -> bool {
        self.checks.values().any(|c| c.status == CheckStatus::Failed)
    }

    /// Get a specific check by name
    pub fn get_check(&self, name: &str) -> Option<&Check> {
        self.checks.get(name)
    }
}

/// Streaming state wrapper for @stream.with_state
#[derive(Debug, Clone)]
pub struct StreamState<T> {
    pub value: T,
    pub state: StreamingState,
}

/// Current streaming state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingState {
    Pending,
    Started,
    Done,
}

impl<T: BamlDecode> BamlDecode for StreamState<T> {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        match &holder.value {
            Some(cffi_value_holder::Value::StreamingStateValue(ss)) => {
                let inner = ss
                    .value
                    .as_ref()
                    .ok_or_else(|| BamlError::internal("missing stream state value"))?;
                let value = T::baml_decode(inner)?;

                let state = match ss.state() {
                    CffiStreamState::Pending => StreamingState::Pending,
                    CffiStreamState::Started => StreamingState::Started,
                    CffiStreamState::Done => StreamingState::Done,
                };

                Ok(StreamState { value, state })
            }
            other => Err(BamlError::internal(format!(
                "expected stream state value, got {:?}",
                other.is_some()
            ))),
        }
    }
}
