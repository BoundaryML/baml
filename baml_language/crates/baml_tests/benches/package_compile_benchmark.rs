//! Public-path `reflect.Package.compile` cold-cost and dispatch parity benches.
//!
//! `runtime_stdlib_dispatch` includes a fresh Package.compile on each sample.
//! Compare its 0-iteration and 100k-iteration medians to derive dispatch cost
//! with the compile constant cancelled; `static_stdlib_dispatch` is the same
//! loop compiled into the host image and uses the same delta calculation.

use std::{path::Path, sync::Arc};

use baml_compiler2_emit::{CompileOptions, OptLevel, generate_project_bytecode_with_opt};
use baml_db::ProjectDatabase;
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use divan::{Bencher, black_box};
use sys_native::{CallId, SysOpsExt};

const OUTER_SOURCE: &str = r####"
function package_compile_cold() -> int throws unknown {
  let package = reflect.Package.compile({
    "schema.baml": "class ColdSchema { name string count int }"
  })
  package.diagnostics().length()
}

function compare_hot<T extends baml.ops.Compare>(value: T, n: int) -> int throws never {
  let count = 0
  for (let i = 0; i < n; i += 1) {
    if value <= value { count += 1 } else { count -= 1 }
  }
  count
}

function static_stdlib_dispatch(n: int) -> int throws never {
  compare_hot<int>(7, n)
}

function runtime_stdlib_dispatch(n: int) -> int throws unknown {
  let package = reflect.Package.compile({ "dispatch.baml": #"
function compare_hot<T extends baml.ops.Compare>(value: T, n: int) -> int throws never {
  let count = 0
  for (let i = 0; i < n; i += 1) {
    if value <= value { count += 1 } else { count -= 1 }
  }
  count
}
function run(n: int) -> int throws never { compare_hot<int>(7, n) }
"# })
  let run = package.get_function<(int) -> int>("root.run") ?? throw "missing run"
  run(n)
}
"####;

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping package_compile_benchmark in debug/test profile.");
        return;
    }
    if std::env::var_os("DIVAN_MAX_TIME").is_none() {
        // SAFETY: single-threaded before divan or the engine reads the env.
        unsafe { std::env::set_var("DIVAN_MAX_TIME", "3") };
    }
    if std::env::var_os("BAML_PROFILE").is_none() {
        // SAFETY: as above; profiling must not contaminate wall-clock results.
        unsafe { std::env::set_var("BAML_PROFILE", "0") };
    }
    divan::main();
}

fn engine() -> Arc<BexEngine> {
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new("."));
    db.file("package_compile_bench.baml", OUTER_SOURCE);
    let program = generate_project_bytecode_with_opt(
        &db,
        &CompileOptions {
            emit_test_cases: false,
        },
        OptLevel::One,
    )
    .expect("compile Package.compile benchmark host");
    Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("create Package.compile benchmark engine"),
    )
}

fn call(engine: &Arc<BexEngine>, runtime: &tokio::runtime::Runtime, entry: &str, n: Option<i64>) {
    let args = n.into_iter().map(BexExternalValue::Int).collect();
    black_box(
        runtime
            .block_on(engine.call_function(
                entry,
                args,
                FunctionCallContextBuilder::new(CallId::next()).build(),
                true,
            ))
            .unwrap_or_else(|error| panic!("{entry} benchmark failed: {error}")),
    );
}

#[divan::bench(sample_count = 5, sample_size = 1)]
fn package_compile_cold(bencher: Bencher) {
    let engine = engine();
    let runtime = tokio::runtime::Runtime::new().expect("tokio benchmark runtime");
    bencher.bench(|| call(&engine, &runtime, "user.package_compile_cold", None));
}

#[divan::bench(args = [0_i64, 100_000_i64])]
fn static_stdlib_dispatch(bencher: Bencher, n: i64) {
    let engine = engine();
    let runtime = tokio::runtime::Runtime::new().expect("tokio benchmark runtime");
    bencher.bench(|| call(&engine, &runtime, "user.static_stdlib_dispatch", Some(n)));
}

#[divan::bench(args = [0_i64, 100_000_i64], sample_count = 5, sample_size = 1)]
fn runtime_stdlib_dispatch(bencher: Bencher, n: i64) {
    let engine = engine();
    let runtime = tokio::runtime::Runtime::new().expect("tokio benchmark runtime");
    bencher.bench(|| call(&engine, &runtime, "user.runtime_stdlib_dispatch", Some(n)));
}
