//! Cloud recording's HTTP: the process's network transport (`baml_http`),
//! tagged [`Destination::CloudTelemetry`] so a `no-phone-home` build refuses
//! it. Requests never follow a redirect (a presigned upload URL or the bearer
//! token must not leak to another origin) or an environment proxy.

use baml_http::{
    BodyStream, CollectError, Redirects, Request, Response,
    outbound::{self, Destination},
};

pub(crate) async fn send(request: Request) -> Result<Response, baml_http::Error> {
    let request = request.redirects(Redirects::None).use_env_proxy(false);
    outbound::send(Destination::CloudTelemetry, request).await
}

pub(crate) fn is_success(status: u16) -> bool {
    (200..300).contains(&status)
}

/// A response's advertised `Content-Length`, if any.
pub(crate) fn content_length(response: &Response) -> Option<u64> {
    response.header("content-length")?.trim().parse().ok()
}

/// Read a body, failing past `limit` bytes.
pub(crate) async fn read_limited(body: BodyStream, limit: usize) -> Result<Vec<u8>, CollectError> {
    baml_http::collect(body, limit)
        .await
        .map(|bytes| bytes.to_vec())
}

/// A header name HTTP accepts: a non-empty token (RFC 9110 §5.1).
pub(crate) fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
}

/// A header value HTTP accepts: visible ASCII, spaces and tabs.
pub(crate) fn valid_header_value(value: &str) -> bool {
    value
        .bytes()
        .all(|b| b == b'\t' || (0x20..0x7f).contains(&b))
}
