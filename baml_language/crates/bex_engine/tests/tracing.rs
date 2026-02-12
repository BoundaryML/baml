//! End-to-end tests for span tracing via `call_function_traced`.
//!
//! These tests verify that `call_function_traced` produces a root span for the
//! entry-point function. Inner expression function calls use `Call` (not `CallWithTrace`)
//! so they do NOT produce child spans. Only LLM function calls emit `CallWithTrace`
//! and would appear as child spans in the trace.

mod common;

use std::collections::HashMap;

use bex_engine::{BexEngine, BexExternalValue, RuntimeEvent};
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

#[tokio::test]
async fn trace_single_function() {
    let source = r#"
        function main() -> int {
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine =
        BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native()).unwrap();

    let (result, events) = engine.call_function_traced("main", vec![], None).await;
    let value = result.unwrap();
    assert_eq!(value, BexExternalValue::Int(42));

    // Root function should produce start + end events
    let names = event_names(&events);
    assert_eq!(names, vec!["start:main", "end:main"]);

    // Both events should share the same root span ID
    let root_id = &events[0].ctx.root_span_id;
    assert_eq!(&events[1].ctx.root_span_id, root_id);

    // Start event should have no parent (it's the root)
    assert!(events[0].ctx.parent_span_id.is_none());

    // End event should also have no parent (same root span)
    assert!(events[1].ctx.parent_span_id.is_none());

    // Both should share the same span_id (same span)
    assert_eq!(events[0].ctx.span_id, events[1].ctx.span_id);
}

#[tokio::test]
async fn trace_nested_expression_calls_no_child_spans() {
    // Expression-to-expression calls use `Call` (not `CallWithTrace`),
    // so inner functions don't produce span events.
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
    let engine =
        BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native()).unwrap();

    let (result, events) = engine.call_function_traced("main", vec![], None).await;
    let value = result.unwrap();
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
    let engine =
        BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native()).unwrap();

    let (result, events) = engine.call_function_traced("main", vec![], None).await;
    let value = result.unwrap();
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
    let engine =
        BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native()).unwrap();

    let (result, events) = engine.call_function_traced("main", vec![], None).await;
    let value = result.unwrap();
    assert_eq!(value, BexExternalValue::Int(3));

    // Only root function produces events; foo() and bar() use Call, not CallWithTrace
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
    let engine =
        BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native()).unwrap();

    let (result, events) = engine
        .call_function_traced(
            "add",
            vec![BexExternalValue::Int(3), BexExternalValue::Int(4)],
            None,
        )
        .await;
    let value = result.unwrap();
    assert_eq!(value, BexExternalValue::Int(7));

    // Check that the root start event captured args
    if let EventKind::Function(FunctionEvent::Start(start)) = &events[0].event {
        assert_eq!(start.name, "add");
        assert_eq!(start.args.len(), 2);
        assert_eq!(start.args[0], BexExternalValue::Int(3));
        assert_eq!(start.args[1], BexExternalValue::Int(4));
    } else {
        panic!("Expected FunctionStart event");
    }
}

#[tokio::test]
async fn trace_captures_root_result() {
    let source = r#"
        function double(x: int) -> int {
            x * 2
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine =
        BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native()).unwrap();

    let (result, events) = engine
        .call_function_traced("double", vec![BexExternalValue::Int(5)], None)
        .await;
    let value = result.unwrap();
    assert_eq!(value, BexExternalValue::Int(10));

    // Check that the root end event captured the result
    if let EventKind::Function(FunctionEvent::End(end)) = &events[1].event {
        assert_eq!(end.name, "double");
        assert_eq!(end.result, BexExternalValue::Int(10));
    } else {
        panic!("Expected FunctionEnd event for 'double'");
    }
}

#[tokio::test]
async fn call_function_without_tracing_produces_no_events() {
    let source = r#"
        function main() -> int {
            42
        }
    "#;

    let snapshot = compile_for_engine(source);
    let engine =
        BexEngine::new(snapshot, HashMap::new(), sys_types::SysOps::native()).unwrap();

    // Regular call_function should work fine (SpanNotify is just ignored)
    let result = engine.call_function("main", vec![]).await.unwrap();
    assert_eq!(result, BexExternalValue::Int(42));
}

/// Verify that LLM function calls compile to `CallWithTrace` instructions.
#[test]
fn llm_functions_compile_to_call_with_trace() {
    use bex_vm_types::bytecode::Instruction;

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

    // Check that InnerPipeline has CallWithTrace for ExtractInfo and SummarizeInfo
    let inner_idx = program
        .function_indices
        .get("InnerPipeline")
        .expect("InnerPipeline should exist");
    let inner_func = match program.objects.get(*inner_idx) {
        Some(bex_vm_types::Object::Function(f)) => f,
        other => panic!("Expected Function object for InnerPipeline, got {other:?}"),
    };

    let call_with_trace_count = inner_func
        .bytecode
        .instructions
        .iter()
        .filter(|inst| matches!(inst, Instruction::CallWithTrace(_)))
        .count();

    let call_count = inner_func
        .bytecode
        .instructions
        .iter()
        .filter(|inst| matches!(inst, Instruction::Call(_)))
        .count();

    assert_eq!(
        call_with_trace_count, 2,
        "InnerPipeline should have 2 CallWithTrace (ExtractInfo + SummarizeInfo), found {call_with_trace_count} CallWithTrace and {call_count} Call"
    );

    // Check that OuterPipeline has Call (not CallWithTrace) for InnerPipeline
    let outer_idx = program
        .function_indices
        .get("OuterPipeline")
        .expect("OuterPipeline should exist");
    let outer_func = match program.objects.get(*outer_idx) {
        Some(bex_vm_types::Object::Function(f)) => f,
        other => panic!("Expected Function object for OuterPipeline, got {other:?}"),
    };

    let outer_call_with_trace = outer_func
        .bytecode
        .instructions
        .iter()
        .filter(|inst| matches!(inst, Instruction::CallWithTrace(_)))
        .count();

    assert_eq!(
        outer_call_with_trace, 0,
        "OuterPipeline should have 0 CallWithTrace (InnerPipeline is not an LLM function)"
    );
}
