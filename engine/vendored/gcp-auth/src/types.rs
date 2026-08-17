use std::{env, fmt, fs::File, path::Path, str::FromStr, sync::Arc, time::Duration};

use bytes::Buf;
use chrono::{DateTime, Utc};
use http_body_util::{BodyExt, Full};
use hyper::{body::Bytes, Request};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client},
    rt::TokioExecutor,
};
use rsa::{
    pkcs1v15::SigningKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer as _},
    RsaPrivateKey,
};
use serde::{Deserialize, Deserializer};
use sha2::Sha256;
use tracing::{debug, warn};

use crate::Error;

#[derive(Clone, Debug)]
pub(crate) struct HttpClient {
    inner: Client<hyper_tls::HttpsConnector<HttpConnector>, Full<Bytes>>,
}

impl HttpClient {
    pub(crate) fn new() -> Result<Self, Error> {
        // Vendored change: native/platform TLS (hyper-tls) instead of
        // hyper-rustls, so `ring` is not pulled into the dependency tree.
        // Uses the OS trust store; connects over HTTP/1.1 (hyper-tls does not
        // forward the negotiated-h2 ALPN hint to hyper-util).
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        let mut tls = native_tls::TlsConnector::builder();
        tls.request_alpns(&["http/1.1"]);
        let tls = tls
            .build()
            .map_err(|err| Error::Other("failed to build native TLS connector", Box::new(err)))?;
        let mut https =
            hyper_tls::HttpsConnector::from((http, tokio_native_tls::TlsConnector::from(tls)));
        https.https_only(false);

        Ok(Self {
            inner: Client::builder(TokioExecutor::new()).build(https),
        })
    }

    pub(crate) async fn token(
        &self,
        request: &impl Fn() -> Request<Full<Bytes>>,
        provider: &'static str,
    ) -> Result<Arc<Token>, Error> {
        let mut retries = 0;
        let body = loop {
            let err = match self.request(request(), provider).await {
                // Early return when the request succeeds
                Ok(body) => break body,
                Err(err) => err,
            };

            warn!(
                ?err,
                provider, retries, "failed to refresh token, trying again..."
            );

            retries += 1;
            if retries >= RETRY_COUNT {
                return Err(err);
            }
        };

        serde_json::from_slice(&body)
            .map_err(|err| Error::Json("failed to deserialize token from response", err))
    }

    pub(crate) async fn request(
        &self,
        req: Request<Full<Bytes>>,
        provider: &'static str,
    ) -> Result<Bytes, Error> {
        debug!(url = ?req.uri(), provider, "requesting token");
        let (parts, body) = self
            .inner
            .request(req)
            .await
            .map_err(|err| Error::Other("HTTP request failed", Box::new(err)))?
            .into_parts();

        let mut body = body
            .collect()
            .await
            .map_err(|err| Error::Http("failed to read HTTP response body", err))?
            .aggregate();

        let body = body.copy_to_bytes(body.remaining());
        if !parts.status.is_success() {
            let body = String::from_utf8_lossy(body.as_ref());
            warn!(%body, status = ?parts.status, "token request failed");
            return Err(Error::Str("token request failed"));
        }

        Ok(body)
    }
}

/// Represents an access token that can be used as a bearer token in HTTP requests
///
/// Tokens should not be cached, the [`AuthenticationManager`] handles the correct caching
/// already.
///
/// The token does not implement [`Display`] to avoid accidentally printing the token in log
/// files, likewise [`Debug`] does not expose the token value itself which is only available
/// using the [Token::`as_str`] method.
///
/// [`AuthenticationManager`]: crate::AuthenticationManager
/// [`Display`]: fmt::Display
/// Token data as returned by the server
///
/// https://cloud.google.com/iam/docs/reference/sts/rest/v1/TopLevel/token#response-body
#[derive(Clone, Deserialize)]
pub struct Token {
    access_token: String,
    #[serde(
        deserialize_with = "deserialize_time",
        rename(deserialize = "expires_in")
    )]
    expires_at: DateTime<Utc>,
}

impl Token {
    pub(crate) fn from_string(access_token: String, expires_in: Duration) -> Self {
        Token {
            access_token,
            expires_at: Utc::now() + expires_in,
        }
    }

    /// Define if the token has has_expired
    ///
    /// This takes an additional 30s margin to ensure the token can still be reasonably used
    /// instead of expiring right after having checked.
    ///
    /// Note:
    /// The official Python implementation uses 20s and states it should be no more than 30s.
    /// The official Go implementation uses 10s (0s for the metadata server).
    /// The docs state, the metadata server caches tokens until 5 minutes before expiry.
    /// We use 20s to be on the safe side.
    pub fn has_expired(&self) -> bool {
        self.expires_at - Duration::from_secs(20) <= Utc::now()
    }

    /// Get str representation of the token.
    pub fn as_str(&self) -> &str {
        &self.access_token
    }

    /// Get expiry of token, if available
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("access_token", &"****")
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

/// An RSA PKCS1 SHA256 signer.
///
/// Vendored change: backed by the pure-Rust `rsa` crate instead of `ring`, so
/// no `ring` dependency is pulled in. Semantics are identical (RSASSA-PKCS1-v1_5
/// over SHA-256), producing the same signatures for a given key and input.
pub struct Signer {
    key: SigningKey<Sha256>,
}

impl Signer {
    pub(crate) fn new(pem_pkcs8: &str) -> Result<Self, Error> {
        let key = match rustls_pemfile::private_key(&mut pem_pkcs8.as_bytes()) {
            Ok(Some(key)) => key,
            Ok(None) => {
                return Err(Error::Str(
                    "no private key found in credentials private key data",
                ))
            }
            Err(err) => {
                return Err(Error::Io(
                    "failed to read credentials private key data",
                    err,
                ))
            }
        };

        let private_key = RsaPrivateKey::from_pkcs8_der(key.secret_der())
            .map_err(|_| Error::Str("invalid private key in credentials"))?;
        Ok(Signer {
            key: SigningKey::<Sha256>::new(private_key),
        })
    }

    /// Sign the input message and return the signature
    pub fn sign(&self, input: &[u8]) -> Result<Vec<u8>, Error> {
        // `Signer::sign` hashes `input` with SHA-256 then applies
        // RSASSA-PKCS1-v1_5, matching ring's `RSA_PKCS1_SHA256`.
        Ok(self.key.sign(input).to_vec())
    }
}

impl fmt::Debug for Signer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Signer").finish()
    }
}

fn deserialize_time<'de, D>(deserializer: D) -> Result<DateTime<Utc>, D::Error>
where
    D: Deserializer<'de>,
{
    let seconds_from_now: u64 = Deserialize::deserialize(deserializer)?;
    Ok(Utc::now() + Duration::from_secs(seconds_from_now))
}

#[derive(Deserialize)]
pub(crate) struct ServiceAccountKey {
    /// project_id
    pub(crate) project_id: Option<Arc<str>>,
    /// private_key
    pub(crate) private_key: String,
    /// client_email
    pub(crate) client_email: String,
    /// token_uri
    pub(crate) token_uri: String,
}

impl ServiceAccountKey {
    pub(crate) fn from_env() -> Result<Option<Self>, Error> {
        env::var_os("GOOGLE_APPLICATION_CREDENTIALS")
            .map(|path| {
                debug!(
                    ?path,
                    "reading credentials file from GOOGLE_APPLICATION_CREDENTIALS env var"
                );
                Self::from_file(&path)
            })
            .transpose()
    }

    pub(crate) fn from_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let file = File::open(path.as_ref())
            .map_err(|err| Error::Io("failed to open application credentials file", err))?;
        serde_json::from_reader(file)
            .map_err(|err| Error::Json("failed to deserialize ApplicationCredentials", err))
    }
}

impl FromStr for ServiceAccountKey {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
            .map_err(|err| Error::Json("failed to deserialize ApplicationCredentials", err))
    }
}

impl fmt::Debug for ServiceAccountKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ApplicationCredentials")
            .field("client_email", &self.client_email)
            .field("project_id", &self.project_id)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
pub(crate) struct AuthorizedUserRefreshToken {
    /// Client id
    pub(crate) client_id: String,
    /// Client secret
    pub(crate) client_secret: String,
    /// Project ID
    pub(crate) quota_project_id: Option<Arc<str>>,
    /// Refresh Token
    pub(crate) refresh_token: String,
}

impl AuthorizedUserRefreshToken {
    pub(crate) fn from_file(path: impl AsRef<Path>) -> Result<Self, Error> {
        let file = File::open(path.as_ref())
            .map_err(|err| Error::Io("failed to open application credentials file", err))?;
        serde_json::from_reader(file)
            .map_err(|err| Error::Json("failed to deserialize ApplicationCredentials", err))
    }
}

impl fmt::Debug for AuthorizedUserRefreshToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UserCredentials")
            .field("client_id", &self.client_id)
            .field("quota_project_id", &self.quota_project_id)
            .finish_non_exhaustive()
    }
}

/// How many times to attempt to fetch a token from the set credentials token endpoint.
const RETRY_COUNT: u8 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_with_time() {
        let s = r#"{"access_token":"abc123","expires_in":100}"#;
        let token: Token = serde_json::from_str(s).unwrap();
        let expires = Utc::now() + Duration::from_secs(100);

        assert_eq!(token.as_str(), "abc123");

        // Testing time is always racy, give it 1s leeway.
        let expires_at = token.expires_at();
        assert!(expires_at < expires + Duration::from_secs(1));
        assert!(expires_at > expires - Duration::from_secs(1));
    }
}

#[cfg(test)]
mod signer_tests {
    use super::Signer;

    // Throwaway 2048-bit RSA test key (never used outside tests).
    const TEST_PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDBNGLuWaOAQTZh\nbbAxunr5KNo47W7840lu/aw74fsgORNyqfJjRiN/YMFfPi4l9ZpGeKREgfwUOT63\n/aGlIybldy6lFGdxvIhYiXXeni0mGb7gCXLHVrxZoAqKHqTw4ggClO8UsvSqdtvk\nGGSzdZErULDzI5KQaoQ3iV29F0cCE/6GIDA62Fam81UcqxnNYxr3TIQ6iL7e7qqH\nWibYKmpj+rf2hV5Osod8KJMXI5hC0uyDT/L3cg0b1E7azPiiSvz47UQ7qXEbAfP2\n397ytXvql+Hd9vcEcGUP3BDRrlFVwU+TTInM6ty7rvOZGt1yZnDwji4jHgEdT4Ko\nwnMZEjAVAgMBAAECggEAJb/bmKCRDq0vN+gbpgu+nVI7GSZjKiwqm/IapfSogYpF\nX4EPKBB7PRclkTtv/uC3DQ/jYLNZEoaA16hJ3h85KVqZFY4gDBv/M/Vfv2h+f9RF\n9DZEY+hxkr1vcb89EQfI8uAwuoWgwnHI0w9lFZ9iBumUOV149JirTsKbOygCKshw\nN3klpQ4AgFFuSH44FFACTxl2KiBoSAH1xN/nWD8KNbpQYEVqzOoy32nTOs0BS2Qb\nkel5rojR5Ei7vQBc1BXB1AuJev3NZWQtih3d7BtzziWyNTLEp7+4miVKl/Sqsmls\nwyP1Evcw4GeHpGV95+XbAHCRSgDxZkmUasJZIvIXXQKBgQDty62ls8ZcOZGzud16\nnkG+rA59lxkyFmahCZGXXs90Dj1WhLb7XXY/U2+BDYtG6zCj9jSUi8Dy6p9ZVwmN\ntuHIO0nlMUc6wc3PAoTnImoAOV+91E5r4Jn1NgMFgdLuG5lhO5ojQ9fD9yWS4lhc\nX2+70A0zZd+4ypdUg3DsJ+wfnwKBgQDP/tA9/lSpO4rNY+moz0k2aBNbeTiF9StM\nOoQpq7pbGacbdc8LeEhhowqLt6mMkj40Ce/vFX7VW2XcB6t4bON//s4vG9THoLlp\n5tUemVlMrbsK48Dv2a3Yg0OHHl8AzdX6xDM51Z6Qv7r6PEPT37p+OPZ/vGEFWDza\ntiqk9uHDywKBgDl/4apKsTFFvmSOEe7/a3hWlF5r9ey1m/VeofTPOSyf8NcF2lUn\nwVsIqtKy2rW4Uxeihg5RSMO0Vfm9YRMCYNAQ/gpMgyPDDyf6PPbCzIznUq5NMvVE\n5xVzDQH85WssA0eOqPPUCM1a6pv83U7gyNzKLxb5kEJXwoXuDpUcBi2TAoGBAK++\nf5gSINjJnbOD+3eOhi75a3m8CF1v1cDYJLnNB25YU5FpTqNDY+1TxOJfMly7aOGx\nj9E1GXEPhBaRSHo9j1CkLPUzD+wJSwFHcMYlDoYyuTsvS+Odyz2JU/KEYAOe6HG1\nfA8fB5cI2eT8LNeGT969JNKzikrozqqCh6/RhttXAoGAUCu2KL45Vm2cu4t/bQhI\nAKDcCqq9fI06bQNYEQgI8IXKzS/yhL6lAlHb/6y6YIJGS9ZCj0JEQHBU+jO1N/HT\nvoDolQmucf4tJ6zSZ8TQV+J3iuV9N/hXecsSXNcfJ0qHGEvBMhbwv/3+UbqbvU2n\ndHuQb10vSP+e3ncv6hCIuNM=\n-----END PRIVATE KEY-----\n";
    // Reference RSASSA-PKCS1-v1_5 / SHA-256 signature over TEST_MSG, produced
    // by `openssl dgst -sha256 -sign`. PKCS1v15 is deterministic, so the
    // vendored (rsa-crate) Signer must reproduce these exact bytes.
    const TEST_MSG: &[u8] = b"baml.vertex.jwt.assertion.test";
    const EXPECTED_SIG_HEX: &str = "8607e65a2cc1c6bcf058a6a78fd80086fa64a54a4ab934f6246763cf33a80a0274539dfbfa051e228284af05d8017975f5976aac8d766b9809876b4ac4d0b4f97202745febb449ce740e67a85dee19a1c30ee8fe29e89276d6e16788137ee39622b1e7e3019716bf5d10eabc32040bd81933000f914508106cb6e7bc8bc737a893c655d4c1619370aa2b2fdefa24626b6c9e9a26f4e2e9bb3c196d73abb1d27dc659f06a91b46264eea1771e703d9b8915c5bbeed6aab529f2b238c63c7e2907ac121941c3b7e8147f214e403ba71f51df05514e8ab203170b40300bd5321bd8a6b1b55670f2bb1eb043f2af68b9e10df16ac516e18710250600790e7938bdf1";

    fn hex_to_bytes(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn rsa_signer_matches_openssl_rs256() {
        let signer = Signer::new(TEST_PEM).expect("load key");
        let got = signer.sign(TEST_MSG).expect("sign");
        assert_eq!(got.len(), 256, "RS256 over a 2048-bit key is 256 bytes");
        assert_eq!(
            got,
            hex_to_bytes(EXPECTED_SIG_HEX),
            "vendored rsa-crate signer must byte-match openssl RS256"
        );
    }
}
