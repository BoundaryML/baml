//! VM-level regressions for Watch graph lifetime and GC roots.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, atomic::AtomicBool};

use baml_project::testing::compile_source;
use bex_vm::{BexVm, VmExecState};
use bex_vm_types::RootHaver;

const MAX_EXEC_CALLS: usize = 256;

fn run_to_completion(source: &str) -> (BexVm, usize) {
    let program = compile_source(source);
    let entry_index = program
        .function_index("user.main")
        .expect("user.main not found");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let entry_ptr = vm.heap.compile_time_ptr(entry_index);
    vm.set_entry_point(entry_ptr, &[]);

    let mut notification_count = 0;
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(_) => return (vm, notification_count),
            VmExecState::Notify(_) => notification_count += 1,
            VmExecState::EarlyYield => {}
            other => panic!("unexpected VM state: {other:?}"),
        }
    }
    panic!("VM did not complete within {MAX_EXEC_CALLS} exec() calls");
}

fn watch_gc_roots(vm: &BexVm) -> Vec<bex_vm_types::HeapPtr> {
    let mut roots = Vec::new();
    vm.watch.collect_roots(&mut roots);
    roots
}

fn assert_no_watch_roots(source: &str) {
    let (vm, notification_count) = run_to_completion(source);
    assert_eq!(notification_count, 0);
    assert!(
        watch_gc_roots(&vm).is_empty(),
        "BAML without `watch` must not retain objects through Watch"
    );
}

#[test]
fn ordinary_field_mutation_does_not_populate_watch() {
    assert_no_watch_roots(
        r#"
        class Box { value: string }

        function main() -> int {
            let box = Box { value: "before" }
            box.value = "after"
            0
        }
        "#,
    );
}

#[test]
fn ordinary_array_mutation_does_not_populate_watch() {
    assert_no_watch_roots(
        r#"
        function main() -> int {
            let array = ["before"]
            array[0] = "after"
            0
        }
        "#,
    );
}

#[test]
fn ordinary_map_mutation_does_not_populate_watch() {
    assert_no_watch_roots(
        r#"
        function main() -> int {
            let map = {"key": "before"}
            map["key"] = "after"
            0
        }
        "#,
    );
}

#[test]
fn function_scope_unwatch_releases_the_entire_object_graph() {
    let (vm, notification_count) = run_to_completion(
        r#"
        class Inner { value: string }
        class Outer { inner: Inner }

        function main() -> int {
            watch let outer = Outer { inner: Inner { value: "before" } }
            outer.inner.value = "after"
            0
        }
        "#,
    );

    assert_eq!(notification_count, 1);
    assert!(
        watch_gc_roots(&vm).is_empty(),
        "Unwatch at function exit must release all structural Watch roots"
    );
}

#[test]
fn watched_array_map_and_uint8array_assignments_notify_before_teardown() {
    for source in [
        r#"
        function main() -> int {
            watch let array = ["before"]
            array[0] = "after"
            0
        }
        "#,
        r#"
        function main() -> int {
            watch let map = {"key": "before"}
            map["key"] = "after"
            0
        }
        "#,
        r#"
        function main() -> int {
            watch let bytes = b"abc"
            bytes[0] = 122
            0
        }
        "#,
    ] {
        let (vm, notification_count) = run_to_completion(source);
        assert_eq!(notification_count, 1, "source did not notify:\n{source}");
        assert!(watch_gc_roots(&vm).is_empty());
    }
}

#[test]
fn native_panic_unwind_unregisters_callee_watch_roots() {
    let (vm, notification_count) = run_to_completion(
        r#"
        class Box { value: string }

        function watched_then_panics() -> int {
            watch let box = Box { value: "temporary" }
            baml.sys.panic("boom")
        }

        function main() -> int {
            watched_then_panics() catch (e) {
                baml.panics.UserPanic => 42
            }
        }
        "#,
    );

    assert_eq!(notification_count, 0);
    assert!(
        watch_gc_roots(&vm).is_empty(),
        "unwound callee left stale Watch roots"
    );
}
