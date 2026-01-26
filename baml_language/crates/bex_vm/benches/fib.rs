//! VM execution benchmarks.
//!
//! Do not measure compilation here, only VM execution time.

use baml_tests::bytecode::TestDatabase;
use bex_vm::BexVm;
use bex_vm_types::Value;

struct Program {
    source: &'static str,
    function: &'static str,
    args: Vec<Value>,
}

fn bootstrap_vm(input: &Program) -> BexVm {
    let mut db = TestDatabase::new();
    let file = db.add_file("bench.baml", input.source);
    let program = baml_compiler_emit::compile_files(&db, &[file])
        .expect("compile_files should succeed for valid benchmark source");

    let function_index = program
        .function_index(input.function)
        .expect("function not found");

    let mut vm = BexVm::from_program(program).expect("All native functions should be attached");
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
        .bench_refs(|vm| vm.exec().unwrap());
}

#[divan::bench(consts = [1000, 2000, 3000])]
#[ignore = "loop codegen causes infinite loop"]
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
        .bench_refs(|vm| vm.exec().unwrap());
}

fn main() {
    divan::main();
}
