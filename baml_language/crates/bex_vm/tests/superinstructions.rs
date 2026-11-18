//! Local-operand superinstructions: the emitter fuses `i += k`
//! (`AddLocalSmall`), a field load from a local (`LoadVarField`), and an int
//! comparison of locals or of a local and a constant followed by a branch
//! (`JumpIfLocalsCmp` / `JumpIfLocalConstCmp`). Each must behave exactly like
//! the sequence it replaces, including the error it raises.

use std::sync::{Arc, atomic::AtomicBool};

use baml_test_support::compile_source;
use bex_vm::{BexVm, BytecodeProgram, VmExecState, convert_program};
use bex_vm_types::{Object, bytecode::OpCode, types::Function};

fn function<'a>(program: &'a BytecodeProgram, name: &str) -> &'a Function {
    program
        .objects
        .iter()
        .find_map(|object| match object {
            Object::Function(function) if function.name() == name => Some(function.as_ref()),
            _ => None,
        })
        .expect("function exists")
}

fn ops(function: &Function) -> Vec<OpCode> {
    let code = &function.bytecode.compact.as_ref().unwrap().code;
    let mut result = Vec::new();
    let mut pc = 0;
    while pc < code.len() {
        let op = OpCode::try_from(code[pc]).expect("valid opcode");
        result.push(op);
        pc += op.encoded_size();
    }
    result
}

/// Run `main` with the fast path on and off; both must agree.
fn run(source: &str) -> i64 {
    let results = [true, false].map(|fast_path| {
        let program = compile_source(source);
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut vm =
            BexVm::from_program(program, Arc::new(AtomicBool::new(false))).expect("valid program");
        vm.dispatch_fast_path = fast_path;
        vm.disable_telemetry();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        match vm.exec().expect("execution succeeds") {
            VmExecState::Complete(value) => value.as_int().expect("integer result"),
            other => panic!("expected completion, got {other:?}"),
        }
    });
    assert_eq!(results[0], results[1], "fast path and full dispatch differ");
    results[0]
}

#[test]
fn loops_fuse_increments_tests_and_field_loads() {
    let source = r#"
        class P { x: int }
        function f(n: int, p: P) -> int {
            let s = 0;
            for (let i = 0; i < n; i += 1) {
                if (i < 10) { s += 1; }
                s = p.x + s;
            }
            s
        }
        function main() -> int { f(20, P { x: 3 }) }
    "#;
    let program = convert_program(compile_source(source)).unwrap();
    let f_ops = ops(function(&program, "f"));
    for op in [
        OpCode::AddLocalSmall,
        OpCode::JumpIfLocalsCmp,
        OpCode::JumpIfLocalConstCmp,
        OpCode::LoadVarField,
    ] {
        assert!(f_ops.contains(&op), "{op:?} missing from {f_ops:?}");
    }
    assert_eq!(run(source), 10 + 20 * 3);
}

#[test]
fn fused_increment_wraps_like_the_addition() {
    assert_eq!(
        run(r#"
            function bump(x: int, k: int) -> int {
                let y = x;
                if (k > 0) { y += 1; } else { y = y + -1; }
                y
            }
            function main() -> int {
                let a = 0;
                if (bump(9223372036854775807, 1) == -9223372036854775808) { a = 1; }
                let b = 0;
                if (bump(-9223372036854775808, -1) == 9223372036854775807) { b = 10; }
                a + b + 100 * bump(41, 1)
            }
        "#),
        1 + 10 + 100 * 42,
    );
}

#[test]
fn fused_branches_keep_every_comparison_and_polarity() {
    // Each comparison runs as an `if` (the branch jumps on the negated
    // comparison) and as an `if`/`else` on locals and on a constant, over a
    // range that crosses equality.
    assert_eq!(
        run(r#"
            function score(a: int, b: int) -> int {
                let s = 0;
                if (a < b) { s += 1; }
                if (a <= b) { s += 2; }
                if (a > b) { s += 4; }
                if (a >= b) { s += 8; }
                if (a == b) { s += 16; }
                if (a != b) { s += 32; }
                if (a < 5) { s += 64; } else { s += 128; }
                if (a >= 5) { s += 256; }
                if (a == 5) { s += 512; }
                if (a != 5) { s += 1024; }
                s
            }
            function main() -> int {
                let total = 0;
                for (let a = 0; a <= 10; a += 1) {
                    for (let b = 3; b > 0 - 3; b = b - 1) {
                        total = total * 3 + score(a, b);
                        total = total % 1000000007;
                    }
                }
                total
            }
        "#),
        {
            let score = |a: i64, b: i64| {
                let mut s = 0;
                if a < b {
                    s += 1;
                }
                if a <= b {
                    s += 2;
                }
                if a > b {
                    s += 4;
                }
                if a >= b {
                    s += 8;
                }
                if a == b {
                    s += 16;
                }
                if a != b {
                    s += 32;
                }
                s += if a < 5 { 64 } else { 128 };
                if a >= 5 {
                    s += 256;
                }
                if a == 5 {
                    s += 512;
                }
                if a != 5 {
                    s += 1024;
                }
                s
            };
            let mut total: i64 = 0;
            for a in 0..=10 {
                for b in (-2..=3).rev() {
                    total = (total * 3 + score(a, b)) % 1_000_000_007;
                }
            }
            total
        },
    );
}

#[test]
fn fused_operations_accept_boxed_ints() {
    // Ints of magnitude 2^62 and above are heap-boxed. The loop counter starts
    // inline and crosses into boxed values, so the fused increment, the fused
    // comparison against a boxed constant and the plain int opcodes all see
    // both forms; the fast path must leave for full dispatch, not misread a
    // pointer as an int.
    assert_eq!(
        run(r#"
            function main() -> int {
                let x = 4611686018427387900;
                let n = 0;
                while (x < 4611686018427387910) {
                    x += 1;
                    n += 1;
                }
                let y = x - 4611686018427387905;
                let big = 9223372036854775800;
                let steps = 0;
                for (let i = big; i < 9223372036854775806; i += 2) {
                    steps += 1;
                }
                if (x == 4611686018427387910 && y * 2 == 10 && big > x) {
                    n * 100 + steps
                } else {
                    -1
                }
            }
        "#),
        10 * 100 + 3,
    );
}
