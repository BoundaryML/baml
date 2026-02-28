//! VM execution benchmarks.
//!
//! Do not measure compilation here, only VM execution time.

use baml_tests::bytecode::{compile_source, make_vm};
use bex_vm::BexVm;
use bex_vm_types::Value;

struct Program {
    source: &'static str,
    function: &'static str,
    args: Vec<Value>,
}

fn bootstrap_vm(input: &Program) -> BexVm {
    let program = compile_source(input.source);

    let Some(function_index) = program.function_index(input.function) else {
        panic!("function not found");
    };

    let mut vm = match make_vm(program) {
        Ok(vm) => vm,
        Err(err) => panic!("native function attachment must succeed: {err}"),
    };
    let function_ptr = vm.heap.compile_time_ptr(function_index);
    vm.set_entry_point(function_ptr, &input.args);
    vm
}

#[divan::bench(consts = [5, 10, 15])]
pub fn recursive_fib<const N: i64>(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            bootstrap_vm(&Program {
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
        .bench_refs(|vm| match vm.exec() {
            Ok(result) => result,
            Err(err) => panic!("vm exec failed: {err}"),
        });
}

#[divan::bench(consts = [1000, 2000, 3000])]
pub fn iterative_fib<const N: i64>(bencher: divan::Bencher) {
    bencher
        .with_inputs(|| {
            bootstrap_vm(&Program {
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
        .bench_refs(|vm| match vm.exec() {
            Ok(result) => result,
            Err(err) => panic!("vm exec failed: {err}"),
        });
}

fn main() {
    divan::main();
}
