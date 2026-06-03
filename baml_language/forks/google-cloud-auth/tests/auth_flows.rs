#![allow(clippy::doc_markdown)]

//! Integration coverage for every GCP credential flow the fork supports, driven
//! through the public API with a mock `TokenIo`. Mirrors the real
//! `google-cloud-auth` behaviors: service-account JWT-bearer, ADC
//! `authorized_user` refresh grant, ADC `service_account`, well-known ADC path
//! discovery, and the GCE metadata server.

mod common;

use common::{MockIo, authorized_user_json, service_account_json, token_response};
use forked_google_cloud_auth::{
    AuthError, CLOUD_PLATFORM_SCOPE, HttpResponse, adc_available, token_from_adc,
    token_from_service_account_json,
};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const METADATA_PREFIX: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

fn ok(body: String) -> HttpResponse {
    HttpResponse { status: 200, body }
}

// --- Service account (RS256 JWT-bearer) -----------------------------------

#[tokio::test]
async fn service_account_mints_token_via_jwt_bearer() {
    let io = MockIo::new().http(|method, url, _h, body| {
        assert_eq!(method, "POST");
        assert_eq!(url, TOKEN_URI);
        assert!(
            body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant%2Dtype%3Ajwt%2Dbearer"),
            "must use the jwt-bearer grant; body={body}"
        );
        Ok(ok(token_response("ya29.sa")))
    });
    let sa = service_account_json("svc@test-project.iam.gserviceaccount.com", TOKEN_URI);
    let token = token_from_service_account_json(&io, &sa, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.sa");

    // The assertion is a well-formed 3-part JWT.
    let body = io.last_http_body().unwrap();
    let assertion = body
        .split('&')
        .find_map(|p| p.strip_prefix("assertion="))
        .unwrap();
    let jwt = percent_decode(assertion);
    assert_eq!(jwt.split('.').count(), 3, "assertion must be a 3-part JWT");
}

#[tokio::test]
async fn service_account_honors_custom_token_uri() {
    let custom = "https://oauth2.example.com/token";
    let io = MockIo::new().http(move |_m, url, _h, _b| {
        assert_eq!(url, "https://oauth2.example.com/token");
        Ok(ok(token_response("ya29.custom")))
    });
    let sa = service_account_json("svc@test-project.iam.gserviceaccount.com", custom);
    let token = token_from_service_account_json(&io, &sa, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.custom");
}

// --- ADC: authorized_user (refresh-token grant) ---------------------------

#[tokio::test]
async fn adc_authorized_user_uses_refresh_grant() {
    // client_id kept alphanumeric so it round-trips through the fork's
    // NON_ALPHANUMERIC percent-encoding unchanged; refresh_token carries `/`
    // to exercise encoding (`1//refresh` -> `1%2F%2Frefresh`).
    let adc = authorized_user_json("cidabc123", "1//refresh", TOKEN_URI);
    let io = MockIo::new()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/adc.json")
        .file("/adc.json", &adc)
        .http(|_m, url, _h, body| {
            assert_eq!(url, TOKEN_URI);
            assert!(body.contains("grant_type=refresh_token"), "body={body}");
            assert!(body.contains("client_id=cidabc123"), "body={body}");
            assert!(body.contains("refresh_token=1%2F%2Frefresh"), "body={body}");
            Ok(ok(token_response("ya29.user")))
        });
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.user");
}

// --- ADC: service_account discovered via GOOGLE_APPLICATION_CREDENTIALS ----

#[tokio::test]
async fn adc_service_account_via_gac_signs_jwt() {
    let sa = service_account_json("svc@test-project.iam.gserviceaccount.com", TOKEN_URI);
    let io = MockIo::new()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/sa.json")
        .file("/sa.json", &sa)
        .http(|_m, _url, _h, body| {
            assert!(
                body.contains("jwt%2Dbearer"),
                "ADC service_account must JWT-sign; body={body}"
            );
            Ok(ok(token_response("ya29.adc-sa")))
        });
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.adc-sa");
}

// --- ADC: well-known config path discovery (no GAC env) -------------------

#[tokio::test]
async fn adc_discovers_well_known_path_from_home() {
    let adc = authorized_user_json("cid", "rt", TOKEN_URI);
    // No GOOGLE_APPLICATION_CREDENTIALS; only HOME + the well-known file.
    let io = MockIo::new()
        .env("HOME", "/home/dev")
        .file(
            "/home/dev/.config/gcloud/application_default_credentials.json",
            &adc,
        )
        .http(|_m, _url, _h, _b| Ok(ok(token_response("ya29.wellknown"))));
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.wellknown");
}

#[tokio::test]
async fn adc_honors_cloudsdk_config_override() {
    let adc = authorized_user_json("cid", "rt", TOKEN_URI);
    let io = MockIo::new()
        .env("CLOUDSDK_CONFIG", "/custom/gcloud")
        .file("/custom/gcloud/application_default_credentials.json", &adc)
        .http(|_m, _url, _h, _b| Ok(ok(token_response("ya29.cloudsdk"))));
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.cloudsdk");
}

// --- ADC: GCE metadata server fallback ------------------------------------

#[tokio::test]
async fn adc_falls_back_to_metadata_server() {
    let io = MockIo::new().http(|method, url, headers, _b| {
        assert_eq!(method, "GET");
        assert!(url.starts_with(METADATA_PREFIX), "url={url}");
        assert!(
            url.contains("scopes="),
            "metadata request must pass scopes; url={url}"
        );
        assert!(
            headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("Metadata-Flavor") && v == "Google"),
            "metadata request must carry Metadata-Flavor: Google"
        );
        Ok(ok(token_response("ya29.metadata")))
    });
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.metadata");
}

// --- adc_available probe ---------------------------------------------------

#[tokio::test]
async fn adc_available_reflects_discoverability() {
    let with_gac = MockIo::new()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/adc.json")
        .file("/adc.json", "{}");
    assert!(adc_available(&with_gac).await);

    let with_well_known = MockIo::new().env("HOME", "/home/dev").file(
        "/home/dev/.config/gcloud/application_default_credentials.json",
        "{}",
    );
    assert!(adc_available(&with_well_known).await);

    // Nothing discoverable (metadata server is NOT counted by adc_available).
    let none = MockIo::new().env("HOME", "/home/dev");
    assert!(!adc_available(&none).await);
}

// --- Error surfaces --------------------------------------------------------

#[tokio::test]
async fn token_endpoint_non_2xx_is_surfaced() {
    let io = MockIo::new().http(|_m, _u, _h, _b| {
        Ok(HttpResponse {
            status: 401,
            body: r#"{"error":"invalid_grant"}"#.into(),
        })
    });
    let sa = service_account_json("svc@test-project.iam.gserviceaccount.com", TOKEN_URI);
    let err = token_from_service_account_json(&io, &sa, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::TokenEndpoint(_)), "got {err:?}");
}

#[tokio::test]
async fn unsupported_adc_type_is_rejected() {
    let bad = serde_json::json!({"type": "external_account", "audience": "x"}).to_string();
    let io = MockIo::new()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/adc.json")
        .file("/adc.json", &bad);
    let err = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap_err();
    assert!(matches!(err, AuthError::NoCredentials(_)), "got {err:?}");
}

#[tokio::test]
async fn missing_access_token_in_response_is_error() {
    let io = MockIo::new().http(|_m, _u, _h, _b| Ok(ok(r#"{"token_type":"Bearer"}"#.to_string())));
    let sa = service_account_json("svc@test-project.iam.gserviceaccount.com", TOKEN_URI);
    let err = token_from_service_account_json(&io, &sa, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::TokenEndpoint(_)), "got {err:?}");
}

fn percent_decode(s: &str) -> String {
    // Minimal %-decode sufficient for the JWT assertion (only checks part count).
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let h = u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(b'?');
            out.push(h);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
