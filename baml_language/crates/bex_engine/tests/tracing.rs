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
use bex_events::{
    DiskEventV1, EventFileHeaderV1, EventKind, EventSink, FunctionEvent, ids::RuntimeId,
};
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

#[derive(Default)]
struct CapturingSink {
    disk_events: Mutex<Vec<DiskEventV1>>,
    headers: Mutex<Vec<EventFileHeaderV1>>,
}

impl EventSink for CapturingSink {
    fn send(&self, _event: RuntimeEvent) {}

    fn send_disk_event(&self, _engine: bex_events::ids::EngineId, event: DiskEventV1) {
        self.disk_events.lock().unwrap().push(event);
    }

    fn send_event_file_header(&self, header: EventFileHeaderV1) {
        self.headers.lock().unwrap().push(header);
    }

    fn flush(&self) {}
}

/// Tier-A contract invariant: on every thread, every `call_id` has exactly
/// one `CallFunction` and exactly one `EndFunction`, and the `CallFunction`
/// precedes the `EndFunction`. This is the consumer's only structural
/// assumption when reconstructing call trees — call it in every disk-event
/// test.
#[track_caller]
fn assert_balanced(events: &[DiskEventV1]) {
    use std::collections::HashMap;
    // (thread_id, call_id) -> (call_count, end_count)
    let mut counts: HashMap<(u64, u64), (usize, usize)> = HashMap::new();
    for event in events {
        match event {
            DiskEventV1::CallFunction {
                thread_id, call_id, ..
            } => {
                let entry = counts.entry((thread_id.0, call_id.0)).or_default();
                assert_eq!(
                    entry.1, 0,
                    "CallFunction after EndFunction for thread {} call {}",
                    thread_id.0, call_id.0
                );
                entry.0 += 1;
            }
            DiskEventV1::EndFunction {
                thread_id, call_id, ..
            } => {
                let entry = counts.entry((thread_id.0, call_id.0)).or_default();
                assert!(
                    entry.0 >= 1,
                    "EndFunction before CallFunction for thread {} call {}",
                    thread_id.0,
                    call_id.0
                );
                entry.1 += 1;
            }
            _ => {}
        }
    }
    for ((thread, call), (calls, ends)) in &counts {
        assert_eq!(
            (*calls, *ends),
            (1, 1),
            "unbalanced lifecycle for thread {thread} call {call}: {calls} CallFunction, {ends} EndFunction"
        );
    }
}

/// Tier-A contract invariant: every thread has exactly one `StartThread` /
/// `EndThread` pair, the `StartThread` is that thread's first event, and the
/// `EndThread` is its last.
#[track_caller]
fn assert_threads_closed(events: &[DiskEventV1]) {
    use std::collections::HashMap;
    fn own_thread(event: &DiskEventV1) -> Option<u64> {
        match event {
            DiskEventV1::StartThread { thread_id, .. }
            | DiskEventV1::CallFunction { thread_id, .. }
            | DiskEventV1::SetId { thread_id, .. }
            | DiskEventV1::EndFunction { thread_id, .. }
            | DiskEventV1::EndThread { thread_id, .. } => Some(thread_id.0),
            DiskEventV1::Heartbeat { .. } => None,
        }
    }
    let mut per_thread: HashMap<u64, Vec<&DiskEventV1>> = HashMap::new();
    for event in events {
        if let Some(thread) = own_thread(event) {
            per_thread.entry(thread).or_default().push(event);
        }
    }
    for (thread, thread_events) in &per_thread {
        let starts = thread_events
            .iter()
            .filter(|e| matches!(e, DiskEventV1::StartThread { .. }))
            .count();
        let ends = thread_events
            .iter()
            .filter(|e| matches!(e, DiskEventV1::EndThread { .. }))
            .count();
        assert_eq!(
            starts, 1,
            "thread {thread}: expected 1 StartThread, got {starts}"
        );
        assert_eq!(ends, 1, "thread {thread}: expected 1 EndThread, got {ends}");
        assert!(
            matches!(thread_events.first(), Some(DiskEventV1::StartThread { .. })),
            "thread {thread}: StartThread was not its first event"
        );
        assert!(
            matches!(thread_events.last(), Some(DiskEventV1::EndThread { .. })),
            "thread {thread}: EndThread was not its last event"
        );
    }
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
    assert_balanced(&events);
    assert_threads_closed(&events);
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
    assert_balanced(&events);
    assert_threads_closed(&events);
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
    assert_balanced(&disk_events);
    assert_threads_closed(&disk_events);
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
    assert_balanced(&disk_events);
    assert_threads_closed(&disk_events);
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

// ── §2.2 contract: `$id` override persistence (T4-T7) ──────────────────────

/// T4: an override set via `$id = ...` must survive a nested bytecode call.
/// The override lives on the engine's span (not just the VM's transient
/// identity), so re-entering `main` after `helper()` returns must restore it.
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

    let disk_events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&disk_events);
    assert_threads_closed(&disk_events);
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

    let disk_events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&disk_events);
    assert_threads_closed(&disk_events);
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
    assert_eq!(call_ref.call_id, bex_engine::BexCallId(2));

    let disk_events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&disk_events);
    assert_threads_closed(&disk_events);
}

/// T7: exactly one `SetId` disk event per override, attributed to the
/// overridden call, ordered after that call's `CallFunction` and before its
/// `EndFunction` (consumers rely on "no `SetId` for a call => `$id` is the
/// `CallRef`").
#[tokio::test]
async fn set_id_emitted_once_per_override() {
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
    engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let disk_events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&disk_events);
    assert_threads_closed(&disk_events);

    let set_id_indices: Vec<usize> = disk_events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| matches!(e, DiskEventV1::SetId { .. }).then_some(i))
        .collect();
    assert_eq!(
        set_id_indices.len(),
        1,
        "expected exactly one SetId event, got {}",
        set_id_indices.len()
    );
    let set_idx = set_id_indices[0];
    let DiskEventV1::SetId {
        thread_id, call_id, ..
    } = &disk_events[set_idx]
    else {
        unreachable!();
    };
    assert_eq!(*thread_id, bex_engine::BexThreadId(1));
    assert_eq!(
        *call_id,
        bex_engine::BexCallId(1),
        "override belongs to the root call"
    );

    let call_idx = disk_events
        .iter()
        .position(|e| {
            matches!(e, DiskEventV1::CallFunction { thread_id, call_id, .. }
                if *thread_id == bex_engine::BexThreadId(1) && *call_id == bex_engine::BexCallId(1))
        })
        .expect("root CallFunction present");
    let end_idx = disk_events
        .iter()
        .position(|e| {
            matches!(e, DiskEventV1::EndFunction { thread_id, call_id, .. }
                if *thread_id == bex_engine::BexThreadId(1) && *call_id == bex_engine::BexCallId(1))
        })
        .expect("root EndFunction present");
    assert!(
        call_idx < set_idx && set_idx < end_idx,
        "SetId must sit between its call's CallFunction ({call_idx}) and EndFunction ({end_idx}), got {set_idx}"
    );
}

// ── §2.1 contract: balance across caught exceptions (T1-T2) ────────────────

/// T1: a throw caught one frame up must not desync the call-identity stack.
/// The unwound callee still gets exactly one `EndFunction` (with a non-Ok
/// status), and calls made after the catch get correct parent edges.
#[tokio::test]
async fn bex_disk_events_balance_across_caught_exception() {
    let source = r#"
        function boom() -> int {
            throw "boom"
        }

        function safe() -> int {
            boom() catch (e) {
                _ => 0
            }
        }

        function after() -> int {
            1
        }

        function main() -> int {
            let a = safe();
            after()
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
    assert_eq!(value, BexExternalValue::Int(1));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    // Exact shape: StartThread, Call(1 main), Call(2 safe), Call(3 boom),
    // End(3 unwound), End(2 ok), Call(4 after), End(4 ok), End(1 ok),
    // EndThread. No duplicates, nothing extra.
    assert_eq!(events.len(), 10, "unexpected event count: {events:#?}");

    let fqn_for = |function_id| {
        engine
            .program_metadata()
            .function_table
            .get(function_id)
            .map(|m| m.fqn.clone())
            .unwrap_or_default()
    };

    let calls: Vec<(u64, Option<u64>, String)> = events
        .iter()
        .filter_map(|e| match e {
            DiskEventV1::CallFunction {
                call_id,
                parent_call_id,
                function_id,
                ..
            } => Some((
                call_id.0,
                parent_call_id.map(|c| c.0),
                fqn_for(*function_id),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls,
        vec![
            (1, None, "user.main".to_string()),
            (2, Some(1), "user.safe".to_string()),
            (3, Some(2), "user.boom".to_string()),
            (4, Some(1), "user.after".to_string()),
        ],
        "call sequence / parent edges wrong: {calls:?}"
    );

    // boom (call 3) was unwound by the caught exception: non-Ok status.
    let boom_status = events
        .iter()
        .find_map(|e| match e {
            DiskEventV1::EndFunction {
                call_id, status, ..
            } if call_id.0 == 3 => Some(status.clone()),
            _ => None,
        })
        .expect("boom must get an EndFunction");
    assert_ne!(
        boom_status,
        bex_events::FunctionEndStatus::Ok,
        "an unwound call must not end Ok"
    );

    // The root EndFunction is the last function event and has call_id 1.
    let last_function_event = events
        .iter()
        .rev()
        .find(|e| {
            matches!(
                e,
                DiskEventV1::CallFunction { .. } | DiskEventV1::EndFunction { .. }
            )
        })
        .unwrap();
    assert!(
        matches!(last_function_event, DiskEventV1::EndFunction { call_id, .. } if call_id.0 == 1),
        "root EndFunction must close last: {last_function_event:?}"
    );
}

/// T1 variant: the catch sits two frames above the throw — multi-frame
/// truncation, where the old positional popping would shift by more than one.
#[tokio::test]
async fn bex_disk_events_balance_across_catch_two_frames_up() {
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

        function after() -> int {
            1
        }

        function main() -> int {
            let a = outer();
            after()
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
    assert_eq!(value, BexExternalValue::Int(1));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    // Both unwound frames (mid call 3, thrower call 4) end non-Ok.
    for unwound_call in [3u64, 4u64] {
        let status = events
            .iter()
            .find_map(|e| match e {
                DiskEventV1::EndFunction {
                    call_id, status, ..
                } if call_id.0 == unwound_call => Some(status.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("call {unwound_call} must get an EndFunction"));
        assert_ne!(status, bex_events::FunctionEndStatus::Ok);
    }

    // after() is parented to main (call 1), not to a stale unwound span.
    let after_parent = events
        .iter()
        .filter_map(|e| match e {
            DiskEventV1::CallFunction {
                call_id,
                parent_call_id,
                ..
            } => Some((call_id.0, parent_call_id.map(|c| c.0))),
            _ => None,
        })
        .next_back()
        .unwrap();
    assert_eq!(
        after_parent,
        (5, Some(1)),
        "after() got a stale parent edge"
    );
}

/// T1 variant: a traced (LLM-style) frame between thrower and catcher — the
/// legacy `SpanNotify` path must also stay balanced across the unwind. `mid` is
/// force-marked `trace: true` on the compiled program, standing in for an LLM
/// function without needing a client.
#[tokio::test]
async fn bex_disk_events_balance_across_catch_with_traced_frame() {
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

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();
    let value = engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(0));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    // The traced span (mid) was unwound: its disk EndFunction is non-Ok...
    let mid_call_id = events
        .iter()
        .find_map(|e| match e {
            DiskEventV1::CallFunction {
                call_id,
                function_id,
                ..
            } if engine
                .program_metadata()
                .function_table
                .get(*function_id)
                .is_some_and(|m| m.fqn == "user.mid") =>
            {
                Some(call_id.0)
            }
            _ => None,
        })
        .expect("mid must get a CallFunction");
    let mid_status = events
        .iter()
        .find_map(|e| match e {
            DiskEventV1::EndFunction {
                call_id, status, ..
            } if call_id.0 == mid_call_id => Some(status.clone()),
            _ => None,
        })
        .expect("traced unwound frame must get an EndFunction");
    assert_ne!(mid_status, bex_events::FunctionEndStatus::Ok);

    // ...and the legacy span stream is balanced too: start:mid has an end:mid.
    let legacy = collect_events(&guard);
    let names = event_names(&legacy);
    assert_eq!(
        names.iter().filter(|n| *n == "start:user.mid").count(),
        names.iter().filter(|n| *n == "end:user.mid").count(),
        "legacy span stream unbalanced for the traced frame: {names:?}"
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

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);
}

// ── §2.4 contract: CallFunction is never skipped (T10, T12) ────────────────

/// T10: `call_callable` (the HTTP-handler path) gets a balanced disk
/// lifecycle with the *real* callee identity — previously the root
/// `CallFunction` was silently skipped (the "<callable>" label resolved to no
/// function id), leaving an orphan `EndFunction`.
#[tokio::test]
async fn call_callable_emits_balanced_disk_lifecycle() {
    let source = r#"
        function the_callee(x: int) -> int {
            x + 1
        }

        function get_callee() -> (int) -> int {
            the_callee
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
    let handle = match engine
        .call_function("get_callee", vec![], call_ctx, false)
        .await
        .unwrap()
    {
        BexExternalValue::Handle(handle) => handle,
        other => panic!("expected a callable handle, got {other:?}"),
    };

    let callable_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    let value = engine
        .call_callable(handle, vec![BexExternalValue::Int(41)], callable_ctx, true)
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(42));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    // The call_callable invocation runs on its own thread (the second one).
    // Its stream is exactly StartThread, CallFunction, EndFunction, EndThread
    // — and the CallFunction resolves to the real callee, not the sentinel.
    let callable_thread = events
        .iter()
        .filter_map(|e| match e {
            DiskEventV1::StartThread { thread_id, .. } => Some(*thread_id),
            _ => None,
        })
        .nth(1)
        .expect("call_callable starts a second thread");
    let thread_events: Vec<&DiskEventV1> = events
        .iter()
        .filter(|e| match e {
            DiskEventV1::StartThread { thread_id, .. }
            | DiskEventV1::CallFunction { thread_id, .. }
            | DiskEventV1::SetId { thread_id, .. }
            | DiskEventV1::EndFunction { thread_id, .. }
            | DiskEventV1::EndThread { thread_id, .. } => *thread_id == callable_thread,
            DiskEventV1::Heartbeat { .. } => false,
        })
        .collect();

    assert_eq!(thread_events.len(), 4, "{thread_events:#?}");
    assert!(matches!(thread_events[0], DiskEventV1::StartThread { .. }));
    let DiskEventV1::CallFunction {
        call_id,
        parent_call_id,
        function_id,
        ..
    } = thread_events[1]
    else {
        panic!("missing root CallFunction for call_callable: {thread_events:#?}");
    };
    assert_eq!(*call_id, bex_engine::BexCallId(1));
    assert_eq!(*parent_call_id, None);
    let fqn = engine
        .program_metadata()
        .function_table
        .get(*function_id)
        .map(|m| m.fqn.clone());
    assert_eq!(
        fqn.as_deref(),
        Some("user.the_callee"),
        "call_callable must carry the real callee identity"
    );
    assert!(
        matches!(thread_events[2], DiskEventV1::EndFunction { call_id, status, .. }
            if *call_id == bex_engine::BexCallId(1) && *status == bex_events::FunctionEndStatus::Ok)
    );
    assert!(
        matches!(thread_events[3], DiskEventV1::EndThread { status, .. }
            if *status == bex_events::ThreadEndStatus::Completed)
    );
}

/// T12a: the reserved unknown-function sentinel row ships in every header, so
/// a consumer can always join a `CallFunction` whose callee could not be
/// resolved.
#[test]
fn unknown_function_sentinel_row_is_in_every_header() {
    let source = r#"
        function main() -> int {
            1
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

    let table = &engine.program_metadata().function_table;
    let sentinel = table
        .functions
        .iter()
        .find(|f| f.fqn == "baml.<unknown-function>")
        .expect("unknown-function sentinel row missing from metadata table");
    // The sentinel sits past the pool (one after the spawn-closure row), so it
    // can never collide with a real function's id.
    let spawn_row = table
        .functions
        .iter()
        .find(|f| f.fqn == "baml.<spawn-closure>")
        .expect("spawn-closure row missing");
    assert_eq!(sentinel.function_id.0, spawn_row.function_id.0 + 1);
    assert!(
        table
            .functions
            .iter()
            .filter(|f| f.function_id == sentinel.function_id)
            .count()
            == 1
    );
}

/// T12b: two functions with the same display name (methods on different
/// classes) must be attributed to their own `function_id`s — resolution is by
/// identity (heap pointer), never by a display-name scan that takes the first
/// match.
#[tokio::test]
async fn same_display_name_functions_are_not_misattributed() {
    let source = r#"
        class ClsA {
            x: int
            function run(self) -> int {
                1
            }
        }

        class ClsB {
            x: int
            function run(self) -> int {
                2
            }
        }

        function main() -> int {
            let a = ClsA { x: 0 };
            let b = ClsB { x: 0 };
            a.run() + b.run()
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
    assert_eq!(value, BexExternalValue::Int(3));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    let method_fqns: Vec<String> = events
        .iter()
        .filter_map(|e| match e {
            DiskEventV1::CallFunction { function_id, .. } => engine
                .program_metadata()
                .function_table
                .get(*function_id)
                .map(|m| m.fqn.clone()),
            _ => None,
        })
        .filter(|fqn| fqn.rsplit('.').next() == Some("run"))
        .collect();
    assert_eq!(
        method_fqns,
        vec!["user.ClsA.run".to_string(), "user.ClsB.run".to_string()],
        "same-display-name methods must resolve to their own ids"
    );
}

// ── §2.5 contract: timestamp semantics (T13-T15) ───────────────────────────

fn event_timestamp_ns(event: &DiskEventV1) -> u64 {
    match event {
        DiskEventV1::StartThread { timestamp_ns, .. }
        | DiskEventV1::CallFunction { timestamp_ns, .. }
        | DiskEventV1::SetId { timestamp_ns, .. }
        | DiskEventV1::EndFunction { timestamp_ns, .. }
        | DiskEventV1::EndThread { timestamp_ns, .. }
        | DiskEventV1::Heartbeat { timestamp_ns } => *timestamp_ns,
    }
}

/// T13: `timestamp_ns` is monotonic-since-process-start, never wall-clock
/// epoch nanos. 10^15 ns is ~11 days of process uptime — generous, while
/// absolute epoch values (~1.78e18) fail forever.
#[tokio::test]
async fn timestamps_are_relative_to_process_start() {
    let source = r#"
        function inner() -> int {
            1
        }

        function main() -> int {
            inner()
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
    engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let events = sink.disk_events.lock().unwrap().clone();
    assert!(!events.is_empty());
    let started_at = engine.event_file_header_v1().started_at_epoch_ns;
    for event in &events {
        let ts = event_timestamp_ns(event);
        assert!(
            ts < 1_000_000_000_000_000,
            "timestamp_ns looks like wall-clock epoch nanos: {ts} ({event:?})"
        );
        assert!(
            u128::from(ts) < started_at,
            "timestamp_ns must be far below the wall anchor"
        );
    }
}

/// T14: the rebase formula consumers use — `wall = started_at_epoch_ns +
/// timestamp_ns` — lands inside the test's own wall-clock window for the
/// first and last event.
#[tokio::test]
async fn timestamps_compose_with_wall_anchor() {
    let source = r#"
        function main() -> int {
            1
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

    // Derive the window with the SAME composition consumers use
    // (anchor + monotonic), not a raw SystemTime read: Instant does not
    // advance across system suspend while SystemTime does, so a raw-wall
    // window would flake on a laptop suspend or NTP step anywhere earlier
    // in this test binary. The composed window still catches the original
    // bug class (absolute-epoch timestamp_ns blows it by ~1.7e18) and pins
    // the formula itself.
    let started_at = engine.event_file_header_v1().started_at_epoch_ns;
    let before = started_at + u128::from(bex_events::now_ns());
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
    engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();
    let after = started_at + u128::from(bex_events::now_ns());

    let events = sink.disk_events.lock().unwrap().clone();
    for event in [events.first().unwrap(), events.last().unwrap()] {
        let wall = started_at + u128::from(event_timestamp_ns(event));
        assert!(
            wall >= before && wall <= after,
            "rebased wall time {wall} outside [{before}, {after}] for {event:?}"
        );
    }
}

/// T15: per-thread timestamps are non-decreasing in emission order (pins the
/// monotonic clock against a future regression to wall-clock or a per-thread
/// clock mixup).
#[tokio::test]
async fn timestamps_are_monotonic_per_thread() {
    let source = r#"
        function inner() -> int {
            1
        }

        function main() -> int {
            let f = spawn { inner() };
            let x = inner();
            (await f) + x
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
    engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .unwrap();

    let events = sink.disk_events.lock().unwrap().clone();
    let mut last_per_thread: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
    for event in &events {
        let thread = match event {
            DiskEventV1::StartThread { thread_id, .. }
            | DiskEventV1::CallFunction { thread_id, .. }
            | DiskEventV1::SetId { thread_id, .. }
            | DiskEventV1::EndFunction { thread_id, .. }
            | DiskEventV1::EndThread { thread_id, .. } => thread_id.0,
            DiskEventV1::Heartbeat { .. } => continue,
        };
        let ts = event_timestamp_ns(event);
        if let Some(last) = last_per_thread.get(&thread) {
            assert!(
                ts >= *last,
                "thread {thread}: timestamp went backwards ({last} -> {ts})"
            );
        }
        last_per_thread.insert(thread, ts);
    }
}

// ── §2.6 contract: termination statuses (T16-T19) ──────────────────────────

/// T16: a cancelled *root* call reads as `Cancelled` on both the function and
/// thread end — the same classification spawned children already get — not as
/// a generic `Error`.
#[tokio::test]
async fn root_cancellation_emits_cancelled_statuses() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(10000);
            1
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

    let cancel = bex_engine::CancellationToken::new();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_cancel_token(cancel.clone())
        .build();
    let engine_clone = Arc::clone(&engine);
    let handle = tokio::spawn(async move {
        engine_clone
            .call_function("main", vec![], call_ctx, true)
            .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    cancel.cancel();
    let result = handle.await.unwrap();
    assert!(result.is_err(), "cancelled call must not return Ok");

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    assert!(
        events.iter().any(|e| matches!(e,
            DiskEventV1::EndFunction { call_id, status, .. }
                if *call_id == bex_engine::BexCallId(1)
                    && *status == bex_events::FunctionEndStatus::Cancelled)),
        "root EndFunction must be Cancelled: {events:#?}"
    );
    assert!(
        events.iter().any(|e| matches!(e,
            DiskEventV1::EndThread { status, .. }
                if *status == bex_events::ThreadEndStatus::Cancelled)),
        "root EndThread must be Cancelled: {events:#?}"
    );
}

/// T17: cancelling only a spawned child marks the *child's* stream Cancelled
/// while the parent completes normally (pins the already-correct child path
/// so the root fix can't regress it).
#[tokio::test]
async fn spawned_child_cancellation_emits_cancelled() {
    let source = r#"
        function main() -> int {
            let tok = baml.spawn.CancelToken.new();
            let f = spawn with baml.spawn.options(cancel = tok) {
                baml.sys.sleep(10000);
                42
            };
            let _ = tok.cancel();
            (await f) catch (e) {
                baml.panics.Cancelled => 7
            }
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
    assert_eq!(value, BexExternalValue::Int(7));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    let child_thread = events
        .iter()
        .filter_map(|e| match e {
            DiskEventV1::StartThread { thread_id, .. } => Some(*thread_id),
            _ => None,
        })
        .nth(1)
        .expect("spawn starts a child thread");

    // Child: cancelled on both levels.
    assert!(
        events.iter().any(|e| matches!(e,
            DiskEventV1::EndFunction { thread_id, status, .. }
                if *thread_id == child_thread
                    && *status == bex_events::FunctionEndStatus::Cancelled)),
        "child EndFunction must be Cancelled: {events:#?}"
    );
    assert!(
        events.iter().any(|e| matches!(e,
            DiskEventV1::EndThread { thread_id, status, .. }
                if *thread_id == child_thread
                    && *status == bex_events::ThreadEndStatus::Cancelled)),
        "child EndThread must be Cancelled: {events:#?}"
    );

    // Parent: completed normally.
    let parent_thread = events
        .iter()
        .find_map(|e| match e {
            DiskEventV1::StartThread { thread_id, .. } => Some(*thread_id),
            _ => None,
        })
        .unwrap();
    assert!(
        events.iter().any(|e| matches!(e,
            DiskEventV1::EndThread { thread_id, status, .. }
                if *thread_id == parent_thread
                    && *status == bex_events::ThreadEndStatus::Completed)),
        "parent thread must complete normally: {events:#?}"
    );
}

/// T18a: an unhandled throw at the root drains *every* open span as Error
/// before EndThread(Error).
#[tokio::test]
async fn root_error_emits_error_statuses_for_all_open_spans() {
    let source = r#"
        function inner() -> int {
            throw "boom"
        }

        function outer() -> int {
            inner()
        }

        function main() -> int {
            outer()
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
    let result = engine.call_function("main", vec![], call_ctx, true).await;
    assert!(result.is_err());

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    // All three calls (main 1, outer 2, inner 3) end Error.
    for call in 1u64..=3 {
        let status = events
            .iter()
            .find_map(|e| match e {
                DiskEventV1::EndFunction {
                    call_id, status, ..
                } if call_id.0 == call => Some(status.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("call {call} must get an EndFunction"));
        assert_eq!(
            status,
            bex_events::FunctionEndStatus::Error,
            "call {call} must end Error"
        );
    }
    assert!(events.iter().any(|e| matches!(e,
        DiskEventV1::EndThread { status, .. } if *status == bex_events::ThreadEndStatus::Error)));
}

/// T18b: an unhandled throw in a spawned child marks the child's stream
/// Error; the parent (which catches at the await) completes normally.
#[tokio::test]
async fn spawned_child_error_emits_error_statuses() {
    let source = r#"
        function main() -> int {
            let f = spawn {
                throw "child boom"
            };
            (await f) catch (e) {
                _ => 9
            }
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
    assert_eq!(value, BexExternalValue::Int(9));

    let events = sink.disk_events.lock().unwrap().clone();
    assert_balanced(&events);
    assert_threads_closed(&events);

    let child_thread = events
        .iter()
        .filter_map(|e| match e {
            DiskEventV1::StartThread { thread_id, .. } => Some(*thread_id),
            _ => None,
        })
        .nth(1)
        .expect("spawn starts a child thread");
    assert!(events.iter().any(|e| matches!(e,
        DiskEventV1::EndFunction { thread_id, status, .. }
            if *thread_id == child_thread && *status == bex_events::FunctionEndStatus::Error)));
    assert!(events.iter().any(|e| matches!(e,
        DiskEventV1::EndThread { thread_id, status, .. }
            if *thread_id == child_thread && *status == bex_events::ThreadEndStatus::Error)));
}

/// T19: `baml.sys.exit` status mapping, pinned as a deliberate decision:
/// exit(0) is a clean termination (Ok / Completed); a non-zero exit code is
/// an Error on both levels.
#[tokio::test]
async fn sys_exit_status_mapping() {
    for (code, want_fn, want_thread) in [
        (
            0i64,
            bex_events::FunctionEndStatus::Ok,
            bex_events::ThreadEndStatus::Completed,
        ),
        (
            3i64,
            bex_events::FunctionEndStatus::Error,
            bex_events::ThreadEndStatus::Error,
        ),
    ] {
        let source = format!(
            r#"
            function main() -> int {{
                baml.sys.exit({code});
                1
            }}
            "#
        );

        let snapshot = compile_for_engine(&source);
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
        let result = engine.call_function("main", vec![], call_ctx, true).await;
        assert!(
            matches!(result, Err(bex_engine::EngineError::Exit { code: c }) if c == code),
            "exit({code}) must surface as EngineError::Exit"
        );

        let events = sink.disk_events.lock().unwrap().clone();
        assert_balanced(&events);
        assert_threads_closed(&events);
        assert!(
            events.iter().any(|e| matches!(e,
                DiskEventV1::EndFunction { call_id, status, .. }
                    if *call_id == bex_engine::BexCallId(1) && *status == want_fn)),
            "exit({code}): root EndFunction must be {want_fn:?}: {events:#?}"
        );
        assert!(
            events.iter().any(|e| matches!(e,
                DiskEventV1::EndThread { status, .. } if *status == want_thread)),
            "exit({code}): EndThread must be {want_thread:?}: {events:#?}"
        );
    }
}

// ── T20: early-yield equivalence ───────────────────────────────────────────

/// Strip timestamps so two streams can be compared structurally.
fn normalize_events(events: &[DiskEventV1]) -> Vec<DiskEventV1> {
    events
        .iter()
        .cloned()
        .map(|mut e| {
            match &mut e {
                DiskEventV1::StartThread { timestamp_ns, .. }
                | DiskEventV1::CallFunction { timestamp_ns, .. }
                | DiskEventV1::SetId { timestamp_ns, .. }
                | DiskEventV1::EndFunction { timestamp_ns, .. }
                | DiskEventV1::EndThread { timestamp_ns, .. }
                | DiskEventV1::Heartbeat { timestamp_ns } => *timestamp_ns = 0,
            }
            e
        })
        .collect()
}

/// T20: suspending and resuming the VM mid-run (GC park via `EarlyYield`) must
/// not duplicate, drop, or reorder disk events — the stream is identical to
/// an uninterrupted run, modulo timestamps.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn early_yield_resume_produces_identical_disk_stream() {
    const N: i64 = 20_000;
    let source = r#"
        function leaf(i: int) -> int {
            i
        }

        function spin(n: int) -> int {
            let i = 0;
            while (i < n) {
                let _ = [leaf(i), i + 1];
                i += 1;
            }
            i
        }
    "#;

    // Run 1: uninterrupted.
    let plain_events = {
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
            .call_function("spin", vec![BexExternalValue::Int(N)], call_ctx, true)
            .await
            .unwrap();
        assert_eq!(value, BexExternalValue::Int(N));
        sink.disk_events.lock().unwrap().clone()
    };

    // Run 2: same program on a fresh engine, with a GC park mid-flight.
    let parked_events = {
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
        let call_handle = {
            let engine = Arc::clone(&engine);
            tokio::spawn(async move {
                engine
                    .call_function(
                        "spin",
                        vec![BexExternalValue::Int(N)],
                        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                        true,
                    )
                    .await
            })
        };
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        engine
            .collect_garbage(::bex_heap::CollectionLevel::Minor)
            .await;
        let value = call_handle.await.unwrap().unwrap();
        assert_eq!(value, BexExternalValue::Int(N));
        sink.disk_events.lock().unwrap().clone()
    };

    assert_balanced(&plain_events);
    assert_balanced(&parked_events);
    assert_eq!(
        normalize_events(&plain_events),
        normalize_events(&parked_events),
        "suspension must not change the disk-event stream"
    );
}

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

/// T22 (documented-policy test): dropping the `call_function` future at an
/// await point truncates the event stream — `StartThread`/`CallFunction`
/// are emitted, no `End*` ever arrives. This is the *current, intentional*
/// contract: hosts that abandon a call must cancel via its token (or
/// `cancel_function_call`) and await completion if they need a closed trace.
/// If a drop-guard is added later, this test must flip to assert the
/// `Cancelled` end events instead.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_call_future_truncates_stream_by_policy() {
    let source = r#"
        function main() -> int {
            baml.sys.sleep(400);
            1
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
    {
        let engine = Arc::clone(&engine);
        let fut = engine.call_function("main", vec![], call_ctx, true);
        // Poll long enough for StartThread/CallFunction to be emitted, then
        // drop the future mid-sleep.
        let _ = tokio::time::timeout(std::time::Duration::from_millis(100), fut).await;
    }
    // Wait well past the program's own completion time: if dropping the
    // future did NOT truncate execution (e.g. the VM moved to a detached
    // task), the program would finish its 400ms sleep and emit End events
    // inside this window — which the assertion below would catch.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    let events = sink.disk_events.lock().unwrap().clone();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, DiskEventV1::CallFunction { .. })),
        "the call started: {events:#?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, DiskEventV1::EndThread { .. })),
        "documented policy: a dropped future truncates the stream (no EndThread). \
         If this fails because End events now appear, a drop-guard was added — \
         update this test to assert Cancelled statuses instead: {events:#?}"
    );
}
