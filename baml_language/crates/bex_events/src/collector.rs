//! Collector — tracks engine spans for in-memory querying of function logs.
//!
//! Users attach a `Collector` to BAML function calls via `baml_options`.
//! The engine calls `collector.track(engine_span_id)` at the start of each
//! call. Each `call_function` invocation creates a unique `engine_span_id`,
//! so the collector naturally gets one log per call.
//!
//! The event store keys events by the span they belong to (via `span_id` and
//! `parent_span_id` matching), so events from different function calls under
//! the same `@trace` root are stored in separate buckets.
//!
//! Drop-based cleanup: when the Collector is dropped, all tracked spans are
//! untracked from the event store (ref-count decremented).

use std::{collections::HashMap, sync::Mutex, time::Duration};

use bex_external_types::BexExternalValue;
use indexmap::IndexSet;

use crate::{EventKind, FunctionEvent, RuntimeEvent, SpanId, event_store};

// ─────────────────────────── Collector ────────────────────────────────────

/// Tracks engine span IDs and provides access to function logs.
pub struct Collector {
    name: String,
    /// Engine span IDs this collector is tracking, in insertion order.
    /// Each entry corresponds to one `call_function` invocation.
    tracked_spans: Mutex<IndexSet<SpanId>>,
}

impl Collector {
    pub fn new(name: String) -> Self {
        Self {
            name,
            tracked_spans: Mutex::new(IndexSet::new()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Start tracking an engine span. Called by the engine when a function
    /// is invoked with this collector attached.
    pub fn track(&self, engine_span_id: &SpanId) {
        let mut spans = self.tracked_spans.lock().unwrap();
        if spans.insert(engine_span_id.clone()) {
            event_store::track(engine_span_id); // increment ref count only on first insert
        }
    }

    /// Get all function logs (one per tracked call), in insertion order.
    pub fn logs(&self) -> Vec<FunctionLog> {
        let span_ids: Vec<SpanId> = self.tracked_spans.lock().unwrap().iter().cloned().collect();
        span_ids
            .iter()
            .filter_map(|span_id| {
                let events = event_store::events_for_span(span_id)?;
                Some(FunctionLog::from_events(span_id.clone(), &events))
            })
            .collect()
    }

    /// Get the most recent function log.
    pub fn last(&self) -> Option<FunctionLog> {
        let last = self.tracked_spans.lock().unwrap().last().cloned()?;
        let events = event_store::events_for_span(&last)?;
        Some(FunctionLog::from_events(last, &events))
    }

    /// Aggregate usage across all tracked calls.
    pub fn usage(&self) -> Usage {
        self.logs()
            .iter()
            .fold(Usage::default(), |acc, log| acc.add(&log.usage))
    }

    /// Clear all tracked logs and release event store references.
    /// Returns the number of spans that were cleared.
    pub fn clear(&self) -> usize {
        let mut spans = self.tracked_spans.lock().unwrap();
        let count = spans.len();
        for span_id in spans.drain(..) {
            event_store::untrack(&span_id);
        }
        count
    }

    /// Look up a specific log by its engine span ID string.
    pub fn id(&self, span_id_str: &str) -> Option<FunctionLog> {
        let matched = {
            let spans = self.tracked_spans.lock().unwrap();
            spans.iter().find(|s| s.to_string() == span_id_str).cloned()
        };
        let span_id = matched?;
        let events = event_store::events_for_span(&span_id)?;
        Some(FunctionLog::from_events(span_id, &events))
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        self.clear();
    }
}

// ─────────────────────────── View Types ──────────────────────────────────

/// Read-only view of a single BAML function invocation.
#[derive(Clone, Debug)]
pub struct FunctionLog {
    pub id: SpanId,
    pub function_name: String,
    pub timing: Timing,
    pub usage: Usage,
    pub calls: Vec<LLMCall>,
    pub tags: HashMap<String, String>,
    pub args: Vec<BexExternalValue>,
    pub result: Option<BexExternalValue>,
}

/// Timing information for a span.
#[derive(Clone, Debug, Default)]
pub struct Timing {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
}

/// Token usage from an LLM call.
#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}

impl Usage {
    /// Add two `Usage` values, summing each field.
    #[must_use]
    pub fn add(&self, other: &Usage) -> Usage {
        Usage {
            input_tokens: sum_option(self.input_tokens, other.input_tokens),
            output_tokens: sum_option(self.output_tokens, other.output_tokens),
            cached_input_tokens: sum_option(self.cached_input_tokens, other.cached_input_tokens),
        }
    }
}

/// A single LLM call within a function invocation (child span).
#[derive(Clone, Debug)]
pub struct LLMCall {
    pub function_name: String,
    pub provider: Option<String>,
    pub timing: Timing,
    pub usage: Usage,
}

// ─────────────────────────── from_events ─────────────────────────────────

impl FunctionLog {
    /// Materialize a `FunctionLog` from a list of `RuntimeEvent`s for an engine span.
    ///
    /// The `engine_span_id` identifies this specific function call. Events are
    /// already filtered to this call's bucket by the event store.
    ///
    /// Walks the event list:
    /// - Root `FunctionStart` -> `function_name`, args, tags, start time
    /// - Root `FunctionEnd` -> result, duration
    /// - Child `FunctionStart`/`FunctionEnd` pairs -> `LLMCall` entries
    /// - `SetTags` -> merged into tags map
    pub fn from_events(engine_span_id: SpanId, events: &[RuntimeEvent]) -> Self {
        struct ChildSpan {
            #[allow(dead_code)]
            function_name: String,
            start_time_utc_ms: i64,
        }

        let mut function_name = String::new();
        let mut args = vec![];
        let mut result = None;
        let mut timing = Timing::default();
        let mut tags: HashMap<String, String> = HashMap::new();
        // TODO: Aggregate usage from child LLMCall spans once usage events are implemented.
        let usage = Usage::default();
        let mut child_starts: HashMap<SpanId, ChildSpan> = HashMap::new();
        let mut calls: Vec<LLMCall> = vec![];

        for event in events {
            let is_root = event.ctx.span_id == engine_span_id;

            match &event.event {
                EventKind::Function(FunctionEvent::Start(start)) => {
                    if is_root {
                        function_name.clone_from(&start.name);
                        args.clone_from(&start.args);
                        timing.start_time_utc_ms = system_time_to_epoch_ms(event.timestamp);
                        // Merge tags from start event
                        for (k, v) in &start.tags {
                            tags.insert(k.clone(), v.clone());
                        }
                    } else {
                        // Child span start — record for pairing with end
                        child_starts.insert(
                            event.ctx.span_id.clone(),
                            ChildSpan {
                                function_name: start.name.clone(),
                                start_time_utc_ms: system_time_to_epoch_ms(event.timestamp),
                            },
                        );
                    }
                }
                EventKind::Function(FunctionEvent::End(end)) => {
                    if is_root {
                        result = Some(end.result.clone());
                        timing.duration_ms =
                            Some(i64::try_from(end.duration.as_millis()).unwrap_or(i64::MAX));
                    } else {
                        // Child span end — pair with the start
                        let child_start = child_starts.remove(&event.ctx.span_id);
                        calls.push(LLMCall {
                            function_name: end.name.clone(),
                            provider: None, // Deferred: requires EventKind::LLMRequest
                            timing: Timing {
                                start_time_utc_ms: child_start
                                    .as_ref()
                                    .map(|s| s.start_time_utc_ms)
                                    .unwrap_or(0),
                                duration_ms: Some(
                                    i64::try_from(end.duration.as_millis()).unwrap_or(i64::MAX),
                                ),
                            },
                            usage: Usage::default(), // Deferred: requires usage events
                        });
                    }
                }
                EventKind::SetTags(tag_list) => {
                    for (k, v) in tag_list {
                        tags.insert(k.clone(), v.clone());
                    }
                }
            }
        }

        FunctionLog {
            id: engine_span_id,
            function_name,
            timing,
            usage,
            calls,
            tags,
            args,
            result,
        }
    }
}

// ─────────────────────────── Helpers ─────────────────────────────────────

fn system_time_to_epoch_ms(t: web_time::SystemTime) -> i64 {
    i64::try_from(
        t.duration_since(web_time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis(),
    )
    .unwrap_or(i64::MAX)
}

fn sum_option(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    }
}

// ─────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use web_time::SystemTime;

    use super::*;
    use crate::{FunctionEnd, FunctionStart, SpanContext};

    fn make_start_event(
        span_id: SpanId,
        root_span_id: SpanId,
        parent: Option<SpanId>,
        name: &str,
        args: Vec<BexExternalValue>,
        tags: Vec<(String, String)>,
    ) -> RuntimeEvent {
        RuntimeEvent {
            ctx: SpanContext {
                span_id,
                parent_span_id: parent,
                root_span_id,
            },
            call_stack: vec![],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: name.to_string(),
                args,
                tags,
            })),
        }
    }

    fn make_end_event(
        span_id: SpanId,
        root_span_id: SpanId,
        parent: Option<SpanId>,
        name: &str,
        result: BexExternalValue,
        duration: Duration,
    ) -> RuntimeEvent {
        RuntimeEvent {
            ctx: SpanContext {
                span_id,
                parent_span_id: parent,
                root_span_id,
            },
            call_stack: vec![],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(Box::new(FunctionEnd {
                name: name.to_string(),
                result,
                duration,
            }))),
        }
    }

    #[test]
    fn from_events_basic() {
        let root = SpanId::new();
        let events = vec![
            make_start_event(
                root.clone(),
                root.clone(),
                None,
                "my_func",
                vec![BexExternalValue::Int(42)],
                vec![],
            ),
            make_end_event(
                root.clone(),
                root.clone(),
                None,
                "my_func",
                BexExternalValue::String("hello".into()),
                Duration::from_millis(100),
            ),
        ];

        let log = FunctionLog::from_events(root, &events);
        assert_eq!(log.function_name, "my_func");
        assert_eq!(log.args, vec![BexExternalValue::Int(42)]);
        assert_eq!(log.result, Some(BexExternalValue::String("hello".into())));
        assert_eq!(log.timing.duration_ms, Some(100));
        assert!(log.timing.start_time_utc_ms > 0);
        assert!(log.calls.is_empty());
    }

    #[test]
    fn from_events_with_child_spans() {
        let root = SpanId::new();
        let child1 = SpanId::new();
        let child2 = SpanId::new();

        let events = vec![
            make_start_event(root.clone(), root.clone(), None, "pipeline", vec![], vec![]),
            make_start_event(
                child1.clone(),
                root.clone(),
                Some(root.clone()),
                "extract",
                vec![],
                vec![],
            ),
            make_end_event(
                child1,
                root.clone(),
                Some(root.clone()),
                "extract",
                BexExternalValue::Null,
                Duration::from_millis(50),
            ),
            make_start_event(
                child2.clone(),
                root.clone(),
                Some(root.clone()),
                "summarize",
                vec![],
                vec![],
            ),
            make_end_event(
                child2,
                root.clone(),
                Some(root.clone()),
                "summarize",
                BexExternalValue::Null,
                Duration::from_millis(30),
            ),
            make_end_event(
                root.clone(),
                root.clone(),
                None,
                "pipeline",
                BexExternalValue::String("done".into()),
                Duration::from_millis(200),
            ),
        ];

        let log = FunctionLog::from_events(root, &events);
        assert_eq!(log.function_name, "pipeline");
        assert_eq!(log.calls.len(), 2);
        assert_eq!(log.calls[0].function_name, "extract");
        assert_eq!(log.calls[0].timing.duration_ms, Some(50));
        assert_eq!(log.calls[1].function_name, "summarize");
        assert_eq!(log.calls[1].timing.duration_ms, Some(30));
    }

    #[test]
    fn from_events_with_tags() {
        let root = SpanId::new();
        let events = vec![
            make_start_event(
                root.clone(),
                root.clone(),
                None,
                "func",
                vec![],
                vec![("env".into(), "test".into())],
            ),
            RuntimeEvent {
                ctx: SpanContext {
                    span_id: root.clone(),
                    parent_span_id: None,
                    root_span_id: root.clone(),
                },
                call_stack: vec![],
                timestamp: SystemTime::now(),
                event: EventKind::SetTags(vec![("user".into(), "alice".into())]),
            },
            make_end_event(
                root.clone(),
                root.clone(),
                None,
                "func",
                BexExternalValue::Null,
                Duration::from_millis(10),
            ),
        ];

        let log = FunctionLog::from_events(root, &events);
        assert_eq!(log.tags.get("env"), Some(&"test".to_string()));
        assert_eq!(log.tags.get("user"), Some(&"alice".to_string()));
    }

    #[test]
    fn collector_track_and_logs() {
        let collector = Collector::new("test".into());

        // Empty before any tracking
        assert!(collector.logs().is_empty());
        assert!(collector.last().is_none());

        // Track a span and emit events
        let root = SpanId::new();
        collector.track(&root);

        event_store::emit(&make_start_event(
            root.clone(),
            root.clone(),
            None,
            "my_func",
            vec![BexExternalValue::Int(1)],
            vec![],
        ));
        event_store::emit(&make_end_event(
            root.clone(),
            root,
            None,
            "my_func",
            BexExternalValue::Int(2),
            Duration::from_millis(50),
        ));

        let logs = collector.logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].function_name, "my_func");

        let last = collector.last().unwrap();
        assert_eq!(last.function_name, "my_func");
    }

    #[test]
    fn collector_clear_releases_events() {
        let collector = Collector::new("test".into());
        let root = SpanId::new();
        collector.track(&root);

        event_store::emit(&make_start_event(
            root.clone(),
            root.clone(),
            None,
            "func",
            vec![],
            vec![],
        ));

        assert_eq!(collector.clear(), 1);
        assert!(collector.logs().is_empty());
        assert!(event_store::events_for_span(&root).is_none());
    }

    #[test]
    fn collector_drop_releases_events() {
        let root = SpanId::new();
        {
            let collector = Collector::new("test".into());
            collector.track(&root);
            event_store::emit(&make_start_event(
                root.clone(),
                root.clone(),
                None,
                "func",
                vec![],
                vec![],
            ));
            // collector dropped here
        }
        assert!(event_store::events_for_span(&root).is_none());
    }

    #[test]
    fn collector_multiple_roots_ordered() {
        let collector = Collector::new("test".into());

        let root1 = SpanId::new();
        let root2 = SpanId::new();
        collector.track(&root1);
        collector.track(&root2);

        event_store::emit(&make_start_event(
            root1.clone(),
            root1.clone(),
            None,
            "first",
            vec![],
            vec![],
        ));
        event_store::emit(&make_end_event(
            root1.clone(),
            root1,
            None,
            "first",
            BexExternalValue::Null,
            Duration::from_millis(10),
        ));
        event_store::emit(&make_start_event(
            root2.clone(),
            root2.clone(),
            None,
            "second",
            vec![],
            vec![],
        ));
        event_store::emit(&make_end_event(
            root2.clone(),
            root2,
            None,
            "second",
            BexExternalValue::Null,
            Duration::from_millis(20),
        ));

        let logs = collector.logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].function_name, "first");
        assert_eq!(logs[1].function_name, "second");

        // last() should be the second one
        let last = collector.last().unwrap();
        assert_eq!(last.function_name, "second");
    }

    #[test]
    fn collector_id_lookup() {
        let collector = Collector::new("test".into());
        let root = SpanId::new();
        collector.track(&root);

        event_store::emit(&make_start_event(
            root.clone(),
            root.clone(),
            None,
            "func",
            vec![],
            vec![],
        ));

        let log = collector.id(&root.to_string()).unwrap();
        assert_eq!(log.function_name, "func");

        assert!(collector.id("nonexistent").is_none());
    }

    #[test]
    fn usage_aggregation() {
        let a = Usage {
            input_tokens: Some(10),
            output_tokens: Some(20),
            cached_input_tokens: None,
        };
        let b = Usage {
            input_tokens: Some(5),
            output_tokens: None,
            cached_input_tokens: Some(3),
        };
        let sum = a.add(&b);
        assert_eq!(sum.input_tokens, Some(15));
        assert_eq!(sum.output_tokens, Some(20));
        assert_eq!(sum.cached_input_tokens, Some(3));

        let empty = Usage::default();
        let sum2 = empty.add(&empty);
        assert_eq!(sum2.input_tokens, None);
    }

    #[test]
    fn collector_ref_counting_two_collectors() {
        let root = SpanId::new();
        let c1 = Collector::new("c1".into());
        let c2 = Collector::new("c2".into());

        c1.track(&root);
        c2.track(&root);

        event_store::emit(&make_start_event(
            root.clone(),
            root.clone(),
            None,
            "shared_func",
            vec![],
            vec![],
        ));

        // Both see the log
        assert_eq!(c1.logs().len(), 1);
        assert_eq!(c2.logs().len(), 1);

        // Drop c1 — ref count goes from 2 to 1, events still alive
        drop(c1);
        assert!(event_store::events_for_span(&root).is_some());
        assert_eq!(c2.logs().len(), 1);

        // Drop c2 — ref count goes to 0, events freed
        drop(c2);
        assert!(event_store::events_for_span(&root).is_none());
    }

    #[test]
    fn collector_clear_then_reuse() {
        let collector = Collector::new("test".into());

        let root1 = SpanId::new();
        collector.track(&root1);
        event_store::emit(&make_start_event(
            root1.clone(),
            root1,
            None,
            "first",
            vec![],
            vec![],
        ));
        assert_eq!(collector.logs().len(), 1);

        collector.clear();
        assert!(collector.logs().is_empty());

        // Track a new span
        let root2 = SpanId::new();
        collector.track(&root2);
        event_store::emit(&make_start_event(
            root2.clone(),
            root2,
            None,
            "second",
            vec![],
            vec![],
        ));
        assert_eq!(collector.logs().len(), 1);
        assert_eq!(collector.logs()[0].function_name, "second");
    }
}
