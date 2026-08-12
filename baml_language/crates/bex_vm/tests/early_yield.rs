//! Tests for the VM's `EarlyYield` emission.
//!
//! The VM decrements an internal counter on every control-flow instruction
//! and polls an atomic `park_requested` flag every ~32M decrements. When the
//! flag is set, the VM emits `VmExecState::EarlyYield` at the next poll.
//! These tests verify:
//!
//! 1. With the flag cleared, long-running programs complete without yielding.
//! 2. With the flag set, the VM yields within one poll window.
//! 3. Clearing the flag after a yield lets the VM resume to completion.
//! 4. A flag store from a different thread is observed by the VM.
//!
//! Tests are gated to native targets because the WASM path uses a different
//! (counter-only, unconditional) yielding rule.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

const LOOP_ITERATIONS: i64 = 10_000;
/// Small interval for testing — production uses 1 << 25 (~32M).
const TEST_YIELD_INTERVAL: u64 = 1 << 10;
const SPIN_FUNCTION: &str = "user.spin";
/// Cap exec-loop iterations so regressions fail fast instead of hanging CI.
const MAX_EXEC_CALLS: usize = 32;

/// Small BAML program: a tight loop that runs for `LOOP_ITERATIONS` iterations
/// and returns the final counter. Each iteration produces at least one
/// control-flow instruction (the jump back to the loop header), so the run
/// crosses several poll-window boundaries.
fn spin_source() -> String {
    format!(
        r#"
        function spin() -> int {{
            let i = 0;
            while (i < {LOOP_ITERATIONS}) {{
                i += 1;
            }}
            i
        }}
        "#
    )
}

/// Compile the `spin` program and build a VM whose entry point is set to `spin`.
/// Uses a small yield interval so tests complete quickly.
fn make_vm(park_requested: Arc<AtomicBool>) -> BexVm {
    let program = compile_source(&spin_source());
    let function_index = program
        .function_index(SPIN_FUNCTION)
        .unwrap_or_else(|| panic!("function {SPIN_FUNCTION} not found in compiled program"));
    let mut vm = BexVm::from_program(program, Arc::clone(&park_requested)).expect("from_program");
    // Override the default interval with a small one for fast tests.
    vm.early_yield =
        bex_vm_types::EarlyYieldCheck::with_interval(park_requested, TEST_YIELD_INTERVAL);
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);
    vm
}

/// With the flag cleared, `exec()` drives the full loop to completion in a
/// single call — no intermediate yield states are produced. This is the
/// baseline: a normal program should never spuriously yield.
#[test]
fn flag_unset_completes_without_early_yield() {
    let flag = Arc::new(AtomicBool::new(false));
    let mut vm = make_vm(Arc::clone(&flag));

    let state = vm.exec().expect("exec");
    match state {
        VmExecState::Complete(v) if v.is_int() => {
            assert_eq!(v.as_int().unwrap(), LOOP_ITERATIONS);
        }
        other => panic!("expected Complete(Int({LOOP_ITERATIONS})), got {other:?}"),
    }
}

/// With the flag set before the first `exec()`, the VM yields at the first
/// poll boundary. The test asserts we see `EarlyYield` within a small
/// number of `exec()` calls so a regression (e.g. the poll being skipped)
/// fails quickly.
#[test]
fn flag_set_before_run_emits_early_yield() {
    let flag = Arc::new(AtomicBool::new(true));
    let mut vm = make_vm(Arc::clone(&flag));

    let state = vm.exec().expect("exec");
    assert!(
        matches!(state, VmExecState::EarlyYield),
        "expected EarlyYield on first exec() with flag set, got {state:?}"
    );
}

/// After yielding, clearing the flag and calling `exec()` again must let the
/// VM resume and complete normally. This is the core liveness property:
/// parking to let the coordinator run (e.g. GC) does not break resumption.
#[test]
fn yield_then_resume_completes() {
    let flag = Arc::new(AtomicBool::new(true));
    let mut vm = make_vm(Arc::clone(&flag));

    // First exec should yield.
    let first = vm.exec().expect("first exec");
    assert!(
        matches!(first, VmExecState::EarlyYield),
        "expected EarlyYield on first exec, got {first:?}"
    );

    // Clear the flag — the coordinator has finished, let the VM run.
    flag.store(false, Ordering::Release);

    // Drive the VM forward. It may yield again on the next poll boundary if
    // the flag had briefly been `true` when that boundary was crossed
    // (unlikely here but harmless), so we accept any number of yields before
    // the final Complete, bounded by MAX_EXEC_CALLS.
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) if v.is_int() => {
                assert_eq!(v.as_int().unwrap(), LOOP_ITERATIONS);
                return;
            }
            VmExecState::EarlyYield => continue,
            other => panic!("unexpected state after resume: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls after clearing flag");
}

/// A store to the flag from a different thread must be observed by the VM.
/// Join the writer thread before calling `exec()` so the test is not timing
/// sensitive — the happens-before edge is established by the join, so the
/// VM is guaranteed to see the new flag value on its first poll.
#[test]
fn flag_written_by_another_thread_yields() {
    let flag = Arc::new(AtomicBool::new(false));
    let writer_flag = Arc::clone(&flag);

    let handle = std::thread::spawn(move || {
        writer_flag.store(true, Ordering::Release);
    });
    handle.join().expect("writer thread panicked");

    let mut vm = make_vm(Arc::clone(&flag));

    let state = vm.exec().expect("exec");
    assert!(
        matches!(state, VmExecState::EarlyYield),
        "expected EarlyYield after cross-thread flag store, got {state:?}"
    );
}
