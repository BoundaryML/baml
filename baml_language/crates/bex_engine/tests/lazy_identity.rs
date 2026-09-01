//! Lazy runtime-identity lifecycle coverage.
//!
//! This file intentionally contains one test so its integration-test process
//! begins with an untouched process-global identity cell.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, FunctionCallContextBuilder, ProcessEuid};
use bex_events::prof::backend::{ProfilerConfig, ProfilerSession};
use common::compile_for_engine;
use sys_native::SysOpsExt as _;

#[tokio::test]
async fn packed_engine_defers_identity_until_concurrent_first_calls() {
    assert_eq!(ProcessEuid::current_if_initialized(), None);

    let mut program = compile_for_engine(
        r#"
            function main() -> string {
                "ok"
            }
        "#,
    );
    // Packed artifacts omit this in-memory source identity, which exercises
    // the random per-engine ProgramId fallback used by generated SDKs.
    program.source_content_hash = None;

    let (profiler_session, diagnostic) = ProfilerSession::from_config(ProfilerConfig {
        enabled: false,
        ..ProfilerConfig::default()
    });
    assert!(diagnostic.is_none());
    let engine = Arc::new(
        BexEngine::new_with_profiler_session(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            profiler_session,
        )
        .expect("engine construction should succeed without resolving identity"),
    );

    assert_eq!(ProcessEuid::current_if_initialized(), None);
    assert!(
        engine.program_metadata_if_resolved().is_none(),
        "packed ProgramId must remain unresolved during engine construction"
    );

    let first = engine.call_function_with_trace(
        "main",
        Vec::new(),
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    );
    let second = engine.call_function_with_trace(
        "main",
        Vec::new(),
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    );
    let (first, second) = tokio::join!(first, second);
    let first = first.expect("first call should succeed");
    let second = second.expect("second call should succeed");

    let process_euid = ProcessEuid::current_if_initialized()
        .expect("the first identity-bearing call must resolve ProcessEuid");
    assert_eq!(first.entry_call_ref.process_euid, process_euid);
    assert_eq!(second.entry_call_ref.process_euid, process_euid);
    assert_eq!(
        first.entry_call_ref.engine_id,
        second.entry_call_ref.engine_id
    );
    assert!(
        engine.program_metadata_if_resolved().is_some(),
        "the first request-time event path must resolve ProgramId"
    );
}
