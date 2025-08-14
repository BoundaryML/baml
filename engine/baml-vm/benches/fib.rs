use baml_vm::{BamlVmProgram, Frame, Value, Vm};
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
    } = baml_compiler::compile(&ast).unwrap();

    // Find the target function index by name
    let (target_function_index, _) = resolved_function_names[input.function];

    Vm {
        frames: vec![Frame {
            function: target_function_index,
            instruction_ptr: 0,
            locals_offset: 0,
        }],
        stack: std::iter::once(Value::Object(target_function_index))
            .chain(input.args)
            .collect(),
        runtime_allocs_offset: objects.len(),
        objects,
        globals,
    }
}

pub fn bench_recursive_fib(c: &mut Criterion) {
    c.bench_function("recursive fib 25", |b| {
        b.iter_batched(
            || {
                bootstrap_vm(Program {
                    source: r#"
                        function fib(n: int) -> int {
                            if n <= 1 {
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
                            let mut a = 0;
                            let mut b = 1;

                            if n == 0 {
                                b
                            } else {
                                let mut i = 0;
                                while i < n {
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
