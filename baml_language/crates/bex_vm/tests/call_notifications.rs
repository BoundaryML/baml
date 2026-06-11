//! VM-level contract tests for the watch-filter `interrupt()` mini-runner
//! and the `$id` no-identity sentinel.
//!
//! (Pre-merge history: this file once pinned `SpanNotification` balance for
//! the traced span stream; that layer was removed with the legacy event
//! system (PR #3616) — per-call lifecycle balance is pinned at the artifact
//! level in `bex_engine/tests/prof_gate.rs`. What survives here is the
//! interrupt contract: filter-internal calls run to completion inside the
//! mini-runner, and a filter exception that escapes the interrupt boundary
//! fails loudly instead of the program's completion being consumed as the
//! filter verdict.)

#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, VmExecState, watch::WatchFilter};

/// Cap exec-loop iterations so regressions fail fast instead of hanging CI.
const MAX_EXEC_CALLS: usize = 256;

fn make_vm(source: &str, entry: &str) -> BexVm {
    let program = compile_source(source);
    let entry_index = program
        .function_index(entry)
        .unwrap_or_else(|| panic!("{entry} not found"));
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let entry_ptr = vm.heap.compile_time_ptr(entry_index);
    vm.set_entry_point(entry_ptr, &[]);
    vm
}

#[test]
fn watch_filter_calling_helper_function_works() {
    let source = r#"
        function threshold() -> int {
            5
        }

        function is_big(v: int) -> bool {
            threshold() < v
        }

        function main() -> int {
            watch let value = 0;
            value = 1;
            value = 6;
            value
        }
    "#;

    let program = compile_source(source);
    let is_big_index = program
        .function_index("user.is_big")
        .expect("user.is_big not found");
    let main_index = program
        .function_index("user.main")
        .expect("user.main not found");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let is_big_ptr = vm.heap.compile_time_ptr(is_big_index);
    let main_ptr = vm.heap.compile_time_ptr(main_index);
    vm.set_entry_point(main_ptr, &[]);

    let mut filter_installed = false;
    let mut notifies_after_install = 0usize;
    for _ in 0..MAX_EXEC_CALLS {
        match vm
            .exec()
            .expect("watch filter calling a helper must not abort")
        {
            VmExecState::Complete(v) => {
                assert_eq!(v.as_int(), Some(6));
                assert!(filter_installed, "the Function filter was never installed");
                assert_eq!(
                    notifies_after_install, 1,
                    "value = 6 passes the filter exactly once"
                );
                return;
            }
            VmExecState::Notify(bex_vm::vm::WatchNotification::Variables(nodes)) => {
                if filter_installed {
                    notifies_after_install += 1;
                    continue;
                }
                // First notification (`value = 1`, Default filter): the root
                // now exists — swap its filter for the user function, exactly
                // what `$watch.options(WatchOptions { when: is_big })` builds.
                for node in nodes {
                    let state = vm
                        .watch
                        .root_state_mut(node)
                        .expect("notified root must exist");
                    state.filter = WatchFilter::Function(is_big_ptr);
                    filter_installed = true;
                }
            }
            VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

/// T9 (adapted): filter-internal calls run wholly inside `interrupt()`'s
/// mini-runner — the program's own execution and result are unaffected by a
/// filter that itself calls helper functions. (The filter calls' ring
/// records are pinned at the artifact level; §7 decision 6.)
#[test]
fn watch_filter_calls_leave_program_result_intact() {
    let source = r#"
        function threshold() -> int {
            5
        }

        function is_big(v: int) -> bool {
            threshold() < v
        }

        function helper() -> int {
            2
        }

        function main() -> int {
            watch let value = 0;
            value = 1;
            value = 6;
            value + helper()
        }
    "#;

    let program = compile_source(source);
    let is_big_index = program
        .function_index("user.is_big")
        .expect("user.is_big not found");
    let main_index = program
        .function_index("user.main")
        .expect("user.main not found");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let is_big_ptr = vm.heap.compile_time_ptr(is_big_index);
    let main_ptr = vm.heap.compile_time_ptr(main_index);
    vm.set_entry_point(main_ptr, &[]);

    let mut filter_installed = false;
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => {
                assert_eq!(v.as_int(), Some(8));
                assert!(filter_installed);
                return;
            }
            VmExecState::Notify(bex_vm::vm::WatchNotification::Variables(nodes)) => {
                if filter_installed {
                    continue;
                }
                for node in nodes {
                    let state = vm
                        .watch
                        .root_state_mut(node)
                        .expect("notified root must exist");
                    state.filter = WatchFilter::Function(is_big_ptr);
                    filter_installed = true;
                }
            }
            VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

/// T25c: `$id` read with no engine-provided identity (direct VM runners,
/// `$init`) is the documented empty-string sentinel — it must not panic or
/// throw.
#[test]
fn id_without_engine_identity_is_empty_sentinel() {
    let source = r#"
        function main() -> string {
            $id
        }
    "#;

    let mut vm = make_vm(source, "user.main");
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => {
                let ptr = v.as_object_ptr().expect("string result");
                let bex_vm_types::Object::String(s) = vm.get_object(ptr) else {
                    panic!("expected string");
                };
                assert_eq!(s.as_str(), "", "no-identity $id is the empty sentinel");
                return;
            }
            VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete");
}

/// A watch-filter exception that escapes the interrupt boundary (caught by
/// the interrupted *program*) must fail loudly: the unwinder reports the
/// boundary crossing and the in-loop unwind sites turn it into a hard
/// `ExpectedCompletion` error — instead of the mini-runner continuing to
/// execute the outer program and consuming its completion as the filter
/// verdict.
#[test]
fn watch_filter_exception_escaping_interrupt_fails_loudly() {
    let source = r#"
        function bad_filter(v: int) -> bool {
            throw "filter boom"
        }

        function assign_watched() -> int {
            watch let value = 0;
            value = 1;
            value = 6;
            value
        }

        function main() -> int {
            assign_watched() catch (e) {
                _ => 42
            }
        }
    "#;

    let program = compile_source(source);
    let filter_index = program
        .function_index("user.bad_filter")
        .expect("user.bad_filter not found");
    let main_index = program
        .function_index("user.main")
        .expect("user.main not found");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let filter_ptr = vm.heap.compile_time_ptr(filter_index);
    let main_ptr = vm.heap.compile_time_ptr(main_index);
    vm.set_entry_point(main_ptr, &[]);

    let mut filter_installed = false;
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec() {
            Ok(VmExecState::Complete(v)) => {
                panic!(
                    "program must not complete normally: the filter exception escaped \
                     the interrupt boundary and was silently absorbed (got {v:?})"
                );
            }
            Ok(VmExecState::Notify(bex_vm::vm::WatchNotification::Variables(nodes))) => {
                if filter_installed {
                    continue;
                }
                for node in nodes {
                    let state = vm
                        .watch
                        .root_state_mut(node)
                        .expect("notified root must exist");
                    state.filter = WatchFilter::Function(filter_ptr);
                    filter_installed = true;
                }
            }
            Ok(VmExecState::EarlyYield) => {}
            Ok(other) => panic!("unexpected state: {other:?}"),
            Err(err) => {
                assert!(filter_installed, "error must come from the filter run");
                let message = format!("{err:?}");
                assert!(
                    message.contains("ExpectedCompletion"),
                    "expected the loud ExpectedCompletion failure, got: {message}"
                );
                return;
            }
        }
    }
    panic!("vm neither completed nor failed within {MAX_EXEC_CALLS} exec() calls");
}
