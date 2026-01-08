use std::collections::HashMap;

use crate::{
    codec::{BamlDecode, BamlEncode},
    error::BamlError,
    proto::baml_cffi_v1::{cffi_value_holder, CffiStreamState, CffiValueHolder, HostValue},
};

/// Result of a @check constraint
#[derive(Debug, Clone)]
pub struct Checked<T> {
    pub value: T,
    pub checks: HashMap<String, Check>,
}

impl<T: Default> Default for Checked<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            checks: HashMap::new(),
        }
    }
}

/// Individual check result
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Check {
    pub name: String,
    pub expression: String,
    pub status: CheckStatus,
}

/// Status of a check constraint
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Succeeded,
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
                        Ok((
                            c.name.clone(),
                            Check {
                                name: c.name.clone(),
                                expression: c.expression.clone(),
                                status: match c.status.as_str() {
                                    "succeeded" => CheckStatus::Succeeded,
                                    "failed" => CheckStatus::Failed,
                                    _ => {
                                        return Err(BamlError::internal(format!(
                                            "invalid check status: {}",
                                            c.status
                                        )));
                                    }
                                },
                            },
                        ))
                    })
                    .collect::<Result<HashMap<String, Check>, BamlError>>()?;

                Ok(Checked { value, checks })
            }
            other => Err(BamlError::internal(format!(
                "expected checked value, got {:?}",
                other.is_some()
            ))),
        }
    }
}

impl<T: BamlEncode> BamlEncode for Checked<T> {
    fn baml_encode(&self) -> HostValue {
        // Encode Checked<T> by encoding just the inner value.
        // The checks metadata is typically computed by the runtime, not sent as input.
        self.value.baml_encode()
    }
}

impl<T> Checked<T> {
    /// Returns true if all checks passed
    pub fn all_passed(&self) -> bool {
        self.checks
            .values()
            .all(|c| c.status == CheckStatus::Succeeded)
    }

    /// Returns true if any check failed
    pub fn any_failed(&self) -> bool {
        self.checks
            .values()
            .any(|c| c.status == CheckStatus::Failed)
    }

    /// Get a specific check by name
    pub fn get_check(&self, name: &str) -> Option<&Check> {
        self.checks.get(name)
    }
}

/// Streaming state wrapper for @`stream.with_state`
#[derive(Debug, Clone)]
pub struct StreamState<T> {
    pub value: T,
    pub state: StreamingState,
}

/// Current streaming state
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamingState {
    Pending,
    Started,
    Done,
}

impl<T> StreamState<T> {
    /// Create a new `StreamState` with the given value in Pending state.
    pub fn new(value: T) -> Self {
        Self {
            value,
            state: StreamingState::Pending,
        }
    }
}

impl<T: Default> Default for StreamState<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
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
