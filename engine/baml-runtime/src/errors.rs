use std::borrow::Cow;

use crate::internal::llm_client::ErrorCode;

#[derive(Clone)]
pub enum ExposedError {
    /// Error in parsing post calling the LLM
    ValidationError {
        prompt: String,
        raw_output: String,
        message: String,
        detailed_message: String,
    },
    FinishReasonError {
        prompt: String,
        raw_output: String,
        message: String,
        finish_reason: Option<String>,
        detailed_message: String,
    },
    ClientHttpError {
        client_name: String,
        message: String,
        status_code: ErrorCode,
        detailed_message: String,
        /// The raw response body from the LLM API (if available)
        raw_response: Option<String>,
    },
    TimeoutError {
        client_name: String,
        message: String,
    },
    AbortError {
        detailed_message: String,
    },
}

impl ExposedError {
    pub fn timeout_error(client_name: impl Into<String>, message: impl Into<String>) -> Self {
        ExposedError::TimeoutError {
            client_name: client_name.into(),
            message: message.into(),
        }
    }

    pub fn to_anyhow_with_details(&self) -> anyhow::Error {
        let detailed_message = match self {
            ExposedError::ValidationError {
                detailed_message, ..
            } => detailed_message,
            ExposedError::FinishReasonError {
                detailed_message, ..
            } => detailed_message,
            ExposedError::ClientHttpError {
                detailed_message, ..
            } => detailed_message,
            ExposedError::TimeoutError { message, .. } => message,
            ExposedError::AbortError {
                detailed_message, ..
            } => detailed_message,
        };
        let with_details = format!("{self}\n\nDetailed message: {detailed_message}");
        anyhow::anyhow!(with_details)
    }
}

impl std::error::Error for ExposedError {}

impl std::fmt::Display for ExposedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExposedError::ValidationError {
                prompt,
                raw_output,
                message,
                detailed_message: _,
            } => {
                write!(
                    f,
                    "Parsing error: {message}\nPrompt: {prompt}\nRaw Response: {raw_output}"
                )
            }
            ExposedError::FinishReasonError {
                prompt,
                raw_output,
                message,
                finish_reason,
                detailed_message: _,
            } => {
                write!(
                    f,
                    "Finish reason error: {}\nPrompt: {}\nRaw Response: {}\nFinish Reason: {}",
                    message,
                    prompt,
                    raw_output,
                    finish_reason.as_ref().map_or("<none>", |f| f.as_str())
                )
            }
            ExposedError::ClientHttpError {
                client_name,
                message,
                status_code,
                ..
            } => {
                write!(
                        f,
                        "LLM client \"{client_name}\" failed with status code: {status_code}\nMessage: {message}"
                    )
            }
            ExposedError::TimeoutError {
                client_name,
                message,
            } => {
                write!(f, "LLM client \"{client_name}\" timed out: {message}")
            }
            ExposedError::AbortError { detailed_message } => {
                write!(f, "AbortError: {detailed_message}")
            }
        }
    }
}

impl std::fmt::Debug for ExposedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{self}"))
    }
}

pub(crate) trait IntoBamlError {
    fn to_baml_error<'b>(&self) -> baml_types::tracing::events::BamlError<'b>;
}

impl IntoBamlError for &anyhow::Error {
    fn to_baml_error<'b>(&self) -> baml_types::tracing::events::BamlError<'b> {
        // print as the downcast_ref of whatever error actually is
        if let Some(er) = self.downcast_ref::<ExposedError>() {
            return match er {
                ExposedError::ValidationError {
                    prompt,
                    message,
                    raw_output: raw_response,
                    detailed_message: _,
                } => baml_types::tracing::events::BamlError::Validation {
                    raw_output: Cow::Owned(raw_response.clone()),
                    message: Cow::Owned(message.clone()),
                    prompt: Cow::Owned(prompt.clone()),
                },
                ExposedError::FinishReasonError {
                    prompt,
                    message,
                    raw_output: raw_response,
                    finish_reason,
                    detailed_message: _,
                } => baml_types::tracing::events::BamlError::ClientFinishReason {
                    finish_reason: match finish_reason {
                        Some(finish_reason) => Cow::Owned(finish_reason.clone()),
                        None => Cow::Owned(String::new()),
                    },
                    message: Cow::Owned(message.clone()),
                    prompt: Cow::Owned(prompt.clone()),
                    raw_output: Cow::Owned(raw_response.clone()),
                },
                ExposedError::ClientHttpError {
                    message,
                    status_code,
                    ..
                } => baml_types::tracing::events::BamlError::ClientHttp {
                    message: Cow::Owned(message.clone()),
                    status_code: status_code.to_u16() as i32,
                },
                ExposedError::TimeoutError {
                    client_name: _,
                    message,
                } => baml_types::tracing::events::BamlError::ClientHttp {
                    message: Cow::Owned(message.clone()),
                    status_code: 408, // HTTP 408 Request Timeout
                },
                ExposedError::AbortError { detailed_message } => {
                    baml_types::tracing::events::BamlError::Base {
                        message: Cow::Owned(format!("AbortError: {detailed_message}")),
                    }
                }
            };
        }
        if let Some(baml_error) = self.downcast_ref::<baml_types::tracing::events::BamlError<'_>>()
        {
            return baml_error.clone();
        }
        baml_types::tracing::events::BamlError::External {
            message: Cow::Owned(format!("ExternalException: {self:?}")),
        }
    }
}
