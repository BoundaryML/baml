//! An open `baml.http.SseStream`: the events a background task reads off the
//! connection, buffered until the program takes them with `next`.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use sys_ops::io::VmBamlError;
use sys_types::{
    OpOutput, RetainedBytes, VmInternalError, VmRustFnError, network::NetworkContext,
    resource::SseStreamResource, sse::SseEvent,
};
use tokio::{
    sync::{Mutex as TokioMutex, Notify},
    task::AbortHandle,
};

/// The events a stream's reader task has parsed and the program has not
/// taken yet, and how the stream ended.
pub(crate) struct SseBuffer {
    pub(crate) events: Vec<SseEvent>,
    pub(crate) done: bool,
    pub(crate) error: Option<VmBamlError>,
    /// The events' footprint, shared with the stream so the heap can read it
    /// without this buffer's lock.
    pub(crate) retained: Arc<RetainedBytes>,
}

impl SseBuffer {
    /// Publish the events' footprint after changing them.
    pub(crate) fn publish_retained(&self) {
        self.retained.set(
            self.events.capacity() * size_of::<SseEvent>()
                + self
                    .events
                    .iter()
                    .map(SseEvent::retained_bytes)
                    .sum::<usize>(),
        );
    }
}

/// An open event stream: its buffer, the task reading the connection into
/// it, and the traced request it belongs to.
pub(crate) struct SseStreamState {
    pub(crate) buffer: Arc<TokioMutex<SseBuffer>>,
    pub(crate) closed: Arc<AtomicBool>,
    pub(crate) notify: Arc<Notify>,
    pub(crate) reader: AbortHandle,
    pub(crate) retained: Arc<RetainedBytes>,
    pub(crate) network: Option<NetworkContext>,
}

impl SseStreamState {
    /// End the stream: stop reading the connection, and wake a `next`
    /// waiting on it, which then finds the stream done.
    fn shut(&self) {
        self.closed.store(true, Ordering::Release);
        if let Ok(mut buffer) = self.buffer.try_lock() {
            buffer.done = true;
            buffer.error = None;
        }
        self.reader.abort();
        self.notify.notify_waiters();
    }
}

impl Drop for SseStreamState {
    fn drop(&mut self) {
        self.shut();
    }
}

impl SseStreamResource for SseStreamState {
    fn next(self: Arc<Self>) -> OpOutput<Option<String>> {
        OpOutput::async_op(async move {
            loop {
                let notified = self.notify.notified();
                {
                    let mut buffer = self.buffer.lock().await;
                    if self.closed.load(Ordering::Acquire) {
                        buffer.done = true;
                        buffer.error = None;
                        return Ok(None);
                    }
                    if !buffer.events.is_empty() {
                        let taken = std::mem::take(&mut buffer.events);
                        buffer.publish_retained();
                        let events: Vec<serde_json::Value> = taken
                            .into_iter()
                            .map(|event| {
                                serde_json::json!({
                                    "event": event.event,
                                    "data": event.data,
                                    "id": event.id,
                                })
                            })
                            .collect();
                        return Ok(Some(serde_json::to_string(&events).map_err(|e| {
                            VmInternalError::BridgeFailure {
                                message: format!("failed to serialize SSE events: {e}"),
                            }
                        })?));
                    }
                    if let Some(error) = buffer.error.take() {
                        return Err(VmRustFnError::from(error));
                    }
                    if buffer.done {
                        return Ok(None);
                    }
                }
                notified.await;
            }
        })
    }

    fn close(self: Arc<Self>) -> OpOutput<()> {
        self.shut();
        OpOutput::ok(())
    }

    fn measure(&self, meter: &mut sys_types::Meter) {
        meter.bytes(self.retained.get());
    }

    fn network(&self) -> Option<&NetworkContext> {
        self.network.as_ref()
    }
}
