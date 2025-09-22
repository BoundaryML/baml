use baml_vm::{BamlVmProgram, EvalStack, Frame, ObjectIndex, StackIndex, Value, Vm};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

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
    }
}

pub fn bench_recursive_fib(c: &mut Criterion) {
    c.bench_function("recursive fib 25", |b| {
        b.iter_batched(
            || {
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
                    args: vec![Value::Int(25)],
                })
            },
            |mut vm| {
                vm.exec().unwrap();
            },
            BatchSize::PerIteration,
        )
    });
}

pub fn bench_iterative_fib(c: &mut Criterion) {
    c.bench_function("iterative fib 3000", |b| {
        b.iter_batched(
            || {
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
                    args: vec![Value::Int(3000)],
                })
            },
            |mut vm| {
                vm.exec().unwrap();
            },
            BatchSize::PerIteration,
        )
    });
}

criterion_group!(benches, bench_recursive_fib, bench_iterative_fib);
criterion_main!(benches);
