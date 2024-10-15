use anyhow::{Context, Result};
use axum::{extract::Path, extract::Query, routing::get, Router};
use base64::{engine::general_purpose, Engine as _};
use derive_more::Constructor;
use etcetera::AppStrategy;
use indexmap::IndexMap;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use reqwest;
use reqwest::RequestBuilder;
use serde::Deserialize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use web_time::SystemTime;

fn app_strategy() -> Result<impl AppStrategy> {
    Ok(etcetera::choose_app_strategy(etcetera::AppStrategyArgs {
        top_level_domain: "com".to_string(),
        author: "boundaryml".to_string(),
        app_name: "baml-cli".to_string(),
    })?)
}

const PROPELAUTH_CLI_REDIRECT_ADDR: &str = "127.0.0.1:24000";

pub(crate) struct PropelAuthClient {
    auth_url: String,
    client_id: String,
    client: reqwest::Client,
}

/// The result of exchanging an authorization code for an access and refresh token.
///
/// https://docs.propelauth.com/reference/api/oauth2#token-endpoint
/// https://www.rfc-editor.org/rfc/rfc6749#section-4.1.3
#[derive(Debug, Deserialize)]
pub struct GetAccessTokenResponse {
    pub access_token: String,
    pub expires_in: u32,
    pub refresh_token: String,
}

/// User info, based on the provided access token. Used to test whether the cached
/// access token is still valid.
///
/// https://docs.propelauth.com/reference/api/oauth2#user-info-endpoint
#[derive(Debug, Deserialize)]
pub struct GetUserInfoResponse {
    pub email: String,
    org_id_to_org_info: IndexMap<String, OrgInfo>,
}

#[derive(Debug, Deserialize)]
struct OrgInfo {
    org_id: String,
    org_name: String,
}

/// TODO: a lot of this should be replaced with the oauth2 crate or openidconnect crate, neither of which
/// I realized existed until after I finished writing all this.
impl PropelAuthClient {
    pub fn new() -> Self {
        let client = reqwest::Client::new();
        if std::env::var("BOUNDARY_API_ENV").as_deref() == Ok("test") {
            Self {
                auth_url: "https://681310426.propelauthtest.com".to_string(),
                client_id: "64ae726d05cddb6a46c541a8e0ff5e4a".to_string(),
                client,
            }
        } else {
            Self {
                auth_url: "https://auth.boundaryml.com".to_string(),
                client_id: "f09552c069706a76d5f3e9a113e7cdfe".to_string(),
                client,
            }
        }
    }

    pub(crate) async fn run_authorization_code_flow(&self) -> Result<PersistedTokenData> {
        let code_verifier = generate_code_verifier();

        let (code, redirect_uri) = self.request_authorization_code(&code_verifier).await?;
        let resp = self
            .request_access_token(&code, &redirect_uri, &code_verifier)
            .await?;
        Ok(PersistedTokenData::new(
            resp.access_token,
            SystemTime::now() + Duration::from_secs(resp.expires_in as u64),
            resp.refresh_token,
        ))
    }

    pub(crate) async fn request_authorization_code(
        &self,
        code_verifier: &str,
    ) -> Result<(String, String)> {
        let (tx, mut rx) = mpsc::channel(1);
        let (handle, redirect_uri) = start_redirect_server(tx).await?;

        // Generate code challenge
        let code_challenge = generate_code_challenge(code_verifier);

        // https://www.rfc-editor.org/rfc/rfc6749#section-4.1.1
        let state = format!("csrf-state-{}", generate_code_verifier());

        // Construct the authorization URL
        let auth_url = reqwest::Url::parse_with_params(
            &format!("{}/propelauth/oauth/authorize", self.auth_url),
            &[
                ("redirect_uri", redirect_uri.as_str()),
                ("client_id", self.client_id.as_str()),
                ("response_type", "code"),
                ("state", state.as_str()),
                ("code_challenge", code_challenge.as_str()),
                ("code_challenge_method", "S256"),
            ],
        )?;

        println!("Press Enter to sign in with your browser...");
        let _ = io::stdin().read_line(&mut String::new());

        match open::that(auth_url.as_str()) {
            Ok(_) => (),
            Err(e) => {
                println!("Click here to login: {}", auth_url);
            }
        }

        // Wait for the code from the channel
        let params = rx
            .recv()
            .await
            .context("Timed out waiting for the authorization server")?;

        if params.state != state {
            anyhow::bail!("CSRF state mismatch");
        }

        log::debug!("Received authorization callback: {:?}", params);
        Ok((params.code, redirect_uri))
    }

    pub(crate) async fn request_access_token(
        &self,
        code: &str,
        redirect_uri: &str,
        code_verifier: &str,
    ) -> Result<GetAccessTokenResponse> {
        // Make the POST request
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/propelauth/oauth/token", self.auth_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("code", &code),
                ("redirect_uri", &redirect_uri),
                ("grant_type", "authorization_code"),
                ("code_verifier", code_verifier),
            ])
            .send()
            .await?;

        let body: GetAccessTokenResponse = response
            .json()
            .await
            .context("Failed to parse access token response")?;
        Ok(body)
    }

    // TODO: test what happens when user is not logged in
    pub(crate) async fn get_user_info(&self, access_token: &str) -> Result<GetUserInfoResponse> {
        // let home_dir = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Unable to get home directory"))?;
        // let creds_path = home_dir.join(".config").join("boundaryml").join("creds.json");

        // if !creds_path.exists() {
        //     return Ok(false);
        // }

        // let creds_content = fs::read_to_string(creds_path)?;
        // let creds: Value = serde_json::from_str(&creds_content)?;

        // let access_token = creds["file"]["token"]
        //     .as_str()
        //     .ok_or_else(|| anyhow::anyhow!("Access token not found in creds.json"))?;

        let client = reqwest::Client::new();
        let response = client
            .get(format!("{}/propelauth/oauth/userinfo", self.auth_url))
            .bearer_auth(access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(response
                .error_for_status()
                .context("Failed to fetch user info")
                .unwrap_err());
        }

        let resp_body: serde_json::Value = response.json().await?;

        log::debug!(
            "Signed in as (full propelauth GetUserInfo): {:#?}",
            resp_body
        );

        let resp_body: GetUserInfoResponse = serde_json::from_value(resp_body)?;

        Ok(resp_body)
    }

    pub fn post(&self, path: &str) -> RequestBuilder {
        self.client.post(format!("{}/{}", self.auth_url, path))
    }
}

#[derive(Deserialize, Debug)]
struct AuthCallback {
    code: String,
    state: String,
}

async fn start_redirect_server(
    tx: mpsc::Sender<AuthCallback>,
) -> Result<(tokio::task::JoinHandle<Result<()>>, String)> {
    let app = Router::new();

    let app = app.route(
        "/api/auth/callback",
        get(move |Query(params): Query<AuthCallback>| {
            let tx = tx.clone();
            async move {
                let _ = tx.send(params).await;
                "Authorization successful! You can close this tab and return to the CLI."
                    .to_string()
            }
        }),
    );

    // WARNING: do NOT change this, the oauth config in propelauth requires this port to be 24000!
    let listener = TcpListener::bind(PROPELAUTH_CLI_REDIRECT_ADDR).await?;
    let addr = listener.local_addr()?;
    let redirect_uri = format!("http://localhost:{}/api/auth/callback", addr.port());
    log::debug!("Redirect handler listening at {}", redirect_uri);

    let server = axum::serve(listener, app);

    let handle = tokio::spawn(async move { server.await.map_err(anyhow::Error::from) });

    Ok((handle, redirect_uri))
}

fn generate_code_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let result = hasher.finalize();
    general_purpose::URL_SAFE_NO_PAD.encode(result)
}

/// Data persisted to disk in the project's config directory.
/// Stores the user's refresh token.
#[derive(Constructor, Debug, Deserialize, Serialize)]
pub(crate) struct PersistedTokenData {
    access_token: String,
    access_token_expires_at: SystemTime,
    refresh_token: String,
}

/// The result of exchanging a refresh token for an access and refresh token.
///
/// https://docs.propelauth.com/reference/api/oauth2#refresh-token-endpoint
/// https://www.rfc-editor.org/rfc/rfc6749#section-6
#[derive(Debug, Deserialize)]
struct RefreshAccessTokenResponse {
    access_token: String,
    expires_in: u32,
    refresh_token: String,
}

impl PersistedTokenData {
    pub(crate) async fn access_token(&mut self) -> Result<&str> {
        log::debug!(
            "access_token_expires_at: {:?}",
            self.access_token_expires_at
        );
        if self.access_token_expires_at < SystemTime::now() + Duration::from_secs(30) {
            let new_token = self.refresh_access_token().await?;
            self.access_token = new_token.access_token;
            self.access_token_expires_at =
                SystemTime::now() + Duration::from_secs(new_token.expires_in as u64);
            self.refresh_token = new_token.refresh_token;
        }

        Ok(&self.access_token)
    }

    async fn refresh_access_token(&mut self) -> Result<RefreshAccessTokenResponse> {
        let client = PropelAuthClient::new();
        let response = client
            .post("/propelauth/oauth/token")
            .header("Content-Type", "application/x-www-form-urlencoded")
            .form(&[
                ("client_id", client.client_id.as_str()),
                ("refresh_token", self.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to refresh access token: {}", response.text().await?);
        }

        let body: RefreshAccessTokenResponse = response
            .json()
            .await
            .context("Failed to parse refresh access token response")?;

        Ok(body)
    }

    pub(crate) fn read_from_storage() -> Result<Self> {
        let creds_path = app_strategy()
            .context("Unable to get project directories")?
            .in_config_dir("creds.json");

        // TODO: if these fail we should tell the user to login
        if !creds_path.exists() {
            anyhow::bail!("No credentials found");
        }

        // TODO: if these fail we should tell the user to login
        let creds_content = std::fs::read_to_string(creds_path)?;
        let creds: Self = serde_json::from_str(&creds_content)?;

        Ok(creds)
    }

    pub(crate) fn write_to_storage(&self) -> Result<()> {
        let strategy = app_strategy().context("Unable to get project directories")?;
        let config_dir = strategy.config_dir();
        std::fs::create_dir_all(&config_dir)?;
        let creds_path = config_dir.join("creds.json");

        let creds_content = serde_json::to_string(&self)?;

        log::debug!("Writing credentials to {creds_path:?}");
        std::fs::write(creds_path, creds_content)?;

        Ok(())
    }
}

/// Generate a string suitable for use as a code verifier in an OAuth2 PKCE flow.
///
/// In practice, this is a 128-char alnum string backed by 762 bits of OS
/// entropy (see [`rand::rngs::ThreadRng`])
///
/// Implementation notes:
///
///   - [RFC 7636, Section 4.1] allows the code verifier to be 43 to 128
///   characters long, consisting of ALPHA / DIGIT / "-" / "." / "_" / "~"; for
///   simplicity, we use the [`Alphanumeric`] distribution (i.e. 62 possible
///   chars) instead of all 66 possible chars.
///
///   - [RFC 7636, Section 7.1] requires that the code challenge contain at
///   least 256 bits of entropy (this is where the 43 char minimum comes from:
///   `log2(66 ** 43) == 259`); our implementation has `log2(62 ** 128) == 762`
///   bits of entropy.
///
/// [RFC 7636, Section 4.1]: https://www.rfc-editor.org/rfc/rfc7636#section-4.1
/// [RFC 7636, Section 7.1]: https://www.rfc-editor.org/rfc/rfc7636#section-7.1
fn generate_code_verifier() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(128)
        .map(char::from)
        .collect()
}
