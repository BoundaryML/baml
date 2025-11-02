//! VM execution benchmarks.
//!
//! Do not measure compilation here, only VM execution time.

use baml_vm::{watch::Watch, BamlVmProgram, EvalStack, Frame, ObjectIndex, StackIndex, Value, Vm};

struct Program {
    source: &'static str,
    function: &'static str,
    args: Vec<Value>,
}

fn bootstrap_vm(input: Program) -> Vm {
    let ast = baml_compiler::test::ast(input.source).unwrap();

    let BamlVmProgram {
        objects,
        globals,
        resolved_function_names,
        ..
    } = baml_compiler::compile(&ast).unwrap();

    // Find the target function index by name
    let (target_function_index, _) = resolved_function_names[input.function];

    Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        }],
        stack: EvalStack::from_vec(
            std::iter::once(Value::Object(target_function_index))
                .chain(input.args)
                .collect(),
        ),
        runtime_allocs_offset: ObjectIndex::from_raw(objects.len()),
        objects,
        globals,
        env_vars: Default::default(),
        watch: Watch::new(),
        watched_vars: Default::default(),
        interrupt_frame: None,
    }
}

#[divan::bench(consts = [5, 10, 15])]
pub fn recursive_fib<const N: i64>(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            bootstrap_vm(Program {
                source: r#"
                    function fib(n: int) -> int {
                        if (n <= 1) {
                            n
                        } else {
                            fib(n - 1) + fib(n - 2)
                        }
                    }
                "#,
                function: "fib",
                args: vec![Value::Int(N)],
            })
        })
        .bench_refs(|vm| vm.exec().unwrap());
}

#[divan::bench(consts = [1000, 2000, 3000])]
pub fn iterative_fib<const N: i64>(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            bootstrap_vm(Program {
                source: r#"
                    function fib(n: int) -> int {
                        let a = 0;
                        let b = 1;

                        if (n == 0) {
                            b
                        } else {
                            let i = 1;
                            while (i <= n) {
                                let c = a + b;
                                a = b;
                                b = c;
                                i += 1;
                            }
                            b
                        }
                    }
                "#,
                function: "fib",
                args: vec![Value::Int(N)],
            })
        })
        .bench_refs(|vm| vm.exec().unwrap());
}

fn main() {
    divan::main();
}
