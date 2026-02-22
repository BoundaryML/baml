//! Native `EventSink` implementation: background thread + bounded channel + JSONL file writer.
//!
//! Call `start(path)` to create a `NativeEventSink` and spawn the publisher thread.
//! Events are buffered in-memory and written to the given JSONL file path
//! on `flush()` or when the channel is closed (process shutdown).
//!
//! **Guaranteed delivery:** Callers must call `flush()` before process shutdown (e.g. before
//! dropping the sink or exiting) to ensure all buffered events are written. The LSP and CFFI
//! bridges do this; short-lived processes that drop the sink without flushing may lose events.
//!
//! This crate does not read env vars — the caller decides where events go.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use bex_events::{EventSink, RuntimeEvent};

/// Messages sent to the publisher thread.
#[allow(clippy::large_enum_variant)]
enum PublisherMessage {
    /// A new event to buffer.
    Event(RuntimeEvent),
    /// Flush buffered events to disk; ack when done.
    Flush(mpsc::SyncSender<()>),
}

const AUTO_FLUSH_INTERVAL: Duration = Duration::from_secs(5);
const AUTO_FLUSH_THRESHOLD: usize = 1024;

/// Native event sink backed by a bounded channel and a background thread.
///
/// Created via [`start()`]. Implements [`EventSink`] — `send` dispatches to the
/// channel (non-blocking, drops on full), `flush` blocks until the publisher
/// thread writes all buffered events.
pub struct NativeEventSink {
    tx: mpsc::SyncSender<PublisherMessage>,
    dropped: AtomicUsize,
}

impl EventSink for NativeEventSink {
    fn send(&self, event: RuntimeEvent) {
        if self.tx.try_send(PublisherMessage::Event(event)).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn flush(&self) {
        let dropped = self.dropped.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            tracing::warn!(dropped, "bex-publisher: events dropped (channel full)");
        }
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.tx.send(PublisherMessage::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(Duration::from_secs(30));
        }
    }
}

/// Start the native event sink: spawn a `"bex-publisher"` background thread
/// that writes JSONL to `trace_file`, and return an `Arc<dyn EventSink>`.
///
/// The caller is responsible for determining the file path (e.g. by reading
/// `BAML_TRACE_FILE` env var). This crate does not read env vars.
pub fn start(trace_file: PathBuf) -> Arc<dyn EventSink> {
    let (tx, rx) = mpsc::sync_channel::<PublisherMessage>(4096);

    std::thread::Builder::new()
        .name("bex-publisher".into())
        .spawn(move || publisher_loop(rx, &trace_file))
        .expect("failed to spawn bex-publisher thread");

    Arc::new(NativeEventSink {
        tx,
        dropped: AtomicUsize::new(0),
    })
}

/// The publisher worker loop.
///
/// Auto-flushes when the buffer reaches `AUTO_FLUSH_THRESHOLD` events or
/// when `AUTO_FLUSH_INTERVAL` elapses without an explicit flush, preventing
/// unbounded buffer growth.
#[allow(clippy::needless_pass_by_value)] // rx is moved into this thread and must be owned
fn publisher_loop(rx: mpsc::Receiver<PublisherMessage>, trace_file: &Path) {
    let mut buffer: Vec<RuntimeEvent> = Vec::new();

    // Block on the first message so we don't spin when idle.
    let first = rx.recv();
    match first {
        Ok(PublisherMessage::Event(e)) => buffer.push(e),
        Ok(PublisherMessage::Flush(ack)) => {
            let _ = ack.send(());
        }
        Err(_) => return,
    }

    loop {
        match rx.recv_timeout(AUTO_FLUSH_INTERVAL) {
            Ok(PublisherMessage::Event(e)) => {
                buffer.push(e);
                if buffer.len() >= AUTO_FLUSH_THRESHOLD {
                    write_jsonl_to_file(&buffer, trace_file);
                    buffer.clear();
                }
            }
            Ok(PublisherMessage::Flush(ack)) => {
                write_jsonl_to_file(&buffer, trace_file);
                buffer.clear();
                let _ = ack.send(());
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                write_jsonl_to_file(&buffer, trace_file);
                buffer.clear();
                // Park until the next message so we don't spin on timeouts when idle.
                match rx.recv() {
                    Ok(PublisherMessage::Event(e)) => buffer.push(e),
                    Ok(PublisherMessage::Flush(ack)) => {
                        let _ = ack.send(());
                    }
                    Err(_) => break,
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                write_jsonl_to_file(&buffer, trace_file);
                break;
            }
        }
    }
}

/// Write buffered events to the given JSONL file (append mode).
fn write_jsonl_to_file(events: &[RuntimeEvent], trace_file: &Path) {
    if events.is_empty() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_file)
    else {
        tracing::warn!(
            ?trace_file,
            "bex-publisher: failed to open trace file, dropping {} events",
            events.len()
        );
        return;
    };
    for event in events {
        let line = bex_events::serialize::event_to_jsonl(event);
        let _ = writeln!(file, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use bex_events::{EventKind, FunctionEvent, FunctionStart, RuntimeEvent, SpanContext, SpanId};
    use web_time::SystemTime;

    use super::*;

    fn make_event(span_id: SpanId) -> RuntimeEvent {
        RuntimeEvent {
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id,
            },
            call_stack: vec![],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: "test_fn".into(),
                args: vec![],
                tags: vec![],
            })),
        }
    }

    #[test]
    fn test_emit_and_flush_to_file() {
        let dir = tempfile::tempdir().unwrap();
        let trace_path = dir.path().join("trace.jsonl");

        let sink = start(trace_path.clone());
        let span = SpanId::new();
        sink.send(make_event(span));
        sink.flush();

        let contents = std::fs::read_to_string(&trace_path).unwrap();
        assert!(!contents.is_empty(), "trace file should have content");
        assert!(contents.contains("test_fn"));
    }
}
