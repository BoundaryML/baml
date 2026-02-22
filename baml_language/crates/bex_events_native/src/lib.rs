//! Native `EventSink` implementation: background thread + bounded channel + JSONL file writer.
//!
//! Call `start(path)` to create a `NativeEventSink` and spawn the publisher thread.
//! Events are buffered in-memory and written to the given JSONL file path
//! on `flush()` or when the channel is closed (process shutdown).
//!
//! This crate does not read env vars — the caller decides where events go.

use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, mpsc},
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

/// Native event sink backed by a bounded channel and a background thread.
///
/// Created via [`start()`]. Implements [`EventSink`] — `send` dispatches to the
/// channel (non-blocking, drops on full), `flush` blocks until the publisher
/// thread writes all buffered events.
pub struct NativeEventSink {
    tx: mpsc::SyncSender<PublisherMessage>,
}

impl EventSink for NativeEventSink {
    fn send(&self, event: RuntimeEvent) {
        let _ = self.tx.try_send(PublisherMessage::Event(event));
    }

    fn flush(&self) {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self.tx.send(PublisherMessage::Flush(ack_tx)).is_ok() {
            let _ = ack_rx.recv_timeout(std::time::Duration::from_secs(30));
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

    Arc::new(NativeEventSink { tx })
}

/// The publisher worker loop.
#[allow(clippy::needless_pass_by_value)]
fn publisher_loop(rx: mpsc::Receiver<PublisherMessage>, trace_file: &PathBuf) {
    let mut buffer: Vec<RuntimeEvent> = Vec::new();
    loop {
        match rx.recv() {
            Ok(PublisherMessage::Event(e)) => {
                buffer.push(e);
            }
            Ok(PublisherMessage::Flush(ack)) => {
                write_jsonl_to_file(&buffer, trace_file);
                buffer.clear();
                let _ = ack.send(());
            }
            Err(_) => {
                // Channel closed — flush remaining events.
                write_jsonl_to_file(&buffer, trace_file);
                break;
            }
        }
    }
}

/// Write buffered events to the given JSONL file (append mode).
fn write_jsonl_to_file(events: &[RuntimeEvent], trace_file: &PathBuf) {
    if events.is_empty() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(trace_file)
    else {
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
