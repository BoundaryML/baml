//! Global `EventStore` — in-memory event aggregation for `Collector` queries.
//!
//! All events flow through `emit()` for routing into the `CollectorStore`.
//! Event persistence (JSONL file, JS callback, etc.) is handled by an
//! `EventSink` injected at the `BexEngine` / `HostSpanManager` level.
//!
//! Collectors track specific engine span IDs (one per `call_function`
//! invocation). Events are routed to the correct bucket by matching
//! `span_id` or `parent_span_id` against tracked IDs.

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use crate::{RuntimeEvent, SpanId};

// ─────────────────────────── Event Sink ─────────────────────────────────

/// Sink for events; implemented by the runtime layer.
///
/// Native: thread + channel + JSONL file writer (in `bex_events_native`).
/// WASM: no-op (or future JS callback).
pub trait EventSink: Send + Sync {
    /// Send an event (e.g. into a channel). Should be non-blocking.
    fn send(&self, event: RuntimeEvent);
    /// Flush buffered events. May block until the consumer has written.
    fn flush(&self);
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

/// Store an event in the collector store if the event's `span_id`
/// or `parent_span_id` matches a tracked engine span.
///
/// This function only performs in-memory aggregation for `Collector` queries.
/// To also persist events (JSONL file, JS callback, etc.), set an `EventSink`
/// on the `BexEngine` — the engine calls `sink.send(event)` after this function.
pub fn emit(event: &RuntimeEvent) {
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
mod tests {
    use web_time::SystemTime;

    use super::*;
    use crate::{EventKind, FunctionEvent, FunctionStart, SpanContext};

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
    fn test_track_emit_query_untrack() {
        let engine_span = SpanId::new();

        // Track the engine span
        track(&engine_span);

        // Emit event with span_id matching the tracked span
        emit(&make_event(engine_span.clone()));

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
        emit(&make_event(engine_span.clone()));

        // Emit a child event (parent_span_id matches)
        emit(&make_child_event(engine_span.clone(), host_root));

        let events = events_for_span(&engine_span).unwrap();
        assert_eq!(events.len(), 2);

        untrack(&engine_span);
    }

    #[test]
    fn test_ref_counting() {
        let span = SpanId::new();

        track(&span);
        track(&span); // ref_count = 2

        emit(&make_event(span.clone()));

        untrack(&span); // ref_count = 1 → still tracked
        assert!(events_for_span(&span).is_some());

        untrack(&span); // ref_count = 0 → purged
        assert!(events_for_span(&span).is_none());
    }
}
