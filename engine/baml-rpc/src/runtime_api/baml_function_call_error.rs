use std::borrow::Cow;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum BamlFunctionCallError<'a> {
    /// For any exceptions that are not handled by the BAML runtime
    ExternalException {
        message: Cow<'a, str>,
    },
    InternalException {
        message: Cow<'a, str>,
    },
    Base {
        message: Cow<'a, str>,
    },
    InvalidArgument {
        message: Cow<'a, str>,
    },
    Client {
        message: Cow<'a, str>,
    },
    ClientHttp {
        message: Cow<'a, str>,
        status_code: i32,
    },
    ClientFinishReason {
        finish_reason: Cow<'a, str>,
        message: Cow<'a, str>,
        prompt: Cow<'a, str>,
        raw_output: Cow<'a, str>,
    },
    Validation {
        raw_output: Cow<'a, str>,
        message: Cow<'a, str>,
        prompt: Cow<'a, str>,
    },
}
