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
    AuthError, CLOUD_PLATFORM_SCOPE, HttpResponse, PathSetting, credential_document, project_id,
    token_from_adc, token_from_credentials_json, token_from_service_account_json,
};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

/// Fails the test if `text` carries any part of a credential document built by
/// `service_account_json(email, …)`.
fn assert_no_service_account_material(text: &str, email: &str) {
    for secret in ["PRIVATE KEY", "MIIE", "key-id", email] {
        assert!(!text.contains(secret), "error leaks {secret:?}: {text}");
    }
}

async fn gac_error(value: &str) -> String {
    let io = MockIo::new().env("GOOGLE_APPLICATION_CREDENTIALS", value);
    token_from_adc(&io, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string()
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
    assert!(
        matches!(err, AuthError::UnusableCredentialPath { .. }),
        "{text}"
    );
    assert_no_service_account_material(&text, email);
    assert!(
        text.contains("holds a service_account credential document")
            && text.contains("must be a file path"),
        "{text}"
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
    let wrapped_text = gac_error(&wrapped).await;
    assert_no_service_account_material(&wrapped_text, email);
    assert!(
        wrapped_text.contains("holds a service_account credential document"),
        "{wrapped_text}"
    );

    let unknown = r#"{"type":"sekrit-type-value","private_key":"-----BEGIN PRIVATE KEY-----"}"#;
    let unknown_text = gac_error(unknown).await;
    assert!(!unknown_text.contains("sekrit"), "{unknown_text}");
    assert!(!unknown_text.contains("PRIVATE KEY"), "{unknown_text}");
    assert!(
        unknown_text.contains("holds a JSON document"),
        "{unknown_text}"
    );
}

/// Anything that is not plainly a path to a `.json` file is hidden, even
/// when it does not look like a document.
#[tokio::test]
async fn non_path_gac_value_is_hidden() {
    let token = "ya29.a0-not-a-path-secret";
    let text = gac_error(token).await;
    assert!(!text.contains(token), "error leaks the value: {text}");
    assert!(text.contains("not shown"), "{text}");

    let control = "/x/\u{1b}[31mred.json";
    let text = gac_error(control).await;
    assert!(
        !text.contains('\u{1b}'),
        "error carries a terminal escape: {text}"
    );
}

/// Common shapes of a mistyped path get a message that says what is wrong,
/// and a plain path is still named.
#[tokio::test]
async fn misshapen_gac_paths_say_what_is_wrong() {
    assert!(
        gac_error("").await.contains("is set but empty"),
        "an empty value is named as such"
    );
    let quoted = gac_error("\"/creds/key.json\"").await;
    assert!(
        quoted.contains("'/creds/key.json' wrapped in quotes"),
        "{quoted}"
    );
    let padded = gac_error("  /creds/key.json ").await;
    assert!(
        padded.contains("'/creds/key.json' with whitespace around it"),
        "{padded}"
    );
    let plain = gac_error("/missing/adc.json").await;
    assert!(plain.contains("points to '/missing/adc.json'"), "{plain}");
}

/// The project lookup reports an unusable `GOOGLE_APPLICATION_CREDENTIALS`
/// as the token mint does, instead of moving on to another source's project.
#[tokio::test]
async fn project_lookup_reports_an_unusable_gac() {
    let email = "project-gac@test-project.iam.gserviceaccount.com";
    let io = MockIo::new()
        .env(
            "GOOGLE_APPLICATION_CREDENTIALS",
            &service_account_json(email, TOKEN_URI),
        )
        .env("HOME", "/home/dev")
        .file(
            "/home/dev/.config/gcloud/application_default_credentials.json",
            r#"{"quota_project_id":"other-project"}"#,
        );
    let err = project_id(&io, None).await.unwrap_err();
    let text = err.to_string();
    assert!(
        matches!(err, AuthError::UnusableCredentialPath { .. }),
        "{text}"
    );
    assert_no_service_account_material(&text, email);
    assert_eq!(io.http_calls(), 0);
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

/// Google's API error shape keeps its status and message; a long message is
/// cut and marked, and its control characters cannot rewrite a log.
#[tokio::test]
async fn token_endpoint_api_error_is_bounded_and_printable() {
    let json = authorized_user_json("cid-api-error", "rt-api-error", TOKEN_URI);
    let long = format!("denied\u{1b}[2J\n{}", "é".repeat(400));
    let body = serde_json::json!({
        "error": { "code": 403, "status": "PERMISSION_DENIED", "message": long },
    })
    .to_string();
    let io = MockIo::new().http(move |_m, _u, _h, _b| {
        Ok(HttpResponse {
            status: 403,
            body: body.clone(),
        })
    });
    let text = token_from_credentials_json(&io, &json, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert!(
        text.contains("status 403: PERMISSION_DENIED: denied"),
        "{text}"
    );
    assert!(!text.contains('\u{1b}') && !text.contains('\n'), "{text}");
    assert!(text.ends_with('…'), "a cut message is marked: {text}");
    assert!(text.chars().filter(|c| *c == 'é').count() < 400, "{text}");
}

/// A server that echoes the request (a misconfigured proxy, a debugging
/// endpoint) would hand back the refresh token and client secret. The secret
/// is alphanumeric so the form encoding cannot disguise an echo.
#[tokio::test]
async fn token_endpoint_body_of_unknown_shape_is_not_quoted() {
    let refresh_token = "rtEchoedSecret0123";
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

/// A document's own `type` is never quoted, at the top level or in an
/// impersonated account's `source_credentials`.
#[tokio::test]
async fn unknown_document_types_are_not_quoted() {
    let top = r#"{"type":"sekrit-top-type"}"#;
    let text = token_from_credentials_json(&MockIo::new(), top, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert!(!text.contains("sekrit"), "{text}");

    let nested = serde_json::json!({
        "type": "impersonated_service_account",
        "service_account_impersonation_url": "https://iam.example/sa:generateAccessToken",
        "source_credentials": { "type": "sekrit-source-type" },
    })
    .to_string();
    let text = token_from_credentials_json(&MockIo::new(), &nested, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert!(!text.contains("sekrit"), "{text}");
}

/// serde's typed errors quote the offending value; a document wrapped in a
/// JSON string is such a value. A missing field is named.
#[tokio::test]
async fn service_account_errors_name_fields_not_values() {
    let email = "wrapped@test-project.iam.gserviceaccount.com";
    let wrapped = serde_json::to_string(&service_account_json(email, TOKEN_URI)).unwrap();
    let text = token_from_service_account_json(&MockIo::new(), &wrapped, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert_no_service_account_material(&text, email);

    let missing = r#"{"type":"service_account","client_email":"missing-key@example.com"}"#;
    let text = token_from_credentials_json(&MockIo::new(), missing, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err()
        .to_string();
    assert!(text.contains("missing 'private_key'"), "{text}");
}

#[test]
fn credential_document_unwraps_one_string_level() {
    let document = r#"{"type":"authorized_user"}"#;
    assert_eq!(credential_document(document).as_deref(), Some(document));
    let wrapped = serde_json::to_string(document).unwrap();
    assert_eq!(credential_document(&wrapped).as_deref(), Some(document));
    // Only one level: a document wrapped twice is not one, as
    // `PathSetting::classify` agrees.
    let double_wrapped = serde_json::to_string(&wrapped).unwrap();
    assert_eq!(credential_document(&double_wrapped), None);
    assert!(matches!(
        PathSetting::classify(&double_wrapped),
        PathSetting::Unrecognized { .. }
    ));
    for not_a_document in ["/creds/key.json", "\"/creds/key.json\"", "[1]", "null", ""] {
        assert_eq!(
            credential_document(not_a_document),
            None,
            "{not_a_document:?}"
        );
    }
}

#[test]
fn path_setting_quotes_only_plain_json_paths() {
    assert_eq!(
        PathSetting::classify("C:\\creds\\Key.JSON"),
        PathSetting::Path("C:\\creds\\Key.JSON".to_string())
    );
    let at_limit = format!("/{}.json", "a".repeat(506));
    assert_eq!(at_limit.len(), 512);
    assert_eq!(
        PathSetting::classify(&at_limit),
        PathSetting::Path(at_limit.clone())
    );
    let over_limit = format!("/{}.json", "a".repeat(507));
    assert!(matches!(
        PathSetting::classify(&over_limit),
        PathSetting::Unrecognized { .. }
    ));
    for hidden in ["/creds/key", "/x/{y}.json", "/x/\u{0}.json", "/x/'y'.json"] {
        assert!(
            matches!(
                PathSetting::classify(hidden),
                PathSetting::Unrecognized { .. }
            ),
            "{hidden:?}"
        );
    }
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
