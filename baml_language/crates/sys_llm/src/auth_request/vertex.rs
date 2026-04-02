//! Vertex AI authentication.
//!
//! On native, uses the `google-cloud-auth` crate (with IO passthrough) for
//! Application Default Credentials and service account key auth.
//!
//! On WASM, `google-cloud-auth` cannot be used because it depends on tokio
//! features (`fs`, `process`) that pull in `mio`, which does not compile on
//! `wasm32-unknown-unknown`. So on WASM we implement service account JWT auth
//! manually using pure-Rust crypto (`rsa` + `sha2`): parse the PKCS8 key,
//! sign a JWT with RSASSA-PKCS1-v1_5/SHA-256, and exchange it for an access
//! token via `HttpSendFn`. ADC is not supported on WASM -- explicit
//! credentials are required.

use crate::{
    BuildRequestCallbacks,
    baml_std::{HttpRequest, PrimitiveClient, ProviderOptions, VertexAiOptions},
    build_request::BuildRequestError,
};

// ---------------------------------------------------------------------------
// Public entry point (shared across native and WASM)
// ---------------------------------------------------------------------------

/// Add Google Cloud `OAuth2` auth headers to a Vertex AI request.
pub(crate) async fn auth_vertex(
    request: &mut HttpRequest,
    client: &PrimitiveClient,
    callbacks: Option<&BuildRequestCallbacks>,
) -> Result<(), BuildRequestError> {
    // If an API key is provided as a query param, skip token-based auth.
    if client.options.query_params.contains_key("key") {
        return Ok(());
    }

    let vertex_opts = match &client.provider_options {
        Some(ProviderOptions::VertexAi(opts)) => Some(opts.clone()),
        _ => None,
    };

    let token = resolve_token(vertex_opts.as_ref(), callbacks).await?;

    request
        .headers
        .insert("authorization".to_string(), format!("Bearer {token}"));

    Ok(())
}

/// Resolve an access token from explicit credentials or ADC.
///
/// Resolution order:
/// 1. `credentials_content` -- inline service account JSON
/// 2. `credentials` -- if valid JSON, treat as inline; otherwise file path
/// 3. ADC (native only)
async fn resolve_token(
    vertex_opts: Option<&VertexAiOptions>,
    callbacks: Option<&BuildRequestCallbacks>,
) -> Result<String, BuildRequestError> {
    // credentials_content: always inline JSON.
    if let Some(json_str) = vertex_opts.and_then(|o| o.credentials_content.as_ref()) {
        return token_from_service_account_json(json_str, callbacks).await;
    }

    // credentials: inline JSON or file path.
    if let Some(creds) = vertex_opts.and_then(|o| o.credentials.as_ref()) {
        if serde_json::from_str::<serde_json::Value>(creds).is_ok() {
            return token_from_service_account_json(creds, callbacks).await;
        }
        let json_str = read_credentials_file(creds, callbacks).await?;
        return token_from_service_account_json(&json_str, callbacks).await;
    }

    // No explicit credentials -- fall back to ADC (native only).
    token_from_adc(callbacks).await
}

/// Read a credentials file via the `FsReadFn` callback.
async fn read_credentials_file(
    path: &str,
    callbacks: Option<&BuildRequestCallbacks>,
) -> Result<String, BuildRequestError> {
    let cb = callbacks.ok_or_else(|| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: cannot read credentials file '{path}' without IO callbacks"
        ))
    })?;
    let bytes = (cb.fs_read)(path.to_string()).await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: failed to read credentials file '{path}': {e}"
        ))
    })?;
    String::from_utf8(bytes).map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: credentials file '{path}' is not valid UTF-8: {e}"
        ))
    })
}

// ===========================================================================
// Native: google-cloud-auth
// ===========================================================================

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use google_cloud_auth::{credentials::Builder, io};

    use super::{BuildRequestCallbacks, BuildRequestError, HttpRequest};

    // -- IO provider adapters (callbacks -> google-cloud-auth traits) --

    pub(super) struct BexEnvProvider {
        pub env_read_fn: crate::EnvReadFn,
    }

    impl std::fmt::Debug for BexEnvProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexEnvProvider").finish()
        }
    }

    impl std::panic::UnwindSafe for BexEnvProvider {}
    impl std::panic::RefUnwindSafe for BexEnvProvider {}

    impl io::EnvProvider for BexEnvProvider {
        fn var(&self, name: &str) -> Option<String> {
            let fut = (self.env_read_fn)(name.to_string());
            let handle = tokio::runtime::Handle::current();
            let result = std::thread::spawn(move || handle.block_on(fut))
                .join()
                .unwrap_or(Err(crate::LlmOpError::Other("thread panicked".into())));
            match result {
                Ok(Some(v)) => Some(v),
                Ok(None) | Err(_) => None,
            }
        }
    }

    pub(super) struct BexFsProvider {
        pub fs_read_fn: crate::FsReadFn,
    }

    impl std::fmt::Debug for BexFsProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexFsProvider").finish()
        }
    }

    impl std::panic::UnwindSafe for BexFsProvider {}
    impl std::panic::RefUnwindSafe for BexFsProvider {}

    impl io::FsProvider for BexFsProvider {
        fn read_to_string(&self, path: &str) -> std::io::Result<String> {
            let fut = (self.fs_read_fn)(path.to_string());
            let handle = tokio::runtime::Handle::current();
            let result = std::thread::spawn(move || handle.block_on(fut))
                .join()
                .unwrap_or(Err(crate::LlmOpError::Other("thread panicked".into())));
            match result {
                Ok(bytes) => String::from_utf8(bytes)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
                Err(_) => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "file not found",
                )),
            }
        }
    }

    pub(super) struct BexHttpClientProvider {
        pub send_fn: crate::HttpSendFn,
    }

    impl std::fmt::Debug for BexHttpClientProvider {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BexHttpClientProvider").finish()
        }
    }

    impl std::panic::UnwindSafe for BexHttpClientProvider {}
    impl std::panic::RefUnwindSafe for BexHttpClientProvider {}

    impl io::HttpClientProvider for BexHttpClientProvider {
        async fn execute(
            &self,
            request: io::HttpRequest,
        ) -> Result<io::HttpResponse, Box<dyn std::error::Error + Send + Sync>> {
            let method = match request.method {
                io::HttpMethod::Get => "GET",
                io::HttpMethod::Post => "POST",
                io::HttpMethod::Put => "PUT",
            };

            let url = if request.query_params.is_empty() {
                request.url.clone()
            } else {
                let params: Vec<String> = request
                    .query_params
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}={}",
                            percent_encoding::utf8_percent_encode(
                                k,
                                percent_encoding::NON_ALPHANUMERIC
                            ),
                            percent_encoding::utf8_percent_encode(
                                v,
                                percent_encoding::NON_ALPHANUMERIC
                            ),
                        )
                    })
                    .collect();
                if request.url.contains('?') {
                    format!("{}&{}", request.url, params.join("&"))
                } else {
                    format!("{}?{}", request.url, params.join("&"))
                }
            };

            let mut headers = indexmap::IndexMap::new();
            for (name, value) in &request.headers {
                headers.insert(name.clone(), value.clone());
            }

            let baml_req = HttpRequest {
                method: method.to_string(),
                url,
                headers,
                // Safe: this adapter is only used for OAuth2 token requests (JSON/form-encoded).
                body: String::from_utf8_lossy(&request.body).into_owned(),
            };

            let resp = (self.send_fn)(baml_req)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let status = io::StatusCode::from_u16(resp.status_code)
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

            let mut response_headers = io::HeaderMap::new();
            for (name, value) in &resp.headers {
                if let (Ok(header_name), Ok(header_value)) = (
                    io::HeaderName::from_bytes(name.as_bytes()),
                    io::HeaderValue::from_str(value),
                ) {
                    response_headers.insert(header_name, header_value);
                }
            }

            Ok(io::HttpResponse {
                status,
                headers: response_headers,
                body: resp.body.into_bytes(),
            })
        }
    }

    // -- Credential builders --

    pub(super) fn build_from_service_account_json(
        json_str: &str,
        callbacks: Option<&BuildRequestCallbacks>,
    ) -> Result<google_cloud_auth::credentials::AccessTokenCredentials, BuildRequestError> {
        let json_value: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse credentials JSON: {e}"
            ))
        })?;

        let mut builder = google_cloud_auth::credentials::service_account::Builder::new(json_value);

        if let Some(cb) = callbacks {
            builder = builder.with_http_client_provider(BexHttpClientProvider {
                send_fn: cb.http_send.clone(),
            });
        }

        builder.build_access_token_credentials().map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to build service account credentials: {e}"
            ))
        })
    }

    pub(super) fn build_from_adc(
        callbacks: Option<&BuildRequestCallbacks>,
    ) -> Result<google_cloud_auth::credentials::AccessTokenCredentials, BuildRequestError> {
        let mut builder =
            Builder::default().with_scopes(["https://www.googleapis.com/auth/cloud-platform"]);

        if let Some(cb) = callbacks {
            builder = builder
                .with_http_client_provider(BexHttpClientProvider {
                    send_fn: cb.http_send.clone(),
                })
                .with_env_provider(BexEnvProvider {
                    env_read_fn: cb.env_read.clone(),
                })
                .with_fs_provider(BexFsProvider {
                    fs_read_fn: cb.fs_read.clone(),
                });
        }

        builder.build_access_token_credentials().map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud ADC: failed to build credentials: {e}"
            ))
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn token_from_service_account_json(
    json_str: &str,
    callbacks: Option<&BuildRequestCallbacks>,
) -> Result<String, BuildRequestError> {
    let creds = native::build_from_service_account_json(json_str, callbacks)?;
    let token = creds.access_token().await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: failed to obtain access token: {e}"
        ))
    })?;
    Ok(token.token)
}

#[cfg(not(target_arch = "wasm32"))]
async fn token_from_adc(
    callbacks: Option<&BuildRequestCallbacks>,
) -> Result<String, BuildRequestError> {
    let creds = native::build_from_adc(callbacks)?;
    let token = creds.access_token().await.map_err(|e| {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud ADC: failed to obtain access token: {e}"
        ))
    })?;
    Ok(token.token)
}

// ===========================================================================
// WASM: manual JWT signing + OAuth2 token exchange
// ===========================================================================

// ==========================================================================
// WASM: pure-Rust JWT signing (rsa + sha2) + OAuth2 token exchange
//
// We use the RustCrypto `rsa` and `sha2` crates instead of the browser's
// `SubtleCrypto` API. This has two advantages:
//   1. It compiles and runs on any WASM host (Node, Deno, Cloudflare Workers,
//      browser) without requiring `window.crypto`.
//   2. Signing is synchronous, so we don't need `spawn_local` or a oneshot
//      channel to bridge `!Send` JS futures.
// ==========================================================================

#[cfg(target_arch = "wasm32")]
mod wasm {
    use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
    use rsa::{RsaPrivateKey, pkcs8::DecodePrivateKey, signature::SignatureEncoding};

    use super::{BuildRequestCallbacks, BuildRequestError, HttpRequest};

    #[derive(serde::Deserialize)]
    pub(super) struct ServiceAccount {
        pub token_uri: String,
        pub client_email: String,
        pub private_key: String,
        #[allow(dead_code)]
        pub private_key_id: Option<String>,
    }

    /// Parse service account JSON, sign a JWT with `rsa`/`sha2`, and exchange
    /// it for an access token via `HttpSendFn`.
    pub(super) async fn service_account_token(
        json_str: &str,
        callbacks: Option<&BuildRequestCallbacks>,
    ) -> Result<String, BuildRequestError> {
        let sa: ServiceAccount = serde_json::from_str(json_str).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse service account JSON: {e}"
            ))
        })?;

        let jwt = sign_jwt(&sa)?;
        exchange_jwt_for_token(&sa.token_uri, &jwt, callbacks).await
    }

    /// Sign a JWT using RSASSA-PKCS1-v1_5 with SHA-256 (pure Rust).
    #[allow(clippy::items_after_statements)]
    pub(super) fn sign_jwt(sa: &ServiceAccount) -> Result<String, BuildRequestError> {
        #[allow(clippy::cast_possible_wrap)]
        let now = web_time::SystemTime::now()
            .duration_since(web_time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let header = serde_json::json!({
            "alg": "RS256",
            "typ": "JWT",
        });
        let claims = serde_json::json!({
            "iss": sa.client_email,
            "scope": "https://www.googleapis.com/auth/cloud-platform",
            "aud": sa.token_uri,
            "iat": now,
            "exp": now + 3600,
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
        let claims_b64 = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signing_input = format!("{header_b64}.{claims_b64}");

        // Parse PEM -> DER -> RsaPrivateKey.
        let private_key = RsaPrivateKey::from_pkcs8_pem(&sa.private_key).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse PKCS8 private key: {e}"
            ))
        })?;

        let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(private_key);

        use rsa::signature::Signer;
        let signature = signing_key.sign(signing_input.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        Ok(format!("{signing_input}.{sig_b64}"))
    }

    /// Exchange a signed JWT for an access token via the token URI.
    pub(super) async fn exchange_jwt_for_token(
        token_uri: &str,
        jwt: &str,
        callbacks: Option<&BuildRequestCallbacks>,
    ) -> Result<String, BuildRequestError> {
        let cb = callbacks.ok_or_else(|| {
            BuildRequestError::AuthorizationFailed(
                "Google Cloud: cannot exchange JWT without HTTP callbacks".into(),
            )
        })?;

        let body = format!(
            "grant_type={}&assertion={}",
            percent_encoding::utf8_percent_encode(
                "urn:ietf:params:oauth:grant-type:jwt-bearer",
                percent_encoding::NON_ALPHANUMERIC,
            ),
            percent_encoding::utf8_percent_encode(jwt, percent_encoding::NON_ALPHANUMERIC),
        );

        let mut headers = indexmap::IndexMap::new();
        headers.insert(
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        );

        let req = HttpRequest {
            method: "POST".to_string(),
            url: token_uri.to_string(),
            headers,
            body,
        };

        let resp = (cb.http_send)(req).await.map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: token exchange HTTP request failed: {e}"
            ))
        })?;

        if resp.status_code < 200 || resp.status_code >= 300 {
            return Err(BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: token exchange returned status {}: {}",
                resp.status_code, resp.body,
            )));
        }

        let token_resp: serde_json::Value = serde_json::from_str(&resp.body).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse token exchange response: {e}"
            ))
        })?;

        token_resp
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| {
                BuildRequestError::AuthorizationFailed(
                    "Google Cloud: token exchange response missing 'access_token'".into(),
                )
            })
    }

    #[cfg(test)]
    mod tests {
        use std::sync::Arc;

        use super::*;

        const TEST_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDUvaOLol62IQRN\nztnkgePa11sFelJ2MbLXcop/0zTyuuY0ZCcF2/Lr/WoSBP1ScH8p4Bc5i/6mX6Qe\nAkpbOpQjIy0bK6kv+7tZauJnqT8KIwyxI/uNt9g8dYO0R1MWP8k0wR9ZTHiZ7YJc\ny7v8xdRxYdQUfSZsDj/DiXbXubzGy8RbJ2OiNKJQhhcQqTUZs3ZwUdjqZW4h6zRS\nzXC1E+s4sWyu4BDLi2nrR/5s7yk9r90tiqYcBBtl/5vRR90NQsQQpel30hEqDhND\nf0mMW5LHCbTEtXd8ohzykahpRWuyGaxrXA9mxVdhHMEwRtLPWb+fNhouCpUY9hxz\n5AmIj1QXAgMBAAECggEACEmO6myb1ep5WXKaaF1q++ZxxEfcmIAdIGl03b/jiyUe\nvKG+J2tHDkxj6mnJWIHLYl05amN6uw50vTqHnQAuLyQ6qJlN0PG0fao9QZ6FNybg\nYrItJXso8En/pHE22mIHu4deakMhW5W2A1lobFNkkDooYdfyPDld4IclWwgAQ5ow\nQ/O4A2UElR4vRl3sm4Tte1RfXmmHkYXQvmMehjAkJ73A82V/hTcrb6fLd6cLkObL\njPTIFSES2ud0w8ysp46Zgw+MYxs0H6U8QeY9c4EFfuf6inZNxiixYi9Vhx1SMlnP\n3EsBhUm98LnC+r5IjO/GcZf0Sjjmk0KR5GN37hs9AQKBgQD7C4WH05su5xCJcWe5\nEQtkblZZYlDoahkzegmPEnw0h/XxP/K/ux8ywgmyY6MdFHUKzdtBLkrvf1dR+iuG\n3N0WKXI0nAN9OkSBx9tl7qKaB5H8Uu6gR0mWws1P4CFxR8XVOwhWKccGWj4UW8aS\nSCmFUsHEiEJU9crDS4EIvvrHgQKBgQDY8JLr1utG4GhgvoV5FNYYmZyRmj1vpwk6\nSJWp87988bgajaqK2c3q3rdqeY2BL7YdGlzDslhokbGlqnHTNS9S42KwroYoKDSr\nvcVyIaLPpIvuWQZKYIRjzgcoQkMIL5Di6QulJ2h75bV9849Dg6Vo/UxWO0WdE0+7\nh+lwVb8nlwKBgB5uHxl/xOfCinaekHwWXNMnrL/Y8wW5FqTuvgnhq7ySXnWH0tz6\nyaVVb+d3vGXh/O36VgFooxy0ytjdAjmuu/3buEQ4RRQA5Bz3JNkOPBd/o2p6gwJa\nocjshAaSnHsmwAxAw5nuJnnWpn/BQCirJp1KksJH4gJ6aMGTfWiZ/bwBAoGBAMl0\nZktB8oyH+gXVBueQ1NxVUdLYU7Lqf6QzIWCIbMsfQOLPqY51gkZYeiUTKbfM0aYn\nA/vrEzRQD5MTO85xtjeX1t7Rwt1psLfHa6J339RJLnSxESliha6U9YqKNetVGIvO\n9DRy6xEbGLYUxnZguutLRWdSdWvPMhyosrvRtMiTAoGBAMs/Z/KLnVffZaU5LAlV\nIR5WlJ0MyQojG9w5iBiJEYcs/xtS9fraXmhgzpnjIa7xNrSHP8b2HF9gnj9RnK1P\nxJcNFKVyi0gDpRPt5Cy4McHQ2kFPmdzeEcIClJO2Mgw7r8lUFbkZqs1jfM7kVv6o\no2RMFg65EnEU9EsYPZKkZlZr\n-----END PRIVATE KEY-----\n";

        fn test_sa_json() -> String {
            serde_json::json!({
                "type": "service_account",
                "project_id": "test-project",
                "private_key_id": "key-id-123",
                "private_key": TEST_PRIVATE_KEY,
                "client_email": "test@test-project.iam.gserviceaccount.com",
                "client_id": "123456789",
                "auth_uri": "https://accounts.google.com/o/oauth2/auth",
                "token_uri": "https://oauth2.googleapis.com/token",
            })
            .to_string()
        }

        fn mock_callbacks(http_send: crate::HttpSendFn) -> BuildRequestCallbacks {
            BuildRequestCallbacks {
                http_send,
                env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
                fs_read: Arc::new(|_path| {
                    Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
                }),
            }
        }

        fn mock_token_http() -> crate::HttpSendFn {
            Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.wasm-test",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            })
        }

        #[test]
        fn sign_jwt_produces_valid_three_part_token() {
            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let parts: Vec<&str> = jwt.split('.').collect();
            assert_eq!(parts.len(), 3, "JWT should have header.claims.sig");
        }

        #[test]
        fn sign_jwt_header_is_rs256() {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let header_b64 = jwt.split('.').next().unwrap();
            let header: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header_b64).unwrap()).unwrap();
            assert_eq!(header["alg"], "RS256");
            assert_eq!(header["typ"], "JWT");
        }

        #[test]
        fn sign_jwt_claims_have_expected_fields() {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let claims_b64 = jwt.split('.').nth(1).unwrap();
            let claims: serde_json::Value =
                serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims_b64).unwrap()).unwrap();
            assert_eq!(claims["iss"], "test@test-project.iam.gserviceaccount.com");
            assert_eq!(
                claims["scope"],
                "https://www.googleapis.com/auth/cloud-platform"
            );
            assert_eq!(claims["aud"], "https://oauth2.googleapis.com/token");
            assert!(claims["iat"].is_number());
            assert!(claims["exp"].is_number());
        }

        #[test]
        fn sign_jwt_signature_verifies() {
            use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
            use rsa::signature::Verifier;

            let sa: ServiceAccount = serde_json::from_str(&test_sa_json()).unwrap();
            let jwt = sign_jwt(&sa).unwrap();
            let parts: Vec<&str> = jwt.split('.').collect();
            let signing_input = format!("{}.{}", parts[0], parts[1]);
            let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();

            let private_key = RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY).unwrap();
            let public_key = private_key.to_public_key();
            let verifying_key = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(public_key);
            let signature = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice()).unwrap();
            verifying_key
                .verify(signing_input.as_bytes(), &signature)
                .expect("JWT signature should verify");
        }

        #[test]
        fn sign_jwt_rejects_invalid_pem() {
            let sa = ServiceAccount {
                token_uri: "https://oauth2.googleapis.com/token".into(),
                client_email: "test@test.iam.gserviceaccount.com".into(),
                private_key: "not-a-real-pem".into(),
                private_key_id: None,
            };
            assert!(sign_jwt(&sa).is_err());
        }

        #[tokio::test]
        async fn exchange_jwt_rejects_non_200() {
            let cb = mock_callbacks(Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 401,
                        headers: indexmap::IndexMap::new(),
                        body: r#"{"error": "invalid_grant"}"#.to_string(),
                    })
                })
            }));
            let result = exchange_jwt_for_token(
                "https://oauth2.googleapis.com/token",
                "fake.jwt.here",
                Some(&cb),
            )
            .await;
            let err = result.unwrap_err().to_string();
            assert!(err.contains("401"), "should mention status: {err}");
        }

        #[tokio::test]
        async fn exchange_jwt_rejects_missing_access_token() {
            let cb = mock_callbacks(Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: r#"{"token_type": "Bearer"}"#.to_string(),
                    })
                })
            }));
            let result = exchange_jwt_for_token(
                "https://oauth2.googleapis.com/token",
                "fake.jwt.here",
                Some(&cb),
            )
            .await;
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("access_token"),
                "should mention missing field: {err}"
            );
        }

        #[tokio::test]
        async fn exchange_jwt_sends_correct_request() {
            use std::sync::Mutex;

            let captured = Arc::new(Mutex::new(None));
            let captured_clone = captured.clone();
            let cb = mock_callbacks(Arc::new(move |req| {
                *captured_clone.lock().unwrap() = Some(req.clone());
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.test",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            }));

            let token = exchange_jwt_for_token(
                "https://oauth2.googleapis.com/token",
                "my.test.jwt",
                Some(&cb),
            )
            .await
            .unwrap();
            assert_eq!(token, "ya29.test");

            let req = captured.lock().unwrap().take().unwrap();
            assert_eq!(req.method, "POST");
            assert_eq!(req.url, "https://oauth2.googleapis.com/token");
            assert_eq!(
                req.headers.get("content-type").unwrap(),
                "application/x-www-form-urlencoded"
            );
            assert!(req.body.contains("grant_type="), "body: {}", req.body);
            assert!(req.body.contains("assertion="), "body: {}", req.body);
        }

        #[tokio::test]
        async fn service_account_token_full_flow() {
            use std::sync::Mutex;

            let captured = Arc::new(Mutex::new(None));
            let captured_clone = captured.clone();
            let cb = mock_callbacks(Arc::new(move |req| {
                *captured_clone.lock().unwrap() = Some(req.clone());
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.wasm-full-flow",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            }));

            let token = service_account_token(&test_sa_json(), Some(&cb))
                .await
                .unwrap();
            assert_eq!(token, "ya29.wasm-full-flow");

            // Verify the HTTP request contained a real signed JWT
            let req = captured.lock().unwrap().take().unwrap();
            assert_eq!(req.method, "POST");

            // Extract the assertion (JWT) from the URL-encoded body
            let assertion = req
                .body
                .split('&')
                .find(|p| p.starts_with("assertion="))
                .unwrap()
                .strip_prefix("assertion=")
                .unwrap();
            let jwt = percent_encoding::percent_decode_str(assertion)
                .decode_utf8()
                .unwrap();
            let parts: Vec<&str> = jwt.split('.').collect();
            assert_eq!(parts.len(), 3, "assertion should be a 3-part JWT");
        }

        #[tokio::test]
        async fn service_account_token_rejects_bad_json() {
            let cb = mock_callbacks(mock_token_http());
            let result = service_account_token("not json", Some(&cb)).await;
            assert!(result.is_err());
        }

        #[tokio::test]
        async fn exchange_jwt_requires_callbacks() {
            let result =
                exchange_jwt_for_token("https://oauth2.googleapis.com/token", "fake.jwt", None)
                    .await;
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("HTTP callbacks"),
                "should mention callbacks: {err}"
            );
        }
    }
}

#[cfg(target_arch = "wasm32")]
async fn token_from_service_account_json(
    json_str: &str,
    callbacks: Option<&BuildRequestCallbacks>,
) -> Result<String, BuildRequestError> {
    wasm::service_account_token(json_str, callbacks).await
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::unused_async)] // must be async to match native signature
async fn token_from_adc(
    _callbacks: Option<&BuildRequestCallbacks>,
) -> Result<String, BuildRequestError> {
    Err(BuildRequestError::AuthorizationFailed(
        "Google Cloud ADC is not supported on WASM. \
         Provide explicit credentials via 'credentials' or 'credentials_content'."
            .into(),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::baml_std::PrimitiveClientOptions;

    fn make_client(provider: &str) -> PrimitiveClient {
        PrimitiveClient::new(
            "test-google".to_string(),
            provider.to_string(),
            PrimitiveClientOptions {
                model: Some("gemini-pro".to_string()),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn fake_request() -> HttpRequest {
        HttpRequest {
            method: "POST".to_string(),
            url: "https://us-central1-aiplatform.googleapis.com/v1/projects/test/locations/us-central1/publishers/google/models/gemini-pro:generateContent".to_string(),
            headers: indexmap::IndexMap::new(),
            body: r#"{"contents":[]}"#.to_string(),
        }
    }

    #[tokio::test]
    async fn fails_without_credentials() {
        let callbacks = BuildRequestCallbacks {
            http_send: Arc::new(|_req| {
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 404,
                        headers: indexmap::IndexMap::new(),
                        body: String::new(),
                    })
                })
            }),
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(|_path| {
                Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
            }),
        };
        let client = make_client("vertex-ai");
        let mut req = fake_request();
        let result = auth_vertex(&mut req, &client, Some(&callbacks)).await;
        assert!(result.is_err());
    }

    // A test RSA private key for service account credential tests.
    const TEST_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDUvaOLol62IQRN\nztnkgePa11sFelJ2MbLXcop/0zTyuuY0ZCcF2/Lr/WoSBP1ScH8p4Bc5i/6mX6Qe\nAkpbOpQjIy0bK6kv+7tZauJnqT8KIwyxI/uNt9g8dYO0R1MWP8k0wR9ZTHiZ7YJc\ny7v8xdRxYdQUfSZsDj/DiXbXubzGy8RbJ2OiNKJQhhcQqTUZs3ZwUdjqZW4h6zRS\nzXC1E+s4sWyu4BDLi2nrR/5s7yk9r90tiqYcBBtl/5vRR90NQsQQpel30hEqDhND\nf0mMW5LHCbTEtXd8ohzykahpRWuyGaxrXA9mxVdhHMEwRtLPWb+fNhouCpUY9hxz\n5AmIj1QXAgMBAAECggEACEmO6myb1ep5WXKaaF1q++ZxxEfcmIAdIGl03b/jiyUe\nvKG+J2tHDkxj6mnJWIHLYl05amN6uw50vTqHnQAuLyQ6qJlN0PG0fao9QZ6FNybg\nYrItJXso8En/pHE22mIHu4deakMhW5W2A1lobFNkkDooYdfyPDld4IclWwgAQ5ow\nQ/O4A2UElR4vRl3sm4Tte1RfXmmHkYXQvmMehjAkJ73A82V/hTcrb6fLd6cLkObL\njPTIFSES2ud0w8ysp46Zgw+MYxs0H6U8QeY9c4EFfuf6inZNxiixYi9Vhx1SMlnP\n3EsBhUm98LnC+r5IjO/GcZf0Sjjmk0KR5GN37hs9AQKBgQD7C4WH05su5xCJcWe5\nEQtkblZZYlDoahkzegmPEnw0h/XxP/K/ux8ywgmyY6MdFHUKzdtBLkrvf1dR+iuG\n3N0WKXI0nAN9OkSBx9tl7qKaB5H8Uu6gR0mWws1P4CFxR8XVOwhWKccGWj4UW8aS\nSCmFUsHEiEJU9crDS4EIvvrHgQKBgQDY8JLr1utG4GhgvoV5FNYYmZyRmj1vpwk6\nSJWp87988bgajaqK2c3q3rdqeY2BL7YdGlzDslhokbGlqnHTNS9S42KwroYoKDSr\nvcVyIaLPpIvuWQZKYIRjzgcoQkMIL5Di6QulJ2h75bV9849Dg6Vo/UxWO0WdE0+7\nh+lwVb8nlwKBgB5uHxl/xOfCinaekHwWXNMnrL/Y8wW5FqTuvgnhq7ySXnWH0tz6\nyaVVb+d3vGXh/O36VgFooxy0ytjdAjmuu/3buEQ4RRQA5Bz3JNkOPBd/o2p6gwJa\nocjshAaSnHsmwAxAw5nuJnnWpn/BQCirJp1KksJH4gJ6aMGTfWiZ/bwBAoGBAMl0\nZktB8oyH+gXVBueQ1NxVUdLYU7Lqf6QzIWCIbMsfQOLPqY51gkZYeiUTKbfM0aYn\nA/vrEzRQD5MTO85xtjeX1t7Rwt1psLfHa6J339RJLnSxESliha6U9YqKNetVGIvO\n9DRy6xEbGLYUxnZguutLRWdSdWvPMhyosrvRtMiTAoGBAMs/Z/KLnVffZaU5LAlV\nIR5WlJ0MyQojG9w5iBiJEYcs/xtS9fraXmhgzpnjIa7xNrSHP8b2HF9gnj9RnK1P\nxJcNFKVyi0gDpRPt5Cy4McHQ2kFPmdzeEcIClJO2Mgw7r8lUFbkZqs1jfM7kVv6o\no2RMFg65EnEU9EsYPZKkZlZr\n-----END PRIVATE KEY-----\n";

    fn test_service_account_json() -> String {
        serde_json::json!({
            "type": "service_account",
            "project_id": "test-project",
            "private_key_id": "key-id-123",
            "private_key": TEST_PRIVATE_KEY,
            "client_email": "test@test-project.iam.gserviceaccount.com",
            "client_id": "123456789",
            "auth_uri": "https://accounts.google.com/o/oauth2/auth",
            "token_uri": "https://oauth2.googleapis.com/token",
        })
        .to_string()
    }

    fn make_client_with_vertex_opts(opts: VertexAiOptions) -> PrimitiveClient {
        use bex_external_types::AsBexExternalValue;
        PrimitiveClient::new(
            "test-google".to_string(),
            "vertex-ai".to_string(),
            PrimitiveClientOptions {
                model: Some("gemini-pro".to_string()),
                provider_options: opts.into_bex_external_value(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn mock_token_http() -> crate::HttpSendFn {
        Arc::new(|_req| {
            Box::pin(async {
                Ok(crate::HttpSendResponse {
                    status_code: 200,
                    headers: indexmap::IndexMap::new(),
                    body: serde_json::json!({
                        "access_token": "ya29.from-service-account",
                        "token_type": "Bearer",
                        "expires_in": 3600,
                    })
                    .to_string(),
                })
            })
        })
    }

    fn stub_callbacks_with_http(http_send: crate::HttpSendFn) -> BuildRequestCallbacks {
        BuildRequestCallbacks {
            http_send,
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(|_path| {
                Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
            }),
        }
    }

    fn assert_bearer_token(req: &HttpRequest) {
        let auth = req
            .headers
            .get("authorization")
            .expect("missing authorization header");
        assert!(
            auth.starts_with("Bearer "),
            "expected Bearer token, got: {auth}"
        );
    }

    #[tokio::test]
    async fn credentials_content_inline_json() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials_content: Some(test_service_account_json()),
            credentials: None,
        });
        let callbacks = stub_callbacks_with_http(mock_token_http());
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Some(&callbacks))
            .await
            .unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn credentials_inline_json() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some(test_service_account_json()),
            credentials_content: None,
        });
        let callbacks = stub_callbacks_with_http(mock_token_http());
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Some(&callbacks))
            .await
            .unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn credentials_file_path() {
        let sa_json = test_service_account_json();
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials: Some("/fake/service-account.json".to_string()),
            credentials_content: None,
        });
        let callbacks = BuildRequestCallbacks {
            http_send: mock_token_http(),
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(move |path| {
                let sa_json = sa_json.clone();
                Box::pin(async move {
                    if path == "/fake/service-account.json" {
                        Ok(sa_json.into_bytes())
                    } else {
                        Err(crate::LlmOpError::Other("not found".into()))
                    }
                })
            }),
        };
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Some(&callbacks))
            .await
            .unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn credentials_content_takes_precedence() {
        let client = make_client_with_vertex_opts(VertexAiOptions {
            credentials_content: Some(test_service_account_json()),
            credentials: Some("/should/not/be/read.json".to_string()),
        });
        let callbacks = stub_callbacks_with_http(mock_token_http());
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Some(&callbacks))
            .await
            .unwrap();
        assert_bearer_token(&req);
    }

    #[tokio::test]
    async fn skips_auth_when_key_query_param_present() {
        let client = PrimitiveClient::new(
            "test-google".to_string(),
            "vertex-ai".to_string(),
            PrimitiveClientOptions {
                model: Some("gemini-pro".to_string()),
                query_params: indexmap::IndexMap::from([(
                    "key".to_string(),
                    "my-api-key".to_string(),
                )]),
                ..Default::default()
            },
        )
        .unwrap();
        let callbacks = BuildRequestCallbacks {
            http_send: Arc::new(|_req| {
                Box::pin(async {
                    panic!("http_send should not be called when key query param is set");
                })
            }),
            env_read: Arc::new(|_key| Box::pin(async { Ok(None) })),
            fs_read: Arc::new(|_path| {
                Box::pin(async { Err(crate::LlmOpError::Other("not found".into())) })
            }),
        };
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Some(&callbacks))
            .await
            .unwrap();
        assert!(!req.headers.contains_key("authorization"));
    }

    // -----------------------------------------------------------------------
    // JWT signing tests (exercises the same pure-Rust rsa+sha2 logic used
    // on WASM, run on native to verify correctness).
    // -----------------------------------------------------------------------

    /// Build a signed JWT from test credentials using rsa+sha2 (same as WASM path).
    fn sign_jwt_with_rsa(sa_json: &str) -> String {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use rsa::{
            pkcs8::DecodePrivateKey,
            signature::{SignatureEncoding, Signer},
        };

        #[derive(serde::Deserialize)]
        struct Sa {
            token_uri: String,
            client_email: String,
            private_key: String,
        }
        let sa: Sa = serde_json::from_str(sa_json).unwrap();

        let header = serde_json::json!({"alg": "RS256", "typ": "JWT"});
        let claims = serde_json::json!({
            "iss": sa.client_email,
            "scope": "https://www.googleapis.com/auth/cloud-platform",
            "aud": sa.token_uri,
            "iat": 1_000_000,
            "exp": 1_003_600,
        });

        let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
        let claims_b64 = URL_SAFE_NO_PAD.encode(claims.to_string());
        let signing_input = format!("{header_b64}.{claims_b64}");

        let private_key = rsa::RsaPrivateKey::from_pkcs8_pem(&sa.private_key).unwrap();
        let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(private_key);
        let signature = signing_key.sign(signing_input.as_bytes());
        let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

        format!("{signing_input}.{sig_b64}")
    }

    #[test]
    fn jwt_has_three_dot_separated_parts() {
        let jwt = sign_jwt_with_rsa(&test_service_account_json());
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT should have 3 parts: {jwt}");
    }

    #[test]
    fn jwt_header_is_rs256() {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

        let jwt = sign_jwt_with_rsa(&test_service_account_json());
        let header_b64 = jwt.split('.').next().unwrap();
        let header_bytes = URL_SAFE_NO_PAD.decode(header_b64).unwrap();
        let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();
        assert_eq!(header["alg"], "RS256");
        assert_eq!(header["typ"], "JWT");
    }

    #[test]
    fn jwt_claims_contain_expected_fields() {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

        let jwt = sign_jwt_with_rsa(&test_service_account_json());
        let claims_b64 = jwt.split('.').nth(1).unwrap();
        let claims_bytes = URL_SAFE_NO_PAD.decode(claims_b64).unwrap();
        let claims: serde_json::Value = serde_json::from_slice(&claims_bytes).unwrap();
        assert_eq!(claims["iss"], "test@test-project.iam.gserviceaccount.com");
        assert_eq!(
            claims["scope"],
            "https://www.googleapis.com/auth/cloud-platform"
        );
        assert_eq!(claims["aud"], "https://oauth2.googleapis.com/token");
    }

    #[test]
    fn jwt_signature_is_valid() {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
        use rsa::{pkcs8::DecodePrivateKey, signature::Verifier};

        let jwt = sign_jwt_with_rsa(&test_service_account_json());
        let parts: Vec<&str> = jwt.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();

        let private_key = rsa::RsaPrivateKey::from_pkcs8_pem(TEST_PRIVATE_KEY).unwrap();
        let public_key = private_key.to_public_key();
        let verifying_key = rsa::pkcs1v15::VerifyingKey::<sha2::Sha256>::new(public_key);
        let signature = rsa::pkcs1v15::Signature::try_from(sig_bytes.as_slice()).unwrap();

        verifying_key
            .verify(signing_input.as_bytes(), &signature)
            .expect("JWT signature verification failed");
    }

    #[test]
    fn jwt_sign_rejects_invalid_pem() {
        use rsa::pkcs8::DecodePrivateKey;
        let result = rsa::RsaPrivateKey::from_pkcs8_pem("not-a-real-pem");
        assert!(result.is_err());
    }

    /// Confirms that the injected env/fs/http providers are actually used
    /// by the google-cloud-auth ADC flow.
    #[tokio::test]
    async fn adc_with_injected_providers() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let http_call_count = Arc::new(AtomicUsize::new(0));
        let http_call_count_clone = http_call_count.clone();

        let adc_json = serde_json::json!({
            "client_id": "test-client-id",
            "client_secret": "test-client-secret",
            "refresh_token": "test-refresh-token",
            "type": "authorized_user",
            "token_uri": "https://fake-oauth.example.com/token",
        })
        .to_string();

        let callbacks = BuildRequestCallbacks {
            http_send: Arc::new(move |_req| {
                http_call_count_clone.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    Ok(crate::HttpSendResponse {
                        status_code: 200,
                        headers: indexmap::IndexMap::new(),
                        body: serde_json::json!({
                            "access_token": "ya29.fake-test-token",
                            "token_type": "Bearer",
                            "expires_in": 3600,
                        })
                        .to_string(),
                    })
                })
            }),
            env_read: Arc::new(|key| {
                Box::pin(async move {
                    match key.as_str() {
                        "GOOGLE_APPLICATION_CREDENTIALS" => Ok(Some("/fake/adc.json".to_string())),
                        _ => Ok(None),
                    }
                })
            }),
            fs_read: Arc::new(move |path| {
                let adc_json = adc_json.clone();
                Box::pin(async move {
                    if path == "/fake/adc.json" {
                        Ok(adc_json.into_bytes())
                    } else {
                        Err(crate::LlmOpError::Other("not found".into()))
                    }
                })
            }),
        };

        let client = make_client("vertex-ai");
        let mut req = fake_request();
        auth_vertex(&mut req, &client, Some(&callbacks))
            .await
            .unwrap();

        assert!(http_call_count.load(Ordering::SeqCst) > 0);
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer ya29.fake-test-token",
        );
    }
}
