use std::borrow::Cow;

use crate::internal::llm_client::ErrorCode;

#[derive(Clone)]
pub enum ExposedError {
    /// Error in parsing post calling the LLM
    ValidationError {
        prompt: String,
        raw_output: String,
        message: String,
    },
    FinishReasonError {
        prompt: String,
        raw_output: String,
        message: String,
        finish_reason: Option<String>,
    },
    ClientHttpError {
        client_name: String,
        message: String,
        status_code: ErrorCode,
    },
}

impl std::error::Error for ExposedError {}

impl std::fmt::Display for ExposedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExposedError::ValidationError {
                prompt,
                raw_output,
                message,
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
            } => {
                write!(
                    f,
                    "LLM client \"{client_name}\" failed with status code: {status_code}\nMessage: {message}"
                )
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
                    client_name,
                    message,
                    status_code,
                } => baml_types::tracing::events::BamlError::ClientHttp {
                    message: Cow::Owned(message.clone()),
                    status_code: status_code.to_u16() as i32,
                },
            };
        }
        if let Some(baml_error) = self.downcast_ref::<baml_types::tracing::events::BamlError<'_>>()
        {
            return baml_error.clone();
        }
        baml_types::tracing::events::BamlError::External {
            message: Cow::Owned(format!("into_baml_error: {self:?}")),
        }
    }
}
