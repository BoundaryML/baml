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
use bex_events::{
    DiskEventV1, EventFileHeaderV1, EventKind, EventSink, FunctionEvent, ids::RuntimeId,
};
use common::compile_for_engine;
use std::sync::{Arc, Mutex};
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

#[derive(Default)]
struct CapturingSink {
    disk_events: Mutex<Vec<DiskEventV1>>,
    headers: Mutex<Vec<EventFileHeaderV1>>,
}

impl EventSink for CapturingSink {
    fn send(&self, _event: RuntimeEvent) {}

    fn send_disk_event(&self, event: DiskEventV1) {
        self.disk_events.lock().unwrap().push(event);
    }

    fn send_event_file_header(&self, header: EventFileHeaderV1) {
        self.headers.lock().unwrap().push(header);
    }

    fn flush(&self) {}
}

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
async fn bex_disk_events_cover_root_function_lifecycle() {
    let source = r#"
        function main() -> int {
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let event_sink: Arc<dyn EventSink> = sink.clone();
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(event_sink),
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(42));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_eq!(events.len(), 4);

    let (thread_id, call_id) = match &events[0] {
        DiskEventV1::StartThread {
            thread_id,
            parent_thread_id,
            parent_call_id,
            ..
        } => {
            assert_eq!(*parent_thread_id, None);
            assert_eq!(*parent_call_id, None);
            (*thread_id, bex_engine::BexCallId(1))
        }
        other => panic!("expected StartThread, got {other:?}"),
    };

    let function_id = match &events[1] {
        DiskEventV1::CallFunction {
            thread_id: event_thread,
            call_id: event_call,
            parent_call_id,
            function_id,
            ..
        } => {
            assert_eq!(*event_thread, thread_id);
            assert_eq!(*event_call, call_id);
            assert_eq!(*parent_call_id, None);
            *function_id
        }
        other => panic!("expected CallFunction, got {other:?}"),
    };

    match &events[2] {
        DiskEventV1::EndFunction {
            thread_id: event_thread,
            call_id: event_call,
            status,
            ..
        } => {
            assert_eq!(*event_thread, thread_id);
            assert_eq!(*event_call, call_id);
            assert_eq!(*status, bex_events::FunctionEndStatus::Ok);
        }
        other => panic!("expected EndFunction, got {other:?}"),
    }

    match &events[3] {
        DiskEventV1::EndThread {
            thread_id: event_thread,
            status,
            ..
        } => {
            assert_eq!(*event_thread, thread_id);
            assert_eq!(*status, bex_events::ThreadEndStatus::Completed);
        }
        other => panic!("expected EndThread, got {other:?}"),
    }

    let metadata = engine
        .program_metadata()
        .function_table
        .get(function_id)
        .expect("disk function_id should resolve");
    assert_eq!(metadata.display_name, "main");
}

#[tokio::test]
async fn bex_disk_events_link_nested_expression_call_to_parent_call() {
    let source = r#"
        function inner() -> int {
            10
        }

        function main() -> int {
            inner() + 1
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let event_sink: Arc<dyn EventSink> = sink.clone();
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(event_sink),
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(11));

    let events = sink.disk_events.lock().unwrap().clone();
    let calls: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            DiskEventV1::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                ..
            } => {
                let fqn = engine
                    .program_metadata()
                    .function_table
                    .get(*function_id)
                    .map(|metadata| metadata.fqn.as_str())
                    .unwrap_or("<missing>");
                Some((*thread_id, *call_id, *parent_call_id, fqn.to_string()))
            }
            _ => None,
        })
        .collect();

    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].1, bex_engine::BexCallId(1));
    assert_eq!(calls[0].2, None);
    assert_eq!(calls[0].3, "user.main");
    assert_eq!(calls[1].0, calls[0].0);
    assert_eq!(calls[1].1, bex_engine::BexCallId(2));
    assert_eq!(calls[1].2, Some(bex_engine::BexCallId(1)));
    assert_eq!(calls[1].3, "user.inner");

    assert!(events.iter().any(|event| matches!(
        event,
        DiskEventV1::EndFunction {
            call_id: bex_engine::BexCallId(2),
            status: bex_events::FunctionEndStatus::Ok,
            ..
        }
    )));
}

#[tokio::test]
async fn bex_disk_events_link_spawned_thread_to_parent_call() {
    let source = r#"
        function main() -> int {
            let f = spawn { 7 };
            await f
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let event_sink: Arc<dyn EventSink> = sink.clone();
    let engine = Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Some(event_sink),
            Vec::new(),
        )
        .unwrap(),
    );

    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(7));

    let events = sink.disk_events.lock().unwrap().clone();
    let start_threads: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            DiskEventV1::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                ..
            } => Some((*thread_id, *parent_thread_id, *parent_call_id)),
            _ => None,
        })
        .collect();

    assert_eq!(start_threads.len(), 2);
    let (root_thread, root_parent_thread, root_parent_call) = start_threads[0];
    assert_eq!(root_parent_thread, None);
    assert_eq!(root_parent_call, None);

    let (child_thread, child_parent_thread, child_parent_call) = start_threads[1];
    assert_ne!(child_thread, root_thread);
    assert_eq!(child_parent_thread, Some(root_thread));
    assert_eq!(child_parent_call, Some(bex_engine::BexCallId(1)));

    let child_call_function = events
        .iter()
        .find_map(|event| match event {
            DiskEventV1::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                ..
            } if *thread_id == child_thread => Some((*call_id, *parent_call_id, *function_id)),
            _ => None,
        })
        .expect("child thread should emit a root CallFunction");
    assert_eq!(child_call_function.0, bex_engine::BexCallId(1));
    assert_eq!(child_call_function.1, None);
    let child_metadata = engine
        .program_metadata()
        .function_table
        .get(child_call_function.2)
        .expect("child root function_id should resolve to metadata");
    assert_eq!(child_metadata.display_name, "<spawn-closure>");

    assert!(events.iter().any(|event| {
        matches!(
            event,
            DiskEventV1::EndFunction {
                thread_id,
                call_id: bex_engine::BexCallId(1),
                status: bex_events::FunctionEndStatus::Ok,
                ..
            } if *thread_id == child_thread
        )
    }));

    assert!(events.iter().any(|event| {
        matches!(
            event,
            DiskEventV1::EndThread {
                thread_id,
                status: bex_events::ThreadEndStatus::Completed,
                ..
            } if *thread_id == child_thread
        )
    }));
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

    let header = engine.event_file_header_v1();
    assert_eq!(header.process_euid, engine.process_euid());
    assert_eq!(header.engine_id, engine.engine_id());
    assert!(
        header
            .function_table
            .functions
            .iter()
            .any(|f| f.display_name == "main")
    );
    assert!(
        header
            .function_table
            .functions
            .iter()
            .any(|f| f.display_name == "inner")
    );

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

#[test]
fn engine_emits_event_file_header_to_sink_on_create() {
    let source = r#"
        function main() -> int {
            1
        }
    "#;

    let snapshot = compile_for_engine(source);
    let sink = Arc::new(CapturingSink::default());
    let event_sink: Arc<dyn EventSink> = sink.clone();
    let engine = BexEngine::new(
        snapshot,
        Arc::new(sys_native::SysOps::native()),
        Some(event_sink),
        Vec::new(),
    )
    .unwrap();

    let headers = sink.headers.lock().unwrap();
    assert_eq!(headers.len(), 1);
    assert_eq!(headers[0].process_euid, engine.process_euid());
    assert_eq!(headers[0].engine_id, engine.engine_id());
    assert_eq!(headers[0].program_id, engine.program_metadata().program_id);
    assert!(
        headers[0]
            .function_table
            .functions
            .iter()
            .any(|metadata| metadata.fqn == "user.main")
    );
}

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
async fn baml_id_current_new_and_set_emit_set_id_event() {
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
    let RuntimeId::OverrideUuid(override_id) = RuntimeId::decode(parts[3]).unwrap() else {
        panic!("expected override runtime ID");
    };

    let disk_events = sink.disk_events.lock().unwrap().clone();
    assert!(disk_events.iter().any(|event| matches!(
        event,
        DiskEventV1::SetId {
            thread_id: bex_engine::BexThreadId(1),
            call_id: bex_engine::BexCallId(1),
            id,
            ..
        } if *id == override_id
    )));
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
    let RuntimeId::OverrideUuid(override_id) = RuntimeId::decode(parts[2]).unwrap() else {
        panic!("expected override runtime ID");
    };

    let disk_events = sink.disk_events.lock().unwrap().clone();
    assert!(disk_events.iter().any(|event| matches!(
        event,
        DiskEventV1::SetId {
            thread_id: bex_engine::BexThreadId(1),
            call_id: bex_engine::BexCallId(1),
            id,
            ..
        } if *id == override_id
    )));
}
