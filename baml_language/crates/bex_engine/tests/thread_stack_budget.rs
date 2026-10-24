//! Per-thread stack accounting through the real engine spawn/await path.
//! Rust derives depths from Frame's layout so simultaneous live frame bytes
//! provably exceed one thread's budget without hard-coding a platform's depth.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use baml_test_support::compile_source;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_vm::vm::{Frame, THREAD_STACK_BUDGET_BYTES};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
    function depth(n: int) -> int {
        if (n == 0) { 0 } else { 1 + depth(n - 1) }
    }
    function parent(n: int, child_depth: int) -> int {
        if (n == 0) {
            let child = spawn { depth(child_depth) };
            await child
        } else {
            1 + parent(n - 1, child_depth)
        }
    }
    function simultaneous(n: int) -> int { parent(n, n) }
    function caught(n: int) -> int {
        depth(n) catch (e) { baml.panics.StackOverflow => -1 }
    }
    function child_overflow(n: int) -> int {
        let failing = spawn { caught(n) };
        let sibling = spawn { depth(1000) };
        (await failing) + (await sibling)
    }
"#;

async fn call(entry: &str, depth: usize) -> BexExternalValue {
    let engine = Arc::new(
        BexEngine::new(
            compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    );
    engine
        .call_function(
            entry,
            vec![BexExternalValue::Int(i64::try_from(depth).unwrap())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("per-thread stack exhaustion must remain a catchable BAML panic")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn main_and_child_stacks_can_together_exceed_one_thread_limit() {
    // Each thread uses 55% of the limit in live frame records, leaving room
    // for its arguments/operands and geometric backing-array growth. The main
    // thread holds its entire recursive stack while awaiting the child.
    let depth = (THREAD_STACK_BUDGET_BYTES / size_of::<Frame>()) * 11 / 20;
    assert!(2 * depth * size_of::<Frame>() > THREAD_STACK_BUDGET_BYTES);
    assert_eq!(
        call("simultaneous", depth).await,
        BexExternalValue::Int(i64::try_from(2 * depth).unwrap())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn main_and_spawned_threads_each_enforce_their_own_limit() {
    // Frame records alone exceed a thread's limit at this depth, irrespective
    // of how many argument/local/operand slots the compiler emits.
    let depth = THREAD_STACK_BUDGET_BYTES / size_of::<Frame>() + 1;
    assert_eq!(call("caught", depth).await, BexExternalValue::Int(-1));
    assert_eq!(
        call("child_overflow", depth).await,
        BexExternalValue::Int(999)
    );
}
