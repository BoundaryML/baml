use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use async_trait::async_trait;
use base64::{engine::general_purpose::URL_SAFE, Engine};
use bytes::Bytes;
use chrono::Utc;
use http_body_util::Full;
use hyper::header::CONTENT_TYPE;
use hyper::Request;
use serde::Serialize;
use tokio::sync::RwLock;
use tracing::{debug, instrument, Level};
use url::form_urlencoded;

use crate::types::{HttpClient, ServiceAccountKey, Signer, Token};
use crate::{Error, TokenProvider};

/// A custom service account containing credentials
///
/// Once initialized, a [`CustomServiceAccount`] can be converted into an [`AuthenticationManager`]
/// using the applicable `From` implementation.
///
/// [`AuthenticationManager`]: crate::AuthenticationManager
#[derive(Debug)]
pub struct CustomServiceAccount {
    client: HttpClient,
    credentials: ServiceAccountKey,
    signer: Signer,
    tokens: RwLock<HashMap<Vec<String>, Arc<Token>>>,
    subject: Option<String>,
    audience: Option<String>,
}

impl CustomServiceAccount {
    /// Check `GOOGLE_APPLICATION_CREDENTIALS` environment variable for a path to JSON credentials
    pub fn from_env() -> Result<Option<Self>, Error> {
        debug!("check for GOOGLE_APPLICATION_CREDENTIALS env var");
        match ServiceAccountKey::from_env()? {
            Some(credentials) => Self::new(credentials, HttpClient::new()?).map(Some),
            None => Ok(None),
        }
    }

    /// Read service account credentials from the given JSON file
    pub fn from_file<T: AsRef<Path>>(path: T) -> Result<Self, Error> {
        Self::new(ServiceAccountKey::from_file(path)?, HttpClient::new()?)
    }

    /// Read service account credentials from the given JSON string
    pub fn from_json(s: &str) -> Result<Self, Error> {
        Self::new(ServiceAccountKey::from_str(s)?, HttpClient::new()?)
    }

    /// Set the `subject` to impersonate a user
    pub fn with_subject(mut self, subject: String) -> Self {
        self.subject = Some(subject);
        self
    }

    /// Set the `Audience` to impersonate a user
    pub fn with_audience(mut self, audience: String) -> Self {
        self.audience = Some(audience);
        self
    }

    fn new(credentials: ServiceAccountKey, client: HttpClient) -> Result<Self, Error> {
        debug!(project = ?credentials.project_id, email = credentials.client_email, "found credentials");
        Ok(Self {
            client,
            signer: Signer::new(&credentials.private_key)?,
            credentials,
            tokens: RwLock::new(HashMap::new()),
            subject: None,
            audience: None,
        })
    }

    #[instrument(level = Level::DEBUG, skip(self))]
    async fn fetch_token(&self, scopes: &[&str]) -> Result<Arc<Token>, Error> {
        let jwt = Claims::new(
            &self.credentials,
            scopes,
            self.subject.as_deref(),
            self.audience.as_deref(),
        )
        .to_jwt(&self.signer)?;
        let body = Bytes::from(
            form_urlencoded::Serializer::new(String::new())
                .extend_pairs(&[("grant_type", GRANT_TYPE), ("assertion", jwt.as_str())])
                .finish()
                .into_bytes(),
        );

        let token = self
            .client
            .token(
                &|| {
                    Request::post(&self.credentials.token_uri)
                        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                        .body(Full::from(body.clone()))
                        .unwrap()
                },
                "CustomServiceAccount",
            )
            .await?;

        Ok(token)
    }

    /// The RSA PKCS1 SHA256 [`Signer`] used to sign JWT tokens
    pub fn signer(&self) -> &Signer {
        &self.signer
    }

    /// The project ID as found in the credentials
    pub fn project_id(&self) -> Option<&str> {
        self.credentials.project_id.as_deref()
    }

    /// The private key as found in the credentials
    pub fn private_key_pem(&self) -> &str {
        &self.credentials.private_key
    }
}

#[async_trait]
impl TokenProvider for CustomServiceAccount {
    async fn token(&self, scopes: &[&str]) -> Result<Arc<Token>, Error> {
        let key: Vec<_> = scopes.iter().map(|x| x.to_string()).collect();
        let token = self.tokens.read().await.get(&key).cloned();
        if let Some(token) = token {
            if !token.has_expired() {
                return Ok(token.clone());
            }

            let mut locked = self.tokens.write().await;
            let token = self.fetch_token(scopes).await?;
            locked.insert(key, token.clone());
            return Ok(token);
        }

        let mut locked = self.tokens.write().await;
        let token = self.fetch_token(scopes).await?;
        locked.insert(key, token.clone());
        return Ok(token);
    }

    async fn project_id(&self) -> Result<Arc<str>, Error> {
        match &self.credentials.project_id {
            Some(pid) => Ok(pid.clone()),
            None => Err(Error::Str("no project ID in application credentials")),
        }
    }
}

/// Permissions requested for a JWT.
/// See https://developers.google.com/identity/protocols/OAuth2ServiceAccount#authorizingrequests.
#[derive(Serialize, Debug)]
pub(crate) struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
    sub: Option<&'a str>,
    scope: String,
}

impl<'a> Claims<'a> {
    pub(crate) fn new(
        key: &'a ServiceAccountKey,
        scopes: &[&str],
        sub: Option<&'a str>,
        aud: Option<&'a str>,
    ) -> Self {
        let mut scope = String::with_capacity(16);
        for (i, s) in scopes.iter().enumerate() {
            if i != 0 {
                scope.push(' ');
            }

            scope.push_str(s);
        }

        let iat = Utc::now().timestamp();
        Claims {
            iss: &key.client_email,
            aud: aud.unwrap_or(&key.token_uri),
            exp: iat + 3600 - 5, // Max validity is 1h
            iat,
            sub,
            scope,
        }
    }

    pub(crate) fn to_jwt(&self, signer: &Signer) -> Result<String, Error> {
        let mut jwt = String::new();
        URL_SAFE.encode_string(GOOGLE_RS256_HEAD, &mut jwt);
        jwt.push('.');
        URL_SAFE.encode_string(serde_json::to_string(self).unwrap(), &mut jwt);

        let signature = signer.sign(jwt.as_bytes())?;
        jwt.push('.');
        URL_SAFE.encode_string(&signature, &mut jwt);
        Ok(jwt)
    }
}

pub(crate) const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
const GOOGLE_RS256_HEAD: &str = r#"{"alg":"RS256","typ":"JWT"}"#;
