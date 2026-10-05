//! Native Boundary identity client shared by the CLI, SDK hosts and packed hosts.
//! Only the refresh credential and caller profile persist, in the OS credential store.
use std::{fmt, io::Read as _, time::Duration};

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{Error, Result, require};

pub const DEFAULT_API_URL: &str = "https://api.cloud.boundaryml.com";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

pub use crate::credentials::Secret;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Caller {
    pub user_id: String,
    pub email: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceLogin {
    pub verification_uri: String,
    pub user_code: String,
    pub device_code: Secret,
    pub expires_at: u64,
    pub interval_seconds: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub access_token: Secret,
    pub expires_at: u64,
    pub refresh_token: Secret,
    pub caller: Caller,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum LoginPoll {
    Pending {
        #[serde(rename = "intervalSeconds")]
        interval_seconds: u32,
    },
    Approved {
        session: Session,
    },
    Denied,
    Expired,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSession {
    pub refresh_token: Secret,
    pub caller: Caller,
}
impl From<&Session> for StoredSession {
    fn from(session: &Session) -> Self {
        Self {
            refresh_token: session.refresh_token.clone(),
            caller: session.caller.clone(),
        }
    }
}

/// Canonical endpoint: credentials for one origin are never sent to another.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint(String);
impl Endpoint {
    pub fn from_env() -> Result<Self> {
        Self::with_default(None)
    }
    pub fn with_default(configured: Option<&str>) -> Result<Self> {
        let override_url = baml_env::string_var("BOUNDARY_API_URL")?;
        Self::parse(
            override_url
                .as_deref()
                .or(configured)
                .unwrap_or(DEFAULT_API_URL),
        )
    }
    pub fn parse(value: &str) -> Result<Self> {
        let url = Url::parse(value)?;
        let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
        if !(url.scheme() == "https" || (url.scheme() == "http" && loopback)) {
            return Err(Error::Endpoint(
                "Boundary API URL must use HTTPS, or HTTP on loopback",
            ));
        }
        if !(url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none())
        {
            return Err(Error::Endpoint(
                "Boundary API URL must be a base URL without credentials, query or fragment",
            ));
        }
        Ok(Self(url.as_str().trim_end_matches('/').to_owned()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub use crate::response::HttpFailure;

pub struct Client {
    pub(crate) endpoint: Endpoint,
    pub(crate) http: reqwest::blocking::Client,
}
impl Client {
    pub fn new(endpoint: Endpoint) -> Result<Self> {
        Ok(Self {
            endpoint,
            http: reqwest::blocking::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(Error::transport)?,
        })
    }
    pub fn start(&self) -> Result<DeviceLogin> {
        self.post("/v1/auth/device", &serde_json::json!({}))
    }
    pub fn poll(&self, code: &Secret) -> Result<LoginPoll> {
        self.post(
            "/v1/auth/device/poll",
            &serde_json::json!({"deviceCode":code}),
        )
    }
    pub fn refresh(&self, stored: &StoredSession) -> Result<Session> {
        self.post(
            "/v1/auth/refresh",
            &serde_json::json!({"refreshToken":stored.refresh_token}),
        )
    }
    /// Verify an API key without requiring any query or ingestion permission.
    pub fn verify_api_key(&self, key: &Secret) -> Result<()> {
        let response = self
            .http
            .post(format!("{}/v1/auth/verify", self.endpoint.0))
            .bearer_auth(key.expose())
            .send()
            .map_err(Error::transport)?;
        if !response.status().is_success() {
            return Err(HttpFailure::blocking(response, &[key.expose()]).into());
        }
        require(
            response.status() == reqwest::StatusCode::NO_CONTENT,
            "Invalid Boundary credential verification response",
        )
    }
    pub fn logout(&self, stored: &StoredSession) -> Result<()> {
        let _: serde_json::Value = self.post(
            "/v1/auth/logout",
            &serde_json::json!({"refreshToken":stored.refresh_token}),
        )?;
        Ok(())
    }
    /// All authenticated operations share renewal and refusal handling; targets stay in bodies.
    pub(crate) fn send_authorized(
        &self,
        authorization: &crate::credentials::RequestAuthorization,
        request: impl Fn() -> reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response> {
        if authorization.authentication.endpoint() != self.endpoint.as_str() {
            return Err(Error::EndpointMismatch);
        }
        for attempt in 0..2 {
            let sent = authorization.authentication.acquire_blocking()?;
            let response = request()
                .bearer_auth(sent.token().expose())
                .send()
                .map_err(Error::transport)?;
            let retry = attempt == 0
                && response.status() == reqwest::StatusCode::UNAUTHORIZED
                && authorization.authentication.rejected(sent.token());
            if retry {
                sent.complete(response.status(), response.headers());
                continue;
            }
            if !response.status().is_success() {
                let failure = HttpFailure::blocking(
                    response,
                    &[
                        sent.token().expose(),
                        authorization.authentication.original_credential(),
                    ],
                );
                sent.complete_failure(failure.clone());
                return Err(failure.into());
            }
            sent.complete(response.status(), response.headers());
            return Ok(response);
        }
        unreachable!("the second attempt returns its response")
    }
    pub(crate) fn post<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        let response = self
            .http
            .post(format!("{}{path}", self.endpoint.0))
            .json(body)
            .send()
            .map_err(Error::transport)?;
        let status = response.status();
        if !status.is_success() {
            let secrets: Vec<_> = ["refreshToken", "deviceCode"]
                .into_iter()
                .filter_map(|key| body.get(key).and_then(serde_json::Value::as_str))
                .collect();
            return Err(HttpFailure::blocking(response, &secrets).into());
        }
        Self::read_json(response)
    }
    pub(crate) fn read_json<T: DeserializeOwned>(
        mut response: reqwest::blocking::Response,
    ) -> Result<T> {
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(Error::Read)?;
        require(
            bytes.len() as u64 <= MAX_RESPONSE_BYTES,
            "Boundary response exceeded the size limit",
        )?;
        serde_json::from_slice(&bytes).map_err(|_| Error::Protocol("Invalid Boundary response"))
    }
}

/// A single OS-keyring entry makes replacing a refresh credential atomic. `BAML_HOME`
/// contributes a namespace so isolated installations do not share credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialStoreLocation {
    pub(crate) backend: &'static str,
    pub(crate) service: &'static str,
    pub(crate) account: String,
}
impl fmt::Display for CredentialStoreLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (service {:?}, account {:?})",
            self.backend, self.service, self.account
        )
    }
}

pub struct Store {
    entry: keyring::Entry,
    location: CredentialStoreLocation,
}
impl Store {
    pub fn new(endpoint: &Endpoint) -> Result<Self> {
        let namespace = baml_env::baml_home_from(baml_env::os_var("BAML_HOME"), dirs::home_dir());
        let account = hex::encode(Sha256::digest(
            format!("{}\n{}", endpoint.0, namespace.display()).as_bytes(),
        ));
        let location = CredentialStoreLocation {
            backend: if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
                "Apple Keychain"
            } else if cfg!(target_os = "windows") {
                "Windows Credential Manager"
            } else if cfg!(target_os = "linux") {
                "Secret Service"
            } else {
                "platform credential store"
            },
            service: "Boundary BAML login",
            account,
        };
        Ok(Self {
            entry: keyring::Entry::new(location.service, &location.account).map_err(|source| {
                Error::Storage {
                    operation: "open",
                    location: location.clone(),
                    source,
                }
            })?,
            location,
        })
    }
    pub fn read(&self) -> Result<Option<StoredSession>> {
        match self.entry.get_password() {
            Ok(json) => {
                let mut session: StoredSession = serde_json::from_str(&json)
                    .map_err(|_| Error::Protocol("Invalid stored Boundary login"))?;
                if !session
                    .refresh_token
                    .expose()
                    .starts_with(crate::credentials::SESSION_PREFIX)
                {
                    session.refresh_token = Secret::new(format!(
                        "{}{}",
                        crate::credentials::SESSION_PREFIX,
                        session.refresh_token.expose()
                    ));
                }
                Ok(Some(session))
            }
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(Error::Storage {
                operation: "read",
                location: self.location.clone(),
                source: error,
            }),
        }
    }
    pub fn write(&self, session: &StoredSession) -> Result<()> {
        self.entry
            .set_password(&serde_json::to_string(session)?)
            .map_err(|source| Error::Storage {
                operation: "save",
                location: self.location.clone(),
                source,
            })
    }
    pub fn clear(&self) -> Result<()> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(Error::Storage {
                operation: "remove",
                location: self.location.clone(),
                source: error,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn endpoint_normalization_and_transport_safety() {
        assert_eq!(
            Endpoint::parse("https://API.CLOUD.BOUNDARYML.COM:443/")
                .unwrap()
                .as_str(),
            DEFAULT_API_URL
        );
        assert!(Endpoint::parse("http://localhost:31100/").is_ok());
        assert!(Endpoint::parse("http://example.com").is_err());
        assert!(Endpoint::parse("https://secret@example.com").is_err());
        assert!(Endpoint::parse("https://example.com?key=secret").is_err());
    }
    #[test]
    fn persistent_session_contains_no_access_token_and_redacts_refresh() {
        let session: Session = serde_json::from_value(serde_json::json!({
            "accessToken":"access-secret", "expiresAt":123, "refreshToken":"refresh-secret",
            "caller":{"userId":"user_1","email":"a@example.com"}
        }))
        .unwrap();
        let stored = StoredSession::from(&session);
        assert!(
            !serde_json::to_string(&stored)
                .unwrap()
                .contains("access-secret")
        );
        assert!(!format!("{stored:?}").contains("refresh-secret"));
    }
}

#[cfg(test)]
mod transport_tests {
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, method, path},
    };

    use super::*;

    #[tokio::test]
    async fn login_lifecycle_uses_only_the_selected_boundary_gateway() {
        let gateway = MockServer::start().await;
        let value = json!({"accessToken":"temporary", "expiresAt":300, "refreshToken":"persistent",
            "caller":{"userId":"u1", "email":"u1@example.test"}});
        Mock::given(method("POST")).and(path("/v1/auth/device")).and(body_json(json!({})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"deviceCode":"private-device", "userCode":"ABCD", "verificationUri":"https://identity.example.test/device", "expiresAt":300,"intervalSeconds":5}))).expect(1).mount(&gateway).await;
        Mock::given(method("POST"))
            .and(path("/v1/auth/device/poll"))
            .and(body_json(json!({"deviceCode":"private-device"})))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"status":"approved","session":value})),
            )
            .expect(1)
            .mount(&gateway)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/auth/refresh"))
            .and(body_json(json!({"refreshToken":"persistent"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(value))
            .expect(1)
            .mount(&gateway)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/auth/logout"))
            .and(body_json(json!({"refreshToken":"persistent"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&gateway)
            .await;
        let url = gateway.uri();
        tokio::task::spawn_blocking(move || {
            let client = Client::new(Endpoint::parse(&url).unwrap()).unwrap();
            let device = client.start().unwrap();
            let LoginPoll::Approved { session } = client.poll(&device.device_code).unwrap() else {
                panic!("approval expected")
            };
            let stored = StoredSession::from(&session);
            assert_eq!(client.refresh(&stored).unwrap().caller.user_id, "u1");
            client.logout(&stored).unwrap();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn redirects_and_error_bodies_cannot_leak_login_credentials() {
        let gateway = MockServer::start().await;
        Mock::given(path("/v1/auth/refresh"))
            .respond_with(
                ResponseTemplate::new(307)
                    .insert_header("Location", "https://unexpected.example.test/auth")
                    .set_body_string("echoed-private-credential"),
            )
            .expect(1)
            .mount(&gateway)
            .await;
        let url = gateway.uri();
        tokio::task::spawn_blocking(move || {
            let client = Client::new(Endpoint::parse(&url).unwrap()).unwrap();
            let stored: StoredSession = serde_json::from_value(json!({"refreshToken":"echoed-private-credential", "caller":{"userId":"u1","email":"u1@example.test"}})).unwrap();
            let error = client.refresh(&stored).unwrap_err();
            assert!(error.to_string().contains("307"));
            assert!(!format!("{error:#}").contains("echoed-private-credential"));
        }).await.unwrap();
    }
}
