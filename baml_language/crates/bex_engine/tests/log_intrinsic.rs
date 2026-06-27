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

use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

/// Compile `source`, run its zero-argument `main`, and return the result.
async fn run_main(source: &str) -> Result<BexExternalValue, EngineError> {
    let snapshot = compile_for_engine(source);
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("Failed to create engine"),
    );
    engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
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
    assert_eq!(run_main(source).await.unwrap(), BexExternalValue::Int(123));
}
