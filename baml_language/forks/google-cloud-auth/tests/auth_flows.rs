#![allow(clippy::doc_markdown)]

//! Integration coverage for every GCP credential flow the fork supports, driven
//! through the public API with a mock `TokenIo`. Mirrors the real
//! `google-auth` behaviors: service-account JWT-bearer, ADC `authorized_user`
//! refresh grant, ADC `service_account`, workload identity federation
//! (file/url subject tokens, optional impersonation), impersonated service
//! accounts, workforce refresh grants, well-known ADC path discovery, and the
//! GCE metadata server.
//!
//! NOTE: minted tokens are cached process-wide keyed by (credential material,
//! scope). Every test therefore uses UNIQUE credential material (distinct
//! emails / refresh tokens) so tests cannot serve each other's cached tokens.

mod common;

use common::{MockIo, authorized_user_json, service_account_json, token_response};
use forked_google_cloud_auth::{
    AuthError, CLOUD_PLATFORM_SCOPE, HttpResponse, adc_available, token_from_adc,
    token_from_credentials_json, token_from_service_account_json,
};

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const STS_URL: &str = "https://sts.googleapis.com/v1/token";
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
    let sa = service_account_json("svc-jwt@test-project.iam.gserviceaccount.com", TOKEN_URI);
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
    let sa = service_account_json("svc-custom@test-project.iam.gserviceaccount.com", custom);
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
    // to exercise encoding (`1//refreshgrant` -> `1%2F%2Frefreshgrant`).
    let adc = authorized_user_json("cidabc123", "1//refreshgrant", TOKEN_URI);
    let io = MockIo::new()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/adc.json")
        .file("/adc.json", &adc)
        .http(|_m, url, _h, body| {
            assert_eq!(url, TOKEN_URI);
            assert!(body.contains("grant_type=refresh_token"), "body={body}");
            assert!(body.contains("client_id=cidabc123"), "body={body}");
            assert!(
                body.contains("refresh_token=1%2F%2Frefreshgrant"),
                "body={body}"
            );
            Ok(ok(token_response("ya29.user")))
        });
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.user");
}

// --- ADC: service_account discovered via GOOGLE_APPLICATION_CREDENTIALS ----

#[tokio::test]
async fn adc_service_account_via_gac_signs_jwt() {
    let sa = service_account_json("svc-gac@test-project.iam.gserviceaccount.com", TOKEN_URI);
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

// --- ADC: GOOGLE_APPLICATION_CREDENTIALS set but unreadable ----------------

#[tokio::test]
async fn gac_set_but_unreadable_is_an_error_not_a_fallthrough() {
    // google-auth parity: a set-but-broken GAC must NOT silently fall through
    // to the well-known file or metadata server.
    let adc = authorized_user_json("cid", "rt-should-not-be-used", TOKEN_URI);
    let io = MockIo::new()
        .env("GOOGLE_APPLICATION_CREDENTIALS", "/missing/adc.json")
        .env("HOME", "/home/dev")
        .file(
            "/home/dev/.config/gcloud/application_default_credentials.json",
            &adc,
        )
        .http(|_m, _u, _h, _b| Ok(ok(token_response("ya29.never"))));
    let err = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap_err();
    match &err {
        AuthError::NoCredentials(msg) => {
            assert!(msg.contains("/missing/adc.json"), "msg={msg}");
        }
        other => panic!("expected NoCredentials, got {other:?}"),
    }
    assert_eq!(io.http_calls(), 0, "must not fall through to other sources");
}

// --- ADC: well-known config path discovery (no GAC env) -------------------

#[tokio::test]
async fn adc_discovers_well_known_path_from_home() {
    let adc = authorized_user_json("cid", "rt-home", TOKEN_URI);
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
    let adc = authorized_user_json("cid", "rt-sdk", TOKEN_URI);
    let io = MockIo::new()
        .env("CLOUDSDK_CONFIG", "/custom/gcloud")
        .file("/custom/gcloud/application_default_credentials.json", &adc)
        .http(|_m, _url, _h, _b| Ok(ok(token_response("ya29.cloudsdk"))));
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.cloudsdk");
}

#[tokio::test]
async fn adc_discovers_well_known_path_from_appdata() {
    // Windows: no HOME; %APPDATA%\gcloud is the config dir.
    let adc = authorized_user_json("cid", "rt-appdata", TOKEN_URI);
    let io = MockIo::new()
        .env("APPDATA", "C:/Users/dev/AppData/Roaming")
        .file(
            "C:/Users/dev/AppData/Roaming/gcloud/application_default_credentials.json",
            &adc,
        )
        .http(|_m, _url, _h, _b| Ok(ok(token_response("ya29.appdata"))));
    let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
    assert_eq!(token, "ya29.appdata");
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

// --- Workload identity federation (external_account) -----------------------

fn wif_json(audience_tag: &str, extra: &serde_json::Value) -> String {
    let mut doc = serde_json::json!({
        "type": "external_account",
        "audience": format!("//iam.googleapis.com/projects/123/locations/global/workloadIdentityPools/{audience_tag}/providers/oidc"),
        "subject_token_type": "urn:ietf:params:oauth:token-type:jwt",
        "token_url": STS_URL,
    });
    for (k, v) in extra.as_object().unwrap() {
        doc[k] = v.clone();
    }
    doc.to_string()
}

#[tokio::test]
async fn wif_file_sourced_subject_token_exchanges_at_sts() {
    let wif = wif_json(
        "pool-file",
        &serde_json::json!({ "credential_source": { "file": "/var/run/oidc/token" } }),
    );
    let io = MockIo::new()
        .file("/var/run/oidc/token", "  oidc-subject-token\n")
        .http(|method, url, _h, body| {
            assert_eq!(method, "POST");
            assert_eq!(url, STS_URL);
            assert!(
                body.contains(
                    "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant%2Dtype%3Atoken%2Dexchange"
                ),
                "must use the token-exchange grant; body={body}"
            );
            assert!(
                body.contains("subject_token=oidc%2Dsubject%2Dtoken"),
                "subject token must be trimmed and sent; body={body}"
            );
            assert!(body.contains("audience="), "body={body}");
            assert!(
                body.contains("pool%2Dfile"),
                "audience must round-trip; body={body}"
            );
            Ok(ok(token_response("ya29.wif-file")))
        });
    let token = token_from_credentials_json(&io, &wif, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.wif-file");
}

#[tokio::test]
async fn wif_url_sourced_json_subject_token() {
    let wif = wif_json(
        "pool-url",
        &serde_json::json!({
            "credential_source": {
                "url": "https://issuer.example.com/token",
                "headers": { "Metadata": "True" },
                "format": { "type": "json", "subject_token_field_name": "id_token" },
            }
        }),
    );
    let io = MockIo::new().http(|method, url, headers, body| {
        if method == "GET" {
            assert_eq!(url, "https://issuer.example.com/token");
            assert!(
                headers.iter().any(|(k, v)| k == "Metadata" && v == "True"),
                "credential_source.headers must be forwarded"
            );
            return Ok(ok(r#"{"id_token":"url-subject-token"}"#.to_string()));
        }
        assert_eq!(url, STS_URL);
        assert!(
            body.contains("subject_token=url%2Dsubject%2Dtoken"),
            "body={body}"
        );
        Ok(ok(token_response("ya29.wif-url")))
    });
    let token = token_from_credentials_json(&io, &wif, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.wif-url");
    assert_eq!(io.http_calls(), 2, "subject fetch + STS exchange");
}

#[tokio::test]
async fn wif_with_impersonation_calls_generate_access_token() {
    let impersonation_url = "https://iamcredentials.googleapis.com/v1/projects/-/serviceAccounts/target@p.iam.gserviceaccount.com:generateAccessToken";
    let wif = wif_json(
        "pool-imp",
        &serde_json::json!({
            "credential_source": { "file": "/oidc/imp-token" },
            "service_account_impersonation_url": impersonation_url,
        }),
    );
    let io = MockIo::new().file("/oidc/imp-token", "imp-subject").http(
        move |method, url, headers, body| {
            if url == STS_URL {
                return Ok(ok(token_response("sts-intermediate")));
            }
            assert_eq!(method, "POST");
            assert_eq!(url, impersonation_url);
            assert!(
                headers
                    .iter()
                    .any(|(k, v)| k == "authorization" && v == "Bearer sts-intermediate"),
                "impersonation must authenticate with the STS token; headers={headers:?}"
            );
            assert!(
                body.contains(CLOUD_PLATFORM_SCOPE),
                "caller scope applied at impersonation; body={body}"
            );
            Ok(ok(serde_json::json!({
                "accessToken": "ya29.wif-impersonated",
                "expireTime": "2035-01-01T00:00:00Z",
            })
            .to_string()))
        },
    );
    let token = token_from_credentials_json(&io, &wif, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.wif-impersonated");
    assert_eq!(io.http_calls(), 2, "STS exchange + generateAccessToken");
}

#[tokio::test]
async fn wif_workforce_pool_sends_user_project_without_client_auth() {
    let wif = serde_json::json!({
        "type": "external_account",
        "audience": "//iam.googleapis.com/locations/global/workforcePools/pool-wf/providers/oidc",
        "subject_token_type": "urn:ietf:params:oauth:token-type:id_token",
        "token_url": STS_URL,
        "workforce_pool_user_project": "wf-user-project",
        "credential_source": { "file": "/oidc/wf-token" },
    })
    .to_string();
    let io = MockIo::new()
        .file("/oidc/wf-token", "wf-subject")
        .http(|_m, _u, headers, body| {
            assert!(
                !headers.iter().any(|(k, _)| k == "authorization"),
                "no client auth configured"
            );
            assert!(
                body.contains("options=") && body.contains("userProject"),
                "workforce user project must ride in options; body={body}"
            );
            Ok(ok(token_response("ya29.workforce")))
        });
    let token = token_from_credentials_json(&io, &wif, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.workforce");
}

// --- Workforce refresh grant (external_account_authorized_user) ------------

#[tokio::test]
async fn external_account_authorized_user_refreshes_at_sts_with_basic_auth() {
    let doc = serde_json::json!({
        "type": "external_account_authorized_user",
        "audience": "//iam.googleapis.com/locations/global/workforcePools/pool-eaau/providers/oidc",
        "client_id": "eaau-client",
        "client_secret": "eaau-secret",
        "refresh_token": "eaau-refresh",
        "token_url": STS_URL,
    })
    .to_string();
    let io = MockIo::new().http(|method, url, headers, body| {
        assert_eq!(method, "POST");
        assert_eq!(url, STS_URL);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v.starts_with("Basic ")),
            "must authenticate with Basic client auth; headers={headers:?}"
        );
        assert!(body.contains("grant_type=refresh_token"), "body={body}");
        assert!(body.contains("refresh_token=eaau%2Drefresh"), "body={body}");
        Ok(ok(token_response("ya29.eaau")))
    });
    let token = token_from_credentials_json(&io, &doc, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.eaau");
}

// --- Impersonated service account ------------------------------------------

#[tokio::test]
async fn impersonated_service_account_exchanges_source_token() {
    let impersonation_url = "https://iamcredentials.googleapis.com/v1/projects/-/serviceAccounts/robot@p.iam.gserviceaccount.com:generateAccessToken";
    let doc = serde_json::json!({
        "type": "impersonated_service_account",
        "service_account_impersonation_url": impersonation_url,
        "delegates": ["delegate@p.iam.gserviceaccount.com"],
        "source_credentials": {
            "type": "authorized_user",
            "client_id": "imp-cid",
            "client_secret": "imp-secret",
            "refresh_token": "imp-refresh",
            "token_uri": TOKEN_URI,
        },
    })
    .to_string();
    let io = MockIo::new().http(move |_m, url, headers, body| {
        if url == TOKEN_URI {
            assert!(body.contains("refresh_token=imp%2Drefresh"), "body={body}");
            return Ok(ok(token_response("source-user-token")));
        }
        assert_eq!(url, impersonation_url);
        assert!(
            headers
                .iter()
                .any(|(k, v)| k == "authorization" && v == "Bearer source-user-token"),
            "impersonation must use the source token; headers={headers:?}"
        );
        assert!(
            body.contains("delegates"),
            "delegates forwarded; body={body}"
        );
        Ok(ok(serde_json::json!({
            "accessToken": "ya29.impersonated",
            "expireTime": "2035-01-01T00:00:00Z",
        })
        .to_string()))
    });
    let token = token_from_credentials_json(&io, &doc, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(token, "ya29.impersonated");
    assert_eq!(io.http_calls(), 2, "source refresh + generateAccessToken");
}

// --- Token caching ----------------------------------------------------------

#[tokio::test]
async fn tokens_are_cached_until_near_expiry() {
    // Long-lived token: second mint must be served from cache (no HTTP).
    let adc = authorized_user_json("cid", "rt-cache-long", TOKEN_URI);
    let io = MockIo::new().http(|_m, _u, _h, _b| Ok(ok(token_response("ya29.cached"))));
    let first = token_from_credentials_json(&io, &adc, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    let second = token_from_credentials_json(&io, &adc, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(first, "ya29.cached");
    assert_eq!(second, "ya29.cached");
    assert_eq!(io.http_calls(), 1, "second call must hit the cache");

    // Token expiring inside the 3m45s refresh threshold: always re-minted.
    let adc_short = authorized_user_json("cid", "rt-cache-short", TOKEN_URI);
    let io_short = MockIo::new().http(|_m, _u, _h, _b| {
        Ok(ok(serde_json::json!({
            "access_token": "ya29.short",
            "token_type": "Bearer",
            "expires_in": 10,
        })
        .to_string()))
    });
    token_from_credentials_json(&io_short, &adc_short, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    token_from_credentials_json(&io_short, &adc_short, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap();
    assert_eq!(
        io_short.http_calls(),
        2,
        "near-expiry tokens must be re-minted"
    );
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
    let sa = service_account_json("svc-err@test-project.iam.gserviceaccount.com", TOKEN_URI);
    let err = token_from_service_account_json(&io, &sa, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::TokenEndpoint(_)), "got {err:?}");
}

#[tokio::test]
async fn unsupported_credential_sources_are_rejected_clearly() {
    let io = MockIo::new();

    // GDCH: supported by google-auth, deliberately not by this fork.
    let gdch = serde_json::json!({ "type": "gdch_service_account" }).to_string();
    let err = token_from_credentials_json(&io, &gdch, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::Unsupported(_)), "got {err:?}");

    // AWS-sourced WIF.
    let aws = wif_json(
        "pool-aws",
        &serde_json::json!({ "credential_source": {
            "environment_id": "aws1",
            "region_url": "http://169.254.169.254/latest/meta-data/placement/availability-zone",
        }}),
    );
    let err = token_from_credentials_json(&io, &aws, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::Unsupported(_)), "got {err:?}");

    // Executable-sourced WIF.
    let exe = wif_json(
        "pool-exe",
        &serde_json::json!({ "credential_source": { "executable": { "command": "/bin/oidc" } } }),
    );
    let err = token_from_credentials_json(&io, &exe, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::Unsupported(_)), "got {err:?}");

    // Unknown type.
    let unknown = serde_json::json!({ "type": "totally_bogus" }).to_string();
    let err = token_from_credentials_json(&io, &unknown, CLOUD_PLATFORM_SCOPE)
        .await
        .unwrap_err();
    assert!(matches!(err, AuthError::NoCredentials(_)), "got {err:?}");
}

#[tokio::test]
async fn missing_access_token_in_response_is_error() {
    let io = MockIo::new().http(|_m, _u, _h, _b| Ok(ok(r#"{"token_type":"Bearer"}"#.to_string())));
    let sa = service_account_json(
        "svc-missing@test-project.iam.gserviceaccount.com",
        TOKEN_URI,
    );
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
