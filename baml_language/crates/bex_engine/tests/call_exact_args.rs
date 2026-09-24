mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_vm_types::{Object, bytecode::OpCode};
use common::compile_for_engine;
use sys_native::SysOpsExt;

#[tokio::test]
async fn engine_retains_exact_calls_after_boxing_float_constants() {
    let snapshot = compile_for_engine(
        r#"
        function leaf(n: int) -> int { n }
        function floating(n: float) -> float { n }
        function main() -> int {
            if (floating(0.5) < 1.0) { leaf(20) + leaf(22) } else { 0 }
        }
    "#,
    );
    let main_index = snapshot.function_indices["user.main"];
    let engine = Arc::new(
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new()).unwrap(),
    );
    let pointer = engine.heap().compile_time_ptr(main_index);
    // SAFETY: the engine owns this sealed compile-time object for the whole
    // test. Compile-time functions do not move or change during collection.
    #[allow(unsafe_code)]
    let Object::Function(main) = (unsafe { pointer.get() }) else {
        unreachable!()
    };
    let code = &main.bytecode.compact.as_ref().unwrap().code;
    let mut pc = 0;
    let mut exact_calls = 0;
    while pc < code.len() {
        let op = OpCode::try_from(code[pc]).unwrap();
        exact_calls += usize::from(op == OpCode::CallExactArgs);
        pc += op.encoded_size();
    }
    assert_eq!(
        exact_calls, 3,
        "engine loading must not erase specialization"
    );
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result, BexExternalValue::Int(42));
}
