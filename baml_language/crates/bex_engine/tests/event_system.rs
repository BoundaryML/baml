//! Integration tests for the event system.
//!
//! These tests verify that runtime events are correctly emitted, collected,
//! and can be inspected. They serve as both documentation and regression
//! tests for the event infrastructure.

mod common;

use bex_engine::{
    BexEngine, BexExternalValue, FunctionCallContextBuilder, HostSpanContext, RuntimeEvent, SpanId,
};
use bex_events::{EventKind, FunctionEvent};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// RAII guard that untracks a span from the event store on drop.
struct TrackingGuard {
    root: SpanId,
}

impl Drop for TrackingGuard {
    fn drop(&mut self) {
        bex_events::event_store::untrack(&self.root);
    }
}

/// Create tracking context for event collection.
fn setup_tracking() -> (HostSpanContext, TrackingGuard) {
    let root = SpanId::new();
    bex_events::event_store::track(&root);
    let host_ctx = HostSpanContext {
        root_span_id: root.clone(),
        parent_span_id: root.clone(),
        call_stack: vec![root.clone()],
    };
    (host_ctx, TrackingGuard { root })
}

/// Drain collected events for the given root span.
fn collect_events(guard: &TrackingGuard) -> Vec<RuntimeEvent> {
    bex_events::event_store::events_for_span(&guard.root).unwrap_or_default()
}

/// Extract event summaries for easier assertion.
fn summarize_events(events: &[RuntimeEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| match &e.event {
            EventKind::Function(FunctionEvent::Start(s)) => format!("start:{}", s.name),
            EventKind::Function(FunctionEvent::End(e)) => format!("end:{}", e.name),
            EventKind::SetTags(_) => "tags".to_string(),
            EventKind::Custom(c) => format!("custom:{}", c.name),
            EventKind::Log(l) => format!("log:{}:{}", l.level, l.message),
        })
        .collect()
}

#[tokio::test]
async fn test_simple_function_events() {
    let source = r#"
        function main() -> int {
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    let result = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    assert_eq!(result, BexExternalValue::Int(42));

    let events = collect_events(&guard);
    let summary = summarize_events(&events);

    assert_eq!(summary, vec!["start:main", "end:main"]);
}

#[tokio::test]
async fn test_function_with_args_events() {
    let source = r#"
        function greet(first: string, second: string) -> string {
            first + " " + second
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    let result = engine
        .call_function(
            "greet",
            vec![
                BexExternalValue::String("Alice".into()),
                BexExternalValue::String("Smith".into()),
            ],
            call_ctx,
            true,
        )
        .await
        .unwrap();

    assert_eq!(result, BexExternalValue::String("Alice Smith".into()));

    let events = collect_events(&guard);

    // Verify args are captured in start event
    let start = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::Start(s)) => Some(s),
            _ => None,
        })
        .expect("Expected FunctionStart event");

    assert_eq!(start.args.len(), 2);
    assert_eq!(start.args[0], BexExternalValue::String("Alice".into()));
    assert_eq!(start.args[1], BexExternalValue::String("Smith".into()));

    // Verify result is captured in end event
    let end = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::End(e)) => Some(e),
            _ => None,
        })
        .expect("Expected FunctionEnd event");

    assert_eq!(end.result, BexExternalValue::String("Alice Smith".into()));
}

#[tokio::test]
async fn test_nested_function_events() {
    // Expression functions have trace: false, so only root produces events
    let source = r#"
        function inner() -> int {
            10
        }

        function outer() -> int {
            inner() + 5
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    let result = engine
        .call_function("outer", vec![], call_ctx, true)
        .await
        .unwrap();

    assert_eq!(result, BexExternalValue::Int(15));

    let events = collect_events(&guard);
    let summary = summarize_events(&events);

    // Only outer (root) function produces events; inner has trace: false
    assert_eq!(summary, vec!["start:outer", "end:outer"]);
}

#[tokio::test]
async fn test_complex_return_type_events() {
    let source = r#"
        function make_map() -> map<string, int> {
            { "a": 1, "b": 2 }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    let result = engine
        .call_function("make_map", vec![], call_ctx, true)
        .await
        .unwrap();

    // Verify map result
    match &result {
        BexExternalValue::Map { entries, .. } => {
            assert_eq!(entries.len(), 2);
            assert_eq!(entries.get("a"), Some(&BexExternalValue::Int(1)));
            assert_eq!(entries.get("b"), Some(&BexExternalValue::Int(2)));
        }
        _ => panic!("Expected Map, got {:?}", result),
    }

    let events = collect_events(&guard);

    // Verify complex result is captured in end event
    let end = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::End(e)) => Some(e),
            _ => None,
        })
        .expect("Expected FunctionEnd event");

    match &end.result {
        BexExternalValue::Map { entries, .. } => {
            assert_eq!(entries.len(), 2);
        }
        _ => panic!("Expected Map in end event, got {:?}", end.result),
    }
}

#[tokio::test]
async fn test_span_context_consistency() {
    let source = r#"
        function main() -> int { 1 }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let events = collect_events(&guard);
    assert_eq!(events.len(), 2);

    // All events should share the same root span ID
    for event in &events {
        assert_eq!(event.ctx.root_span_id, guard.root);
    }

    // Start and end should have the same span_id (same span)
    assert_eq!(events[0].ctx.span_id, events[1].ctx.span_id);
}

// === Future tests for Phase 4+ ===
// These tests are commented out until the corresponding features are implemented.

/*
#[tokio::test]
async fn test_custom_event_emission() {
    let source = r#"
        function emit_event() -> null {
            baml.events.send("user_clicked", { button: "submit", x: 100 })
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, std::sync::Arc::new(sys_native::SysOps::native()), None).unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine.call_function("emit_event", vec![], call_ctx, true).await.unwrap();

    let events = collect_events(&guard);

    // Should have: start:emit_event, custom:user_clicked, end:emit_event
    let custom = events.iter().find_map(|e| match &e.event {
        EventKind::Custom(c) => Some(c),
        _ => None,
    }).expect("Expected Custom event");

    assert_eq!(custom.name, "user_clicked");
    // Verify data contains expected fields
}

#[tokio::test]
async fn test_log_event_emission() {
    let source = r#"
        function log_something() -> null {
            log.info("Processing started", { step: 1 })
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(snapshot, std::sync::Arc::new(sys_native::SysOps::native()), None).unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine.call_function("log_something", vec![], call_ctx, true).await.unwrap();

    let events = collect_events(&guard);

    let log = events.iter().find_map(|e| match &e.event {
        EventKind::Log(l) => Some(l),
        _ => None,
    }).expect("Expected Log event");

    assert_eq!(log.level, "info");
    assert_eq!(log.message, "Processing started");
}
*/
