//! One reader owns each fetch/send body; task references never own VM carriers.
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::{Bytes, BytesMut};
use sys_types::{
    CancellationToken, VmBamlError,
    network::{NetworkContext, NetworkError},
};
use tokio::sync::Notify;

use crate::io_impls::{HttpTimeoutOptions, http_transport_error};

/// Explicit BAML allowance, independent of Hyper's internal buffering.
pub(crate) const HTTP_EAGER_BODY_BYTES: usize = 8 * 1024;
const OWNERSHIP_DRAIN_BUDGET: Duration = Duration::from_millis(100);
const BOUNDARY_LOOKAHEAD_BUDGET: Duration = Duration::from_millis(10);

pub(crate) struct ClientBody {
    state: Arc<State>,
    network: Option<NetworkContext>,
}
struct State {
    consumed: AtomicBool,
    finished: AtomicBool,
    requested: Notify,
    released: CancellationToken,
    done: Notify,
    result: Mutex<Option<Result<Bytes, VmBamlError>>>,
    retained: AtomicUsize,
}
impl Drop for ClientBody {
    fn drop(&mut self) {
        self.state.released.cancel();
    }
}
impl ClientBody {
    pub(crate) fn new(
        response: reqwest::Response,
        options: HttpTimeoutOptions,
        network: Option<NetworkContext>,
        readers: &sys_types::http_readers::HttpReaders,
    ) -> Self {
        let state = Arc::new(State {
            consumed: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            requested: Notify::new(),
            released: CancellationToken::new(),
            done: Notify::new(),
            result: Mutex::new(None),
            retained: AtomicUsize::new(0),
        });
        if let Some(network) = &network {
            let release = state.released.clone();
            network.on_owner_lost(Arc::new(move || release.cancel()));
        }
        let reader = Arc::clone(&state);
        let release = state.released.clone();
        let on_refused = network.clone();
        let finished = ReaderFinished {
            state: Arc::clone(&reader),
            network: network.clone(),
        };
        if !readers.spawn(release, async move {
            // Abort/panic must wake an application already waiting for this
            // reader, rather than leaving its body sys-op pending forever.
            let _finished = finished;
            reader.run(response, options, network).await;
        }) {
            state.stopped();
            if let Some(network) = &on_refused {
                network.abandon();
            }
        }
        Self {
            state,
            network: on_refused,
        }
    }
    pub(crate) fn retained(&self) -> usize {
        self.state.retained.load(Ordering::Relaxed)
    }
    pub(crate) async fn read(&self) -> Result<Bytes, VmBamlError> {
        if self.state.consumed.swap(true, Ordering::AcqRel) {
            return Err(VmBamlError::Io {
                message: "Response body has already been consumed".into(),
            });
        }
        let mut reading = ApplicationRead {
            released: &self.state.released,
            network: self.network.as_ref(),
            completed: false,
        };
        self.state.requested.notify_one();
        loop {
            let ready = self.state.done.notified();
            tokio::pin!(ready);
            ready.as_mut().enable();
            if let Some(result) = self
                .state
                .result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                self.state.retained.store(0, Ordering::Relaxed);
                reading.completed = true;
                return result;
            }
            ready.await;
        }
    }
}
struct ApplicationRead<'a> {
    released: &'a CancellationToken,
    network: Option<&'a NetworkContext>,
    completed: bool,
}
impl Drop for ApplicationRead<'_> {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(network) = self.network {
                network.cancel_read();
            }
            self.released.cancel();
        }
    }
}
struct ReaderFinished {
    state: Arc<State>,
    network: Option<NetworkContext>,
}
impl Drop for ReaderFinished {
    fn drop(&mut self) {
        // An abort can drop the last lifecycle reference after GC released its
        // carrier. Preserve the terminal drop before the weak registry loses it.
        // A known/queued EOF or transport error wins the lifecycle CAS.
        if let Some(network) = &self.network {
            network.abandon();
        }
        self.state.stopped();
    }
}
impl State {
    fn stopped(&self) {
        if !self.finished.swap(true, Ordering::AcqRel) {
            *self
                .result
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Err(VmBamlError::Io {
                message: "Response body reader stopped before completion".into(),
            }));
            self.retained.store(0, Ordering::Relaxed);
            self.done.notify_waiters();
        }
    }

    fn finish(&self, result: Result<Bytes, VmBamlError>, network: Option<&NetworkContext>) {
        if let Some(network) = network {
            match &result {
                Ok(body) => network.complete(Some(body.clone()), None),
                Err(error) => network.complete(
                    None,
                    Some(NetworkError {
                        // Body transport errors contain no application body; omit
                        // reqwest's raw URL entirely from the owned capture.
                        message: if matches!(error, VmBamlError::Timeout { .. }) {
                            "HTTP response body deadline elapsed".into()
                        } else {
                            "HTTP response body transfer failed".into()
                        },
                        timeout: matches!(error, VmBamlError::Timeout { .. }),
                    }),
                ),
            }
        }
        if result.is_err() {
            self.retained.store(0, Ordering::Relaxed);
        }
        *self
            .result
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(result);
        self.finished.store(true, Ordering::Release);
        self.done.notify_waiters();
    }
    async fn run(
        &self,
        mut response: reqwest::Response,
        options: HttpTimeoutOptions,
        network: Option<NetworkContext>,
    ) {
        let mut prefix = BytesMut::new();
        let mut eager = network.is_some()
            && response
                .content_length()
                .is_none_or(|length| length <= HTTP_EAGER_BODY_BYTES as u64);
        let mut application = false;
        let mut release_deadline = None;
        let mut boundary_deadline = None;
        loop {
            if !application && self.consumed.load(Ordering::Acquire) {
                application = true;
            }
            if !application && !eager {
                tokio::select! {
                    biased;
                    () = self.released.cancelled() => { if let Some(network) = &network { network.abandon(); } return; }
                    () = self.requested.notified() => { application = true; }
                }
            }
            if self.released.is_cancelled() {
                if !eager || application {
                    if let Some(network) = &network {
                        network.abandon();
                    }
                    return;
                }
                release_deadline
                    .get_or_insert_with(|| tokio::time::Instant::now() + OWNERSHIP_DRAIN_BUDGET);
            }
            // At the boundary, one bounded lookahead establishes framed EOF.
            // Any already delivered overflow remains available to the app.
            let boundary = !application && prefix.len() >= HTTP_EAGER_BODY_BYTES;
            if boundary {
                boundary_deadline
                    .get_or_insert_with(|| tokio::time::Instant::now() + BOUNDARY_LOOKAHEAD_BUDGET);
            }
            let deadline = release_deadline
                .into_iter()
                .chain(boundary_deadline.filter(|_| boundary))
                .min();
            let read = async {
                match deadline {
                    Some(deadline) => tokio::time::timeout_at(deadline, response.chunk())
                        .await
                        .ok(),
                    None => Some(response.chunk().await),
                }
            };
            let chunk = tokio::select! {
                biased;
                result = read => result,
                () = self.released.cancelled(), if release_deadline.is_none() => { continue; }
                () = self.requested.notified(), if !application => { application = true; continue; }
            };
            match chunk {
                Some(Ok(Some(bytes))) => {
                    prefix.extend_from_slice(&bytes);
                    self.retained.store(prefix.capacity(), Ordering::Relaxed);
                    if prefix.len() > HTTP_EAGER_BODY_BYTES {
                        eager = false;
                    }
                }
                Some(Ok(None)) => {
                    self.finish(Ok(prefix.freeze()), network.as_ref());
                    return;
                }
                Some(Err(error)) => {
                    self.finish(
                        Err(http_transport_error(
                            "failed to read response body",
                            &error,
                            options,
                        )),
                        network.as_ref(),
                    );
                    return;
                }
                None => {
                    if self.released.is_cancelled() {
                        if let Some(network) = &network {
                            network.abandon();
                        }
                        return;
                    }
                    eager = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use sys_types::{
        http_readers::HttpReaders,
        network::{NetworkEventKind, NetworkQueue},
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    // Raw framing is intentional: BAML cannot force the exact chunk/window,
    // truncated transport, or reader/GC races exercised by these regressions.
    async fn response(
        wire: Vec<u8>,
        hold: Duration,
        timeout: Option<Duration>,
    ) -> reqwest::Response {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket.write_all(&wire).await.unwrap();
            tokio::time::sleep(hold).await;
        });
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let request = client.get(format!("http://{addr}/"));
        let request = if let Some(timeout) = timeout {
            request.timeout(timeout)
        } else {
            request
        };
        request.send().await.unwrap()
    }

    fn context() -> (Arc<NetworkQueue>, NetworkContext) {
        let queue = Arc::new(NetworkQueue::default());
        let ticks = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let network = NetworkContext::new(
            7,
            Arc::new(move || ticks.fetch_add(1, Ordering::Relaxed)),
            Arc::clone(&queue),
        );
        (queue, network)
    }

    fn options(total: Option<Duration>) -> HttpTimeoutOptions {
        HttpTimeoutOptions {
            total,
            connect: None,
        }
    }

    async fn terminal(queue: &NetworkQueue) -> Vec<sys_types::network::NetworkEvent> {
        tokio::time::timeout(Duration::from_secs(2), async {
            let mut all = Vec::new();
            loop {
                all.extend(queue.drain());
                if all.iter().any(|event| {
                    matches!(
                        event.kind,
                        NetworkEventKind::Complete { .. } | NetworkEventKind::Drop
                    )
                }) {
                    return all;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap()
    }

    fn fixed(body: &[u8]) -> Vec<u8> {
        let mut wire = format!(
            "HTTP/1.1 500 Error\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        wire.extend_from_slice(body);
        wire
    }

    #[tokio::test]
    async fn small_eof_completes_without_application_and_keeps_once_only_bytes() {
        let (queue, network) = context();
        let readers = HttpReaders::default();
        let body = ClientBody::new(
            response(fixed(&[b'a', 0xff]), Duration::from_secs(1), None).await,
            options(None),
            Some(network.clone()),
            &readers,
        );
        let events = terminal(&queue).await;
        assert!(
            matches!(&events[..], [data, complete] if data.kind == NetworkEventKind::Body(Bytes::from_static(&[b'a', 0xff])) && complete.kind == NetworkEventKind::Complete { error: None })
        );
        assert_eq!(body.read().await.unwrap(), &[b'a', 0xff][..]);
        assert!(body.read().await.is_err());
        network.application_await();
        network.application_await();
        assert_eq!(
            queue.drain().iter().map(|e| &e.kind).collect::<Vec<_>>(),
            [&NetworkEventKind::Await]
        );
        drop(body);
        readers.shutdown(Duration::from_millis(100)).await;
        queue.abandon_open();
        assert!(queue.drain().is_empty());
    }

    #[tokio::test]
    async fn exact_window_finishes_from_fixed_and_chunked_framing() {
        for chunked in [false, true] {
            let bytes = vec![b'x'; HTTP_EAGER_BODY_BYTES];
            let wire = if chunked {
                let mut wire =
                    b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2000\r\n".to_vec();
                wire.extend_from_slice(&bytes);
                wire.extend_from_slice(b"\r\n0\r\n\r\n");
                wire
            } else {
                fixed(&bytes)
            };
            let (queue, network) = context();
            let readers = HttpReaders::default();
            let body = ClientBody::new(
                response(wire, Duration::from_secs(1), None).await,
                options(None),
                Some(network),
                &readers,
            );
            assert!(
                terminal(&queue)
                    .await
                    .iter()
                    .any(|e| e.kind == NetworkEventKind::Complete { error: None })
            );
            assert_eq!(body.read().await.unwrap(), bytes);
            readers.shutdown(Duration::from_millis(100)).await;
        }
    }

    #[tokio::test]
    async fn overflow_and_known_large_bodies_stay_pending_and_preserve_every_byte() {
        for chunked in [false, true] {
            let bytes = vec![b'x'; HTTP_EAGER_BODY_BYTES + 2000];
            let wire = if chunked {
                let mut wire = format!(
                    "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n",
                    bytes.len()
                )
                .into_bytes();
                wire.extend_from_slice(&bytes);
                wire.extend_from_slice(b"\r\n0\r\n\r\n");
                wire
            } else {
                fixed(&bytes)
            };
            let (queue, network) = context();
            let readers = HttpReaders::default();
            let body = ClientBody::new(
                response(wire, Duration::from_secs(1), None).await,
                options(None),
                Some(network),
                &readers,
            );
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(queue.drain().is_empty());
            if !chunked {
                assert_eq!(body.retained(), 0, "known large bodies skip eager reads");
            }
            assert_eq!(body.read().await.unwrap(), bytes);
            assert!(
                terminal(&queue)
                    .await
                    .iter()
                    .any(|e| e.kind == NetworkEventKind::Complete { error: None })
            );
            readers.shutdown(Duration::from_millis(100)).await;
        }
    }

    #[tokio::test]
    async fn stalled_boundary_never_fabricates_eof_or_restarts_draining() {
        let mut wire = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2000\r\n".to_vec();
        wire.extend_from_slice(&vec![b'x'; HTTP_EAGER_BODY_BYTES]);
        wire.extend_from_slice(b"\r\n");
        let (queue, network) = context();
        let readers = HttpReaders::default();
        let body = ClientBody::new(
            response(wire, Duration::from_secs(5), None).await,
            options(None),
            Some(network),
            &readers,
        );
        tokio::time::sleep(Duration::from_millis(40)).await;
        assert!(queue.drain().is_empty());
        drop(body);
        assert_eq!(
            terminal(&queue).await.last().unwrap().kind,
            NetworkEventKind::Drop
        );
        readers.shutdown(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn owner_release_allows_small_drain_and_internal_refs_do_not_hide_large_drop() {
        for large in [false, true] {
            let wire = if large {
                fixed(&vec![b'x'; HTTP_EAGER_BODY_BYTES + 1])
            } else {
                fixed(b"small")
            };
            let (queue, network) = context();
            let readers = HttpReaders::default();
            let body = ClientBody::new(
                response(wire, Duration::from_secs(1), None).await,
                options(None),
                Some(network.clone()),
                &readers,
            );
            drop(body);
            let events = terminal(&queue).await;
            if large {
                assert_eq!(events.last().unwrap().kind, NetworkEventKind::Drop);
            } else {
                assert_eq!(
                    events.last().unwrap().kind,
                    NetworkEventKind::Complete { error: None }
                );
            }
            readers.shutdown(Duration::from_millis(100)).await;
            queue.abandon_open();
            assert!(queue.drain().is_empty());
        }
    }

    #[tokio::test]
    async fn eager_transport_error_and_request_timeout_survive_later_shutdown() {
        for timeout in [false, true] {
            let total = timeout.then_some(Duration::from_millis(50));
            let wire = b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nhi".to_vec();
            let (queue, network) = context();
            let readers = HttpReaders::default();
            let body = ClientBody::new(
                response(
                    wire,
                    if timeout {
                        Duration::from_secs(2)
                    } else {
                        Duration::ZERO
                    },
                    total,
                )
                .await,
                options(total),
                Some(network),
                &readers,
            );
            let events = terminal(&queue).await;
            assert!(
                matches!(&events.last().unwrap().kind, NetworkEventKind::Complete { error: Some(error) } if error.timeout == timeout)
            );
            assert_eq!(
                matches!(body.read().await.unwrap_err(), VmBamlError::Timeout { .. }),
                timeout
            );
            drop(body);
            readers.shutdown(Duration::from_millis(100)).await;
            queue.abandon_open();
            assert!(queue.drain().is_empty());
        }
    }

    #[tokio::test]
    async fn headers_are_immediate_and_release_drains_only_until_its_deadline() {
        for finish in [true, false] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let addr = listener.local_addr().unwrap();
            let (send, receive) = tokio::sync::oneshot::channel();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                assert!(socket.read(&mut request).await.unwrap() > 0);
                socket
                    .write_all(b"HTTP/1.1 500 Error\r\nContent-Length: 2\r\n\r\n")
                    .await
                    .unwrap();
                // The test gets headers before permitting any body byte.
                receive.await.unwrap();
                if finish {
                    socket.write_all(b"ok").await.unwrap();
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            });
            let response = tokio::time::timeout(
                Duration::from_secs(1),
                reqwest::Client::new().get(format!("http://{addr}/")).send(),
            )
            .await
            .unwrap()
            .unwrap();
            assert_eq!(response.status(), 500);
            let (queue, network) = context();
            let readers = HttpReaders::default();
            let body = ClientBody::new(response, options(None), Some(network), &readers);
            drop(body);
            send.send(()).unwrap();
            let started = tokio::time::Instant::now();
            let events = terminal(&queue).await;
            assert!(started.elapsed() < Duration::from_millis(500));
            assert_eq!(
                events.last().unwrap().kind,
                if finish {
                    NetworkEventKind::Complete { error: None }
                } else {
                    NetworkEventKind::Drop
                }
            );
            readers.shutdown(Duration::from_millis(100)).await;
            server.abort();
            let _ = server.await;
        }
    }

    #[tokio::test]
    async fn untraced_responses_remain_lazy_and_application_handoff_decodes_lossily() {
        let readers = HttpReaders::default();
        let body = crate::http_server::HttpBody::client(
            response(fixed(&[b'a', 0xff]), Duration::from_secs(1), None).await,
            options(None),
            None,
            &readers,
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
        let crate::http_server::HttpBody::Client { body: client, .. } = &*body else {
            panic!("client body");
        };
        assert_eq!(client.retained(), 0, "telemetry off does not eagerly read");
        assert_eq!(body.read_text().await.unwrap(), "a\u{fffd}");
        assert!(body.read_bytes().await.is_err());
        readers.shutdown(Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn shutdown_wakes_waiting_reads_and_refuses_late_admission() {
        let readers = HttpReaders::default();
        let body = ClientBody::new(
            response(
                b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n".to_vec(),
                Duration::from_secs(2),
                None,
            )
            .await,
            options(None),
            None,
            &readers,
        );
        let read = body.read();
        let shutdown = readers.shutdown(Duration::from_millis(10));
        let (result, ()) = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(read, shutdown)
        })
        .await
        .unwrap();
        assert!(result.is_err());
        let late = ClientBody::new(
            response(fixed(b"late"), Duration::from_secs(1), None).await,
            options(None),
            None,
            &readers,
        );
        assert!(
            tokio::time::timeout(Duration::from_secs(1), late.read())
                .await
                .unwrap()
                .is_err()
        );
    }

    #[tokio::test]
    async fn shutdown_records_abandonment_after_gc_loses_the_last_carrier() {
        let readers = HttpReaders::default();
        let (queue, network) = context();
        let body = ClientBody::new(
            response(
                b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n".to_vec(),
                Duration::from_secs(2),
                None,
            )
            .await,
            options(None),
            Some(network),
            &readers,
        );
        drop(body);
        // Expire the overall budget before the per-reader drain budget. No
        // application carrier survives for the weak shutdown sweep to find.
        readers.shutdown(Duration::from_millis(10)).await;
        queue.abandon_open();
        let events = queue.drain();
        assert_eq!(
            events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
            [&NetworkEventKind::Drop]
        );
    }

    #[tokio::test]
    async fn cancelled_application_read_stops_with_an_alias_still_alive() {
        let readers = HttpReaders::default();
        let (queue, network) = context();
        let body = ClientBody::new(
            response(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nhi\r\n".to_vec(),
                Duration::from_secs(2),
                None,
            )
            .await,
            options(None),
            Some(network),
            &readers,
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(10), body.read())
                .await
                .is_err()
        );
        // The carrier remains reachable, so ownership-loss cleanup cannot help.
        tokio::time::timeout(Duration::from_secs(1), async {
            while !body.state.finished.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(body.retained(), 0);
        assert_eq!(
            queue
                .drain()
                .iter()
                .map(|event| &event.kind)
                .collect::<Vec<_>>(),
            [&NetworkEventKind::Cancelled]
        );
        assert!(body.read().await.is_err());
        readers.shutdown(Duration::from_millis(100)).await;
        queue.abandon_open();
        assert!(queue.drain().is_empty());
    }

    #[tokio::test]
    async fn many_stalled_readers_share_one_shutdown_budget() {
        let readers = HttpReaders::default();
        let (queue, network) = context();
        let mut bodies = Vec::new();
        for _ in 0..20 {
            bodies.push(ClientBody::new(
                response(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\n\r\n".to_vec(),
                    Duration::from_secs(5),
                    None,
                )
                .await,
                options(None),
                Some(network.clone()),
                &readers,
            ));
        }
        let started = tokio::time::Instant::now();
        readers.shutdown(Duration::from_millis(100)).await;
        assert!(started.elapsed() < Duration::from_millis(500));
        queue.abandon_open();
        assert_eq!(
            terminal(&queue)
                .await
                .iter()
                .filter(|e| e.kind == NetworkEventKind::Drop)
                .count(),
            1
        );
        drop(bodies);
        assert!(queue.drain().is_empty());
    }
}
