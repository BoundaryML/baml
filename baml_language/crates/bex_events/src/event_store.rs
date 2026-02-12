//! Global EventStore with an MPSC publisher thread.
//!
//! All events (engine + host-language spans) flow through this module.
//! The publisher thread buffers events and writes JSONL to file on `flush()`
//! (if `BAML_TRACE_FILE` is set).
//!
//! Collectors are separate — they track specific `root_span_id`s for
//! in-memory querying; ref-counting manages memory cleanup.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Mutex, OnceLock, mpsc};

use crate::{RuntimeEvent, SpanId};

// ─────────────────────────── Publisher Channel ───────────────────────────

/// Messages sent to the publisher thread.
enum PublisherMessage {
    /// A new event to buffer.
    Event(RuntimeEvent),
    /// Flush buffered events to disk; ack when done.
    Flush(mpsc::SyncSender<()>),
}

/// The global sender half of the publisher channel.
static PUBLISHER_TX: OnceLock<mpsc::SyncSender<PublisherMessage>> = OnceLock::new();

/// Lazily start the publisher thread and return the sender.
fn ensure_publisher() -> &'static mpsc::SyncSender<PublisherMessage> {
    PUBLISHER_TX.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel(4096);
        std::thread::Builder::new()
            .name("bex-publisher".into())
            .spawn(move || publisher_loop(rx))
            .expect("failed to spawn publisher thread");
        tx
    })
}

/// The publisher worker loop. Receives events and flush requests.
fn publisher_loop(rx: mpsc::Receiver<PublisherMessage>) {
    let mut buffer: Vec<RuntimeEvent> = Vec::new();
    loop {
        match rx.recv() {
            Ok(PublisherMessage::Event(e)) => {
                buffer.push(e);
            }
            Ok(PublisherMessage::Flush(ack)) => {
                write_jsonl_to_file(&buffer);
                buffer.clear();
                let _ = ack.send(());
            }
            Err(_) => {
                // Channel closed (process shutting down) — flush remaining events.
                write_jsonl_to_file(&buffer);
                break;
            }
        }
    }
}

/// Write buffered events to the JSONL file specified by `BAML_TRACE_FILE`.
/// If the env var is not set, this is a no-op (just discards the buffer).
fn write_jsonl_to_file(events: &[RuntimeEvent]) {
    let Some(trace_file) = std::env::var("BAML_TRACE_FILE").ok() else {
        return;
    };
    if events.is_empty() {
        return;
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&trace_file)
    else {
        return;
    };
    for event in events {
        let line = crate::serialize::event_to_jsonl(event);
        let _ = writeln!(file, "{line}");
    }
}

// ─────────────────────────── Collector Store ─────────────────────────────

/// In-memory storage for tracked root_span_ids (collector use case).
struct CollectorStore {
    events: HashMap<SpanId, Vec<RuntimeEvent>>,
    ref_counts: HashMap<SpanId, usize>,
}

impl CollectorStore {
    fn new() -> Self {
        Self {
            events: HashMap::new(),
            ref_counts: HashMap::new(),
        }
    }
}

static COLLECTOR_STORE: OnceLock<Mutex<CollectorStore>> = OnceLock::new();

fn collector_store() -> &'static Mutex<CollectorStore> {
    COLLECTOR_STORE.get_or_init(|| Mutex::new(CollectorStore::new()))
}

// ─────────────────────────── Public API ──────────────────────────────────

/// Send an event to the publisher thread. Always succeeds (drops if channel full).
///
/// Also stores the event in the collector map if the event's `root_span_id`
/// is being tracked.
pub fn emit(event: RuntimeEvent) {
    // Store in collector if tracked
    {
        let mut store = collector_store().lock().unwrap();
        let root = &event.ctx.root_span_id;
        if store.ref_counts.contains_key(root) {
            store
                .events
                .entry(root.clone())
                .or_default()
                .push(event.clone());
        }
    }

    // Send to publisher thread (drop on full — bounded channel)
    let tx = ensure_publisher();
    let _ = tx.try_send(PublisherMessage::Event(event));
}

/// Flush the publisher — writes all buffered events to JSONL file (if `BAML_TRACE_FILE` set).
/// Blocks until the publisher acknowledges the flush.
pub fn flush() {
    let tx = ensure_publisher();
    let (ack_tx, ack_rx) = mpsc::sync_channel(1);
    if tx.send(PublisherMessage::Flush(ack_tx)).is_ok() {
        // Block until publisher acks (30s timeout to avoid deadlock)
        let _ = ack_rx.recv_timeout(std::time::Duration::from_secs(30));
    }
}

/// Start tracking a root_span_id for in-memory querying (collector use case).
pub fn track(root_span_id: &SpanId) {
    let mut store = collector_store().lock().unwrap();
    *store.ref_counts.entry(root_span_id.clone()).or_insert(0) += 1;
    store.events.entry(root_span_id.clone()).or_default();
}

/// Stop tracking. When ref-count reaches 0, purge stored events for this root_span_id.
pub fn untrack(root_span_id: &SpanId) {
    let mut store = collector_store().lock().unwrap();
    if let Some(count) = store.ref_counts.get_mut(root_span_id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            store.ref_counts.remove(root_span_id);
            store.events.remove(root_span_id);
        }
    }
}

/// Query events for a tracked root_span_id (collector use case).
pub fn events_for_span(id: &SpanId) -> Option<Vec<RuntimeEvent>> {
    let store = collector_store().lock().unwrap();
    store.events.get(id).cloned()
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use crate::{EventKind, FunctionEvent, FunctionStart, SpanContext};
    use std::time::SystemTime;

    fn make_event(root_span_id: SpanId) -> RuntimeEvent {
        let span_id = SpanId::new();
        RuntimeEvent {
            ctx: SpanContext {
                span_id,
                parent_span_id: None,
                root_span_id,
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

        // SAFETY: test is single-threaded for env var access
        unsafe {
            std::env::set_var("BAML_TRACE_FILE", trace_path.to_str().unwrap());
        }

        let root = SpanId::new();
        emit(make_event(root));
        flush();

        let contents = std::fs::read_to_string(&trace_path).unwrap();
        assert!(!contents.is_empty(), "trace file should have content");
        assert!(contents.contains("test_fn"));

        // Clean up
        unsafe {
            std::env::remove_var("BAML_TRACE_FILE");
        }
    }

    #[test]
    fn test_track_emit_query_untrack() {
        let root = SpanId::new();

        // Track the root
        track(&root);

        // Emit events
        let event = make_event(root.clone());
        emit(event);

        // Query
        let events = events_for_span(&root).unwrap();
        assert_eq!(events.len(), 1);

        // Untrack → purge
        untrack(&root);
        assert!(events_for_span(&root).is_none());
    }

    #[test]
    fn test_ref_counting() {
        let root = SpanId::new();

        track(&root);
        track(&root); // ref_count = 2

        emit(make_event(root.clone()));

        untrack(&root); // ref_count = 1 → still tracked
        assert!(events_for_span(&root).is_some());

        untrack(&root); // ref_count = 0 → purged
        assert!(events_for_span(&root).is_none());
    }
}
