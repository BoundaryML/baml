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
use bex_events::prof::backend::{ProfilerConfig, ProfilerSession};
use common::compile_for_engine;
use sys_native::SysOpsExt;

#[test]
fn vm_publication_never_initializes_transport() {
    let source = include_str!("../../bex_vm/src/vm.rs");
    assert!(!source.contains("ring_for_engine("));
    assert!(!source.contains("enable_log_collection("));
}

#[test]
fn completely_off_engine_never_starts_consumer() {
    const CHILD: &str = "BEX_TEST_OFF_TRANSPORT_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "completely_off_engine_never_starts_consumer"])
            .env(CHILD, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    assert!(!bex_events::prof::consumer_thread_started());
    let (session, _) = ProfilerSession::from_config(ProfilerConfig {
        enabled: false,
        ..Default::default()
    });
    let engine = BexEngine::new_with_profiler_session(
        compile_for_engine(r#"function main() -> void { log.info("ignored"); }"#),
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
        session,
    )
    .unwrap();
    let engine = Arc::new(engine);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(engine.call_function(
            "main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        ))
        .unwrap();
    drop(engine);
    assert!(!bex_events::prof::consumer_thread_started());
}

#[test]
fn concurrent_first_activation_waits_for_transport_warmup() {
    let (session, _) = ProfilerSession::from_config(ProfilerConfig {
        enabled: false,
        ..Default::default()
    });
    let engine = BexEngine::new_with_profiler_session(
        compile_for_engine(r#"function main() -> void { log.info("first"); }"#),
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
        session.clone(),
    )
    .unwrap();
    assert!(session.enable_log_collection());
    let start = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                start.wait();
                engine.activate_profiling();
                assert!(bex_events::prof::consumer_thread_started());
                assert!(!session.is_on());
            });
        }
    });
    let weak = Arc::downgrade(&session);
    drop(session);
    drop(engine);
    assert!(bex_events::prof::drain_logs(
        std::time::Duration::from_secs(5)
    ));
    assert!(weak.upgrade().is_none());
}

#[test]
fn memory_collection_prewarms_consumer_before_execution() {
    let (session, _) = ProfilerSession::from_config(ProfilerConfig {
        enabled: false,
        ..Default::default()
    });
    assert!(session.enable_log_collection());
    assert!(!session.is_on());
    let engine = BexEngine::new_with_profiler_session(
        compile_for_engine(r#"function main() -> void { log.info("first"); }"#),
        Arc::new(sys_native::SysOps::native()),
        Vec::new(),
        session,
    )
    .unwrap();
    assert!(bex_events::prof::consumer_thread_started());
    drop(engine);
}

/// Compile `source`, run its zero-argument `main`, and return the result plus
/// display-ready structured logs drained through the engine boundary.
async fn run_main_with_logs(
    source: &str,
) -> (Result<BexExternalValue, EngineError>, TraceLogDrainReport) {
    let snapshot = compile_for_engine(source);
    let (profiler_session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: false,
        ..ProfilerConfig::default()
    });
    let weak_session = Arc::downgrade(&profiler_session);
    assert!(diagnostic.is_none());
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            profiler_session,
        )
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
    drop(engine);
    let report = logs.drain_rendered_logs();
    assert!(logs.drain_rendered_logs().logs.is_empty());
    for _ in 0..100 {
        if weak_session.upgrade().is_none() {
            return (result, report);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("closed engine retained its log session");
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
