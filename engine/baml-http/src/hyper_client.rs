//! hyper-backed implementation of the reqwest API subset BAML uses.
//!
//! Built on hyper + hyper-util (legacy pooling client) with a feature-selected
//! TLS backend (native-tls by default, rustls optional; see the crate docs).
//! Differences from reqwest, by design:
//!
//! - No environment proxy support (HTTP_PROXY/HTTPS_PROXY are ignored).
//! - `read_timeout` is applied as an idle-timeout between body chunks (and as
//!   a total timeout for buffered body reads) rather than a socket read
//!   timeout.
//!
//! Redirects follow reqwest's default policy: up to 10 hops; 301/302/303
//! rewrite POST to GET and drop the body; 307/308 preserve method and body;
//! sensitive headers are stripped when the host changes.

use std::{fmt, sync::Arc, time::Duration};

use bytes::Bytes;
pub use http::{header, Method, StatusCode};
use http_body_util::{BodyExt, BodyStream, Full};
use hyper_util::{
    client::legacy::{connect::HttpConnector, Client as LegacyClient},
    rt::{TokioExecutor, TokioTimer},
};
pub use url::Url;

#[cfg(feature = "native-tls")]
type Connector = hyper_tls::HttpsConnector<HttpConnector>;
#[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
type Connector = hyper_rustls::HttpsConnector<HttpConnector>;

#[cfg(not(any(feature = "native-tls", feature = "rustls-tls")))]
compile_error!("baml-http: enable a TLS backend feature (`native-tls` (default) or `rustls-tls`)");

type Inner = LegacyClient<Connector, Full<Bytes>>;

const MAX_REDIRECTS: usize = 10;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Builder,
    Connect,
    Request,
    Timeout,
    Body,
    Decode,
    Redirect,
    Status,
}

/// Boxed internals (like reqwest's) so `Result<T, Error>` stays small.
struct ErrorInner {
    kind: Kind,
    url: Option<Url>,
    status: Option<StatusCode>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
    message: Option<String>,
}

pub struct Error {
    inner: Box<ErrorInner>,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    fn new(kind: Kind) -> Self {
        Error {
            inner: Box::new(ErrorInner {
                kind,
                url: None,
                status: None,
                source: None,
                message: None,
            }),
        }
    }

    fn with_url(mut self, url: Url) -> Self {
        self.inner.url = Some(url);
        self
    }

    fn with_status(mut self, status: StatusCode) -> Self {
        self.inner.status = Some(status);
        self
    }

    fn with_source(mut self, source: impl Into<Box<dyn std::error::Error + Send + Sync>>) -> Self {
        self.inner.source = Some(source.into());
        self
    }

    fn with_message(mut self, message: impl Into<String>) -> Self {
        self.inner.message = Some(message.into());
        self
    }

    pub fn is_timeout(&self) -> bool {
        self.inner.kind == Kind::Timeout
    }

    pub fn is_connect(&self) -> bool {
        self.inner.kind == Kind::Connect
    }

    pub fn is_request(&self) -> bool {
        matches!(
            self.inner.kind,
            Kind::Request | Kind::Connect | Kind::Timeout
        )
    }

    pub fn is_body(&self) -> bool {
        self.inner.kind == Kind::Body
    }

    pub fn is_decode(&self) -> bool {
        self.inner.kind == Kind::Decode
    }

    pub fn is_builder(&self) -> bool {
        self.inner.kind == Kind::Builder
    }

    pub fn is_redirect(&self) -> bool {
        self.inner.kind == Kind::Redirect
    }

    pub fn status(&self) -> Option<StatusCode> {
        self.inner.status
    }

    pub fn url(&self) -> Option<&Url> {
        self.inner.url.as_ref()
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut b = f.debug_struct("baml_http::Error");
        b.field("kind", &self.inner.kind);
        if let Some(url) = &self.inner.url {
            b.field("url", url);
        }
        if let Some(status) = &self.inner.status {
            b.field("status", status);
        }
        if let Some(message) = &self.inner.message {
            b.field("message", message);
        }
        if let Some(source) = &self.inner.source {
            b.field("source", source);
        }
        b.finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner.kind {
            Kind::Builder => write!(f, "builder error")?,
            Kind::Connect => write!(f, "error trying to connect")?,
            Kind::Request => write!(f, "error sending request")?,
            // "operation timed out" matches reqwest's wording; some callers
            // classify timeouts by string-matching the error text.
            Kind::Timeout => write!(f, "operation timed out")?,
            Kind::Body => write!(f, "error reading response body")?,
            Kind::Decode => write!(f, "error decoding response body")?,
            Kind::Redirect => write!(f, "error following redirect")?,
            Kind::Status => match self.inner.status {
                Some(status) if status.is_client_error() => {
                    write!(f, "HTTP status client error ({status})")?
                }
                Some(status) => write!(f, "HTTP status server error ({status})")?,
                None => write!(f, "HTTP status error")?,
            },
        }
        if let Some(message) = &self.inner.message {
            write!(f, ": {message}")?;
        }
        if let Some(url) = &self.inner.url {
            write!(f, " for url ({url})")?;
        }
        if let Some(source) = &self.inner.source {
            write!(f, ": {source}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.inner
            .source
            .as_deref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

fn classify_legacy_error(err: hyper_util::client::legacy::Error, url: &Url) -> Error {
    let kind = if err.is_connect() {
        Kind::Connect
    } else {
        Kind::Request
    };
    Error::new(kind).with_url(url.clone()).with_source(err)
}

// ---------------------------------------------------------------------------
// IntoUrl
// ---------------------------------------------------------------------------

pub trait IntoUrl {
    fn into_url(self) -> Result<Url>;
}

impl IntoUrl for Url {
    fn into_url(self) -> Result<Url> {
        if self.has_host() {
            Ok(self)
        } else {
            Err(Error::new(Kind::Builder)
                .with_message(format!("URL scheme is not allowed: {self}")))
        }
    }
}

impl IntoUrl for &str {
    fn into_url(self) -> Result<Url> {
        Url::parse(self)
            .map_err(|e| {
                Error::new(Kind::Builder).with_message(format!("invalid URL {self:?}: {e}"))
            })
            .and_then(IntoUrl::into_url)
    }
}

impl IntoUrl for &String {
    fn into_url(self) -> Result<Url> {
        self.as_str().into_url()
    }
}

impl IntoUrl for String {
    fn into_url(self) -> Result<Url> {
        self.as_str().into_url()
    }
}

// ---------------------------------------------------------------------------
// Body
// ---------------------------------------------------------------------------

pub struct Body {
    bytes: Bytes,
}

impl Body {
    pub fn as_bytes(&self) -> Option<&[u8]> {
        Some(&self.bytes)
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Body { bytes }
    }
}

impl From<Vec<u8>> for Body {
    fn from(v: Vec<u8>) -> Self {
        Body { bytes: v.into() }
    }
}

impl From<String> for Body {
    fn from(s: String) -> Self {
        Body { bytes: s.into() }
    }
}

impl From<&'static str> for Body {
    fn from(s: &'static str) -> Self {
        Body { bytes: s.into() }
    }
}

impl From<&'static [u8]> for Body {
    fn from(s: &'static [u8]) -> Self {
        Body { bytes: s.into() }
    }
}

// ---------------------------------------------------------------------------
// TLS: accept-invalid-certs verifier (used for user-configured local proxies)
// ---------------------------------------------------------------------------

#[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
#[derive(Debug)]
struct DangerAcceptAnyCert(rustls::crypto::CryptoProvider);

#[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
impl rustls::client::danger::ServerCertVerifier for DangerAcceptAnyCert {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &rustls::pki_types::CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

// ---------------------------------------------------------------------------
// ClientBuilder / Client
// ---------------------------------------------------------------------------

pub struct ClientBuilder {
    connect_timeout: Option<Duration>,
    read_timeout: Option<Duration>,
    pool_idle_timeout: Option<Duration>,
    pool_max_idle_per_host: Option<usize>,
    http2_keep_alive_interval: Option<Duration>,
    danger_accept_invalid_certs: bool,
}

impl ClientBuilder {
    pub fn new() -> Self {
        ClientBuilder {
            connect_timeout: None,
            read_timeout: None,
            pool_idle_timeout: Some(Duration::from_secs(90)),
            pool_max_idle_per_host: None,
            http2_keep_alive_interval: None,
            danger_accept_invalid_certs: false,
        }
    }

    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    pub fn read_timeout(mut self, timeout: Duration) -> Self {
        self.read_timeout = Some(timeout);
        self
    }

    pub fn pool_idle_timeout(mut self, timeout: impl Into<Option<Duration>>) -> Self {
        self.pool_idle_timeout = timeout.into();
        self
    }

    pub fn pool_max_idle_per_host(mut self, max: usize) -> Self {
        self.pool_max_idle_per_host = Some(max);
        self
    }

    pub fn http2_keep_alive_interval(mut self, interval: impl Into<Option<Duration>>) -> Self {
        self.http2_keep_alive_interval = interval.into();
        self
    }

    pub fn danger_accept_invalid_certs(mut self, accept: bool) -> Self {
        self.danger_accept_invalid_certs = accept;
        self
    }

    pub fn build(self) -> Result<Client> {
        let mut http = HttpConnector::new();
        http.enforce_http(false);
        http.set_connect_timeout(self.connect_timeout);

        let https = self.build_https_connector(http)?;

        let mut builder = LegacyClient::builder(TokioExecutor::new());
        builder
            .pool_timer(TokioTimer::new())
            .timer(TokioTimer::new())
            .pool_idle_timeout(self.pool_idle_timeout);
        if let Some(max) = self.pool_max_idle_per_host {
            builder.pool_max_idle_per_host(max);
        }
        if let Some(interval) = self.http2_keep_alive_interval {
            builder.http2_keep_alive_interval(interval);
        }

        Ok(Client {
            inner: Arc::new(builder.build(https)),
            read_timeout: self.read_timeout,
        })
    }

    #[cfg(feature = "native-tls")]
    fn build_https_connector(&self, http: HttpConnector) -> Result<Connector> {
        let mut tls = native_tls::TlsConnector::builder();
        // Force HTTP/1.1: hyper-tls does not forward the negotiated-h2 ALPN
        // hint to hyper-util, so offering h2 makes the server speak h2 while
        // hyper still sends HTTP/1.1 frames (hyper::Error(UnexpectedMessage)).
        // LLM request/response and SSE streaming work fine over HTTP/1.1.
        tls.request_alpns(&["http/1.1"]);
        if self.danger_accept_invalid_certs {
            tls.danger_accept_invalid_certs(true);
        }
        let tls = tls
            .build()
            .map_err(|e| Error::new(Kind::Builder).with_source(e))?;
        let mut https =
            hyper_tls::HttpsConnector::from((http, tokio_native_tls::TlsConnector::from(tls)));
        // Permit plain-HTTP requests (e.g. user-configured local proxies),
        // matching the rustls backend's `https_or_http()`.
        https.https_only(false);
        Ok(https)
    }

    #[cfg(all(feature = "rustls-tls", not(feature = "native-tls")))]
    fn build_https_connector(&self, http: HttpConnector) -> Result<Connector> {
        let provider = rustls::crypto::ring::default_provider();

        let tls = if self.danger_accept_invalid_certs {
            rustls::ClientConfig::builder_with_provider(Arc::new(provider.clone()))
                .with_safe_default_protocol_versions()
                .map_err(|e| Error::new(Kind::Builder).with_source(e))?
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(DangerAcceptAnyCert(provider)))
                .with_no_client_auth()
        } else {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            rustls::ClientConfig::builder_with_provider(Arc::new(provider))
                .with_safe_default_protocol_versions()
                .map_err(|e| Error::new(Kind::Builder).with_source(e))?
                .with_root_certificates(roots)
                .with_no_client_auth()
        };
        // Note: ALPN is configured by hyper-rustls (enable_http1/enable_http2
        // below); pre-setting tls.alpn_protocols here would panic.

        Ok(hyper_rustls::HttpsConnectorBuilder::new()
            .with_tls_config(tls)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(http))
    }
}

impl Default for ClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
    read_timeout: Option<Duration>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("baml_http::Client").finish_non_exhaustive()
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// Panics on TLS setup failure, matching `reqwest::Client::new`.
    pub fn new() -> Client {
        ClientBuilder::new()
            .build()
            .expect("baml_http::Client::new: failed to initialize TLS backend")
    }

    pub fn builder() -> ClientBuilder {
        ClientBuilder::new()
    }

    pub fn get(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::GET, url)
    }

    pub fn post(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::POST, url)
    }

    pub fn request(&self, method: Method, url: impl IntoUrl) -> RequestBuilder {
        RequestBuilder {
            client: self.clone(),
            request: url.into_url().map(|url| Request {
                method,
                url,
                headers: header::HeaderMap::new(),
                body: None,
                timeout: None,
            }),
        }
    }

    pub fn head(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::HEAD, url)
    }

    pub fn put(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::PUT, url)
    }

    pub fn delete(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::DELETE, url)
    }

    pub fn patch(&self, url: impl IntoUrl) -> RequestBuilder {
        self.request(Method::PATCH, url)
    }

    pub async fn execute(&self, request: Request) -> Result<Response> {
        match request.timeout {
            Some(timeout) => tokio::time::timeout(timeout, self.execute_inner(request))
                .await
                .map_err(|_| Error::new(Kind::Timeout))?,
            None => self.execute_inner(request).await,
        }
    }

    async fn execute_inner(&self, request: Request) -> Result<Response> {
        let Request {
            mut method,
            mut url,
            mut headers,
            body,
            timeout: _,
        } = request;
        let mut body = body.map(|b| b.bytes).unwrap_or_default();

        for _hop in 0..=MAX_REDIRECTS {
            let mut req = http::Request::builder()
                .method(method.clone())
                .uri(url.as_str())
                .body(Full::new(body.clone()))
                .map_err(|e| {
                    Error::new(Kind::Builder)
                        .with_url(url.clone())
                        .with_source(e)
                })?;
            *req.headers_mut() = headers.clone();

            let resp = self
                .inner
                .request(req)
                .await
                .map_err(|e| classify_legacy_error(e, &url))?;

            let status = resp.status();
            if !status.is_redirection() {
                let (parts, incoming) = resp.into_parts();
                return Ok(Response {
                    status: parts.status,
                    headers: parts.headers,
                    url,
                    body: incoming,
                    read_timeout: self.read_timeout,
                });
            }

            let Some(location) = resp
                .headers()
                .get(header::LOCATION)
                .and_then(|l| l.to_str().ok())
                .and_then(|l| url.join(l).ok())
            else {
                // Redirect status without a usable Location: return as-is.
                let (parts, incoming) = resp.into_parts();
                return Ok(Response {
                    status: parts.status,
                    headers: parts.headers,
                    url,
                    body: incoming,
                    read_timeout: self.read_timeout,
                });
            };

            match status {
                StatusCode::MOVED_PERMANENTLY | StatusCode::FOUND | StatusCode::SEE_OTHER => {
                    // Per reqwest's policy these rewrite to GET and drop the body.
                    method = Method::GET;
                    body = Bytes::new();
                    for h in [
                        header::CONTENT_TYPE,
                        header::CONTENT_LENGTH,
                        header::CONTENT_ENCODING,
                    ] {
                        headers.remove(h);
                    }
                }
                _ => {} // 307/308: preserve method and body
            }
            if location.host_str() != url.host_str() || location.port() != url.port() {
                for h in [
                    header::AUTHORIZATION,
                    header::COOKIE,
                    header::PROXY_AUTHORIZATION,
                    header::WWW_AUTHENTICATE,
                ] {
                    headers.remove(h);
                }
            }
            url = location;
        }

        Err(Error::new(Kind::Redirect)
            .with_url(url)
            .with_message(format!("too many redirects (max {MAX_REDIRECTS})")))
    }
}

/// Like `reqwest::get`: one-shot GET with a default client.
pub async fn get(url: impl IntoUrl) -> Result<Response> {
    Client::new().get(url).send().await
}

// ---------------------------------------------------------------------------
// Request / RequestBuilder
// ---------------------------------------------------------------------------

pub struct Request {
    method: Method,
    url: Url,
    headers: header::HeaderMap,
    body: Option<Body>,
    timeout: Option<Duration>,
}

impl Request {
    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn headers(&self) -> &header::HeaderMap {
        &self.headers
    }

    pub fn headers_mut(&mut self) -> &mut header::HeaderMap {
        &mut self.headers
    }

    pub fn body(&self) -> Option<&Body> {
        self.body.as_ref()
    }
}

pub struct RequestBuilder {
    client: Client,
    request: Result<Request>,
}

impl RequestBuilder {
    pub fn header<K, V>(mut self, key: K, value: V) -> Self
    where
        header::HeaderName: TryFrom<K>,
        <header::HeaderName as TryFrom<K>>::Error: Into<http::Error>,
        header::HeaderValue: TryFrom<V>,
        <header::HeaderValue as TryFrom<V>>::Error: Into<http::Error>,
    {
        if let Ok(req) = &mut self.request {
            let name = match header::HeaderName::try_from(key) {
                Ok(name) => name,
                Err(e) => {
                    self.request = Err(Error::new(Kind::Builder).with_source(e.into()));
                    return self;
                }
            };
            let value = match header::HeaderValue::try_from(value) {
                Ok(value) => value,
                Err(e) => {
                    self.request = Err(Error::new(Kind::Builder).with_source(e.into()));
                    return self;
                }
            };
            req.headers.append(name, value);
        }
        self
    }

    pub fn headers(mut self, headers: header::HeaderMap) -> Self {
        if let Ok(req) = &mut self.request {
            for (name, value) in headers.iter() {
                req.headers.append(name.clone(), value.clone());
            }
        }
        self
    }

    pub fn bearer_auth(self, token: impl fmt::Display) -> Self {
        self.header(header::AUTHORIZATION, format!("Bearer {token}"))
    }

    pub fn json<T: serde::Serialize + ?Sized>(mut self, json: &T) -> Self {
        if let Ok(req) = &mut self.request {
            match serde_json::to_vec(json) {
                Ok(bytes) => {
                    if !req.headers.contains_key(header::CONTENT_TYPE) {
                        req.headers.insert(
                            header::CONTENT_TYPE,
                            header::HeaderValue::from_static("application/json"),
                        );
                    }
                    req.body = Some(Body::from(bytes));
                }
                Err(e) => {
                    self.request = Err(Error::new(Kind::Builder).with_source(e));
                }
            }
        }
        self
    }

    pub fn query<T: serde::Serialize + ?Sized>(mut self, query: &T) -> Self {
        if let Ok(req) = &mut self.request {
            let mut serializer = url::form_urlencoded::Serializer::new(String::new());
            match query.serialize(serde_urlencoded::Serializer::new(&mut serializer)) {
                Ok(_) => {
                    let encoded = serializer.finish();
                    if !encoded.is_empty() {
                        let new_query = match req.url.query() {
                            Some(existing) if !existing.is_empty() => {
                                format!("{existing}&{encoded}")
                            }
                            _ => encoded,
                        };
                        req.url.set_query(Some(&new_query));
                    }
                }
                Err(e) => {
                    self.request = Err(Error::new(Kind::Builder).with_source(e));
                }
            }
        }
        self
    }

    pub fn body(mut self, body: impl Into<Body>) -> Self {
        if let Ok(req) = &mut self.request {
            req.body = Some(body.into());
        }
        self
    }

    pub fn form<T: serde::Serialize + ?Sized>(mut self, form: &T) -> Self {
        if let Ok(req) = &mut self.request {
            match serde_urlencoded::to_string(form) {
                Ok(encoded) => {
                    req.headers.insert(
                        header::CONTENT_TYPE,
                        header::HeaderValue::from_static("application/x-www-form-urlencoded"),
                    );
                    req.body = Some(Body::from(encoded));
                }
                Err(e) => {
                    self.request = Err(Error::new(Kind::Builder).with_source(e));
                }
            }
        }
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        if let Ok(req) = &mut self.request {
            req.timeout = Some(timeout);
        }
        self
    }

    pub fn build(self) -> Result<Request> {
        self.request
    }

    pub async fn send(self) -> Result<Response> {
        let client = self.client.clone();
        client.execute(self.request?).await
    }
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

pub struct Response {
    status: StatusCode,
    headers: header::HeaderMap,
    url: Url,
    body: hyper::body::Incoming,
    read_timeout: Option<Duration>,
}

impl fmt::Debug for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("baml_http::Response")
            .field("url", &self.url)
            .field("status", &self.status)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

impl Response {
    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn headers(&self) -> &header::HeaderMap {
        &self.headers
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn error_for_status(self) -> Result<Response> {
        if self.status.is_client_error() || self.status.is_server_error() {
            Err(Error::new(Kind::Status)
                .with_url(self.url.clone())
                .with_status(self.status))
        } else {
            Ok(self)
        }
    }

    pub async fn bytes(self) -> Result<Bytes> {
        let url = self.url;
        let collect = self.body.collect();
        let collected = match self.read_timeout {
            Some(timeout) => tokio::time::timeout(timeout, collect)
                .await
                .map_err(|_| Error::new(Kind::Timeout).with_url(url.clone()))?,
            None => collect.await,
        };
        collected
            .map(|c| c.to_bytes())
            .map_err(|e| Error::new(Kind::Body).with_url(url).with_source(e))
    }

    pub async fn text(self) -> Result<String> {
        let url = self.url.clone();
        let bytes = self.bytes().await?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| Error::new(Kind::Decode).with_url(url).with_source(e))
    }

    pub async fn json<T: serde::de::DeserializeOwned>(self) -> Result<T> {
        let url = self.url.clone();
        let bytes = self.bytes().await?;
        serde_json::from_slice(&bytes)
            .map_err(|e| Error::new(Kind::Decode).with_url(url).with_source(e))
    }

    /// Stream of body chunks. `read_timeout` (if configured on the client)
    /// bounds the wait between consecutive chunks; the timer resets after
    /// each received chunk (idle timeout).
    pub fn bytes_stream(self) -> BytesStream {
        BytesStream {
            inner: BodyStream::new(self.body),
            url: self.url,
            read_timeout: self.read_timeout,
            sleep: None,
        }
    }
}

/// Body chunk stream returned by [`Response::bytes_stream`].
///
/// Hand-rolled (rather than an `async` combinator) so it is `Send + Sync +
/// Unpin`, matching what BAML's stream plumbing requires of reqwest's
/// equivalent.
pub struct BytesStream {
    inner: BodyStream<hyper::body::Incoming>,
    url: Url,
    read_timeout: Option<Duration>,
    sleep: Option<std::pin::Pin<Box<tokio::time::Sleep>>>,
}

impl futures::Stream for BytesStream {
    type Item = Result<Bytes>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        use futures::Future;

        let this = self.get_mut();
        loop {
            match std::pin::Pin::new(&mut this.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(frame))) => {
                    match frame.into_data() {
                        Ok(data) => {
                            // Reset the idle timer on progress.
                            this.sleep = None;
                            return Poll::Ready(Some(Ok(data)));
                        }
                        // Skip non-data (trailer) frames.
                        Err(_) => continue,
                    }
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(Error::new(Kind::Body)
                        .with_url(this.url.clone())
                        .with_source(e))));
                }
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => {
                    if let Some(timeout) = this.read_timeout {
                        let sleep = this
                            .sleep
                            .get_or_insert_with(|| Box::pin(tokio::time::sleep(timeout)));
                        if sleep.as_mut().poll(cx).is_ready() {
                            this.sleep = None;
                            return Poll::Ready(Some(Err(
                                Error::new(Kind::Timeout).with_url(this.url.clone())
                            )));
                        }
                    }
                    return Poll::Pending;
                }
            }
        }
    }
}
