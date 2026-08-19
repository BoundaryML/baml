//! Regression: a `Call`/`VirtualCall` terminator's destination must be a
//! `Place::Local`. The emitter materializes a terminator-call result with
//! `emit_store_place`, which only handles locals — a `Place::Field`/`Index`/
//! `Capture` destination hits `unreachable!`. MIR lowering must therefore route
//! the result through a temp and copy it into the projection after the call's
//! resume block.
//!
//! Both cases below assign a call result *into a field*, which panicked at emit
//! time before the destination was normalized:
//!   - `==`/`!=` lowered through the `baml.ops.equals_equals` driver, and
//!   - a virtual interface-method call on an existential receiver.

use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

/// Cap exec-loop iterations so regressions fail fast instead of hanging CI.
const MAX_EXEC_CALLS: usize = 256;

/// Compile `src`, run the no-arg `user.<fn_name>`, and return its `bool` result.
fn run_bool(src: &str, fn_name: &str) -> bool {
    let program = compile_source(src);
    let idx = program
        .function_index(fn_name)
        .unwrap_or_else(|| panic!("function {fn_name:?} not found"));
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("from_program");
    let fptr = vm.heap.compile_time_ptr(idx);
    vm.set_entry_point(fptr, &[]);
    for _ in 0..MAX_EXEC_CALLS {
        match vm.exec().expect("exec") {
            VmExecState::Complete(v) => return v.as_bool().expect("entry returns bool"),
            VmExecState::EarlyYield => {}
            other => panic!("unexpected VM state: {other:?}"),
        }
    }
    panic!("vm did not complete within {MAX_EXEC_CALLS} exec() calls");
}

// `==`/`!=` on two class instances lowers to the `baml.ops.equals_equals` driver
// (a block-splitting `Call`). Assigning the result to a field gives that call a
// `Place::Field` destination, which must be normalized through a temp. The `==`
// (non-negated) path is the regression: it previously wrote straight into the
// projection. The `!=` path always used a temp, so it is the control.
#[test]
fn equality_result_assigned_to_field() {
    const SRC: &str = r#"
        class Tag { id: int }
        implement baml.ops.Equals for Tag {
            function eq(self, other: Self) -> bool throws never { self.id == other.id }
        }
        class BoolBox { flag: bool }
        function tag(id: int) -> Tag { Tag { id: id } }

        function eq_into_field_true() -> bool {
            let b = BoolBox { flag: false };
            b.flag = (tag(1) == tag(1));
            b.flag
        }
        function eq_into_field_false() -> bool {
            let b = BoolBox { flag: true };
            b.flag = (tag(1) == tag(2));
            b.flag
        }
        function ne_into_field_true() -> bool {
            let b = BoolBox { flag: false };
            b.flag = (tag(1) != tag(2));
            b.flag
        }
    "#;
    assert!(run_bool(SRC, "user.eq_into_field_true"));
    assert!(!run_bool(SRC, "user.eq_into_field_false"));
    assert!(run_bool(SRC, "user.ne_into_field_true"));
}

// A virtual interface-method call on an existential receiver lowers to a
// `VirtualCall` terminator. Assigning its result to a field gives it a
// `Place::Field` destination — the same normalization requirement as a plain
// call. Two implementors keep the dispatch genuinely virtual.
#[test]
fn virtual_call_result_assigned_to_field() {
    const SRC: &str = r#"
        interface Flag { function flag(self) -> bool throws never }
        class Yes { implements Flag { function flag(self) -> bool { true } } }
        class No { implements Flag { function flag(self) -> bool { false } } }
        class BoolBox { flag: bool }

        function flag_into_field_true() -> bool {
            let f: Flag = Yes {  };
            let b = BoolBox { flag: false };
            b.flag = f.flag();
            b.flag
        }
        function flag_into_field_false() -> bool {
            let f: Flag = No {  };
            let b = BoolBox { flag: true };
            b.flag = f.flag();
            b.flag
        }
    "#;
    assert!(run_bool(SRC, "user.flag_into_field_true"));
    assert!(!run_bool(SRC, "user.flag_into_field_false"));
}

// The base of a projection is itself allowed to be a call expression. MIR must
// evaluate that call into a local before appending field projections.
#[test]
fn method_call_result_can_be_the_base_of_nested_field_assignment() {
    const SRC: &str = r#"
        class Info { title: string? }
        class Record { info: Info }
        class Store {
            record: Record
            calls: int
            function require(self) -> Record {
                self.calls += 1;
                self.record
            }
        }

        function main() -> bool {
            let store = Store {
                record: Record { info: Info { title: null } },
                calls: 0,
            };
            store.require().info.title = "triage";
            store.record.info.title == "triage" && store.calls == 1
        }
    "#;

    assert!(run_bool(SRC, "user.main"));
}
