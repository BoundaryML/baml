//! A sys-op's result is admitted to the op's declared return type, as an
//! argument is to its parameter. The result inhabits that type or the op is
//! at fault: an engine-internal error, never a value of the wrong type on the
//! stack and never a user-facing type error.
#![cfg(not(target_arch = "wasm32"))]
mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    function read_pid() -> int { baml.sys.pid() }
"#;

async fn call_read_pid(ops: sys_ops::SysOps) -> Result<BexExternalValue, bex_engine::EngineError> {
    let program = common::compile_for_engine(SOURCE);
    let engine = Arc::new(BexEngine::new(program, Arc::new(ops), Vec::new()).unwrap());
    // An engine call's future is large; boxed, it does not inflate every
    // test that awaits this helper.
    Box::pin(engine.call_function(
        "read_pid",
        vec![],
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    ))
    .await
}

#[tokio::test]
async fn sysop_result_lands_as_its_declared_type() {
    let result = call_read_pid(sys_ops::SysOps::native()).await.unwrap();
    assert!(matches!(result, BexExternalValue::Int(_)), "{result:?}");
}

#[tokio::test]
async fn sysop_result_outside_its_declared_return_type_is_the_ops_fault() {
    let mut ops = sys_ops::SysOps::native();
    ops.baml_sys_pid = Arc::new(|_heap, _permit, _args, _ctx, _call_id| {
        sys_types::SysOpResult::Ready(Ok(BexExternalValue::String("not a pid".into())))
    });
    let error = call_read_pid(ops)
        .await
        .expect_err("a string is no `int`, whoever produced it");
    let message = error.to_string();
    assert!(
        message.contains("baml.sys.pid")
            && message.contains("does not inhabit its declared return type `int`")
            && message.contains("string"),
        "{message}"
    );
}
