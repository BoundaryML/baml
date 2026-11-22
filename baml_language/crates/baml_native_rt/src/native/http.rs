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

use tokio::sync::{mpsc, watch};

use self::events::Decoder;
use super::{
    Big, Bytes, Map, MapRef, NativeBamlHttp, NativeBamlHttpResponse, NativeBamlHttpSseStream,
    Natives, Obj, Opaque, Root, Scope, Str, Thrown,
    failure::{Failure, limit},
    net::unless,
};
use crate::layout::{NAMED_HTTP_RESPONSE, NAMED_HTTP_SSE_STREAM};

mod events;

/// The fields of `baml.http.Response`, by position.
const RESPONSE_STATUS: usize = 0;
const RESPONSE_HEADERS: usize = 1;
const RESPONSE_URL: usize = 2;
const RESPONSE_BODY: usize = 3;

/// The limits one request runs under: `total` from connecting to the end
/// of the body, and `connect` for connecting alone. `None` is no limit.
#[derive(Clone, Copy)]
struct Limits {
    total: Option<Duration>,
    connect: Option<Duration>,
}

/// The body of a `Response`.
enum Body {
    /// Of a response to a request: unread until it is read, once, under
    /// the limits of the request, which go on while it is read.
    Client {
        unread: Mutex<Option<reqwest::Response>>,
        limits: Limits,
    },
    /// What a program gave `Response.new`.
    Bytes(Vec<u8>),
    /// What `Response.new_streaming` made: `write` sends a chunk and waits
    /// while the one before it has not been taken.
    Streaming {
        sender: tokio::sync::Mutex<Option<mpsc::Sender<Vec<u8>>>>,
        /// For what answers a request with the response to take. Nothing
        /// in this runtime serves requests yet, and until something does
        /// this only keeps the channel open.
        _receiver: mpsc::Receiver<Vec<u8>>,
    },
}

/// `error` as a failure of the step `context` names. The text names the
/// request's URL without the parts that carry credentials.
fn failed(context: &str, error: &reqwest::Error, limits: Limits) -> Failure {
    let text = error.to_string();
    let text = match error.url() {
        Some(url) => text.replace(
            url.as_str(),
            &btel_settings::network::sanitize_url(url.as_str(), btel_settings::network::Scrub::Log),
        ),
        None => text,
    };
    let message = format!("{context}: {text}");
    if !error.is_timeout() {
        return Failure::Io(message);
    }
    // `reqwest` does not say which limit passed. Only the one on
    // connecting fails inside the connector.
    let (which, limit) = if error.is_connect() && limits.connect.is_some() {
        ("connect_timeout", limits.connect)
    } else {
        ("timeout", limits.total)
    };
    Failure::Timeout {
        message,
        limit,
        which,
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
    status: i64,
    headers: Vec<(String, String)>,
    url: String,
}

impl Head {
    fn of(response: &reqwest::Response) -> Self {
        Self {
            status: i64::from(response.status().as_u16()),
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
        }
    }
}

/// A request, read out of a `baml.http.Request` or made for `fetch`.
struct Asked {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Asked {
    fn of(scope: &Scope<'_>, request: Root<'_, Obj>) -> Self {
        let request = request.get(scope);
        // SAFETY: a `Request` starts with its method, its URL, its headers
        // and its body: three strings and a map of strings.
        unsafe {
            let text = |field| {
                scope
                    .field_ref::<Str>(request, field)
                    .map_or_else(String::new, |text| text.as_str().to_owned())
            };
            Self {
                method: text(0),
                url: text(1),
                headers: scope
                    .field_ref::<Map>(request, 2)
                    .map_or_else(Vec::new, MapRef::to_strings),
                body: text(3),
            }
        }
    }

    /// Send the request under `limits`. `context` names the step in the
    /// error of one that fails.
    async fn send(self, limits: Limits, context: &str) -> Result<reqwest::Response, Failure> {
        if self.method.is_empty() {
            return Err(Failure::InvalidArgument(
                "Invalid HTTP method '': the request has no method set".to_owned(),
            ));
        }
        let method = reqwest::Method::from_bytes(self.method.as_bytes()).map_err(|error| {
            Failure::InvalidArgument(format!("Invalid HTTP method '{}': {error}", self.method))
        })?;
        let client = client(limits.connect)
            .map_err(|error| failed("HTTP client initialization failed", &error, limits))?;
        let mut request = client.request(method, &self.url);
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        if !self.body.is_empty() {
            request = request.body(self.body);
        }
        if let Some(total) = limits.total {
            request = request.timeout(total);
        }
        request
            .send()
            .await
            .map_err(|error| failed(context, &error, limits))
    }
}

/// The limits `baml.http` passes a request as two counts of nanoseconds.
fn limits(scope: &Scope<'_>, total: Root<'_, Big>, connect: Root<'_, Big>) -> Limits {
    Limits {
        total: limit(&total.get(scope).value()),
        connect: limit(&connect.get(scope).value()),
    }
}

/// A new map of `pairs`.
fn headers_map<'s>(scope: &mut Scope<'s>, pairs: &[(String, String)]) -> Root<'s, Map> {
    let headers = scope.alloc_str_map(pairs.len());
    for (name, value) in pairs {
        // The name and the value are reached through the map once the
        // round is over.
        scope.scoped(|scope, _| {
            let name = scope.alloc_str(name);
            let value = scope.alloc_str(value);
            scope.str_map_set(headers, name, Some(value));
        });
    }
    headers
}

/// A `Response` of this status, these headers and this URL that owns
/// `body`.
fn response<'s>(scope: &mut Scope<'s>, head: &Head, body: Body) -> Root<'s, Obj> {
    let headers = headers_map(scope, &head.headers);
    let url = scope.alloc_str(&head.url);
    let body = scope.alloc_owned(Arc::new(body));
    let mut response = scope.build(scope.named_class(NAMED_HTTP_RESPONSE));
    response.set(scope, RESPONSE_STATUS, head.status);
    response.set(scope, RESPONSE_HEADERS, headers);
    response.set(scope, RESPONSE_URL, url);
    response.set(scope, RESPONSE_BODY, body);
    response.finish()
}

/// Send `asked` and make a `Response` of the answer, its body unread.
async fn answered<'s>(
    scope: &mut Scope<'s>,
    asked: Asked,
    limits: Limits,
    context: &'static str,
) -> Result<Root<'s, Obj>, Thrown<'s>> {
    let sent = scope
        .io(async move {
            let response = asked.send(limits, context).await?;
            Ok((Head::of(&response), response))
        })
        .await;
    let (head, unread) = sent.map_err(|failure: Failure| failure.throw(scope))?;
    let body = Body::Client {
        unread: Mutex::new(Some(unread)),
        limits,
    };
    Ok(response(scope, &head, body))
}

/// The events of a response that is still arriving.
struct Events {
    /// The response and what decodes it, until its body has ended or
    /// failed.
    reading: tokio::sync::Mutex<Option<(reqwest::Response, Decoder)>>,
    /// Told when the stream is closed: a `next` that waits ends then.
    closed: watch::Sender<bool>,
    limits: Limits,
}

impl Events {
    /// The next events, as JSON: as many as the next chunk of the body
    /// that completes any completes. Nothing once the body has ended, or
    /// the stream is closed.
    async fn next(&self) -> Result<Option<String>, Failure> {
        let mut closed = self.closed.subscribe();
        let mut reading = self.reading.lock().await;
        loop {
            let Some((response, decoder)) = reading.as_mut() else {
                return Ok(None);
            };
            if *closed.borrow() {
                reading.take();
                return Ok(None);
            }
            let Some(chunk) = unless(closed.changed(), response.chunk()).await else {
                reading.take();
                return Ok(None);
            };
            let (events, ended) = match chunk {
                Ok(Some(chunk)) => (decoder.feed(&chunk), false),
                Ok(None) => (decoder.finish(), true),
                Err(error) => {
                    reading.take();
                    return Err(failed("SSE stream failed", &error, self.limits));
                }
            };
            if ended || events.is_err() {
                reading.take();
            }
            match events {
                Ok(events) if !events.is_empty() => return Ok(Some(events::to_json(&events))),
                Ok(_) => {}
                // Only the binary stream of AWS can be malformed.
                Err(message) => {
                    return Err(Failure::Io(format!(
                        "malformed AWS event stream: {message}"
                    )));
                }
            }
        }
    }
}

impl NativeBamlHttp for Natives {
    async fn _fetch<'s>(
        scope: &mut Scope<'s>,
        url: Root<'s, Str>,
        timeout_nanos: Root<'s, Big>,
        connect_timeout_nanos: Root<'s, Big>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let asked = Asked {
            method: "GET".to_owned(),
            url: url.get(scope).as_str().to_owned(),
            headers: Vec::new(),
            body: String::new(),
        };
        let limits = limits(scope, timeout_nanos, connect_timeout_nanos);
        answered(scope, asked, limits, "HTTP fetch failed").await
    }

    async fn _send<'s>(
        scope: &mut Scope<'s>,
        request: Root<'s, Obj>,
        timeout_nanos: Root<'s, Big>,
        connect_timeout_nanos: Root<'s, Big>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let asked = Asked::of(scope, request);
        let limits = limits(scope, timeout_nanos, connect_timeout_nanos);
        answered(scope, asked, limits, "HTTP send failed").await
    }

    async fn _send_sse<'s>(
        scope: &mut Scope<'s>,
        request: Root<'s, Obj>,
        timeout_nanos: Root<'s, Big>,
        connect_timeout_nanos: Root<'s, Big>,
    ) -> Result<Root<'s, Obj>, Thrown<'s>> {
        let asked = Asked::of(scope, request);
        let limits = limits(scope, timeout_nanos, connect_timeout_nanos);
        let sent = scope
            .io(async move {
                let response = asked.send(limits, "SSE connection failed").await?;
                let status = response.status();
                if status.is_success() {
                    return Ok((Head::of(&response), response));
                }
                let body = match response.bytes().await {
                    Ok(body) => String::from_utf8_lossy(&body).into_owned(),
                    Err(error) if error.is_timeout() => {
                        return Err(failed("SSE error response body failed", &error, limits));
                    }
                    Err(_) => "<could not read body>".to_owned(),
                };
                Err(Failure::Io(format!(
                    "SSE request failed with status {}: {body}",
                    status.as_u16()
                )))
            })
            .await;
        let (head, response) = sent.map_err(|failure| failure.throw(scope))?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        let decoder = Decoder::for_content_type(content_type);
        let events = Arc::new(Events {
            reading: tokio::sync::Mutex::new(Some((response, decoder))),
            closed: watch::channel(false).0,
            limits,
        });
        let url = scope.alloc_str(&head.url);
        let headers = headers_map(scope, &head.headers);
        let events = scope.alloc_owned(events);
        let mut stream = scope.build(scope.named_class(NAMED_HTTP_SSE_STREAM));
        stream.set(scope, 0, url);
        stream.set(scope, 1, head.status);
        stream.set(scope, 2, headers);
        stream.set(scope, SSE_EVENTS, events);
        Ok(stream.finish())
    }
}

/// The field of `baml.http.SseStream` that owns its events, by position.
const SSE_EVENTS: usize = 3;

impl NativeBamlHttpSseStream for Natives {
    async fn next<'s>(
        scope: &mut Scope<'s>,
        stream: Root<'s, Obj>,
    ) -> Result<Option<Root<'s, Str>>, Thrown<'s>> {
        // SAFETY: an `SseStream` has its events at that position.
        let events = unsafe { stream.get(scope).field::<Opaque>(SSE_EVENTS) }.share::<Events>();
        let next = scope.io(async move { events.next().await }).await;
        match next {
            Ok(events) => Ok(events.map(|events| scope.alloc_str(&events))),
            Err(failure) => Err(failure.throw(scope)),
        }
    }

    async fn close<'s>(scope: &mut Scope<'s>, stream: Root<'s, Obj>) -> Result<(), Thrown<'s>> {
        // SAFETY: as in `Self::next`.
        let events = unsafe { stream.get(scope).field::<Opaque>(SSE_EVENTS) }.share::<Events>();
        // The connection closes on a thread of the I/O runtime, where it
        // was made. A `next` that waits holds the response and drops it
        // when it is told.
        scope
            .io(async move {
                events.closed.send_replace(true);
                if let Ok(mut reading) = events.reading.try_lock() {
                    reading.take();
                }
            })
            .await;
        Ok(())
    }
}

/// The body of `response`, for a job to use.
fn body_of(scope: &Scope<'_>, response: Root<'_, Obj>) -> Arc<Body> {
    // SAFETY: a `Response` has its body at that position.
    unsafe { response.get(scope).field::<Opaque>(RESPONSE_BODY) }.share::<Body>()
}

/// What reading a body that is written, not read, fails with.
fn written_not_read() -> Failure {
    Failure::Io(
        "a streaming response body cannot be read with bytes()/text(); it is written with \
         write()/end()"
            .to_owned(),
    )
}

/// Read the body of a response to a request, which can be read once.
async fn read_once(
    unread: &Mutex<Option<reqwest::Response>>,
    limits: Limits,
) -> Result<Vec<u8>, Failure> {
    let unread = unread.lock().unwrap_or_else(PoisonError::into_inner).take();
    match unread {
        Some(unread) => unread
            .bytes()
            .await
            .map(Vec::from)
            .map_err(|error| failed("failed to read response body", &error, limits)),
        None => Err(Failure::Io(
            "Response body has already been consumed".to_owned(),
        )),
    }
}

impl NativeBamlHttpResponse for Natives {
    async fn text<'s>(
        scope: &mut Scope<'s>,
        response: Root<'s, Obj>,
    ) -> Result<Root<'s, Str>, Thrown<'s>> {
        let body = body_of(scope, response);
        let read = scope
            .io(async move {
                match &*body {
                    // Bytes that are not UTF-8 are replaced, as `reqwest`
                    // reads text.
                    Body::Client { unread, limits } => read_once(unread, *limits)
                        .await
                        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
                    Body::Bytes(bytes) => String::from_utf8(bytes.clone()).map_err(|error| {
                        Failure::Io(format!("Invalid UTF-8 in response body: {error}"))
                    }),
                    Body::Streaming { .. } => Err(written_not_read()),
                }
            })
            .await;
        match read {
            Ok(text) => Ok(scope.alloc_str(&text)),
            Err(failure) => Err(failure.throw(scope)),
        }
    }

    async fn bytes<'s>(
        scope: &mut Scope<'s>,
        response: Root<'s, Obj>,
    ) -> Result<Root<'s, Bytes>, Thrown<'s>> {
        let body = body_of(scope, response);
        let read = scope
            .io(async move {
                match &*body {
                    Body::Client { unread, limits } => read_once(unread, *limits).await,
                    Body::Bytes(bytes) => Ok(bytes.clone()),
                    Body::Streaming { .. } => Err(written_not_read()),
                }
            })
            .await;
        match read {
            Ok(bytes) => Ok(scope.alloc_bytes(&bytes)),
            Err(failure) => Err(failure.throw(scope)),
        }
    }

    fn new<'s>(
        scope: &mut Scope<'s>,
        status_code: i64,
        headers: Root<'s, Map>,
        body: Root<'s, Bytes>,
    ) -> Root<'s, Obj> {
        let body = Body::Bytes(body.get(scope).to_vec());
        made(scope, status_code, headers, body)
    }

    fn new_streaming<'s>(
        scope: &mut Scope<'s>,
        status_code: i64,
        headers: Root<'s, Map>,
    ) -> Root<'s, Obj> {
        // One chunk in flight: a `write` waits for the one before it.
        let (sender, receiver) = mpsc::channel(1);
        let body = Body::Streaming {
            sender: tokio::sync::Mutex::new(Some(sender)),
            _receiver: receiver,
        };
        made(scope, status_code, headers, body)
    }

    async fn write<'s>(
        scope: &mut Scope<'s>,
        response: Root<'s, Obj>,
        data: Root<'s, Bytes>,
    ) -> Result<(), Thrown<'s>> {
        let body = body_of(scope, response);
        let data = data.get(scope).hold();
        let written = scope
            .io(async move {
                let Body::Streaming { sender, .. } = &*body else {
                    return Err(Failure::Io(
                        "Response.write requires a response built with Response.new_streaming"
                            .to_owned(),
                    ));
                };
                // The lock is not held while the chunk waits to be taken.
                let sender = sender.lock().await.clone();
                let Some(sender) = sender else {
                    return Err(Failure::Io(
                        "streaming response has already been ended".to_owned(),
                    ));
                };
                sender.send(data.to_vec()).await.map_err(|_| {
                    Failure::Io(
                        "streaming response could not be written: the client has hung up"
                            .to_owned(),
                    )
                })
            })
            .await;
        written.map_err(|failure| failure.throw(scope))
    }

    async fn end<'s>(scope: &mut Scope<'s>, response: Root<'s, Obj>) -> Result<(), Thrown<'s>> {
        let body = body_of(scope, response);
        scope
            .io(async move {
                if let Body::Streaming { sender, .. } = &*body {
                    sender.lock().await.take();
                }
            })
            .await;
        Ok(())
    }
}

/// A `Response` a program makes: its headers are a map of the response's
/// own, and it has no URL.
fn made<'s>(
    scope: &mut Scope<'s>,
    status_code: i64,
    headers: Root<'s, Map>,
    body: Body,
) -> Root<'s, Obj> {
    let head = Head {
        status: status_code,
        headers: headers.get(scope).to_strings(),
        url: String::new(),
    };
    response(scope, &head, body)
}
