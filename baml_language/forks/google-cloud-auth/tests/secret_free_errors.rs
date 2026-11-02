#![allow(clippy::doc_markdown)]

//! No credential material reaches an error's text. Users put credential
//! documents and tokens where a path is expected, and token endpoints answer
//! with whatever their server chooses; errors reach application logs, so each
//! case below asserts what the text must NOT contain.
//!
//! NOTE: minted tokens are cached process-wide keyed by credential material,
//! so every test uses unique material.

mod common;

use common::{MockIo, authorized_user_json, service_account_json};
use forked_google_cloud_auth::{
    AuthError, CLOUD_PLATFORM_SCOPE, HttpResponse, token_from_adc, token_from_credentials_json,
    token_from_service_account_json,
};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

/// Fails the test if `text` carries any part of a credential document built by
/// `service_account_json(email, …)`.
fn assert_no_service_account_material(text: &str, email: &str) {
    for secret in ["PRIVATE KEY", "MIIE", "key-id", email] {
        assert!(!text.contains(secret), "error leaks {secret:?}: {text}");
    }
}

#[tokio::test]
async fn inline_document_in_gac_is_described_not_quoted() {
    let email = "inline-gac@test-project.iam.gserviceaccount.com";
    let io = MockIo::new().env(
        "GOOGLE_APPLICATION_CREDENTIALS",
        &service_account_json(email, TOKEN_URI),
    );
    let err = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap_err();
    let text = err.to_string();
    assert!(matches!(err, AuthError::NoCredentials(_)), "{text}");
    assert_no_service_account_material(&text, email);
    assert!(
        text.contains("holds a service_account credential document")
            && text.contains("GOOGLE_APPLICATION_CREDENTIALS_CONTENT"),
        "the error must say how to pass an inline document: {text}"
    );
    assert_eq!(io.http_calls(), 0);
}

/// `vercel env pull` and friends store a document as a JSON string; it is
/// still recognized as a document, and a `type` outside google-auth's set is
/// not quoted either.
#[tokio::test]
async fn wrapped_or_unknown_documents_in_gac_are_described_not_quoted() {
    let email = "wrapped-gac@test-project.iam.gserviceaccount.com";
    let wrapped = serde_json::to_string(&service_account_json(email, TOKEN_URI)).unwrap();
    let unknown = r#"{"type":"sekrit-type-value","private_key":"-----BEGIN PRIVATE KEY-----"}"#;
    let mut texts = Vec::new();
    for value in [wrapped.as_str(), unknown] {
        let io = MockIo::new().env("GOOGLE_APPLICATION_CREDENTIALS", value);
        texts.push(
            token_from_adc(&io, CLOUD_PLATFORM_SCOPE)
                .await
                .unwrap_err()
                .to_string(),
        );
    }
    assert_no_service_account_material(&texts[0], email);
    assert!(
        texts[0].contains("holds a service_account credential document"),
        "{}",
        texts[0]
    );
    assert!(!texts[1].contains("sekrit-type-value"), "{}", texts[1]);
    assert!(!texts[1].contains("PRIVATE KEY"), "{}", texts[1]);
    assert!(texts[1].contains("holds a JSON document"), "{}", texts[1]);
}

/// Anything that is not plainly a path to a `.json` file is hidden, even
/// when it does not look like a document.
#[tokio::test]
async fn non_path_gac_value_is_hidden() {
    let token = "ya29.a0-not-a-path-secret";
    let io = MockIo::new().env("GOOGLE_APPLICATION_CREDENTIALS", token);
    let text = token_from_adc(&io, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert!(!text.contains(token), "error leaks the value: {text}");
    assert!(text.contains("not shown"), "{text}");
}

#[tokio::test]
async fn token_endpoint_oauth_error_keeps_its_code_and_description() {
    let json = authorized_user_json("cid-oauth-error", "rt-oauth-error", TOKEN_URI);
    let io = MockIo::new().http(|_m, _u, _h, _b| {
        Ok(HttpResponse {
            status: 400,
            body: r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#
                .to_string(),
        })
    });
    let text = token_from_credentials_json(&io, &json, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        text.contains("status 400: invalid_grant: Token has been expired or revoked."),
        "{text}"
    );
}

/// A server that echoes the request (a misconfigured proxy, a debugging
/// endpoint) would hand back the refresh token and client secret.
#[tokio::test]
async fn token_endpoint_body_of_unknown_shape_is_not_quoted() {
    let refresh_token = "rt-echoed-secret";
    let json = authorized_user_json("cid-echo", refresh_token, TOKEN_URI);
    let io = MockIo::new().http(|_m, _u, _h, body| {
        Ok(HttpResponse {
            status: 400,
            body: format!("<html>bad request: {body}</html>"),
        })
    });
    let text = token_from_credentials_json(&io, &json, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        !text.contains(refresh_token),
        "error leaks the request: {text}"
    );
    assert!(text.contains("body (not shown)"), "{text}");
}

/// serde's typed errors quote the offending value; a document wrapped in a
/// JSON string is such a value.
#[tokio::test]
async fn string_wrapped_document_is_not_quoted_by_the_parse_error() {
    let email = "wrapped@test-project.iam.gserviceaccount.com";
    let wrapped = serde_json::to_string(&service_account_json(email, TOKEN_URI)).unwrap();
    let text = token_from_service_account_json(&MockIo::new(), &wrapped, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert_no_service_account_material(&text, email);
    assert!(text.contains("an unexpected shape at line 1"), "{text}");
}

#[test]
fn http_response_debug_hides_the_body() {
    let response = HttpResponse {
        status: 200,
        body: r#"{"access_token":"ya29.debug-secret"}"#.to_string(),
    };
    let debug = format!("{response:?}");
    assert!(!debug.contains("ya29.debug-secret"), "{debug}");
}
