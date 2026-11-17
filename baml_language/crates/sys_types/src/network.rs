//! The runtime's side of HTTP request tracing.
//!
//! Each request is opened by the VM. Native readers publish owned metadata to
//! an engine-owned, heap-free recording producer, including when BAML is idle.
//! Platforms without that producer queue events for the next VM-safe drain.
//! Raw values stay private to the engine and are sanitized before capture.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

use bex_vm_types::{BexRustData, RustDataArc};
use bytes::Bytes;

/// A request as the program sent it. Raw: the VM sanitizes it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
}

/// What happened to a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NetworkEventKind {
    /// The response's status and headers arrived. Raw: the VM sanitizes them.
    Connection {
        status: u16,
        headers: Vec<(String, String)>,
    },
    /// A whole body that is not a stream, read once.
    Body(Bytes),
    /// One server-sent event, when the stream parsed it.
    SseEvent {
        event: Option<String>,
        data: String,
        id: Option<String>,
    },
    /// The server-sent event stream finished on the wire.
    StreamEnd,
    /// The program finished reading the response. Written by the engine.
    Await,
    /// The program closed a stream before its end. Written by the engine.
    Close,
    /// The response was collected, or the engine shut down, before a full read.
    Drop,
    /// Internal terminal notification; never encoded as a network event.
    Complete { error: Option<NetworkError> },
}

/// Owned transport failure, safe to record without a VM heap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkError {
    pub message: String,
    pub timeout: bool,
}

/// One queued event: its span, and when it happened on the issuing run's clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkEvent {
    pub span: u64,
    pub at_ticks: u64,
    pub kind: NetworkEventKind,
}

/// Serialized native recording (or a VM-safe fallback queue), one per engine.
#[derive(Debug, Default)]
pub struct NetworkQueue(Mutex<Queue>);

#[derive(Default)]
struct Queue {
    events: Vec<Queued>,
    // Weak ownership: tracing must not keep a response or its reader alive.
    active: HashMap<u64, Weak<Lifecycle>>,
    recorder: Option<Arc<dyn Fn(NetworkEvent) + Send + Sync>>,
    sealed: bool,
}

const OPEN: u8 = 0;
const CLOSED: u8 = 1;
const ABANDONED: u8 = 2;
const COMPLETED: u8 = 3;

struct Lifecycle {
    state: AtomicU8,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
    owner_lost: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    awaited: AtomicBool,
}

impl std::fmt::Debug for Lifecycle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lifecycle")
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

/// A queued event, with its span's lifecycle when it was pushed through a
/// [`NetworkContext`].
#[derive(Debug)]
struct Queued {
    event: NetworkEvent,
    open: Option<Arc<Lifecycle>>,
}

impl std::fmt::Debug for Queue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Queue")
            .field("events", &self.events)
            .finish_non_exhaustive()
    }
}

impl Queue {
    fn emit(&mut self, queued: Queued) {
        if self.sealed {
            return;
        }
        if let Some(record) = &self.recorder {
            record(queued.event);
        } else {
            self.events.push(queued);
        }
    }
}

impl NetworkQueue {
    /// Native recording accepts only owned metadata, under the queue lock so
    /// connection/data/drop/completion ordering survives concurrent producers.
    pub fn set_recorder(&self, record: Arc<dyn Fn(NetworkEvent) + Send + Sync>) {
        let mut queue = self.queue();
        for queued in std::mem::take(&mut queue.events) {
            record(queued.event);
        }
        queue.recorder = Some(record);
    }

    /// Called only after all native readers have joined and spans are terminal.
    pub fn seal(&self) {
        let record = {
            let mut queue = self.queue();
            queue.sealed = true;
            queue.recorder.take()
        };
        drop(record);
    }

    pub fn push(&self, event: NetworkEvent) {
        self.queue().emit(Queued { event, open: None });
    }

    /// Everything queued so far, in the order it was pushed, except the
    /// events of an explicitly closed span: nothing follows a completion.
    /// Abandonment retains preceding events, including the response status.
    pub fn drain(&self) -> Vec<NetworkEvent> {
        let mut queue = self.queue();
        queue.active.retain(|_, weak| {
            weak.upgrade()
                .is_some_and(|life| life.state.load(Ordering::Acquire) == OPEN)
        });
        std::mem::take(&mut queue.events)
            .into_iter()
            .filter(|queued| {
                queued
                    .open
                    .as_ref()
                    .is_none_or(|open| open.state.load(Ordering::Acquire) != CLOSED)
            })
            .map(|queued| queued.event)
            .collect()
    }

    /// Abandon responses still retained by host handles or globals at shutdown.
    /// Uses each request's issuing clock, just like its IO events.
    pub fn abandon_open(&self) {
        let mut queue = self.queue();
        let active = std::mem::take(&mut queue.active);
        for (span, weak) in active {
            if let Some(life) = weak.upgrade() {
                Self::abandon(&mut queue, span, &life);
            }
        }
    }

    fn abandon(queue: &mut Queue, span: u64, life: &Arc<Lifecycle>) {
        if life
            .state
            .compare_exchange(OPEN, ABANDONED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            queue.emit(Queued {
                event: NetworkEvent {
                    span,
                    at_ticks: (life.now)(),
                    kind: NetworkEventKind::Drop,
                },
                open: Some(Arc::clone(life)),
            });
        }
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, Queue> {
        // A push never panics while holding the lock, so a poisoned queue
        // still holds whole events.
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// How the IO side reports on one traced request. The engine puts it on the
/// per-call [`crate::SysOpContext`] of an HTTP op; the IO side keeps it with
/// the response it returns.
#[derive(Clone)]
pub struct NetworkContext {
    /// The raw `TelemetryId` of the request's span.
    pub span: u64,
    /// Reads the issuing run's clock. Any OS thread of the run may call it.
    pub now: Arc<dyn Fn() -> u64 + Send + Sync>,
    pub sink: Arc<NetworkQueue>,
    /// Shared terminal state: closed by a read/error, or abandoned by its owner.
    open: Arc<Lifecycle>,
}

impl NetworkContext {
    pub fn new(
        span: u64,
        now: Arc<dyn Fn() -> u64 + Send + Sync>,
        sink: Arc<NetworkQueue>,
    ) -> Self {
        let open = Arc::new(Lifecycle {
            state: AtomicU8::new(OPEN),
            now: Arc::clone(&now),
            owner_lost: Mutex::new(None),
            awaited: AtomicBool::new(false),
        });
        sink.queue().active.insert(span, Arc::downgrade(&open));
        Self {
            span,
            now,
            sink,
            open,
        }
    }

    /// Queue `kind` for this request, stamped now. Nothing once the span
    /// ended: a stream closed early can still be read on the wire.
    pub fn push(&self, kind: NetworkEventKind) {
        let mut queue = self.sink.queue();
        if !self.is_open() {
            return;
        }
        queue.emit(Queued {
            event: NetworkEvent {
                span: self.span,
                at_ticks: (self.now)(),
                kind,
            },
            open: Some(Arc::clone(&self.open)),
        });
    }

    /// Delegate ownership loss to a native reader, which may drain the rest of
    /// its small window before choosing EOF, transport error, or abandonment.
    pub fn on_owner_lost(&self, notify: Arc<dyn Fn() + Send + Sync>) {
        *self
            .open
            .owner_lost
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(notify);
    }

    pub fn abandon(&self) {
        NetworkQueue::abandon(&mut self.sink.queue(), self.span, &self.open);
    }

    /// Atomically preserve the observed outcome and time before GC/shutdown.
    pub fn complete(&self, body: Option<Bytes>, error: Option<NetworkError>) {
        // Claim the observed outcome before waiting for the recording lock;
        // GC and shutdown cannot replace a known EOF/error with abandonment.
        let at_ticks = (self.now)();
        if self
            .open
            .state
            .compare_exchange(OPEN, COMPLETED, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let mut queue = self.sink.queue();
        if let Some(body) = body {
            queue.emit(Queued {
                event: NetworkEvent {
                    span: self.span,
                    at_ticks,
                    kind: NetworkEventKind::Body(body),
                },
                open: None,
            });
        }
        queue.emit(Queued {
            event: NetworkEvent {
                span: self.span,
                at_ticks,
                kind: NetworkEventKind::Complete { error },
            },
            open: None,
        });
    }

    /// An application read remains observable even when eager EOF already
    /// completed the transfer. It never changes the terminal outcome/time.
    pub fn application_await(&self) {
        let mut queue = self.sink.queue();
        if self.open.state.load(Ordering::Acquire) == COMPLETED
            && !self.open.awaited.swap(true, Ordering::AcqRel)
        {
            queue.emit(Queued {
                event: NetworkEvent {
                    span: self.span,
                    at_ticks: (self.now)(),
                    kind: NetworkEventKind::Await,
                },
                open: None,
            });
        }
    }

    pub fn is_open(&self) -> bool {
        self.open.state.load(Ordering::Acquire) == OPEN
    }

    /// End the span, for every clone: later events, and events queued but
    /// not drained yet, are dropped. True the first time.
    pub fn close(&self) -> bool {
        self.open
            .state
            .compare_exchange(OPEN, CLOSED, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

impl std::fmt::Debug for NetworkContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkContext")
            .field("span", &self.span)
            .finish_non_exhaustive()
    }
}

/// `$rust_type` data that came from a traced request: a client response body
/// or a stream. The IO side stores it behind this one concrete type, because
/// `dyn Any` cannot be downcast to a trait: the engine finds the span on it
/// without knowing the IO side's types, and closes it once.
///
/// The last carrier queues a terminal `drop` when collected unread. Context
/// clones held by IO tasks do not own the response and cannot delay this.
pub struct NetworkTraced {
    network: NetworkContext,
    data: Arc<dyn BexRustData>,
}

impl Drop for NetworkTraced {
    fn drop(&mut self) {
        let notify = self
            .network
            .open
            .owner_lost
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(notify) = notify {
            notify();
        } else {
            self.network.abandon();
        }
    }
}

impl BexRustData for NetworkTraced {
    fn measure(&self, meter: &mut bex_vm_types::Meter) {
        // The context is the engine's; only the wrapped value is this payload's.
        meter.shared(&self.data, |meter| self.data.measure(meter));
    }
}

impl NetworkTraced {
    /// `data` as `$rust_type` data that carries `network`.
    pub fn wrap(network: NetworkContext, data: Arc<dyn BexRustData>) -> Arc<dyn BexRustData> {
        Arc::new(Self { network, data })
    }

    /// The traced request `data` came from, if it came from one.
    pub fn of(data: &dyn BexRustData) -> Option<&Self> {
        data.downcast_ref::<Self>()
    }

    /// The IO side's own value: unwrapped when traced, as it is otherwise.
    pub fn inner(data: Arc<dyn BexRustData>) -> Arc<dyn BexRustData> {
        match data.downcast_payload::<Self>() {
            Ok(traced) => Arc::clone(&traced.data),
            Err(data) => data,
        }
    }

    pub fn network(&self) -> &NetworkContext {
        &self.network
    }

    /// The span to close: once, then `None`. Closing it ends the request's
    /// [`NetworkContext`] too.
    pub fn take_span(&self) -> Option<u64> {
        self.network.close().then_some(self.network.span)
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::TestRustData;

    use super::*;

    fn context(sink: &Arc<NetworkQueue>) -> NetworkContext {
        let ticks = Arc::new(std::sync::atomic::AtomicU64::new(10));
        NetworkContext::new(
            7,
            Arc::new(move || ticks.fetch_add(1, Ordering::Relaxed)),
            Arc::clone(sink),
        )
    }

    #[test]
    fn events_drain_in_push_order_with_their_own_times() {
        let sink = Arc::new(NetworkQueue::default());
        let network = context(&sink);
        network.push(NetworkEventKind::Connection {
            status: 200,
            headers: vec![("content-type".into(), "text/plain".into())],
        });
        network.push(NetworkEventKind::Body(Bytes::from_static(b"hi")));
        let events = sink.drain();
        assert_eq!(
            events
                .iter()
                .map(|event| (event.span, event.at_ticks))
                .collect::<Vec<_>>(),
            [(7, 10), (7, 11)]
        );
        assert_eq!(
            events[1].kind,
            NetworkEventKind::Body(Bytes::from_static(b"hi"))
        );
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn traced_data_gives_its_span_once_and_unwraps_to_the_inner_value() {
        let sink = Arc::new(NetworkQueue::default());
        let traced = NetworkTraced::wrap(context(&sink), Arc::new(TestRustData(42)));
        let plain: Arc<dyn BexRustData> = Arc::new(TestRustData(5));
        assert!(NetworkTraced::of(plain.as_ref()).is_none());
        let view = NetworkTraced::of(traced.as_ref()).unwrap();
        assert_eq!(view.network().span, 7);
        assert_eq!(view.take_span(), Some(7));
        assert_eq!(view.take_span(), None);
        let inner = NetworkTraced::inner(traced);
        assert_eq!(
            inner.downcast_ref::<TestRustData>(),
            Some(&TestRustData(42))
        );
        assert_eq!(
            NetworkTraced::inner(plain).downcast_ref::<TestRustData>(),
            Some(&TestRustData(5))
        );
    }

    #[test]
    fn last_carrier_drop_preserves_connection_and_ignores_io_context_clones() {
        let sink = Arc::new(NetworkQueue::default());
        let network = context(&sink);
        let io = network.clone();
        network.push(NetworkEventKind::Connection {
            status: 504,
            headers: vec![],
        });
        let body = NetworkTraced::wrap(network, Arc::new(TestRustData(0)));
        let alias = Arc::clone(&body);
        drop(body);
        assert!(io.is_open());
        drop(alias);
        assert!(!io.is_open());
        io.push(NetworkEventKind::StreamEnd);
        assert!(!io.close());
        sink.abandon_open();
        let events = sink.drain();
        assert_eq!(
            events.iter().map(|event| &event.kind).collect::<Vec<_>>(),
            [
                &NetworkEventKind::Connection {
                    status: 504,
                    headers: vec![]
                },
                &NetworkEventKind::Drop,
            ]
        );
        assert_eq!(events[1].at_ticks, 11);
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn shutdown_abandons_retained_carriers_once() {
        let sink = Arc::new(NetworkQueue::default());
        let body = NetworkTraced::wrap(context(&sink), Arc::new(TestRustData(0)));
        sink.abandon_open();
        sink.abandon_open();
        drop(body);
        assert_eq!(
            sink.drain()
                .iter()
                .map(|event| &event.kind)
                .collect::<Vec<_>>(),
            [&NetworkEventKind::Drop]
        );
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn completed_body_drop_never_queues_a_terminal_event() {
        let sink = Arc::new(NetworkQueue::default());
        let body = NetworkTraced::wrap(context(&sink), Arc::new(TestRustData(0)));
        assert_eq!(
            NetworkTraced::of(body.as_ref()).unwrap().take_span(),
            Some(7)
        );
        drop(body);
        sink.abandon_open();
        assert!(sink.drain().is_empty());
    }

    #[test]
    fn native_completion_records_immediately_and_survives_drop_and_shutdown() {
        let sink = Arc::new(NetworkQueue::default());
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&recorded);
        sink.set_recorder(Arc::new(move |event| output.lock().unwrap().push(event)));
        let network = context(&sink);
        let body = NetworkTraced::wrap(network.clone(), Arc::new(TestRustData(0)));
        network.push(NetworkEventKind::Connection {
            status: 500,
            headers: vec![],
        });
        network.complete(Some(Bytes::from_static(b"error body")), None);
        network.complete(
            None,
            Some(NetworkError {
                message: "late".into(),
                timeout: true,
            }),
        );
        drop(body);
        sink.abandon_open();
        assert!(!network.close());
        let events = recorded.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[1].at_ticks, events[2].at_ticks);
        assert_eq!(events[2].kind, NetworkEventKind::Complete { error: None });
        drop(events);
        assert!(sink.drain().is_empty());
        sink.seal();
        network.application_await();
        assert_eq!(recorded.lock().unwrap().len(), 3);
    }

    #[test]
    fn an_ended_span_records_nothing_more() {
        let sink = Arc::new(NetworkQueue::default());
        let network = context(&sink);
        let reader = network.clone();
        let other = context(&sink);
        network.push(NetworkEventKind::Connection {
            status: 200,
            headers: Vec::new(),
        });
        // Queued before the span ended, drained after: dropped.
        reader.push(NetworkEventKind::StreamEnd);
        other.push(NetworkEventKind::StreamEnd);
        let traced = NetworkTraced::wrap(network, Arc::new(TestRustData(0)));
        assert_eq!(
            NetworkTraced::of(traced.as_ref()).unwrap().take_span(),
            Some(7)
        );
        assert!(!reader.is_open());
        assert!(!reader.close());
        // Pushed after: dropped.
        reader.push(NetworkEventKind::Body(Bytes::from_static(b"late")));
        // The other span is untouched; so is an event queued without a context.
        sink.push(NetworkEvent {
            span: 9,
            at_ticks: 1,
            kind: NetworkEventKind::Await,
        });
        assert_eq!(
            sink.drain()
                .into_iter()
                .map(|event| event.kind)
                .collect::<Vec<_>>(),
            [NetworkEventKind::StreamEnd, NetworkEventKind::Await]
        );
    }
}
