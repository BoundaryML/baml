//! `baml.http`: a request sent with `reqwest` on the runtime's own Tokio
//! threads, over the TLS provider the crate's features choose. A
//! `Response` owns the body it has not read yet: Rust state the heap drops,
//! closing the connection, when the `Response` is dead.

use std::{
    collections::HashMap,
    ffi::OsString,
    sync::{Arc, LazyLock, Mutex, PoisonError},
    time::Duration,
};

use num_bigint::{BigInt, Sign};

use super::{
    Big, Map, NativeBamlHttp, NativeBamlHttpResponse, Natives, Obj, Opaque, Root, Scope, Str,
    Thrown,
};
use crate::layout::{NAMED_HTTP_RESPONSE, NAMED_TIMEOUT};

/// The fields of `baml.http.Response`, by position.
const RESPONSE_STATUS: usize = 0;
const RESPONSE_HEADERS: usize = 1;
const RESPONSE_URL: usize = 2;
const RESPONSE_BODY: usize = 3;

/// The field of `baml.errors.Timeout` that holds the limit that passed, a
/// `baml.time.Duration` or `null`, by position.
const TIMEOUT_LIMIT: usize = 1;

/// The limits one request runs under: `total` from connecting to the end
/// of the body, and `connect` for connecting alone. `None` is no limit.
#[derive(Clone, Copy)]
struct Limits {
    total: Option<Duration>,
    connect: Option<Duration>,
}

/// A limit from the nanoseconds `baml.http` passes: none when zero or
/// negative, and the longest there is when past it.
fn limit(nanos: &BigInt) -> Option<Duration> {
    (nanos.sign() == Sign::Plus)
        .then(|| Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX)))
}

/// The body of a response, until it is read, and the limits of the request
/// it answers, which go on while it is read.
struct Body {
    unread: Mutex<Option<reqwest::Response>>,
    limits: Limits,
}

/// Why a request failed, as `baml.http` throws it.
enum Failure {
    Io(String),
    Timeout {
        message: String,
        limit: Option<Duration>,
        which: &'static str,
    },
}

impl Failure {
    /// `error` as a failure of the step `context` names. The text names the
    /// request's URL without the parts that carry credentials.
    fn of(context: &str, error: &reqwest::Error, limits: Limits) -> Self {
        let text = error.to_string();
        let text = match error.url() {
            Some(url) => text.replace(
                url.as_str(),
                &btel_settings::network::sanitize_url(
                    url.as_str(),
                    btel_settings::network::Scrub::Log,
                ),
            ),
            None => text,
        };
        let message = format!("{context}: {text}");
        if !error.is_timeout() {
            return Self::Io(message);
        }
        // `reqwest` does not say which limit passed. Only the one on
        // connecting fails inside the connector.
        let (which, limit) = if error.is_connect() && limits.connect.is_some() {
            ("connect_timeout", limits.connect)
        } else {
            ("timeout", limits.total)
        };
        Self::Timeout {
            message,
            limit,
            which,
        }
    }

    /// The error to throw: `baml.errors.Io`, or `baml.errors.Timeout` with
    /// the limit that passed as a `baml.time.Duration`.
    fn throw<'s>(self, scope: &mut Scope<'s>) -> Thrown<'s> {
        let (message, limit, which) = match self {
            Self::Io(message) => return scope.io_error(&message),
            Self::Timeout {
                message,
                limit,
                which,
            } => (message, limit, which),
        };
        let class = scope.named_class(NAMED_TIMEOUT);
        let message = scope.alloc_str(&message);
        let which = scope.alloc_str(which);
        let limit = limit.map(|limit| {
            let duration = class
                .field_class(TIMEOUT_LIMIT)
                .expect("a timeout's limit is of a class");
            let nanos = scope.make_big(&BigInt::from(limit.as_nanos()));
            let mut built = scope.build(duration);
            built.set(scope, 0, nanos);
            built.finish()
        });
        let mut thrown = scope.build(class);
        thrown.set(scope, 0, message);
        thrown.set(scope, TIMEOUT_LIMIT, limit);
        thrown.set(scope, 2, which);
        let thrown = thrown.finish();
        scope.throw(thrown)
    }
}

/// The variables `reqwest` reads when it makes a client: the proxy settings
/// and where the certificate authorities are.
const CLIENT_ENV: [&str; 11] = [
    "ALL_PROXY",
    "all_proxy",
    "HTTPS_PROXY",
    "https_proxy",
    "HTTP_PROXY",
    "http_proxy",
    "NO_PROXY",
    "no_proxy",
    "REQUEST_METHOD",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
];

/// Install the TLS provider the crate's features name, if none is yet.
fn install_tls_provider() {
    #[cfg(feature = "ring-crypto")]
    let provider = rustls::crypto::ring::default_provider();
    #[cfg(all(feature = "aws-crypto", not(feature = "ring-crypto")))]
    let provider = rustls::crypto::aws_lc_rs::default_provider();
    // Another caller has installed one: that is the one in use.
    let _ = provider.install_default();
}

/// A client that limits connecting to `connect`. A client owns its pool of
/// connections, so requests with the same limit share one, and so the
/// connections. The variables a client reads when it is made are part of
/// what it is shared by, so a proxy set between requests holds for the
/// next.
fn client(connect: Option<Duration>) -> reqwest::Result<reqwest::Client> {
    type Key = (Option<Duration>, [Option<OsString>; CLIENT_ENV.len()]);
    /// A program that makes a limit for every request starts over here
    /// instead of keeping every client.
    const MOST_CLIENTS: usize = 32;
    static CLIENTS: LazyLock<Mutex<HashMap<Key, reqwest::Client>>> = LazyLock::new(Mutex::default);

    let key = (connect, CLIENT_ENV.map(std::env::var_os));
    let mut clients = CLIENTS.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(client) = clients.get(&key) {
        return Ok(client.clone());
    }
    install_tls_provider();
    let mut builder = reqwest::Client::builder();
    if let Some(connect) = connect {
        builder = builder.connect_timeout(connect);
    }
    let client = builder.build()?;
    if clients.len() >= MOST_CLIENTS {
        clients.clear();
    }
    clients.insert(key, client.clone());
    Ok(client)
}

/// What of a response is known before its body is read.
struct Head {
    status: u16,
    headers: Vec<(String, String)>,
    url: String,
    body: Arc<Body>,
}

impl NativeBamlHttp for Natives {
    async fn _fetch<'s>(
        scope: &mut Scope<'s>,
        url: Root<'s, Str>,
        timeout_nanos: Root<'s, Big>,
        connect_timeout_nanos: Root<'s, Big>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let url = url.get(scope).as_str().to_owned();
        let limits = Limits {
            total: limit(&timeout_nanos.get(scope).value()),
            connect: limit(&connect_timeout_nanos.get(scope).value()),
        };
        let sent = scope
            .io(async move {
                let client = client(limits.connect).map_err(|error| {
                    Failure::of("HTTP client initialization failed", &error, limits)
                })?;
                let mut request = client.get(&url);
                if let Some(total) = limits.total {
                    request = request.timeout(total);
                }
                let response = request
                    .send()
                    .await
                    .map_err(|error| Failure::of("HTTP fetch failed", &error, limits))?;
                Ok(Head {
                    status: response.status().as_u16(),
                    headers: response
                        .headers()
                        .iter()
                        .map(|(name, value)| {
                            (
                                name.as_str().to_owned(),
                                value.to_str().unwrap_or("").to_owned(),
                            )
                        })
                        .collect(),
                    url: response.url().to_string(),
                    body: Arc::new(Body {
                        unread: Mutex::new(Some(response)),
                        limits,
                    }),
                })
            })
            .await;
        let head = match sent {
            Ok(head) => head,
            Err(failure) => return Err(Failure::throw(failure, scope)),
        };
        let headers: Root<'s, Map> = scope.alloc_str_map(head.headers.len());
        for (name, value) in &head.headers {
            // The name and the value are reached through the map once
            // the round is over.
            scope.scoped(|scope, _| {
                let name = scope.alloc_str(name);
                let value = scope.alloc_str(value);
                scope.str_map_set(headers, name, Some(value));
            });
        }
        let url = scope.alloc_str(&head.url);
        let body = scope.alloc_owned(head.body);
        let class = scope.named_class(NAMED_HTTP_RESPONSE);
        let mut response = scope.build(class);
        response.set(scope, RESPONSE_STATUS, i64::from(head.status));
        response.set(scope, RESPONSE_HEADERS, headers);
        response.set(scope, RESPONSE_URL, url);
        response.set(scope, RESPONSE_BODY, body);
        Ok(response.finish())
    }
}

impl NativeBamlHttpResponse for Natives {
    async fn text<'s>(
        scope: &mut Scope<'s>,
        response: Root<'s, Obj>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        // SAFETY: a `Response` has its body at that position.
        let body = unsafe { response.get(scope).field::<Opaque>(RESPONSE_BODY) }.share::<Body>();
        let read = scope
            .io(async move {
                let unread = body
                    .unread
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take();
                match unread {
                    Some(unread) => unread.bytes().await.map_err(|error| {
                        Failure::of("failed to read response body", &error, body.limits)
                    }),
                    None => Err(Failure::Io(
                        "Response body has already been consumed".to_owned(),
                    )),
                }
            })
            .await;
        match read {
            // Bytes that are not UTF-8 are replaced, as `reqwest` reads text.
            Ok(bytes) => Ok(scope.alloc_str(&String::from_utf8_lossy(&bytes))),
            Err(failure) => Err(Failure::throw(failure, scope)),
        }
    }
}
