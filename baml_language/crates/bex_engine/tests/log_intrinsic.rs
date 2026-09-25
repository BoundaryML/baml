//! Runtime coverage for the structured-log intrinsic (`log.info` / `debug` /
//! `warn` / `error`, lowered to a `$baml_log` event whose payload is a
//! `map<string, unknown>` of `{ level, data }`).
//!
//! Regression guard: the emitter must push the payload map's key/value type
//! tags before its `AllocMap`, otherwise the VM reads the entry keys as the
//! type operands and the log statement faults at runtime. A scalar `data` is
//! enough to build (and previously mis-read) that wrapper map.

mod common;

use std::sync::Arc;

use bex_engine::{
    BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder,
    logger::{TraceLogDrainReport, TraceLogger},
};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Compile `source`, run its zero-argument `main`, and return the result plus
/// display-ready structured logs drained through the engine boundary.
async fn run_main_with_logs(
    source: &str,
) -> (Result<BexExternalValue, EngineError>, TraceLogDrainReport) {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    let logs = TraceLogger::bounded(16);
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_logger(logs.clone())
                .build(),
            true,
        )
        .await;
    (result, logs.drain_rendered_logs())
}

/// Each `log.*` call builds the `{ level, data }` event map at runtime. The
/// statements must execute without faulting (reaching the `123` return proves
/// the wrapper `AllocMap` read its type tags rather than the entry keys),
/// across scalar, structured-map, and list payloads and every level.
#[tokio::test]
async fn structured_log_executes_without_faulting() {
    let source = r#"
        function main() -> int {
            log.info("hello");
            log.debug(42);
            log.warn([1, 2, 3]);
            log.error({"user": "ada", "role": "admin"});
            123
        }
    "#;
    let (result, report) = run_main_with_logs(source).await;
    assert_eq!(result.unwrap(), BexExternalValue::Int(123));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.logs.len(), 4);
    assert_eq!(report.logs[0].metadata.level.as_deref(), Some("info"));
    assert_eq!(report.logs[0].body, "hello");
    assert_eq!(report.logs[1].metadata.level.as_deref(), Some("debug"));
    assert_eq!(report.logs[1].body, "42");
    assert_eq!(report.logs[2].metadata.level.as_deref(), Some("warn"));
    assert_eq!(report.logs[2].body, "[1, 2, 3]");
    assert_eq!(report.logs[3].metadata.level.as_deref(), Some("error"));
    assert_eq!(report.logs[3].body, r#"{"user": "ada", "role": "admin"}"#);
}

#[tokio::test]
async fn structured_logs_survive_nested_calls_and_exception_unwind() {
    let (result, report) = run_main_with_logs(
        r#"
        function child() -> int throws string {
            log.info("child");
            throw "failed";
        }
        function main() -> int {
            log.info("before");
            let value = child() catch (e) { _ => 7 };
            log.info("after");
            value
        }
    "#,
    )
    .await;
    assert_eq!(result.unwrap(), BexExternalValue::Int(7));
    assert!(report.failures.is_empty());
    assert_eq!(
        report
            .logs
            .iter()
            .map(|log| log.body.as_str())
            .collect::<Vec<_>>(),
        ["before", "child", "after"]
    );
    assert!(report.logs.iter().all(|log| log.metadata.source.is_some()));
}

#[tokio::test]
async fn log_records_keep_run_and_thread_identity_across_spawn() {
    let snapshot = compile_for_engine(
        r#"
        function main() -> int {
            log.info("root");
            let task = spawn { log.info("child"); 7 };
            let result = await task;
            log.info("root again");
            result
        }
    "#,
    );
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let logs = TraceLogger::bounded(16);
    let value = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_logger(logs.clone())
                .build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(value, BexExternalValue::Int(7));
    let report = logs.drain_encoded_logs();
    assert!(report.failures.is_empty());
    assert_eq!(report.logs.len(), 3);
    let root = &report.logs[0];
    let child = &report.logs[1];
    let resumed = &report.logs[2];
    assert_eq!(root.boundary_id, child.boundary_id);
    assert_eq!(root.boundary_id, resumed.boundary_id);
    assert_eq!(root.call, resumed.call);
    assert_eq!(root.call.process_euid, child.call.process_euid);
    assert_eq!(root.call.engine_id, child.call.engine_id);
    assert_ne!(root.call.thread_id, child.call.thread_id);
}
