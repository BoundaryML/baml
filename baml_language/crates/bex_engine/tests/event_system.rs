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
            EventKind::Log(l) => format!("log:{}:{:?}", l.level, l.data),
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
            vec![],
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
async fn test_function_error_emits_function_end_with_error() {
    let source = r#"
        function main() -> string {
            baml.sys.panic("boom")
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            vec![],
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    let result = engine.call_function("main", vec![], call_ctx, true).await;
    assert!(result.is_err(), "expected function call to fail");

    let events = collect_events(&guard);
    let summary = summarize_events(&events);
    assert_eq!(summary, vec!["start:main", "end:main"]);

    let end = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::End(e)) => Some(e),
            _ => None,
        })
        .expect("Expected FunctionEnd event");

    let error = end.error.as_deref().expect("expected FunctionEnd error");
    assert!(error.contains("boom"), "unexpected error: {error}");
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
            vec![],
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
            vec![],
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
            vec![],
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
        _ => panic!("Expected Map, got {result:?}"),
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
            vec![],
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

// === Phase 2: VM Span Context ===

/// Verify that `vm.current_span_context` is populated by the engine during execution.
///
/// We cannot read `BexVm.current_span_context` directly from an integration test, but
/// we can observe its effects: events emitted during execution must carry span IDs that
/// are consistent with the root span ID provided at call time.  This is only possible
/// if the engine correctly writes the span context into the VM before each `vm.exec()`
/// step.
#[tokio::test]
async fn test_vm_span_context_is_set_during_execution() {
    let source = r#"
        function check_ctx() -> int {
            1
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            vec![],
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let expected_root = guard.root.clone();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine
        .call_function("check_ctx", vec![], call_ctx, true)
        .await
        .unwrap();

    let events = collect_events(&guard);
    assert!(
        !events.is_empty(),
        "Expected at least one event to be emitted"
    );

    // Every event must carry the root span ID that was injected via HostSpanContext.
    // The engine sets vm.current_span_context from the SpanState before vm.exec(),
    // so events emitted during that step reflect the correct span hierarchy.
    for event in &events {
        assert_eq!(
            event.ctx.root_span_id, expected_root,
            "Event root_span_id should match the tracking root — \
             this requires vm.current_span_context to be set correctly before vm.exec()"
        );
    }

    // The span IDs used in events must be non-null (real spans were created).
    let start_event = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Function(FunctionEvent::Start(s)) => Some(s),
            _ => None,
        })
        .expect("Expected a FunctionStart event");
    assert_eq!(start_event.name, "check_ctx");
}

// === Phase 3: SendEvent Instruction ===

/// Verify that `Instruction::SendEvent` yields a `CustomEvent` through the engine.
///
/// Strategy: compile a trivial `function main() -> null { null }`, then patch its
/// bytecode by injecting `LoadConst(name_idx)`, `LoadConst(data_idx)`, `SendEvent`
/// just before the final `Return` instruction. The engine should handle the yielded
/// `VmExecState::Event` and emit a `CustomEvent` into the event store.
#[tokio::test]
async fn test_send_event_bytecode_yields_custom_event() {
    use bex_vm_types::{ConstValue, Instruction, Object, indexable::ObjectIndex};

    // 1. Compile a trivial null-returning function.
    let mut program = compile_for_engine(
        r#"
        function main() -> null { null }
        "#,
    );

    // 2. Find the compiled "user.main" function's index in the object pool.
    let func_obj_idx = *program
        .function_indices
        .get("user.main")
        .expect("user.main should be compiled");

    // 3. Record the current end of the object pool; we will append a String there.
    let string_obj_idx = program.objects.0.len();
    program
        .objects
        .0
        .push(Object::String("phase3_event".to_string()));

    // 4. Patch the function bytecode: inject SendEvent before Return.
    let func = match &mut program.objects.0[func_obj_idx] {
        Object::Function(f) => f,
        other => panic!("expected Function at index {func_obj_idx}, got {other:?}"),
    };

    // Add the event-name string and a Null data value as constants.
    let name_const_idx = func.bytecode.constants.len();
    func.bytecode
        .constants
        .push(ConstValue::Object(ObjectIndex::from_raw(string_obj_idx)));
    let data_const_idx = func.bytecode.constants.len();
    func.bytecode.constants.push(ConstValue::Null);

    // Find the Return instruction and insert SendEvent + its operands before it.
    let return_pos = func
        .bytecode
        .instructions
        .iter()
        .rposition(|i| matches!(i, Instruction::Return))
        .expect("function must have a Return instruction");

    // Stack layout expected by SendEvent: name is pushed first, data on top.
    func.bytecode
        .instructions
        .insert(return_pos, Instruction::SendEvent);
    func.bytecode
        .instructions
        .insert(return_pos, Instruction::LoadConst(data_const_idx));
    func.bytecode
        .instructions
        .insert(return_pos, Instruction::LoadConst(name_const_idx));

    // meta and line_table are debug-only; leave them as-is (length mismatch is harmless).

    // 5. Create the engine with the patched program.
    let engine = std::sync::Arc::new(
        BexEngine::new(
            program,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            vec![],
        )
        .expect("engine creation must succeed"),
    );

    // 6. Call the function with span tracking.
    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine
        .call_function("main", vec![], call_ctx, true)
        .await
        .expect("call_function should succeed");

    // 7. Verify the CustomEvent was emitted.
    let events = collect_events(&guard);
    let custom = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Custom(c) => Some(c),
            _ => None,
        })
        .expect("Expected a CustomEvent from SendEvent instruction");

    assert_eq!(custom.name, "phase3_event");

    // The event's span context must be rooted at our tracking root.
    let custom_event = events
        .iter()
        .find(|e| matches!(&e.event, EventKind::Custom(_)))
        .unwrap();
    assert_eq!(custom_event.ctx.root_span_id, guard.root);
}

// === Phase 4: baml.events.send() API ===

#[tokio::test]
async fn test_custom_event_emission() {
    let source = r#"
        function emit_event() -> void {
            baml.events.send("user_clicked", { button: "submit", x: 100 })
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            vec![],
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine
        .call_function("emit_event", vec![], call_ctx, true)
        .await
        .unwrap();

    let events = collect_events(&guard);

    // Should have: start:emit_event, custom:user_clicked, end:emit_event
    let custom = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Custom(c) => Some(c),
            _ => None,
        })
        .expect("Expected Custom event");

    assert_eq!(custom.name, "user_clicked");
    // The event's span context must be rooted at our tracking root.
    let custom_event = events
        .iter()
        .find(|e| matches!(&e.event, EventKind::Custom(_)))
        .unwrap();
    assert_eq!(custom_event.ctx.root_span_id, guard.root);
}

// === Phase 5: log Package ===

/// Verify that `log.info()` emits a custom event with name="log" and the expected
/// level/data fields. The `log.*` functions are pure BAML wrappers around
/// `baml.events.send("log", { level, data })`.
#[tokio::test]
async fn test_log_info_event_emission() {
    let source = r#"
        function log_something() -> void {
            log.info({ step: 1, message: "Processing started" })
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            vec![],
        )
        .unwrap(),
    );

    let (host_ctx, guard) = setup_tracking();
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine
        .call_function("log_something", vec![], call_ctx, true)
        .await
        .unwrap();

    let events = collect_events(&guard);

    // log.info() emits a LogEvent
    let log_event = events
        .iter()
        .find_map(|e| match &e.event {
            EventKind::Log(log) => Some(log),
            _ => None,
        })
        .expect("Expected LogEvent");

    assert_eq!(log_event.level, "info");

    // Check data contains step and message
    match &log_event.data {
        bex_external_types::BexExternalValue::Map { entries, .. } => {
            assert_eq!(
                entries.get("step"),
                Some(&bex_external_types::BexExternalValue::Int(1))
            );
        }
        other => panic!("Expected Map data in LogEvent, got {other:?}"),
    }

    // Verify source location is captured
    assert!(
        log_event.source.is_some(),
        "Expected source location in LogEvent"
    );
}

/// Verify that all log levels (debug, warn, error) emit the correct "level" field.
#[tokio::test]
async fn test_log_all_levels() {
    let levels = [
        ("log.debug", "debug"),
        ("log.warn", "warn"),
        ("log.error", "error"),
    ];

    for (fn_call, expected_level) in levels {
        let source = format!(
            r#"
            function emit_log() -> void {{
                {fn_call}({{ msg: "level check" }})
            }}
            "#
        );

        let snapshot = compile_for_engine(&source);
        let engine = std::sync::Arc::new(
            BexEngine::new(
                snapshot,
                std::sync::Arc::new(sys_native::SysOps::native()),
                None,
                vec![],
            )
            .unwrap(),
        );

        let (host_ctx, guard) = setup_tracking();
        let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
            .with_host_ctx(host_ctx)
            .build();

        engine
            .call_function("emit_log", vec![], call_ctx, true)
            .await
            .unwrap();

        let events = collect_events(&guard);
        let log_event = events
            .iter()
            .find_map(|e| match &e.event {
                EventKind::Log(log) => Some(log),
                _ => None,
            })
            .unwrap_or_else(|| panic!("Expected LogEvent for level {expected_level}"));

        assert_eq!(
            log_event.level, expected_level,
            "Wrong level for {fn_call} call"
        );

        // Verify source location is captured
        assert!(
            log_event.source.is_some(),
            "Expected source location for {fn_call} call"
        );
    }
}

/// Verify that the Collector's `FunctionLog.log_events` field is populated from
/// `log.info()` calls via the `extract_log_from_custom` path.
#[tokio::test]
async fn test_collector_log_events_extraction() {
    let source = r#"
        function do_logging() -> void {
            log.info({ step: 1, message: "first message" })
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine = std::sync::Arc::new(
        BexEngine::new(
            snapshot,
            std::sync::Arc::new(sys_native::SysOps::native()),
            None,
            vec![],
        )
        .unwrap(),
    );

    let root = SpanId::new();
    bex_events::event_store::track(&root);

    let host_ctx = HostSpanContext {
        root_span_id: root.clone(),
        parent_span_id: root.clone(),
        call_stack: vec![root.clone()],
    };
    let call_ctx = FunctionCallContextBuilder::new(sys_types::CallId::next())
        .with_host_ctx(host_ctx)
        .build();

    engine
        .call_function("do_logging", vec![], call_ctx, true)
        .await
        .unwrap();

    // Build a FunctionLog from the raw events and check log_events.
    let events = bex_events::event_store::events_for_span(&root).unwrap_or_default();
    let log = bex_events::FunctionLog::from_events(root.clone(), &events);

    assert_eq!(log.log_events.len(), 1, "Expected one log event");
    assert_eq!(log.log_events[0].level, "info");
    // Check that data contains expected fields
    match &log.log_events[0].data {
        bex_external_types::BexExternalValue::Map { entries, .. } => {
            assert_eq!(
                entries.get("step"),
                Some(&bex_external_types::BexExternalValue::Int(1))
            );
        }
        other => panic!("Expected Map data, got {other:?}"),
    }

    bex_events::event_store::untrack(&root);
}
