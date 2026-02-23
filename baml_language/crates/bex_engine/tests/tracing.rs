//! End-to-end tests for span tracing via `call_function`.
//!
//! These tests verify that `call_function` produces a root span for the
//! entry-point function. Inner expression function calls are not traced
//! so they do NOT produce child spans. Only LLM functions have `trace: true`
//! set on their `Function` objects and would appear as child spans in the trace.
//!
//! Events are collected via the global event store (`track` / `events_for_span` / `untrack`).

mod common;

use bex_engine::{
    BexEngine, BexExternalValue, FunctionCallContextBuilder, HostSpanContext, RuntimeEvent, SpanId,
};
use bex_events::{EventKind, FunctionEvent};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Helper to extract function start/end event names from a list of events.
fn event_names(events: &[RuntimeEvent]) -> Vec<String> {
    events
        .iter()
        .map(|e| match &e.event {
            EventKind::Function(FunctionEvent::Start(start)) => {
                format!("start:{}", start.name)
            }
            EventKind::Function(FunctionEvent::End(end)) => {
                format!("end:{}", end.name)
            }
            _ => "<other>".to_string(),
        })
        .collect()
}

/// RAII guard that untracks a span from the event store on drop,
/// preventing span leaks if a test panics before calling `collect_events`.
struct TrackingGuard {
    root: SpanId,
}

impl Drop for TrackingGuard {
    fn drop(&mut self) {
        bex_events::event_store::untrack(&self.root);
    }
}

/// Create a `HostSpanContext` with a fresh root span and start tracking it
/// in the event store. Returns `(host_ctx, guard)` where the guard untracks
/// on drop.
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

#[tokio::test]
async fn trace_single_function() {
    let source = r#"
        function main() -> int {
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )
    .unwrap();

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx)
        .await
        .unwrap();
    let events = collect_events(&guard);

    assert_eq!(value, BexExternalValue::Int(42));

    // Root function should produce start + end events
    let names = event_names(&events);
    assert_eq!(names, vec!["start:main", "end:main"]);

    // Both events should share the same root span ID
    assert_eq!(&events[0].ctx.root_span_id, &guard.root);
    assert_eq!(&events[1].ctx.root_span_id, &guard.root);

    // Both should share the same span_id (same span)
    assert_eq!(events[0].ctx.span_id, events[1].ctx.span_id);
}

#[tokio::test]
async fn trace_nested_expression_calls_no_child_spans() {
    // Expression functions have `trace: false`, so inner functions
    // don't produce span events.
    let source = r#"
        function inner() -> int {
            10
        }

        function main() -> int {
            let x = inner();
            x + 1
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )
    .unwrap();

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx)
        .await
        .unwrap();
    let events = collect_events(&guard);

    assert_eq!(value, BexExternalValue::Int(11));

    // Only the root function (main) produces span events.
    // inner() uses Call instruction, so no child spans.
    let names = event_names(&events);
    assert_eq!(names, vec!["start:main", "end:main"]);
}

#[tokio::test]
async fn trace_deeply_nested_expression_calls_no_child_spans() {
    // Even deeply nested expression calls produce no child spans.
    let source = r#"
        function level3() -> int {
            1
        }

        function level2() -> int {
            level3() + 1
        }

        function level1() -> int {
            level2() + 1
        }

        function main() -> int {
            level1() + 1
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )
    .unwrap();

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx)
        .await
        .unwrap();
    let events = collect_events(&guard);

    assert_eq!(value, BexExternalValue::Int(4));

    // Only the root function produces span events
    let names = event_names(&events);
    assert_eq!(names, vec!["start:main", "end:main"]);
}

#[tokio::test]
async fn trace_sibling_expression_calls_no_child_spans() {
    let source = r#"
        function foo() -> int {
            1
        }

        function bar() -> int {
            2
        }

        function main() -> int {
            foo() + bar()
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )
    .unwrap();

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx)
        .await
        .unwrap();
    let events = collect_events(&guard);

    assert_eq!(value, BexExternalValue::Int(3));

    // Only root function produces events; foo() and bar() have trace: false
    let names = event_names(&events);
    assert_eq!(names, vec!["start:main", "end:main"]);
}

#[tokio::test]
async fn trace_captures_root_args() {
    let source = r#"
        function add(a: int, b: int) -> int {
            a + b
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )
    .unwrap();

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function(
            "add",
            vec![BexExternalValue::Int(3), BexExternalValue::Int(4)],
            call_ctx,
        )
        .await
        .unwrap();
    let events = collect_events(&guard);

    assert_eq!(value, BexExternalValue::Int(7));

    // Check that the root start event captured args
    let start = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::Start(s)) => Some(s),
            _ => None,
        })
        .expect("Expected FunctionStart event");
    assert_eq!(start.name, "add");
    assert_eq!(start.args.len(), 2);
    assert_eq!(start.args[0], BexExternalValue::Int(3));
    assert_eq!(start.args[1], BexExternalValue::Int(4));
}

#[tokio::test]
async fn trace_captures_root_result() {
    let source = r#"
        function double(x: int) -> int {
            x * 2
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        std::sync::Arc::new(sys_types::SysOps::native()),
        None,
    )
    .unwrap();

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("double", vec![BexExternalValue::Int(5)], call_ctx)
        .await
        .unwrap();
    let events = collect_events(&guard);

    assert_eq!(value, BexExternalValue::Int(10));

    // Check that the root end event captured the result
    let end = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::End(e)) => Some(e),
            _ => None,
        })
        .expect("Expected FunctionEnd event for 'double'");
    assert_eq!(end.name, "double");
    assert_eq!(end.result, BexExternalValue::Int(10));
}

/// Verify that LLM functions have `trace: true` and expression functions have `trace: false`.
#[test]
fn llm_functions_have_trace_flag() {
    let source = r##"
        client<llm> MockClient {
            provider openai
            options {
                model "mock-model"
                base_url "http://localhost:9999"
                api_key "test-key"
            }
        }

        function ExtractInfo(text: string) -> string {
            client MockClient
            prompt #"Extract: {{ text }}"#
        }

        function SummarizeInfo(text: string) -> string {
            client MockClient
            prompt #"Summarize: {{ text }}"#
        }

        function InnerPipeline(input: string) -> string {
            let a = ExtractInfo(input);
            let b = SummarizeInfo(input);
            a + " " + b
        }

        function OuterPipeline(input: string) -> string {
            let result = InnerPipeline(input);
            "Result: " + result
        }
    "##;

    let program = compile_for_engine(source);

    // LLM functions should have trace: true
    for name in ["ExtractInfo", "SummarizeInfo"] {
        let idx = program
            .function_indices
            .get(name)
            .unwrap_or_else(|| panic!("{name} should exist"));
        let func = match program.objects.get(*idx) {
            Some(bex_vm_types::Object::Function(f)) => f,
            other => panic!("Expected Function object for {name}, got {other:?}"),
        };
        assert!(func.trace, "LLM function {name} should have trace: true");
    }

    // Expression functions should have trace: false
    for name in ["InnerPipeline", "OuterPipeline"] {
        let idx = program
            .function_indices
            .get(name)
            .unwrap_or_else(|| panic!("{name} should exist"));
        let func = match program.objects.get(*idx) {
            Some(bex_vm_types::Object::Function(f)) => f,
            other => panic!("Expected Function object for {name}, got {other:?}"),
        };
        assert!(
            !func.trace,
            "Expression function {name} should have trace: false"
        );
    }
}
