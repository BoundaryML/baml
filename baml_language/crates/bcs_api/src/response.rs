//! Bounded decoding of caller-facing API errors, shared by blocking and async clients.
#[cfg(feature = "auth")]
use std::io::Read;
use std::{fmt, sync::Arc};

use reqwest::StatusCode;
use serde::Deserialize;

const MAX_ERROR_BYTES: usize = 16 * 1024;

/// The public error contract. Unknown response fields never enter diagnostics.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ApiErrorBody {
    pub code: String,
    pub retryable: bool,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpFailure {
    pub status: StatusCode,
    pub body: Option<Arc<ApiErrorBody>>,
}

impl HttpFailure {
    pub fn new(status: StatusCode) -> Self {
        Self { status, body: None }
    }

    fn decode(status: StatusCode, bytes: &[u8], secrets: &[&str]) -> Self {
        let mut failure = Self::new(status);
        if bytes.len() > MAX_ERROR_BYTES {
            return failure;
        }
        let Ok(mut body) = serde_json::from_slice::<ApiErrorBody>(bytes) else {
            return failure;
        };
        if body.code.is_empty()
            || body.code.len() > 128
            || !body
                .code
                .bytes()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
            || secrets
                .iter()
                .any(|secret| !secret.is_empty() && body.code.contains(secret))
        {
            return failure;
        }
        if let Some(message) = &mut body.message {
            for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
                *message = message.replace(secret, "[REDACTED]");
            }
            // Preserve explanatory line breaks; strip terminal escape/control and bidi controls.
            message
                .chars()
                .filter(|c| {
                    (!c.is_control() || *c == '\n')
                        && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                })
                .collect::<String>()
                .trim()
                .clone_into(message);
            if message.is_empty() {
                body.message = None;
            }
        }
        failure.body = Some(Arc::new(body));
        failure
    }

    #[cfg(feature = "auth")]
    pub(crate) fn blocking(mut response: reqwest::blocking::Response, secrets: &[&str]) -> Self {
        let status = response.status();
        let mut bytes = Vec::new();
        if response
            .by_ref()
            .take((MAX_ERROR_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return Self::new(status);
        }
        Self::decode(status, &bytes, secrets)
    }

    pub(crate) async fn asynchronous(mut response: reqwest::Response, secrets: &[&str]) -> Self {
        let status = response.status();
        let mut bytes = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if chunk.len() <= MAX_ERROR_BYTES - bytes.len() => {
                    bytes.extend_from_slice(&chunk);
                }
                Ok(Some(_)) | Err(_) => return Self::new(status),
                Ok(None) => return Self::decode(status, &bytes, secrets),
            }
        }
    }
}

impl fmt::Display for HttpFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Boundary request failed with HTTP {}", self.status)?;
        if let Some(body) = &self.body {
            write!(f, " ({})", body.code)?;
            if let Some(message) = &body.message {
                for line in message.lines() {
                    write!(f, "\n  {line}")?;
                }
            }
        }
        Ok(())
    }
}
impl std::error::Error for HttpFailure {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_message_is_optional_and_unknown_fields_are_discarded() {
        let failure = HttpFailure::decode(StatusCode::BAD_REQUEST, br#"{"code":"ENVIRONMENT_REQUIRED","retryable":false,"message":"Choose an environment.","internal":"private"}"#, &[]);
        assert_eq!(
            failure.body.as_deref(),
            Some(&ApiErrorBody {
                code: "ENVIRONMENT_REQUIRED".into(),
                retryable: false,
                message: Some("Choose an environment.".into())
            })
        );
        assert_eq!(
            failure.to_string(),
            r#"Boundary request failed with HTTP 400 Bad Request (ENVIRONMENT_REQUIRED)
  Choose an environment."#
        );
        for bytes in [
            br#"{"code":"ENVIRONMENT_REQUIRED","retryable":false}"#.as_slice(),
            br#"{"code":"ENVIRONMENT_REQUIRED","retryable":false,"message":null}"#,
        ] {
            let failure = HttpFailure::decode(StatusCode::BAD_REQUEST, bytes, &[]);
            assert_eq!(failure.body.as_deref().unwrap().message, None);
            assert_eq!(
                failure.to_string(),
                "Boundary request failed with HTTP 400 Bad Request (ENVIRONMENT_REQUIRED)"
            );
        }
    }

    #[test]
    fn malformed_unstructured_and_oversized_bodies_keep_the_http_status() {
        for bytes in [
            b"<html>proxy error</html>".to_vec(),
            b"not json".to_vec(),
            br#"{"code":"bad\ncode","retryable":false}"#.to_vec(),
            vec![b'x'; MAX_ERROR_BYTES + 1],
        ] {
            let failure = HttpFailure::decode(StatusCode::BAD_REQUEST, &bytes, &[]);
            assert_eq!(failure, HttpFailure::new(StatusCode::BAD_REQUEST));
            assert_eq!(
                failure.to_string(),
                "Boundary request failed with HTTP 400 Bad Request"
            );
        }
    }

    #[test]
    fn credential_echoes_and_terminal_controls_do_not_reach_the_message() {
        let failure = HttpFailure::decode(StatusCode::UNAUTHORIZED, br#"{"code":"UNAUTHORIZED","retryable":false,"message":"Rejected bdry_session_secret\nTry again\u001b."}"#, &["bdry_session_secret"]);
        assert_eq!(
            failure.body.as_deref().unwrap().message.as_deref(),
            Some(
                r#"Rejected [REDACTED]
Try again."#
            )
        );
        assert_eq!(
            failure.to_string(),
            r#"Boundary request failed with HTTP 401 Unauthorized (UNAUTHORIZED)
  Rejected [REDACTED]
  Try again."#
        );
    }
}
