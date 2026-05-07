//! BAML Runtime / VM Execution Benchmarks
//!
//! Run with: cargo bench --bench runtime_benchmark
//!
//! These complement the compiler benchmarks in compiler_benchmark.rs.
//! Compiler benchmarks measure parse/compile cost; these measure VM execution.

use std::{fmt::Write, path::Path, sync::Arc};

use baml_compiler2_emit::{CompileOptions, generate_project_bytecode};
use baml_project::ProjectDatabase;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use divan::{Bencher, black_box};
use sys_native::{CallId, SysOpsExt};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping runtime_benchmark in debug/test profile.");
        return;
    }
    divan::main();
}

// ============================================================================
// Helpers
// ============================================================================

/// Compile BAML source into a ready-to-run engine.
fn compile_source(source: &str) -> (ProjectDatabase, BexEngine) {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new("."));
    db.add_file("bench.baml", source);
    let bytecode = generate_project_bytecode(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
    )
    .expect("benchmark compilation failed");
    let engine = BexEngine::new(
        bytecode,
        Arc::new(sys_native::SysOps::native()),
        None,
        vec![],
    )
    .expect("benchmark engine creation failed");
    (db, engine)
}

/// Call `main()` on a pre-compiled engine and return the result.
fn call_main(engine: &Arc<BexEngine>) -> BexExternalValue {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(engine.call_function(
        "main",
        vec![],
        FunctionCallContextBuilder::new(CallId::next()).build(),
        true,
    ))
    .expect("benchmark execution failed")
}

/// Generate a chain of N functions: f0 -> f1 -> ... -> f{N-1}.
/// Each adds 1 to its argument. main() calls f0(0) in a loop.
fn generate_call_chain(depth: usize, iterations: usize) -> String {
    let mut s = String::new();
    for i in 0..depth - 1 {
        writeln!(s, "function f{i}(n: int) -> int {{ f{}(n + 1) }}", i + 1).unwrap();
    }
    writeln!(s, "function f{}(n: int) -> int {{ n }}", depth - 1).unwrap();
    writeln!(
        s,
        "function main() -> int {{
  let s = 0;
  for (let i = 0; i < {iterations}; i += 1) {{
    s += f0(0);
  }};
  return s;
}}"
    )
    .unwrap();
    s
}

/// Generate a large program with N functions that main() calls sequentially.
fn generate_n_functions(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        writeln!(s, "function f{i}(x: int) -> int {{ x + {i} }}").unwrap();
    }
    write!(s, "function main() -> int {{\n  let s = 0;\n").unwrap();
    for i in 0..n {
        writeln!(s, "  s += f{i}(1);").unwrap();
    }
    writeln!(s, "  return s;\n}}").unwrap();
    s
}

// ============================================================================
// Startup / overhead benchmarks
// ============================================================================

#[divan::bench]
fn startup_empty_expression(bencher: Bencher) {
    bencher.bench(|| {
        let (db, engine) = compile_source(r#"function main() -> string { "hello" }"#);
        let engine = Arc::new(engine);
        black_box(call_main(&engine));
        let _ = db;
    });
}

#[divan::bench]
fn compile_to_engine(bencher: Bencher) {
    let source = r#"
function fib(n: int) -> int {
  if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
function main() -> int { fib(10) }
"#;
    bencher
        .with_inputs(|| {
            let mut db = ProjectDatabase::new();
            db.set_project_root(Path::new("."));
            db.add_file("bench.baml", source);
            db
        })
        .bench_values(|db| {
            let bytecode = generate_project_bytecode(
                &db,
                &CompileOptions {
                    emit_test_cases: false,
                },
            )
            .expect("compilation failed");
            let engine = BexEngine::new(
                bytecode,
                Arc::new(sys_native::SysOps::native()),
                None,
                vec![],
            )
            .expect("engine creation failed");
            black_box(engine);
        });
}

#[divan::bench]
fn engine_init_cost(bencher: Bencher) {
    let source = r#"
function fib(n: int) -> int {
  if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
function main() -> int { fib(10) }
"#;
    bencher
        .with_inputs(|| {
            let mut db = ProjectDatabase::new();
            db.set_project_root(Path::new("."));
            db.add_file("bench.baml", source);
            let bytecode = generate_project_bytecode(
                &db,
                &CompileOptions {
                    emit_test_cases: false,
                },
            )
            .expect("compilation failed");
            (db, bytecode)
        })
        .bench_values(|(db, bytecode)| {
            let engine = BexEngine::new(
                bytecode,
                Arc::new(sys_native::SysOps::native()),
                None,
                vec![],
            )
            .expect("engine creation failed");
            black_box(engine);
            let _ = db;
        });
}

// ============================================================================
// Pure VM execution benchmarks
// ============================================================================

#[divan::bench]
fn vm_fib_20(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
function fib(n: int) -> int {
  if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
function main() -> int { fib(20) }
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_loop_500k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
function main() -> int {
  let sum = 0;
  for (let i = 0; i < 500000; i += 1) {
    sum += i;
  };
  return sum;
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_string_concat_5k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
function main() -> int {
  let s = "";
  for (let i = 0; i < 5000; i += 1) {
    s = s + "hello";
  };
  return s.length();
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_array_push_50k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
function main() -> int {
  let arr: int[] = [];
  for (let i = 0; i < 50000; i += 1) {
    arr.push(i);
  };
  return arr.length();
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_array_iter_10k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
function build_array() -> int[] {
  let arr: int[] = [];
  for (let i = 0; i < 10000; i += 1) {
    arr.push(i);
  };
  return arr;
}

function main() -> int {
  let arr = build_array();
  let s = 0;
  for (let x in arr) {
    s += x;
  };
  return s;
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_class_create_50k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
class Point {
  x int
  y int
}

function main() -> int {
  let s = 0;
  for (let i = 0; i < 50000; i += 1) {
    let p = Point { x: i, y: i * 2 };
    s += p.x + p.y;
  };
  return s;
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_field_access_50k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
class Point {
  x int
  y int
  z int
}

function main() -> int {
  let p = Point { x: 1, y: 2, z: 3 };
  let s = 0;
  for (let i = 0; i < 50000; i += 1) {
    s += p.x + p.y + p.z;
  };
  return s;
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_call_chain_100_x_5k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let source = generate_call_chain(100, 5000);
            let (db, engine) = compile_source(&source);
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_nested_loop(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
function main() -> int {
  let sum = 0;
  for (let i = 0; i < 200; i += 1) {
    for (let j = 0; j < 200; j += 1) {
      sum += i * j;
    };
  };
  return sum;
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_mixed_ops(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
class Point {
  x int
  y int
}

function main() -> int {
  let sum = 0;
  let s = "";
  let arr: int[] = [];
  for (let i = 0; i < 5000; i += 1) {
    sum += i * 3 - 1;
    s = s + "x";
    arr.push(i);
    let p = Point { x: i, y: i + 1 };
    sum += p.x + p.y;
    if i > 2500 { sum += 1; };
  };
  return sum + s.length() + arr.length();
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

#[divan::bench]
fn vm_closure_call_50k(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let (db, engine) = compile_source(
                r#"
function add_one(n: int) -> int { n + 1 }

function main() -> int {
  let f = add_one;
  let s = 0;
  for (let i = 0; i < 50000; i += 1) {
    s += f(i);
  };
  return s;
}
"#,
            );
            (db, Arc::new(engine))
        })
        .bench_values(|(db, engine)| {
            let _ = &db;
            black_box(call_main(&engine));
        });
}

// ============================================================================
// End-to-end benchmarks (compile + execute)
// ============================================================================

#[divan::bench]
fn e2e_hello_world(bencher: Bencher) {
    bencher.bench(|| {
        let (db, engine) = compile_source(r#"function main() -> string { "hello world" }"#);
        let engine = Arc::new(engine);
        black_box(call_main(&engine));
        let _ = db;
    });
}

#[divan::bench]
fn e2e_arithmetic(bencher: Bencher) {
    bencher.bench(|| {
        let (db, engine) =
            compile_source(r#"function main() -> int { 1 + 2 * 3 + 4 * 5 - 6 / 2 }"#);
        let engine = Arc::new(engine);
        black_box(call_main(&engine));
        let _ = db;
    });
}

#[divan::bench]
fn e2e_fib_20(bencher: Bencher) {
    bencher.bench(|| {
        let (db, engine) = compile_source(
            r#"
function fib(n: int) -> int {
  if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
function main() -> int { fib(20) }
"#,
        );
        let engine = Arc::new(engine);
        black_box(call_main(&engine));
        let _ = db;
    });
}

#[divan::bench]
fn e2e_class_and_loop(bencher: Bencher) {
    bencher.bench(|| {
        let (db, engine) = compile_source(
            r#"
class Point {
  x int
  y int
}

function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000; i += 1) {
    let p = Point { x: i, y: i * 2 };
    s += p.x + p.y;
  };
  return s;
}
"#,
        );
        let engine = Arc::new(engine);
        black_box(call_main(&engine));
        let _ = db;
    });
}

#[divan::bench]
fn e2e_100_functions(bencher: Bencher) {
    let source = generate_n_functions(100);
    bencher.bench_local(move || {
        let (db, engine) = compile_source(&source);
        let engine = Arc::new(engine);
        black_box(call_main(&engine));
        let _ = db;
    });
}
