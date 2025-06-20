use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Serialize, Deserialize, Display, Clone)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum BamlError<'a> {
    // For errors that are not from within BAML
    // I.e. Python / Rust / etc.
    External {
        message: Cow<'a, str>,
    },
    Internal {
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

// impl<'a> From<&anyhow::Error> for BamlError<'a> {
//     fn from(e: &anyhow::Error) -> Self {
//         if let Some(baml_error) = e.downcast_ref::<BamlError>() {
//             baml_error.clone()
//         } else {
//             BamlError::External {
//                 message: Cow::Owned(e.to_string()),
//             }
//         }
//     }
// }
