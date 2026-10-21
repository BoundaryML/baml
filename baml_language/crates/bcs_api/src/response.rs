//! Bounded decoding of caller-facing API errors, shared by blocking and async clients.
#[cfg(feature = "auth")]
use std::io::Read;
use std::{
    fmt,
    sync::Arc,
    time::{Duration, SystemTime},
};

use reqwest::{StatusCode, header};
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
    /// Minimum wait before another request, independent of whether the body can be decoded.
    pub retry_after: Option<Duration>,
}

impl HttpFailure {
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            body: None,
            retry_after: None,
        }
    }

    fn from_headers(status: StatusCode, headers: &header::HeaderMap) -> Self {
        Self {
            retry_after: retry_after(headers, SystemTime::now()),
            ..Self::new(status)
        }
    }

    fn decode(mut self, bytes: &[u8], secrets: &[&str]) -> Self {
        if bytes.len() > MAX_ERROR_BYTES {
            return self;
        }
        let Ok(mut body) = serde_json::from_slice::<ApiErrorBody>(bytes) else {
            return self;
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
            return self;
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
        self.body = Some(Arc::new(body));
        self
    }

    #[cfg(feature = "auth")]
    pub(crate) fn blocking(mut response: reqwest::blocking::Response, secrets: &[&str]) -> Self {
        let failure = Self::from_headers(response.status(), response.headers());
        let mut bytes = Vec::new();
        if response
            .by_ref()
            .take((MAX_ERROR_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return failure;
        }
        failure.decode(&bytes, secrets)
    }

    pub(crate) async fn asynchronous(mut response: reqwest::Response, secrets: &[&str]) -> Self {
        let failure = Self::from_headers(response.status(), response.headers());
        let mut bytes = Vec::new();
        loop {
            match response.chunk().await {
                Ok(Some(chunk)) if chunk.len() <= MAX_ERROR_BYTES - bytes.len() => {
                    bytes.extend_from_slice(&chunk);
                }
                Ok(Some(_)) | Err(_) => return failure,
                Ok(None) => return failure.decode(&bytes, secrets),
            }
        }
    }
}

fn retry_after(headers: &header::HeaderMap, now: SystemTime) -> Option<Duration> {
    let value = headers.get(header::RETRY_AFTER)?.to_str().ok()?.trim();
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        return value.parse().ok().map(Duration::from_secs);
    }
    httpdate::parse_http_date(value)
        .ok()
        .map(|at| at.duration_since(now).unwrap_or_default())
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

    fn headers(value: &str) -> header::HeaderMap {
        let mut headers = header::HeaderMap::new();
        headers.insert(header::RETRY_AFTER, value.parse().unwrap());
        headers
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_dates_but_not_malformed_values() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_mins(25_000_000);
        for (value, expected) in [
            ("5", Some(Duration::from_secs(5))),
            (" 5 ", Some(Duration::from_secs(5))),
            ("0", Some(Duration::ZERO)),
            ("18446744073709551615", Some(Duration::from_secs(u64::MAX))),
            ("", None),
            ("-1", None),
            ("+5", None),
            ("1.5", None),
            ("18446744073709551616", None),
            ("later", None),
        ] {
            assert_eq!(retry_after(&headers(value), now), expected, "{value}");
        }
        let future = httpdate::fmt_http_date(now + Duration::from_secs(5));
        assert_eq!(
            retry_after(&headers(&future), now),
            Some(Duration::from_secs(5))
        );
        let past = httpdate::fmt_http_date(now - Duration::from_secs(1));
        assert_eq!(retry_after(&headers(&past), now), Some(Duration::ZERO));
        assert_eq!(retry_after(&header::HeaderMap::new(), now), None);
    }

    #[test]
    fn retry_after_survives_unstructured_and_oversized_error_bodies() {
        for bytes in [
            b"".to_vec(),
            b"proxy unavailable".to_vec(),
            vec![b'x'; MAX_ERROR_BYTES + 1],
        ] {
            let failure = HttpFailure::from_headers(StatusCode::SERVICE_UNAVAILABLE, &headers("5"))
                .decode(&bytes, &[]);
            assert_eq!(failure.retry_after, Some(Duration::from_secs(5)));
            assert_eq!(failure.body, None);
        }
    }

    #[tokio::test]
    async fn async_errors_preserve_retry_after_even_when_body_decoding_stops() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        for body in [vec![], vec![b'x'; MAX_ERROR_BYTES + 1]] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .respond_with(
                    ResponseTemplate::new(503)
                        .insert_header("Retry-After", "5")
                        .set_body_bytes(body),
                )
                .expect(1)
                .mount(&server)
                .await;
            let response = reqwest::Client::new()
                .get(server.uri())
                .send()
                .await
                .unwrap();
            let failure = HttpFailure::asynchronous(response, &[]).await;
            assert_eq!(failure.status, StatusCode::SERVICE_UNAVAILABLE);
            assert_eq!(failure.retry_after, Some(Duration::from_secs(5)));
            assert_eq!(failure.body, None);
        }
    }

    #[cfg(feature = "auth")]
    #[tokio::test]
    async fn blocking_errors_preserve_retry_after() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "5"))
            .expect(1)
            .mount(&server)
            .await;
        let endpoint = server.uri();
        let failure = tokio::task::spawn_blocking(move || {
            let response = reqwest::blocking::Client::new()
                .get(endpoint)
                .send()
                .unwrap();
            HttpFailure::blocking(response, &[])
        })
        .await
        .unwrap();
        assert_eq!(failure.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failure.retry_after, Some(Duration::from_secs(5)));
    }

    #[test]
    fn public_message_is_optional_and_unknown_fields_are_discarded() {
        let failure = HttpFailure::new(StatusCode::BAD_REQUEST).decode( br#"{"code":"ENVIRONMENT_REQUIRED","retryable":false,"message":"Choose an environment.","internal":"private"}"#, &[]);
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
            let failure = HttpFailure::new(StatusCode::BAD_REQUEST).decode(bytes, &[]);
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
            let failure = HttpFailure::new(StatusCode::BAD_REQUEST).decode(&bytes, &[]);
            assert_eq!(failure, HttpFailure::new(StatusCode::BAD_REQUEST));
            assert_eq!(
                failure.to_string(),
                "Boundary request failed with HTTP 400 Bad Request"
            );
        }
    }

    #[test]
    fn credential_echoes_and_terminal_controls_do_not_reach_the_message() {
        let failure = HttpFailure::new(StatusCode::UNAUTHORIZED).decode( br#"{"code":"UNAUTHORIZED","retryable":false,"message":"Rejected bdry_session_secret\nTry again\u001b."}"#, &["bdry_session_secret"]);
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
