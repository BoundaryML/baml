//! VM execution benchmarks.
//!
//! Do not measure compilation here, only VM execution time.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, atomic::AtomicU32},
};

use baml_base::{FileId, SourceFile};
use baml_vm::{EvalStack, Frame, ObjectIndex, StackIndex, Value, Vm, watch::Watch};

struct Program {
    source: &'static str,
    function: &'static str,
    args: Vec<Value>,
}

/// Minimal test database for benchmarks.
#[salsa::db]
#[derive(Clone)]
struct BenchDatabase {
    storage: salsa::Storage<Self>,
    next_file_id: Arc<AtomicU32>,
}

#[salsa::db]
impl salsa::Database for BenchDatabase {}

#[salsa::db]
impl baml_hir::Db for BenchDatabase {}

#[salsa::db]
impl baml_thir::Db for BenchDatabase {}

impl BenchDatabase {
    fn new() -> Self {
        Self {
            storage: salsa::Storage::default(),
            next_file_id: Arc::new(AtomicU32::new(0)),
        }
    }

    fn add_file(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> SourceFile {
        let file_id = FileId::new(
            self.next_file_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );
        SourceFile::new(self, text.into(), path.into(), file_id)
    }
}

fn bootstrap_vm(input: Program) -> Vm {
    let mut db = BenchDatabase::new();
    let file = db.add_file("bench.baml", input.source);
    let program = baml_codegen::compile_files(&db, &[file]);

    // Find the target function index by name
    let target_function_index = *program
        .function_indices
        .get(input.function)
        .expect("function not found");

    let target_function_index = ObjectIndex::from_raw(target_function_index);

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
        runtime_allocs_offset: ObjectIndex::from_raw(program.objects.len()),
        objects: program.objects.clone(),
        globals: program.globals.clone(),
        env_vars: HashMap::default(),
        watch: Watch::new(),
        watched_vars: HashMap::default(),
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
