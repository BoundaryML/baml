//! The runtime's side of HTTP request tracing.
//!
//! Each request the runtime makes is a span, opened and written by the VM
//! thread. The IO side only timestamps what happens to the request and queues
//! it with [`NetworkContext::push`]; the engine drains the queue on the VM
//! thread, which sanitizes and records each event. Raw header values pass
//! through the queue in memory, so it stays private to the engine.

use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU8, Ordering},
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
}

/// One queued event: its span, and when it happened on the issuing run's clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkEvent {
    pub span: u64,
    pub at_ticks: u64,
    pub kind: NetworkEventKind,
}

/// Events the IO side queued, waiting for the engine to drain them on a VM
/// thread. One per engine.
#[derive(Debug, Default)]
pub struct NetworkQueue(Mutex<Queue>);

#[derive(Debug, Default)]
struct Queue {
    events: Vec<Queued>,
    // Weak ownership: tracing must not keep a response or its reader alive.
    active: HashMap<u64, Weak<Lifecycle>>,
}

const OPEN: u8 = 0;
const CLOSED: u8 = 1;
const ABANDONED: u8 = 2;

struct Lifecycle {
    state: AtomicU8,
    now: Arc<dyn Fn() -> u64 + Send + Sync>,
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

impl NetworkQueue {
    pub fn push(&self, event: NetworkEvent) {
        self.queue().events.push(Queued { event, open: None });
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
            queue.events.push(Queued {
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
        queue.events.push(Queued {
            event: NetworkEvent {
                span: self.span,
                at_ticks: (self.now)(),
                kind,
            },
            open: Some(Arc::clone(&self.open)),
        });
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
        NetworkQueue::abandon(
            &mut self.network.sink.queue(),
            self.network.span,
            &self.network.open,
        );
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
