use std::sync::{Arc, atomic::AtomicBool};

use baml_db::testing::compile_source;
use bex_vm::{BexVm, VmExecState};

fn run(source: &str) -> i64 {
    let program = compile_source(source);
    let entry = program.function_index("user.main").expect("main");
    let mut vm =
        BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("valid program");
    let entry = vm.heap.compile_time_ptr(entry);
    vm.set_entry_point(entry, &[]);
    match vm.exec().expect("execution succeeds") {
        VmExecState::Complete(value) => value.as_int().expect("integer result"),
        other => panic!("expected completion, got {other:?}"),
    }
}

#[test]
fn zero_argument_return_can_replace_its_only_stack_slot() {
    assert_eq!(run("function main() -> int { 42 }"), 42);
    assert_eq!(
        run("function leaf() -> int { 32 } function main() -> int { 10 + leaf() }"),
        42,
    );
}

#[test]
fn recursive_returns_preserve_caller_locals_and_pending_operands() {
    assert_eq!(
        run(r#"
            function sum(n: int) -> int {
                if (n == 0) { return 0; }
                let saved = n;
                return saved + sum(n - 1);
            }
            function main() -> int { 1000 + sum(100) }
        "#),
        6050,
    );
}

#[test]
fn returned_heap_values_survive_reuse_of_callee_slots() {
    assert_eq!(
        run(r#"
            class Cell { value: int }
            function make(n: int) -> Cell {
                let values = [n, n + 1];
                Cell { value: values[1] }
            }
            function main() -> int {
                let saved = make(40);
                let other = make(100);
                saved.value + other.value
            }
        "#),
        142,
    );
}

#[test]
fn checked_arithmetic_preserves_catch_and_caller_stack() {
    assert_eq!(
        run(r#"
            function add(a: int, b: int) -> int { 100 + (a + b) }
            function subtract(a: int, b: int) -> int { 100 + (a - b) }
            function main() -> int {
                let first = add(4611686018427387903, 1)
                    catch (e) { baml.panics.IntegerOverflow => 7 };
                let second = subtract(-4611686018427387904, 1)
                    catch (e) { baml.panics.IntegerOverflow => 11 };
                first + second + add(2, 3) + subtract(9, 4)
            }
        "#),
        228,
    );
}

#[test]
fn in_place_comparisons_preserve_operand_order_and_float_total_order() {
    assert_eq!(
        run(r#"
            function ints(a: int, b: int) -> int {
                let mask = 0;
                if (a == b) { mask += 1; }
                if (a != b) { mask += 2; }
                if (a < b) { mask += 4; }
                if (a <= b) { mask += 8; }
                if (a > b) { mask += 16; }
                if (a >= b) { mask += 32; }
                mask
            }
            function floats(a: float, b: float) -> int {
                let mask = 0;
                if (a == b) { mask += 1; }
                if (a != b) { mask += 2; }
                if (a < b) { mask += 4; }
                if (a <= b) { mask += 8; }
                if (a > b) { mask += 16; }
                if (a >= b) { mask += 32; }
                mask
            }
            function main() -> int {
                ints(-1, 1) + ints(1, -1) + ints(-1, -1)
                    + floats(0.0 / 0.0, 0.0 / 0.0) + floats(0.0, -0.0)
                    + floats(1.0, 2.0) + floats(2.0, 1.0)
            }
        "#),
        251,
    );
}
