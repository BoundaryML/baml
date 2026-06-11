//! VM-level contract tests for `RuntimeCallNotification` balance.
//!
//! The engine reconstructs a per-thread call stack purely from the VM's call
//! notifications, so the VM must uphold: every `FunctionEnter` is balanced by
//! exactly one `FunctionExit` *or* covered by an `Unwound` notification when
//! an exception pops the frame before it can return. This pins the invariant
//! independently of the engine — if event emission later moves into the VM,
//! the balance guarantee survives the move.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, RuntimeCallNotification, VmExecState};

/// Cap exec-loop iterations so regressions fail fast instead of hanging CI.
const MAX_EXEC_CALLS: usize = 256;

fn make_vm(source: &str, entry: &str) -> BexVm {
    let program = compile_source(source);
    let function_index = program
        .function_index(entry)
        .unwrap_or_else(|| panic!("function {entry} not found in compiled program"));
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &[]);
    vm
}

/// Mirror of the engine's span bookkeeping: a stack of frame depths, pushed
/// on enter, popped on exit, truncated on unwind.
#[derive(Default)]
struct BalanceTracker {
    depths: Vec<usize>,
    enters: usize,
    exits: usize,
    unwound: usize,
}

impl BalanceTracker {
    fn observe(&mut self, notification: &RuntimeCallNotification) {
        match notification {
            RuntimeCallNotification::FunctionEnter { frame_depth, .. } => {
                self.enters += 1;
                self.depths.push(*frame_depth);
            }
            RuntimeCallNotification::FunctionExit { frame_depth } => {
                self.exits += 1;
                let top = self.depths.pop();
                assert_eq!(
                    top,
                    Some(*frame_depth),
                    "exit depth must match the matching enter"
                );
            }
            RuntimeCallNotification::Unwound { frames_remaining } => {
                while self.depths.last().is_some_and(|d| *d >= *frames_remaining) {
                    self.depths.pop();
                    self.unwound += 1;
                }
            }
        }
    }
}

/// A throw caught two frames up: the unwound frames' enters are covered by an
/// `Unwound` notification, and the tracker is empty at `Complete` — i.e.
/// enters == exits + unwound, with no stale depths left.
#[test]
fn caught_cross_frame_throw_keeps_notifications_balanced() {
    let source = r#"
        function boom() -> int {
            throw "boom"
        }

        function mid() -> int {
            boom()
        }

        function safe() -> int {
            mid() catch (e) {
                _ => 7
            }
        }

        function main() -> int {
            safe()
        }
    "#;

    let mut vm = make_vm(source, "user.main");
    let mut tracker = BalanceTracker::default();

    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => {
                assert_eq!(v.as_int(), Some(7));
                assert!(
                    tracker.depths.is_empty(),
                    "unbalanced call notifications at Complete: {:?}",
                    tracker.depths
                );
                assert_eq!(tracker.enters, 3, "safe, mid, boom");
                assert_eq!(tracker.exits, 1, "only safe returns normally");
                assert_eq!(tracker.unwound, 2, "mid and boom are unwound");
                assert_eq!(tracker.enters, tracker.exits + tracker.unwound);
                return;
            }
            VmExecState::RuntimeCallNotify(notification) => tracker.observe(&notification),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

/// Normal nested calls (no exceptions): plain enter/exit balance.
#[test]
fn nested_calls_balance_without_exceptions() {
    let source = r#"
        function inner() -> int {
            1
        }

        function middle() -> int {
            inner() + 1
        }

        function main() -> int {
            middle() + 1
        }
    "#;

    let mut vm = make_vm(source, "user.main");
    let mut tracker = BalanceTracker::default();

    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => {
                assert_eq!(v.as_int(), Some(3));
                assert!(tracker.depths.is_empty());
                assert_eq!(tracker.enters, 2);
                assert_eq!(tracker.exits, 2);
                assert_eq!(tracker.unwound, 0);
                return;
            }
            VmExecState::RuntimeCallNotify(notification) => tracker.observe(&notification),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

/// A throw caught directly by the caller unwinds exactly the callee's frame:
/// one enter, zero exits, one unwound — and nothing stale at `Complete`.
#[test]
fn single_frame_unwind_is_covered_by_unwound_notification() {
    let source = r#"
        function may_fail(x: int) -> int {
            match (x) {
                0 => throw "zero",
                _ => x
            }
        }

        function main() -> int {
            may_fail(0) catch (e) {
                _ => 42
            }
        }
    "#;

    let mut vm = make_vm(source, "user.main");
    let mut tracker = BalanceTracker::default();

    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => {
                assert_eq!(v.as_int(), Some(42));
                assert!(tracker.depths.is_empty());
                assert_eq!(tracker.enters, 1, "may_fail");
                assert_eq!(tracker.exits, 0, "may_fail never returns");
                assert_eq!(tracker.unwound, 1, "may_fail is unwound");
                return;
            }
            VmExecState::RuntimeCallNotify(notification) => tracker.observe(&notification),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

// ── §2.3 contract: watch filters may call functions (T8-T9) ────────────────
//
// The `$watch.options` BAML surface is currently blocked by a pre-existing
// member-resolution bug ("type `T` has no member `$watch`", E0007), so these
// tests install `WatchFilter::Function` directly on the VM's watch state —
// the exact state that surface produces — and drive execution through the
// notification path.

use bex_vm::watch::WatchFilter;

/// T8 (regression vs canary): a watch filter function whose body calls a
/// helper runs on the VM's internal `interrupt()` mini-runner. That runner
/// must consume per-call notifications instead of surfacing
/// `VmInternalError::ExpectedCompletion` and killing the program.
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
            VmExecState::Notify(_)
            | VmExecState::RuntimeCallNotify(_)
            | VmExecState::SpanNotify(_)
            | VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

/// T9: filter-internal calls are invisible to the notification stream — the
/// engine never sees enters/exits for them, so its span bookkeeping stays
/// balanced (filter-internal calls mint no identity, by design).
#[test]
fn watch_filter_calls_do_not_leak_call_notifications() {
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
    let mut tracker = BalanceTracker::default();
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => {
                assert_eq!(v.as_int(), Some(8));
                assert!(filter_installed);
                assert!(tracker.depths.is_empty());
                // Only `helper()` is visible: the filter body and its callee
                // (`is_big`, `threshold`) ran inside `interrupt()` and were
                // swallowed there.
                assert_eq!(
                    tracker.enters, 1,
                    "filter-internal calls must not leak call notifications"
                );
                assert_eq!(tracker.exits, 1);
                return;
            }
            VmExecState::RuntimeCallNotify(notification) => tracker.observe(&notification),
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
            VmExecState::Notify(_) | VmExecState::SpanNotify(_) | VmExecState::EarlyYield => {}
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
            VmExecState::RuntimeCallNotify(_) | VmExecState::EarlyYield => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }
    panic!("vm did not complete");
}

/// A watch-filter exception that escapes the interrupt boundary (caught by
/// the interrupted *program*) must fail loudly, not silently desync: once
/// the unwinder crosses the interrupt frame, the popped program frames WERE
/// announced to the engine, so the mini-runner may no longer swallow
/// notifications — the escaping `Unwound` surfaces as a hard error instead
/// of being discarded (or worse, the program's own completion being consumed
/// as the filter verdict).
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
            Ok(
                VmExecState::Notify(_)
                | VmExecState::RuntimeCallNotify(_)
                | VmExecState::SpanNotify(_)
                | VmExecState::EarlyYield,
            ) => {}
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
