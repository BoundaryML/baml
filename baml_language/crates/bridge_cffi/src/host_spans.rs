//! Bridge-agnostic HostSpanManager — core span tracking for `@trace`.
//!
//! This module contains the core span lifecycle logic without any
//! Python/PyO3 dependencies. The PyO3 wrapper in `bridge_python` delegates
//! all operations to this struct.

use std::{collections::HashMap, time::Instant};

use bex_events::{
    EventKind, FunctionEvent, FunctionStart, RuntimeEvent, SpanContext, SpanId, TraceTags,
    event_store,
};

/// One entry on the host-language span stack.
#[derive(Clone, Debug)]
struct HostSpanEntry {
    span_id: SpanId,
    root_span_id: SpanId,
    function_name: String,
    started_at: Instant,
}

/// Manages host-side span tracking for `@trace`.
///
/// Each instance tracks a single async task or thread's span stack.
/// `enter()` / `exit_ok()` / `exit_error()` drive the lifecycle and
/// emit events to the global `bex_events::event_store`.
#[derive(Clone)]
pub struct HostSpanManager {
    stack: Vec<HostSpanEntry>,
    tags: HashMap<String, String>,
}

impl HostSpanManager {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            tags: HashMap::new(),
        }
    }

    /// Deep clone for async context forking.
    pub fn deep_clone(&self) -> Self {
        self.clone()
    }

    /// Build the call_stack (list of SpanIds from root to tip).
    fn call_stack(&self) -> Vec<SpanId> {
        self.stack.iter().map(|e| e.span_id.clone()).collect()
    }

    /// Enter a new host-language span (`@trace` function start).
    ///
    /// Creates a `SpanId`, determines parent/root from the current stack,
    /// emits a `function_start` event via the global EventStore, and pushes the entry.
    pub fn enter(&mut self, name: String, args: serde_json::Value) {
        let span_id = SpanId::new();

        let (parent_span_id, root_span_id) = match self.stack.last() {
            Some(parent) => (Some(parent.span_id.clone()), parent.root_span_id.clone()),
            None => (None, span_id.clone()),
        };

        // Build call_stack: existing path + this new span
        let mut call_stack = self.call_stack();
        call_stack.push(span_id.clone());

        // Convert args JSON value to BexExternalValue for the event
        let args_external = json_to_bex_values(&args);

        // Inherit current tags so children carry parent's tags in their start event.
        let inherited_tags: TraceTags = self
            .tags
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        event_store::emit(RuntimeEvent {
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id,
                root_span_id: root_span_id.clone(),
            },
            call_stack,
            timestamp: std::time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: name.clone(),
                args: args_external,
                tags: inherited_tags,
            })),
        });

        self.stack.push(HostSpanEntry {
            span_id,
            root_span_id,
            function_name: name,
            started_at: Instant::now(),
        });
    }

    /// Exit the current span successfully.
    pub fn exit_ok(&mut self) {
        self.exit_inner(bex_project::BexExternalValue::Null);
    }

    /// Exit the current span with an error.
    pub fn exit_error(&mut self, error_message: String) {
        self.exit_inner(bex_project::BexExternalValue::String(error_message));
    }

    /// Merge tags into the current span and emit a `SetTags` event.
    pub fn upsert_tags(&mut self, tags: HashMap<String, String>) {
        for (k, v) in &tags {
            self.tags.insert(k.clone(), v.clone());
        }

        if let Some(entry) = self.stack.last() {
            let call_stack = self.call_stack();
            let trace_tags: TraceTags = tags.into_iter().collect();

            event_store::emit(RuntimeEvent {
                ctx: SpanContext {
                    span_id: entry.span_id.clone(),
                    parent_span_id: self.stack.iter().rev().nth(1).map(|e| e.span_id.clone()),
                    root_span_id: entry.root_span_id.clone(),
                },
                call_stack,
                timestamp: std::time::SystemTime::now(),
                event: EventKind::SetTags(trace_tags),
            });
        }
    }

    /// Number of active spans (call depth).
    pub fn context_depth(&self) -> usize {
        self.stack.len()
    }

    /// Build a `HostSpanContext` for passing to `call_function`.
    ///
    /// Returns `None` if there are no active host spans.
    pub fn host_span_context(&self) -> Option<bex_events::HostSpanContext> {
        let current = self.stack.last()?;
        Some(bex_events::HostSpanContext {
            root_span_id: current.root_span_id.clone(),
            parent_span_id: current.span_id.clone(),
            call_stack: self.call_stack(),
        })
    }

    fn exit_inner(&mut self, result: bex_project::BexExternalValue) {
        let Some(entry) = self.stack.pop() else {
            #[cfg(debug_assertions)]
            eprintln!("HostSpanManager::exit_inner called with empty stack");
            return;
        };

        // call_stack includes the popped span
        let mut call_stack = self.call_stack();
        call_stack.push(entry.span_id.clone());

        event_store::emit(RuntimeEvent {
            ctx: SpanContext {
                span_id: entry.span_id,
                parent_span_id: self.stack.last().map(|e| e.span_id.clone()),
                root_span_id: entry.root_span_id,
            },
            call_stack,
            timestamp: std::time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(bex_events::FunctionEnd {
                name: entry.function_name,
                result,
                duration: entry.started_at.elapsed(),
            })),
        });
    }
}

impl Default for HostSpanManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a `serde_json::Value` (typically the args dict from Python) into
/// a Vec<BexExternalValue> suitable for the event.
///
/// If the value is a JSON object, each key-value pair becomes a separate entry.
/// Otherwise wraps the whole value as a single element.
fn json_to_bex_values(value: &serde_json::Value) -> Vec<bex_project::BexExternalValue> {
    if let serde_json::Value::Object(map) = value {
        map.values().map(json_value_to_bex).collect()
    } else {
        vec![json_value_to_bex(value)]
    }
}

fn json_value_to_bex(value: &serde_json::Value) -> bex_project::BexExternalValue {
    use bex_project::BexExternalValue;

    match value {
        serde_json::Value::Null => BexExternalValue::Null,
        serde_json::Value::Bool(b) => BexExternalValue::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                BexExternalValue::Int(i)
            } else if let Some(f) = n.as_f64() {
                BexExternalValue::Float(f)
            } else {
                BexExternalValue::Null
            }
        }
        serde_json::Value::String(s) => BexExternalValue::String(s.clone()),
        serde_json::Value::Array(items) => BexExternalValue::Array {
            element_type: bex_project::Ty::Null,
            items: items.iter().map(json_value_to_bex).collect(),
        },
        serde_json::Value::Object(map) => BexExternalValue::Map {
            key_type: bex_project::Ty::String,
            value_type: bex_project::Ty::Null,
            entries: map
                .iter()
                .map(|(k, v)| (k.clone(), json_value_to_bex(v)))
                .collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enter_exit_depth() {
        let mut mgr = HostSpanManager::new();
        assert_eq!(mgr.context_depth(), 0);

        mgr.enter("outer".into(), serde_json::json!({}));
        assert_eq!(mgr.context_depth(), 1);

        mgr.enter("inner".into(), serde_json::json!({}));
        assert_eq!(mgr.context_depth(), 2);

        mgr.exit_ok();
        assert_eq!(mgr.context_depth(), 1);

        mgr.exit_ok();
        assert_eq!(mgr.context_depth(), 0);
    }

    #[test]
    fn test_deep_clone_independent() {
        let mut mgr = HostSpanManager::new();
        mgr.enter("func".into(), serde_json::json!({}));

        let clone = mgr.deep_clone();
        assert_eq!(clone.context_depth(), 1);

        mgr.exit_ok();
        assert_eq!(mgr.context_depth(), 0);
        assert_eq!(clone.context_depth(), 1); // clone unaffected
    }

    #[test]
    fn test_upsert_tags() {
        let mut mgr = HostSpanManager::new();
        mgr.enter("func".into(), serde_json::json!({}));

        let mut tags = HashMap::new();
        tags.insert("env".into(), "test".into());
        mgr.upsert_tags(tags);

        // Tags should be merged
        assert_eq!(mgr.tags.get("env"), Some(&"test".to_string()));

        mgr.exit_ok();
    }
}
