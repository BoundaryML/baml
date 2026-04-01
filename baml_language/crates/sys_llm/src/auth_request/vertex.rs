//! Vertex AI authentication.
//!
//! On native, uses the `google-cloud-auth` crate (with IO passthrough) for
//! Application Default Credentials and service account key auth.
//!
//! On WASM, `google-cloud-auth` cannot be used because it depends on tokio
//! features (`fs`, `process`) that pull in `mio`, which does not compile on
//! `wasm32-unknown-unknown`. This is the same class of problem the old engine
//! had with `gcp_auth` (which depended on `ring`). So on WASM we implement
//! service account JWT auth manually: parse the key, sign a JWT with rustls,
//! and exchange it for an access token via `HttpSendFn`. ADC is not supported
//! on WASM -- explicit credentials are required.

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
    if request.headers.contains_key("authorization") {
        return Ok(());
    }

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

#[cfg(target_arch = "wasm32")]
mod wasm {
    use base64::{
        Engine,
        engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    };
    use js_sys::{Array, Object, Uint8Array};
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{CryptoKey, SubtleCrypto};

    use super::{BuildRequestCallbacks, BuildRequestError, HttpRequest};

    #[derive(serde::Deserialize)]
    pub(super) struct ServiceAccount {
        pub token_uri: String,
        pub client_email: String,
        pub private_key: String,
        #[allow(dead_code)]
        pub private_key_id: Option<String>,
    }

    /// Parse service account JSON, sign a JWT via browser `SubtleCrypto`, and
    /// exchange it for an access token via `HttpSendFn`.
    pub(super) async fn service_account_token(
        json_str: &str,
        callbacks: Option<&BuildRequestCallbacks>,
    ) -> Result<String, BuildRequestError> {
        let sa: ServiceAccount = serde_json::from_str(json_str).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to parse service account JSON: {e}"
            ))
        })?;

        let jwt = sign_jwt(&sa).await?;
        exchange_jwt_for_token(&sa.token_uri, &jwt, callbacks).await
    }

    #[allow(clippy::needless_pass_by_value)] // JsValue is a u32 index, cheap to move; map_err requires owned
    fn js_err(e: JsValue) -> BuildRequestError {
        BuildRequestError::AuthorizationFailed(format!(
            "Google Cloud: JavaScript crypto error: {e:?}"
        ))
    }

    /// Sign a JWT using the browser's `SubtleCrypto` API (`RSASSA-PKCS1-v1_5` with `SHA-256`).
    async fn sign_jwt(sa: &ServiceAccount) -> Result<String, BuildRequestError> {
        let subtle = get_subtle_crypto()?;

        // Build JWT header and claims.
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

        // Parse PEM private key to DER.
        let pem = sa
            .private_key
            .trim()
            .replace("-----BEGIN PRIVATE KEY-----", "")
            .replace("-----END PRIVATE KEY-----", "")
            .replace('\n', "");
        let key_data = STANDARD.decode(&pem).map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to decode private key base64: {e}"
            ))
        })?;

        // Import the key via SubtleCrypto.
        let import_params = Object::new();
        js_sys::Reflect::set(&import_params, &"name".into(), &"RSASSA-PKCS1-v1_5".into())
            .map_err(js_err)?;
        js_sys::Reflect::set(&import_params, &"hash".into(), &"SHA-256".into()).map_err(js_err)?;

        let key_usage = Array::new();
        key_usage.push(&"sign".into());

        let key: CryptoKey = JsFuture::from(
            subtle
                .import_key_with_object(
                    "pkcs8",
                    &Uint8Array::from(&key_data[..]),
                    &import_params,
                    false,
                    &key_usage,
                )
                .map_err(js_err)?,
        )
        .await
        .map_err(js_err)?
        .into();

        // Sign the JWT.
        let sign_params = Object::new();
        js_sys::Reflect::set(&sign_params, &"name".into(), &"RSASSA-PKCS1-v1_5".into())
            .map_err(js_err)?;

        let signature = JsFuture::from(
            subtle
                .sign_with_object_and_u8_array(&sign_params, &key, signing_input.as_bytes())
                .map_err(js_err)?,
        )
        .await
        .map_err(js_err)?;

        let sig_array = Uint8Array::new(&signature);
        let mut sig_vec = vec![0u8; sig_array.length() as usize];
        sig_array.copy_to(&mut sig_vec);

        let sig_b64 = URL_SAFE_NO_PAD.encode(&sig_vec);
        Ok(format!("{signing_input}.{sig_b64}"))
    }

    fn get_subtle_crypto() -> Result<SubtleCrypto, BuildRequestError> {
        let window = web_sys::window().ok_or_else(|| {
            BuildRequestError::AuthorizationFailed(
                "Google Cloud: SubtleCrypto requires a browser window object".into(),
            )
        })?;
        let crypto = window.crypto().map_err(|e| {
            BuildRequestError::AuthorizationFailed(format!(
                "Google Cloud: failed to access window.crypto: {e:?}"
            ))
        })?;
        Ok(crypto.subtle())
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
    async fn skips_auth_when_authorization_header_present() {
        let client = make_client("vertex-ai");
        let mut req = fake_request();
        req.headers.insert(
            "authorization".to_string(),
            "Bearer manual-token".to_string(),
        );
        auth_vertex(&mut req, &client, None).await.unwrap();
        assert_eq!(
            req.headers.get("authorization").unwrap(),
            "Bearer manual-token"
        );
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
