//! End-to-end tests for span tracing via `call_function`.
//!
//! These tests verify that `call_function` produces a root span for the
//! entry-point function. Inner expression function calls are not traced
//! so they do NOT produce child spans. Only LLM functions have `trace: true`
//! set on their `Function` objects and would appear as child spans in the trace.
//!
//! Events are collected via the global event store (`track` / `events_for_span` / `untrack`).

mod common;

use std::sync::{Arc, Mutex};

use bex_engine::{
    BexEngine, BexExternalValue, FunctionCallContextBuilder, HostSpanContext, RuntimeEvent, SpanId,
};
use bex_events::{EventKind, EventSink, FunctionEvent, ids::RuntimeId};
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

/// Captures the engine's live `RuntimeEvent` span stream (the `send` path).
/// Unlike the event store's tracked buckets — which only route a span or a
/// direct child of a tracked span — the sink sees every span event the
/// engine emits, including traced grandchild frames. Per-call disk lifecycle
/// no longer flows through the sink (see NOTE below).
#[derive(Default)]
struct CapturingSink {
    runtime_events: Mutex<Vec<RuntimeEvent>>,
}

impl EventSink for CapturingSink {
    fn send(&self, event: RuntimeEvent) {
        self.runtime_events.lock().unwrap().push(event);
    }

    fn flush(&self) {}
}

// NOTE: the per-call disk-event stream (StartThread/CallFunction/
// EndFunction/EndThread/SetId) no longer flows through the EventSink — it is
// written lock-free into the profiling ring and lands in .bamlprof files.
// Lifecycle/linkage coverage lives in bex_engine/tests/prof_gate.rs (G3
// balance, reconstruction, spawn edges, $id overrides) against the real
// artifact.

#[tokio::test]
async fn trace_single_function() {
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
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
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

    let start_identity = events[0]
        .identity
        .as_ref()
        .expect("root start event should carry BEX identity");
    let end_identity = events[1]
        .identity
        .as_ref()
        .expect("root end event should carry BEX identity");
    assert_eq!(start_identity, end_identity);
    assert_eq!(start_identity.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(start_identity.call_id, bex_engine::BexCallId(1));
    assert_eq!(start_identity.parent_call_id, None);
    assert_eq!(
        bex_engine::CallRef::decode(&start_identity.call_ref.encode()).unwrap(),
        start_identity.call_ref
    );
    assert_eq!(start_identity.call_ref.process_euid, engine.process_euid());
    assert_eq!(start_identity.call_ref.engine_id, engine.engine_id());

    let function_id = start_identity
        .function_id
        .expect("root function should resolve to function metadata");
    let metadata = engine
        .program_metadata()
        .function_table
        .get(function_id)
        .expect("function_id should resolve in metadata table");
    assert_eq!(metadata.display_name, "main");
    assert!(metadata.fqn.ends_with("main"));
}

#[tokio::test]
async fn baml_id_inside_spawn_uses_child_thread_root_call() {
    let source = r#"
        function main() -> string {
            let f = spawn { $id };
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected spawned $id string");
    };
    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected default call runtime ID");
    };
    assert_eq!(call_ref.process_euid, engine.process_euid());
    assert_eq!(call_ref.engine_id, engine.engine_id());
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(2));
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(1));
}

#[tokio::test]
async fn baml_id_inside_nested_expression_call_uses_nested_call_id() {
    let source = r#"
        function inner() -> string {
            $id
        }

        function main() -> string {
            inner()
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected nested $id string");
    };
    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected default call runtime ID");
    };
    assert_eq!(call_ref.process_euid, engine.process_euid());
    assert_eq!(call_ref.engine_id, engine.engine_id());
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(2));
}

#[tokio::test]
async fn bex_identity_exposes_event_header_metadata() {
    let source = r#"
        function inner() -> int {
            10
        }

        function main() -> int {
            inner() + 1
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let table = &engine.program_metadata().function_table;
    assert!(table.functions.iter().any(|f| f.display_name == "main"));
    assert!(table.functions.iter().any(|f| f.display_name == "inner"));

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(11));

    let events = collect_events(&guard);
    assert_eq!(event_names(&events), vec!["start:main", "end:main"]);

    let main_start = events[0].identity.as_ref().unwrap();
    let main_end = events[1].identity.as_ref().unwrap();
    assert_eq!(main_start.call_id, bex_engine::BexCallId(1));
    assert_eq!(main_start.parent_call_id, None);
    assert_eq!(main_end.call_id, main_start.call_id);
    assert_eq!(main_start.call_ref.engine_id, engine.engine_id());
}

// NOTE: engine_emits_event_file_header_to_sink_on_create was removed —
// headers now live in the .bamlprof artifact (written by the consumer), not
// the sink; bex_engine/tests/prof_gate.rs asserts header contents there.

#[test]
fn function_metadata_derives_owner_type_for_class_methods() {
    let source = r#"
        class Holder {
            value int

            function unwrap(self) -> int {
                self.value
            }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = BexEngine::new(
        snapshot,
        Arc::new(sys_native::SysOps::native()),
        None,
        Vec::new(),
    )
    .unwrap();

    let method = engine
        .program_metadata()
        .function_table
        .functions
        .iter()
        .find(|metadata| metadata.fqn == "user.Holder.unwrap")
        .expect("expected method metadata");
    assert_eq!(
        method.owner_type,
        Some(bex_events::DefinitionKey("class:user.Holder".to_string()))
    );
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
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
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
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
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
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
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
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function(
            "add",
            vec![BexExternalValue::Int(3), BexExternalValue::Int(4)],
            call_ctx,
            true,
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
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("double", vec![BexExternalValue::Int(5)], call_ctx, true)
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
    for name in ["user.ExtractInfo", "user.SummarizeInfo"] {
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
    for name in ["user.InnerPipeline", "user.OuterPipeline"] {
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

#[tokio::test]
async fn baml_id_current_new_and_set_roundtrip() {
    let source = r#"
        function main() -> string {
            let before = $id;
            let next = baml.id.new();
            let set_result = baml.id.set(next);
            let after = $id;
            before + "|" + next + "|" + set_result + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 4);

    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(parts[0]).unwrap() else {
        panic!("expected default call runtime ID");
    };
    assert_eq!(call_ref.process_euid, engine.process_euid());
    assert_eq!(call_ref.engine_id, engine.engine_id());
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(1));

    assert_eq!(parts[1], parts[2]);
    assert_eq!(parts[2], parts[3]);
    let RuntimeId::OverrideUuid(_override_id) = RuntimeId::decode(parts[3]).unwrap() else {
        panic!("expected override runtime ID");
    };

    // The SetFunctionId record assertion lives in prof_gate.rs
    // (set_function_id_recorded) against the .bamlprof stream.
}

#[tokio::test]
async fn baml_id_assignment_overrides_current_id() {
    let source = r#"
        function main() -> string {
            let before = $id;
            let next = baml.id.new();
            $id = next;
            let after = $id;
            before + "|" + next + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(parts[0]).unwrap() else {
        panic!("expected default call runtime ID");
    };
    assert_eq!(call_ref.process_euid, engine.process_euid());
    assert_eq!(call_ref.engine_id, engine.engine_id());
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(1));

    assert_eq!(parts[1], parts[2]);
    let RuntimeId::OverrideUuid(_override_id) = RuntimeId::decode(parts[2]).unwrap() else {
        panic!("expected override runtime ID");
    };

    // The SetFunctionId record assertion lives in prof_gate.rs
    // (set_function_id_recorded) against the .bamlprof stream.
}

// ── §2.2 contract: `$id` override persistence (T4-T7) ──────────────────────

/// T4: an override set via `$id = ...` must survive a nested bytecode call.
/// The override is VM-owned, keyed by the overridden call's `call_id`, so
/// re-entering `main` after `helper()` returns must restore it.
#[tokio::test]
async fn id_override_survives_nested_call() {
    let source = r#"
        function helper() -> int {
            1
        }

        function main() -> string {
            let next = baml.id.new();
            $id = next;
            let mid = $id;
            let x = helper();
            let after = $id;
            next + "|" + mid + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    assert_eq!(parts[0], parts[1], "mid should equal the override (next)");
    assert_eq!(
        parts[1], parts[2],
        "after a nested call, $id should still be the override"
    );
}

/// T5: same as T4 but inside a `spawn` body (each child thread has its own
/// span state; the override must persist there too).
#[tokio::test]
async fn id_override_survives_nested_call_in_spawn() {
    let source = r#"
        function helper() -> int {
            1
        }

        function main() -> string {
            let f = spawn {
                let next = baml.id.new();
                $id = next;
                let mid = $id;
                let x = helper();
                let after = $id;
                next + "|" + mid + "|" + after
            };
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    assert_eq!(parts[0], parts[1], "mid should equal the override (next)");
    assert_eq!(
        parts[1], parts[2],
        "after a nested call in spawn, $id should still be the override"
    );
}

/// T6: the inverse scoping rule — an override on the *caller* must NOT leak
/// into the callee. The callee's `$id` is its own default `CallRef`.
#[tokio::test]
async fn id_override_not_inherited_by_nested_call() {
    let source = r#"
        function helper() -> string {
            $id
        }

        function main() -> string {
            let next = baml.id.new();
            $id = next;
            let inner = helper();
            next + "|" + inner
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 2);
    assert_ne!(
        parts[0], parts[1],
        "helper's $id must not inherit the caller's override"
    );

    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(parts[1]).unwrap() else {
        panic!("helper's $id should be a default CallRef, got {}", parts[1]);
    };
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
    // Call ids count sys-op calls too (the ring records them as call pairs,
    // minted unconditionally in `prof_enter_sysop`): main = 1,
    // baml.id.new() = 2, the `$id =` set-op = 3, helper = 4.
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(4));
}

/// §2.2 (T4-T7 companion): overrides nest. A callee that sets its own
/// override must shadow — never destroy — the caller's: after the callee
/// returns, the caller's `$id` is its own override again (the VM keeps a
/// per-call override stack, popped with the callee's frame).
#[tokio::test]
async fn id_override_survives_callee_override() {
    let source = r#"
        function helper() -> string {
            let mine = baml.id.new();
            $id = mine;
            $id
        }

        function main() -> string {
            let outer = baml.id.new();
            $id = outer;
            let inner = helper();
            let after = $id;
            outer + "|" + inner + "|" + after
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let parts: Vec<&str> = result.as_str().split('|').collect();
    assert_eq!(parts.len(), 3);

    let RuntimeId::OverrideUuid(_) = RuntimeId::decode(parts[0]).unwrap() else {
        panic!("outer should be an override ID");
    };
    let RuntimeId::OverrideUuid(_) = RuntimeId::decode(parts[1]).unwrap() else {
        panic!("helper's $id should be the helper's own override");
    };
    assert_ne!(
        parts[0], parts[1],
        "helper's override is its own, not the caller's"
    );
    assert_eq!(
        parts[0], parts[2],
        "the caller's override must survive a callee that also overrides"
    );
}

/// T7 (adapted): the override is still in force when the overridden call
/// returns — `$id` read at the end of `main`, after a nested call, decodes
/// as the override UUID rather than the default `CallRef`. The per-record
/// emission-count/ordering semantics (exactly one `SetFunctionId`, between
/// the call's `CallFunction` and `EndFunction`) now live in the ring; see
/// `prof_gate.rs::set_function_id_recorded`.
#[tokio::test]
async fn id_override_read_at_return_is_override_uuid() {
    let source = r#"
        function helper() -> int {
            1
        }

        function main() -> string {
            let next = baml.id.new();
            $id = next;
            let x = helper();
            $id
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected string result");
    };
    let RuntimeId::OverrideUuid(_override_id) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected override runtime ID, got {id}");
    };

    // The SetFunctionId record assertion lives in prof_gate.rs
    // (set_function_id_recorded) against the .bamlprof stream.
}

// ── §2.1 contract: balance across caught exceptions (T1-T2) ────────────────

// NOTE: bex_disk_events_balance_across_caught_exception and
// bex_disk_events_balance_across_catch_two_frames_up were ported to
// bex_engine/tests/prof_gate.rs (caught-exception ring balance) against
// the .bamlprof artifact.

/// T1 variant: a traced (LLM-style) frame between thrower and catcher pins
/// the engine's `SpanNotification::Unwound` handling — a traced frame popped
/// by a caught exception must emit a `FunctionEnd` `RuntimeEvent` with error
/// "unwound by exception". `mid` is force-marked `trace: true` on the
/// compiled program, standing in for an LLM function without needing a
/// client. (Ring-level balance across this unwind is covered in
/// `prof_gate.rs`, caught-exception ring balance.)
#[tokio::test]
async fn traced_frame_unwound_by_caught_exception_closes_span() {
    let source = r#"
        function thrower() -> int {
            throw "deep"
        }

        function mid() -> int {
            thrower()
        }

        function outer() -> int {
            mid() catch (e) {
                _ => 0
            }
        }

        function main() -> int {
            outer()
        }
    "#;

    let mut snapshot = compile_for_engine(source);
    let mut marked = false;
    for obj in &mut snapshot.objects.0 {
        if let bex_vm_types::Object::Function(f) = obj
            && f.name == "user.mid"
        {
            f.trace = true;
            marked = true;
        }
    }
    assert!(marked, "user.mid not found in compiled program");

    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let (host_ctx, _guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(0));

    // The traced frame's span events ride the sink's live stream (the
    // tracked event-store bucket only routes direct children of the host
    // root, so a grandchild frame never lands there). The stream stays
    // balanced across the unwind: exactly one start:mid and one end:mid.
    let spans = sink.runtime_events.lock().unwrap().clone();
    let names = event_names(&spans);
    assert_eq!(
        names.iter().filter(|n| *n == "start:user.mid").count(),
        1,
        "traced frame must open exactly one span: {names:?}"
    );
    assert_eq!(
        names.iter().filter(|n| *n == "end:user.mid").count(),
        1,
        "traced frame must close exactly one span: {names:?}"
    );

    // ...and the unwound traced frame closes with the unwind error.
    let mid_end = spans
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::End(end)) if end.name == "user.mid" => Some(end),
            _ => None,
        })
        .expect("traced unwound frame must get a FunctionEnd RuntimeEvent");
    assert_eq!(
        mid_end.error.as_deref(),
        Some("unwound by exception"),
        "traced frame popped by a caught exception must close as unwound"
    );
}

/// T2: `$id` read after a caught exception reflects the *current* call (the
/// root), not a stale unwound span.
#[tokio::test]
async fn id_is_correct_after_caught_exception() {
    let source = r#"
        function boom() -> int {
            throw "boom"
        }

        function safe() -> int {
            boom() catch (e) {
                _ => 0
            }
        }

        function main() -> string {
            let a = safe();
            $id
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(sink.clone()),
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let BexExternalValue::String(id) = value else {
        panic!("expected string result");
    };
    let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(id.as_str()).unwrap() else {
        panic!("expected default CallRef, got {id}");
    };
    assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
    assert_eq!(
        call_ref.call_id,
        bex_engine::BexCallId(1),
        "$id after a catch must be the root call, not a stale unwound call"
    );
}

// NOTE: the §2.4 CallFunction-coverage tests were ported to
// bex_engine/tests/prof_gate.rs: call_callable_has_real_identity_and_balance
// (T10, port of the JSONL-era call_callable_emits_balanced_disk_lifecycle),
// sentinel_rows_present_in_header (T12a, header sentinel rows), and
// same_display_name_functions_are_not_misattributed (T12b) now assert
// against the .bamlprof artifact.

// NOTE: the §2.5 timestamp tests (T13-T15) timed the JSONL disk-event
// stream, which is gone. The live clock is bex_events::prof::clock
// (minstant); its semantics are covered by tests in the prof module.

// NOTE: the §2.6 termination-status tests (T16-T19: root/child
// cancellation, error drain, sys.exit mapping) were ported to
// bex_engine/tests/prof_gate.rs (thread-status coverage);
// root_error_emits_error_statuses_for_all_open_spans is covered by
// prof_gate.rs::unwind_emits_error_ends.

// NOTE: T20 (early_yield_resume_produces_identical_disk_stream) compared
// two JSONL streams structurally; that stream is gone. Porting the
// suspension-equivalence check to the .bamlprof stream is a follow-up.

// ── §3.3 contract: baml.id.set throws clause (T25) ─────────────────────────

/// T25a/b: `baml.id.set` rejects non-override inputs with a *catchable*
/// `InvalidArgument` — including a default `CallRef` (so a call cannot adopt
/// another call's identity) and garbage strings.
#[tokio::test]
async fn baml_id_set_invalid_inputs_throw_catchable_invalid_argument() {
    let source = r#"
        function set_current() -> string {
            baml.id.set(baml.id.current()) catch (e) {
                baml.errors.InvalidArgument => "caught"
            }
        }

        function set_garbage() -> string {
            baml.id.set("garbage") catch (e) {
                baml.errors.InvalidArgument => "caught"
            }
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .unwrap(),
    );

    for entry in ["set_current", "set_garbage"] {
        let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let value = engine
            .call_function(entry, vec![], call_ctx, true)
            .await
            .unwrap_or_else(|e| panic!("{entry}: InvalidArgument must be catchable: {e}"));
        assert_eq!(
            value,
            BexExternalValue::String("caught".into()),
            "{entry}: catch arm must run"
        );
    }
}

// ── Remaining contract net: T28, two-engine scoping, T22 ───────────────────

/// T28: `call_id` minting is sink-independent — `$id` returns exact
/// `(thread_id, call_id)` identities with **no** event sink configured.
/// This is the regression guard for any future "gate the per-call yield"
/// optimization: `$id` is a language feature and must survive tracing-off.
#[tokio::test]
async fn sinkless_engine_still_mints_correct_ids() {
    let source = r#"
        function deep() -> string {
            $id
        }

        function mid() -> string {
            deep() + "|" + $id
        }

        function main() -> string {
            let spawned = spawn { $id };
            mid() + "|" + $id + "|" + (await spawned)
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None, // no sink: identity must still be minted
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    let BexExternalValue::String(result) = value else {
        panic!("expected string result");
    };
    let ids: Vec<(u64, u64)> = result
        .as_str()
        .split('|')
        .map(|part| {
            let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(part).unwrap() else {
                panic!("expected default CallRef, got {part}");
            };
            (call_ref.thread_id.0, call_ref.call_id.0)
        })
        .collect();

    // deep (called by mid), mid, main root, spawned child root.
    // Call ids on the main thread: main=1, spawn-closure-thread is separate,
    // mid=2, deep=3 (spawn dispatch happens before mid()).
    assert_eq!(ids[2], (1, 1), "main is thread 1 call 1: {ids:?}");
    assert_eq!(ids[0].0, 1, "deep runs on the main thread: {ids:?}");
    assert_eq!(ids[1].0, 1, "mid runs on the main thread: {ids:?}");
    assert!(ids[0].1 > ids[1].1, "deep is called by mid: {ids:?}");
    assert_eq!(ids[3], (2, 1), "spawned body is thread 2 call 1: {ids:?}");
}

/// TICKET §11.2's two-engine row: two engines in one process get distinct
/// engine ids, and their thread-1/call-1 `CallRefs` encode to distinct strings
/// — the actual collision-avoidance mechanism behind header-only scoping.
#[tokio::test]
async fn two_engines_mint_distinct_call_refs() {
    let source = r#"
        function main() -> string {
            $id
        }
    "#;

    let mut encoded = Vec::new();
    let mut engine_ids = Vec::new();
    for _ in 0..2 {
        let snapshot = compile_for_engine(source);
        let engine = Arc::new(
            BexEngine::new(
                snapshot,
                Arc::new(sys_native::SysOps::native()),
                None,
                Vec::new(),
            )
            .unwrap(),
        );
        engine_ids.push(engine.engine_id());
        let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let BexExternalValue::String(id) = engine
            .call_function("main", vec![], call_ctx, true)
            .await
            .unwrap()
        else {
            panic!("expected string");
        };
        encoded.push(id.to_string());
    }

    assert_ne!(engine_ids[0], engine_ids[1]);
    assert_ne!(
        encoded[0], encoded[1],
        "thread 1 / call 1 in two engines must encode to distinct CallRefs"
    );
    for (id, engine_id) in encoded.iter().zip(&engine_ids) {
        let RuntimeId::DefaultCall(call_ref) = RuntimeId::decode(id).unwrap() else {
            panic!("expected default CallRef");
        };
        assert_eq!(call_ref.engine_id, *engine_id);
        assert_eq!(call_ref.thread_id, bex_engine::BexThreadId(1));
        assert_eq!(call_ref.call_id, bex_engine::BexCallId(1));
    }
}

// NOTE: T22 (dropped_call_future_truncates_stream_by_policy) pinned the
// JSONL truncation policy for dropped call futures; that stream is gone.
// Porting the drop-policy check to the .bamlprof stream is a follow-up.
