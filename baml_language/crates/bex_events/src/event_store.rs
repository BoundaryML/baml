//! Global `EventStore` with an MPSC publisher thread.
//!
//! All events (engine + host-language spans) flow through this module.
//! The publisher thread buffers events and writes JSONL to file on `flush()`
//! (if `BAML_TRACE_FILE` is set).
//!
//! Collectors track specific engine span IDs (one per `call_function`
//! invocation). Events are routed to the correct bucket by matching
//! `span_id` or `parent_span_id` against tracked IDs.

use std::{
    collections::HashMap,
    io::Write,
    sync::{Mutex, OnceLock, mpsc},
};

use crate::{RuntimeEvent, SpanId};

// ─────────────────────────── Publisher Channel ───────────────────────────

/// Messages sent to the publisher thread.
#[allow(clippy::large_enum_variant)]
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
#[allow(clippy::needless_pass_by_value)] // Receiver must be owned by the thread
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

/// In-memory storage for tracked engine span IDs (collector use case).
///
/// Events are routed by matching `span_id` or `parent_span_id` against
/// tracked keys, so each function call's events land in the right bucket
/// even when multiple calls share the same `root_span_id` under `@trace`.
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
/// Also stores the event in the collector store if the event's `span_id`
/// or `parent_span_id` matches a tracked engine span.
pub fn emit(event: RuntimeEvent) {
    // Store in collector if tracked — route by span_id or parent_span_id
    {
        let mut store = collector_store().lock().unwrap();

        // The function's own events have span_id == engine_span_id.
        // Child events (LLM calls) have parent_span_id == engine_span_id.
        let span = &event.ctx.span_id;
        if store.ref_counts.contains_key(span) {
            store
                .events
                .entry(span.clone())
                .or_default()
                .push(event.clone());
        } else if let Some(parent) = &event.ctx.parent_span_id {
            if store.ref_counts.contains_key(parent) {
                store
                    .events
                    .entry(parent.clone())
                    .or_default()
                    .push(event.clone());
            }
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

/// Start tracking a span ID for in-memory querying (collector use case).
/// Typically called with the `engine_span_id` (unique per `call_function`).
pub fn track(span_id: &SpanId) {
    let mut store = collector_store().lock().unwrap();
    *store.ref_counts.entry(span_id.clone()).or_insert(0) += 1;
    store.events.entry(span_id.clone()).or_default();
}

/// Stop tracking. When ref-count reaches 0, purge stored events for this span.
pub fn untrack(span_id: &SpanId) {
    let mut store = collector_store().lock().unwrap();
    if let Some(count) = store.ref_counts.get_mut(span_id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            store.ref_counts.remove(span_id);
            store.events.remove(span_id);
        }
    }
}

/// Query events for a tracked span ID (collector use case).
pub fn events_for_span(id: &SpanId) -> Option<Vec<RuntimeEvent>> {
    let store = collector_store().lock().unwrap();
    store.events.get(id).cloned()
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use std::{
        sync::{Mutex, OnceLock},
        time::SystemTime,
    };

    use super::*;
    use crate::{EventKind, FunctionEvent, FunctionStart, SpanContext};

    /// Global lock to guard `BAML_TRACE_FILE` env var mutations against parallel test races.
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    /// Create an event whose `span_id` matches the given ID (function's own event).
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

    /// Create a child event whose `parent_span_id` matches the given parent.
    fn make_child_event(parent_span_id: SpanId, root_span_id: SpanId) -> RuntimeEvent {
        RuntimeEvent {
            ctx: SpanContext {
                span_id: SpanId::new(),
                parent_span_id: Some(parent_span_id),
                root_span_id,
            },
            call_stack: vec![],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: "child_fn".into(),
                args: vec![],
                tags: vec![],
            })),
        }
    }

    #[test]
    fn test_emit_and_flush_to_file() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let trace_path = dir.path().join("trace.jsonl");

        // SAFETY: guarded by ENV_LOCK to prevent parallel test races.
        unsafe {
            std::env::set_var("BAML_TRACE_FILE", trace_path.to_str().unwrap());
        }

        let span = SpanId::new();
        emit(make_event(span));
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
        let engine_span = SpanId::new();

        // Track the engine span
        track(&engine_span);

        // Emit event with span_id matching the tracked span
        emit(make_event(engine_span.clone()));

        // Query
        let events = events_for_span(&engine_span).unwrap();
        assert_eq!(events.len(), 1);

        // Untrack → purge
        untrack(&engine_span);
        assert!(events_for_span(&engine_span).is_none());
    }

    #[test]
    fn test_child_events_routed_by_parent() {
        let engine_span = SpanId::new();
        let host_root = SpanId::new();

        track(&engine_span);

        // Emit the function's own event (span_id matches)
        emit(make_event(engine_span.clone()));

        // Emit a child event (parent_span_id matches)
        emit(make_child_event(engine_span.clone(), host_root));

        let events = events_for_span(&engine_span).unwrap();
        assert_eq!(events.len(), 2);

        untrack(&engine_span);
    }

    #[test]
    fn test_ref_counting() {
        let span = SpanId::new();

        track(&span);
        track(&span); // ref_count = 2

        emit(make_event(span.clone()));

        untrack(&span); // ref_count = 1 → still tracked
        assert!(events_for_span(&span).is_some());

        untrack(&span); // ref_count = 0 → purged
        assert!(events_for_span(&span).is_none());
    }
}
