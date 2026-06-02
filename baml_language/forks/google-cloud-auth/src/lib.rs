//! A slim, pure-Rust Google Cloud OAuth2 access-token resolver.
//!
//! Replaces `google-cloud-auth` for BAML's use case: minting Google Cloud
//! access tokens for Vertex AI. All IO (env, file, HTTP) is routed through the
//! [`TokenIo`] trait so the host can sandbox it; JWT signing is pure Rust
//! (`rsa` + `sha2`), so a single code path works on both native and wasm.
//!
//! Supported credential sources:
//! - **Service account** JSON — RS256 JWT bearer assertion exchanged for a
//!   token at the account's `token_uri`.
//! - **Application Default Credentials** — `authorized_user` JSON (OAuth2
//!   refresh-token grant), `service_account` JSON, or the GCE metadata server,
//!   discovered from `GOOGLE_APPLICATION_CREDENTIALS` or the well-known ADC
//!   config path.

#![allow(clippy::doc_markdown)]

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rsa::{
    RsaPrivateKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};

/// The default OAuth2 cloud-platform scope used for Vertex AI.
pub const CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

// ---------------------------------------------------------------------------
// IO abstraction
// ---------------------------------------------------------------------------

/// A minimal HTTP response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Async IO operations the resolver needs. Implemented by the host, typically
/// by delegating to a sandboxed runtime.
#[async_trait]
pub trait TokenIo: Send + Sync {
    /// Read an environment variable. Returns `None` if unset.
    async fn env(&self, key: &str) -> Option<String>;

    /// Read a file to a string. Returns `None` if it cannot be read.
    async fn read_file(&self, path: &str) -> Option<String>;

    /// Perform an HTTP request with the given extra headers and body.
    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, AuthError>;
}

/// Errors produced while resolving a Google Cloud access token.
#[derive(Debug, Clone)]
pub enum AuthError {
    /// A credential payload could not be parsed.
    Parse(String),
    /// JWT signing failed (e.g. an invalid private key).
    Signing(String),
    /// An IO operation failed.
    Io(String),
    /// The token endpoint returned a non-success status or an unexpected body.
    TokenEndpoint(String),
    /// No credential source could be discovered.
    NoCredentials(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Parse(m)
            | AuthError::Signing(m)
            | AuthError::Io(m)
            | AuthError::TokenEndpoint(m)
            | AuthError::NoCredentials(m) => write!(f, "Google Cloud: {m}"),
        }
    }
}

impl std::error::Error for AuthError {}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Mint an access token from service-account JSON via the RS256 JWT-bearer flow.
pub async fn token_from_service_account_json(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<String, AuthError> {
    let sa: ServiceAccount = serde_json::from_str(json_str)
        .map_err(|e| AuthError::Parse(format!("failed to parse service account JSON: {e}")))?;
    let jwt = sign_service_account_jwt(&sa, scope)?;
    let token_uri = sa.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
    let body = format!(
        "grant_type={}&assertion={}",
        encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
        encode(&jwt),
    );
    post_token(io, token_uri, &body).await
}

/// Returns `true` when Application Default Credentials look discoverable (an
/// inline/file `GOOGLE_APPLICATION_CREDENTIALS` or the well-known ADC config
/// file). Does not perform any network IO. Mirrors the resolution-time probe
/// the AWS SDK performs.
pub async fn adc_available(io: &dyn TokenIo) -> bool {
    if let Some(val) = io.env("GOOGLE_APPLICATION_CREDENTIALS").await {
        if !val.is_empty() && io.read_file(&val).await.is_some() {
            return true;
        }
    }
    if let Some(path) = adc_config_path(io).await {
        if io.read_file(&path).await.is_some() {
            return true;
        }
    }
    false
}

/// Mint an access token via Application Default Credentials: the
/// `GOOGLE_APPLICATION_CREDENTIALS` file, the well-known ADC config file, then
/// the GCE metadata server.
pub async fn token_from_adc(io: &dyn TokenIo, scope: &str) -> Result<String, AuthError> {
    // 1. GOOGLE_APPLICATION_CREDENTIALS file path.
    if let Some(val) = io.env("GOOGLE_APPLICATION_CREDENTIALS").await {
        if !val.is_empty() {
            if let Some(contents) = io.read_file(&val).await {
                return token_from_adc_json(io, &contents, scope).await;
            }
        }
    }

    // 2. Well-known ADC config file.
    if let Some(path) = adc_config_path(io).await {
        if let Some(contents) = io.read_file(&path).await {
            return token_from_adc_json(io, &contents, scope).await;
        }
    }

    // 3. GCE metadata server.
    token_from_metadata(io, scope).await
}

// ---------------------------------------------------------------------------
// ADC JSON dispatch
// ---------------------------------------------------------------------------

async fn token_from_adc_json(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<String, AuthError> {
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| AuthError::Parse(format!("failed to parse ADC JSON: {e}")))?;
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("authorized_user") => token_from_authorized_user(io, &value).await,
        Some("service_account") => token_from_service_account_json(io, json_str, scope).await,
        Some(other) => Err(AuthError::NoCredentials(format!(
            "unsupported ADC credential type '{other}' (only service_account and authorized_user are supported)"
        ))),
        None => Err(AuthError::Parse(
            "ADC JSON missing 'type' field".to_string(),
        )),
    }
}

/// OAuth2 refresh-token grant for `authorized_user` ADC credentials.
async fn token_from_authorized_user(
    io: &dyn TokenIo,
    value: &serde_json::Value,
) -> Result<String, AuthError> {
    let field = |k: &str| {
        value
            .get(k)
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AuthError::Parse(format!("authorized_user ADC missing '{k}'")))
    };
    let client_id = field("client_id")?;
    let client_secret = field("client_secret")?;
    let refresh_token = field("refresh_token")?;
    let token_uri = value
        .get("token_uri")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_TOKEN_URI);

    let body = format!(
        "grant_type=refresh_token&client_id={}&client_secret={}&refresh_token={}",
        encode(client_id),
        encode(client_secret),
        encode(refresh_token),
    );
    post_token(io, token_uri, &body).await
}

/// Fetch a token from the GCE metadata server.
async fn token_from_metadata(io: &dyn TokenIo, scope: &str) -> Result<String, AuthError> {
    let url = format!("{METADATA_TOKEN_URL}?scopes={}", encode(scope));
    let headers = vec![("Metadata-Flavor".to_string(), "Google".to_string())];
    let resp = io.http("GET", &url, &headers, "").await?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(AuthError::NoCredentials(format!(
            "no ADC credentials found and GCE metadata server returned status {}",
            resp.status
        )));
    }
    extract_access_token(&resp.body)
}

// ---------------------------------------------------------------------------
// Token endpoint helpers
// ---------------------------------------------------------------------------

/// POST a form-encoded body to an OAuth2 token endpoint and return the token.
async fn post_token(io: &dyn TokenIo, url: &str, body: &str) -> Result<String, AuthError> {
    let headers = vec![(
        "content-type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    )];
    let resp = io.http("POST", url, &headers, body).await?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(AuthError::TokenEndpoint(format!(
            "token endpoint returned status {}: {}",
            resp.status, resp.body
        )));
    }
    extract_access_token(&resp.body)
}

fn extract_access_token(body: &str) -> Result<String, AuthError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| AuthError::TokenEndpoint(format!("failed to parse token response: {e}")))?;
    value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .ok_or_else(|| {
            AuthError::TokenEndpoint("token response missing 'access_token'".to_string())
        })
}

// ---------------------------------------------------------------------------
// Service account JWT signing (RS256, pure Rust)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
}

/// Sign a service-account JWT using RSASSA-PKCS1-v1_5 with SHA-256.
fn sign_service_account_jwt(sa: &ServiceAccount, scope: &str) -> Result<String, AuthError> {
    #[allow(clippy::cast_possible_wrap)]
    let now = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let token_uri = sa.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
    let header = serde_json::json!({ "alg": "RS256", "typ": "JWT" });
    let claims = serde_json::json!({
        "iss": sa.client_email,
        "scope": scope,
        "aud": token_uri,
        "iat": now,
        "exp": now + 3600,
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let claims_b64 = URL_SAFE_NO_PAD.encode(claims.to_string());
    let signing_input = format!("{header_b64}.{claims_b64}");

    let private_key = RsaPrivateKey::from_pkcs8_pem(&sa.private_key)
        .map_err(|e| AuthError::Signing(format!("failed to parse PKCS8 private key: {e}")))?;
    let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(private_key);

    let signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    Ok(format!("{signing_input}.{sig_b64}"))
}

// ---------------------------------------------------------------------------
// ADC config path discovery
// ---------------------------------------------------------------------------

/// The well-known ADC config file path
/// (`$CLOUDSDK_CONFIG/application_default_credentials.json` or
/// `$HOME/.config/gcloud/application_default_credentials.json`).
async fn adc_config_path(io: &dyn TokenIo) -> Option<String> {
    let config_dir = if let Some(dir) = io.env("CLOUDSDK_CONFIG").await.filter(|s| !s.is_empty()) {
        dir
    } else {
        // Unix uses HOME; Windows uses APPDATA\gcloud, but BAML targets Unix +
        // wasm for this path. Fall back to HOME.
        let home = io.env("HOME").await.filter(|s| !s.is_empty())?;
        format!("{home}/.config/gcloud")
    };
    Some(format!("{config_dir}/application_default_credentials.json"))
}

fn encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn gen_sa_json() -> String {
        use rsa::pkcs8::EncodePrivateKey;
        let key = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let pem = key
            .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
            .unwrap()
            .to_string();
        serde_json::json!({
            "type": "service_account",
            "project_id": "test-project",
            "private_key_id": "key-id",
            "private_key": pem,
            "client_email": "test@test-project.iam.gserviceaccount.com",
            "client_id": "123",
            "token_uri": "https://oauth2.googleapis.com/token",
        })
        .to_string()
    }

    /// Records the last HTTP request and returns a canned response.
    struct MockIo {
        env: std::collections::HashMap<String, String>,
        files: std::collections::HashMap<String, String>,
        status: u16,
        resp_body: String,
        last: Mutex<Option<(String, String, String)>>, // (method, url, body)
    }

    impl MockIo {
        fn token(access_token: &str) -> Self {
            Self {
                env: std::collections::HashMap::new(),
                files: std::collections::HashMap::new(),
                status: 200,
                resp_body: serde_json::json!({
                    "access_token": access_token,
                    "token_type": "Bearer",
                    "expires_in": 3600,
                })
                .to_string(),
                last: Mutex::new(None),
            }
        }
    }

    #[async_trait]
    impl TokenIo for MockIo {
        async fn env(&self, key: &str) -> Option<String> {
            self.env.get(key).cloned()
        }
        async fn read_file(&self, path: &str) -> Option<String> {
            self.files.get(path).cloned()
        }
        async fn http(
            &self,
            method: &str,
            url: &str,
            _headers: &[(String, String)],
            body: &str,
        ) -> Result<HttpResponse, AuthError> {
            *self.last.lock().unwrap() =
                Some((method.to_string(), url.to_string(), body.to_string()));
            Ok(HttpResponse {
                status: self.status,
                body: self.resp_body.clone(),
            })
        }
    }

    #[tokio::test]
    async fn service_account_mints_token_with_jwt_assertion() {
        let io = MockIo::token("ya29.sa");
        let token = token_from_service_account_json(&io, &gen_sa_json(), CLOUD_PLATFORM_SCOPE)
            .await
            .unwrap();
        assert_eq!(token, "ya29.sa");
        let (method, url, body) = io.last.lock().unwrap().clone().unwrap();
        assert_eq!(method, "POST");
        assert_eq!(url, "https://oauth2.googleapis.com/token");
        assert!(
            body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant%2Dtype%3Ajwt%2Dbearer")
        );
        // assertion is a 3-part JWT (header.claims.sig), each part non-empty.
        let assertion = body
            .split('&')
            .find_map(|p| p.strip_prefix("assertion="))
            .unwrap();
        let jwt = percent_encoding::percent_decode_str(assertion)
            .decode_utf8()
            .unwrap();
        assert_eq!(jwt.split('.').count(), 3);
    }

    #[tokio::test]
    async fn authorized_user_uses_refresh_grant() {
        let mut io = MockIo::token("ya29.refresh");
        let adc = serde_json::json!({
            "type": "authorized_user",
            "client_id": "cid",
            "client_secret": "secret",
            "refresh_token": "rtok",
        })
        .to_string();
        io.env.insert(
            "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
            "/adc.json".to_string(),
        );
        io.files.insert("/adc.json".to_string(), adc);

        let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
        assert_eq!(token, "ya29.refresh");
        let (_, url, body) = io.last.lock().unwrap().clone().unwrap();
        assert_eq!(url, "https://oauth2.googleapis.com/token");
        assert!(body.contains("grant_type=refresh_token"));
        assert!(body.contains("client_id=cid"));
        assert!(body.contains("refresh_token=rtok"));
    }

    #[tokio::test]
    async fn adc_falls_back_to_metadata_server() {
        // No env, no files -> metadata server.
        let io = MockIo::token("ya29.metadata");
        let token = token_from_adc(&io, CLOUD_PLATFORM_SCOPE).await.unwrap();
        assert_eq!(token, "ya29.metadata");
        let (method, url, _) = io.last.lock().unwrap().clone().unwrap();
        assert_eq!(method, "GET");
        assert!(url.starts_with(
            "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token"
        ));
    }

    #[tokio::test]
    async fn adc_available_detects_gac_file() {
        let mut io = MockIo::token("");
        io.env.insert(
            "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
            "/adc.json".to_string(),
        );
        io.files.insert("/adc.json".to_string(), "{}".to_string());
        assert!(adc_available(&io).await);

        let empty = MockIo::token("");
        assert!(!adc_available(&empty).await);
    }

    #[tokio::test]
    async fn token_endpoint_error_is_surfaced() {
        let mut io = MockIo::token("");
        io.status = 401;
        io.resp_body = r#"{"error":"invalid_grant"}"#.to_string();
        let err = token_from_service_account_json(&io, &gen_sa_json(), CLOUD_PLATFORM_SCOPE)
            .await
            .unwrap_err();
        assert!(matches!(err, AuthError::TokenEndpoint(_)));
    }

    #[test]
    fn rejects_invalid_private_key() {
        let sa = ServiceAccount {
            client_email: "x@y.iam.gserviceaccount.com".into(),
            private_key: "not-a-pem".into(),
            token_uri: None,
        };
        assert!(sign_service_account_jwt(&sa, CLOUD_PLATFORM_SCOPE).is_err());
    }
}
