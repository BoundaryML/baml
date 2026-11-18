//! The dispatch loop runs scalar instructions (local and constant loads,
//! stores, int arithmetic and comparisons, truthiness, branches) in a
//! register-resident fast path that stops before any case it does not handle:
//! a full operand stack, an int overflow, a heap value whose truthiness needs
//! inspection, or a branch whose early-yield tick would poll. The full
//! dispatch then runs that instruction. These tests cross each of those
//! hand-offs and check the results match what the language defines.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{BexVm, VmExecState};

fn run(source: &str) -> i64 {
    let program = compile_source(source);
    let entry = program.rendered_callables()["user.main"].object.raw();
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
fn int_arithmetic_and_comparisons_in_loops() {
    assert_eq!(
        run(r#"
            function main() -> int {
                let s = 0;
                for (let i = 0; i < 50; i += 1) {
                    for (let j = 0; j < 40; j += 1) {
                        if (i * j > 300 && i != j) { s += i - j; } else { s -= 1; }
                        if (i <= j || j >= 39) { s += 2; }
                        if (i == 7) { s += j * 3; }
                    }
                }
                s
            }
        "#),
        {
            let mut s: i64 = 0;
            for i in 0..50 {
                for j in 0..40 {
                    if i * j > 300 && i != j {
                        s += i - j;
                    } else {
                        s -= 1;
                    }
                    if i <= j || j >= 39 {
                        s += 2;
                    }
                    if i == 7 {
                        s += j * 3;
                    }
                }
            }
            s
        },
    );
}

#[test]
fn overflow_in_a_hot_loop_wraps() {
    // Each loop runs in the fast path until its values leave the inline range
    // or wrap, which the full dispatch must compute with wrapping arithmetic.
    assert_eq!(
        run(r#"
            function grow_mul(seed: int) -> int {
                let x = seed;
                for (let i = 0; i < 100; i += 1) { x = x * 3; }
                x
            }
            function grow_add(seed: int) -> int {
                let x = seed;
                for (let i = 0; i < 3000; i += 1) { x = x + 4611686018427387; }
                x
            }
            function shrink_sub(seed: int) -> int {
                let x = seed;
                for (let i = 0; i < 2000; i += 1) { x = x - 4611686018427387; }
                x
            }
            function main() -> int {
                grow_mul(1) + grow_add(0) + shrink_sub(0)
            }
        "#),
        3_i64
            .wrapping_pow(100)
            .wrapping_add(4_611_686_018_427_387_i64.wrapping_mul(3000))
            .wrapping_sub(4_611_686_018_427_387_i64.wrapping_mul(2000)),
    );
}

#[test]
fn deep_operand_stacks_grow_through_the_full_dispatch() {
    // An array literal's elements all sit on the operand stack until the
    // array is built, so this one holds 300 at once: the fast path meets a
    // full stack more than once and hands each growing push to the full
    // dispatch.
    let len = 300;
    let elements = (0..len)
        .map(|i| format!("x * {i} + 1"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"
        function f(x: int) -> int {{
            let xs = [{elements}];
            let s = 0;
            for (let i = 0; i < xs.length(); i += 1) {{ s += xs[i]; }}
            s
        }}
        function main() -> int {{ f(1) + 1000000 * f(2) }}
        "#
    );
    let f = |x: i64| (0..len).map(|i| x * i + 1).sum::<i64>();
    assert_eq!(run(&source), f(1) + 1_000_000 * f(2));
}

#[test]
fn truthiness_of_scalars_and_heap_values() {
    // Scalars take the fast path; strings, arrays and maps need the full
    // dispatch's object inspection.
    assert_eq!(
        run(r#"
            function score(flag: bool, n: int, s: string, xs: int[]) -> int {
                let r = 0;
                if (!flag) { r += 1; }
                if (!n) { r += 10; }
                if (!s) { r += 100; }
                if (!xs) { r += 1000; }
                r
            }
            function main() -> int {
                score(false, 0, "", []) + 10000 * score(true, 5, "x", [1])
            }
        "#),
        1111,
    );
}

#[test]
fn short_circuit_and_null_coalescing_branches() {
    assert_eq!(
        run(r#"
            function pick(a: int?, b: int?) -> int { a ?? b ?? -1 }
            function main() -> int {
                let s = 0;
                for (let i = 0; i < 30; i += 1) {
                    let a: int? = null;
                    if (i % 3 == 0) { a = i; }
                    let b: int? = null;
                    if (i % 5 == 0) { b = 100; }
                    s += pick(a, b);
                    if (i > 10 && i < 20) { s += 1000; }
                    if (i < 2 || i > 27) { s += 7; }
                }
                s
            }
        "#),
        {
            let mut s: i64 = 0;
            for i in 0..30 {
                let a = (i % 3 == 0).then_some(i);
                let b = (i % 5 == 0).then_some(100);
                s += a.or(b).unwrap_or(-1);
                if i > 10 && i < 20 {
                    s += 1000;
                }
                if !(2..=27).contains(&i) {
                    s += 7;
                }
            }
            s
        },
    );
}
