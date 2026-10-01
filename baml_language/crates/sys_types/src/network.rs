//! The runtime's side of HTTP request tracing.
//!
//! Each request the runtime makes is a span, opened and written by the VM
//! thread. The IO side only timestamps what happens to the request and queues
//! it with [`NetworkContext::push`]; the engine drains the queue on the VM
//! thread, which sanitizes and records each event. Raw header values pass
//! through the queue in memory, so it stays private to the engine.

use std::{
    any::Any,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

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
    /// The response was collected without being read.
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
pub struct NetworkQueue(Mutex<Vec<NetworkEvent>>);

impl NetworkQueue {
    pub fn push(&self, event: NetworkEvent) {
        self.events().push(event);
    }

    /// Everything queued so far, in the order it was pushed.
    pub fn drain(&self) -> Vec<NetworkEvent> {
        std::mem::take(&mut *self.events())
    }

    fn events(&self) -> std::sync::MutexGuard<'_, Vec<NetworkEvent>> {
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
}

impl NetworkContext {
    /// Queue `kind` for this request, stamped now.
    pub fn push(&self, kind: NetworkEventKind) {
        self.sink.push(NetworkEvent {
            span: self.span,
            at_ticks: (self.now)(),
            kind,
        });
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
/// Data collected without being read leaves its span open. A `Drop` impl
/// here is where its `drop` event, and a marker to close the span, will be
/// queued for the next drain.
pub struct NetworkTraced {
    network: NetworkContext,
    data: Arc<dyn Any + Send + Sync>,
    open: AtomicBool,
}

impl NetworkTraced {
    /// `data` as `$rust_type` data that carries `network`.
    pub fn wrap(
        network: NetworkContext,
        data: Arc<dyn Any + Send + Sync>,
    ) -> Arc<dyn Any + Send + Sync> {
        Arc::new(Self {
            network,
            data,
            open: AtomicBool::new(true),
        })
    }

    /// The traced request `data` came from, if it came from one.
    pub fn of(data: &(dyn Any + Send + Sync)) -> Option<&Self> {
        data.downcast_ref::<Self>()
    }

    /// The IO side's own value: unwrapped when traced, as it is otherwise.
    pub fn inner(data: Arc<dyn Any + Send + Sync>) -> Arc<dyn Any + Send + Sync> {
        match data.downcast::<Self>() {
            Ok(traced) => Arc::clone(&traced.data),
            Err(data) => data,
        }
    }

    pub fn network(&self) -> &NetworkContext {
        &self.network
    }

    /// The span to close: once, then `None`.
    pub fn take_span(&self) -> Option<u64> {
        self.open
            .swap(false, Ordering::AcqRel)
            .then_some(self.network.span)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(sink: &Arc<NetworkQueue>) -> NetworkContext {
        let ticks = Arc::new(std::sync::atomic::AtomicU64::new(10));
        NetworkContext {
            span: 7,
            now: Arc::new(move || ticks.fetch_add(1, Ordering::Relaxed)),
            sink: Arc::clone(sink),
        }
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
        let traced = NetworkTraced::wrap(context(&sink), Arc::new(42_u32));
        let plain: Arc<dyn Any + Send + Sync> = Arc::new(5_u32);
        assert!(NetworkTraced::of(plain.as_ref()).is_none());
        let view = NetworkTraced::of(traced.as_ref()).unwrap();
        assert_eq!(view.network().span, 7);
        assert_eq!(view.take_span(), Some(7));
        assert_eq!(view.take_span(), None);
        let inner = NetworkTraced::inner(traced);
        assert_eq!(inner.downcast_ref::<u32>(), Some(&42));
        assert_eq!(NetworkTraced::inner(plain).downcast_ref::<u32>(), Some(&5));
    }
}
