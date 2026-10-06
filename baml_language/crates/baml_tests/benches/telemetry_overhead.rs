//! Telemetry overhead per call, for the type-definition A/B comparison
//! (`baml_language/docs/dyn-type-definitions/SPEC.md`).
//! Run with: cargo bench -p baml_tests --bench telemetry_overhead
//!
//! Each scenario's `main` makes `CALLS` calls in a loop; every scenario runs
//! with telemetry off (no recording) and on (recording to local files). The
//! overhead per call is `(on - off) / CALLS`, from the two medians.
#![allow(clippy::disallowed_methods)]

use std::{path::Path, sync::Arc};

use baml_compiler2_emit::OptLevel;
use baml_db::{ProjectDatabase, compile_program};
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, FunctionCallContextBuilder, TelemetryRecording};
use divan::{Bencher, black_box};
use sys_native::{CallId, SysOpsExt};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping telemetry_overhead in debug/test profile.");
        return;
    }
    if std::env::var_os("DIVAN_MAX_TIME").is_none() {
        // SAFETY: single-threaded here at the very top of main, before divan
        // reads its args/env. No other thread can observe the environment.
        unsafe { std::env::set_var("DIVAN_MAX_TIME", "3") };
    }
    divan::main();
}

/// Compile once, then measure calling `main()`. With `recording`, telemetry
/// records to a fresh local directory for the whole run.
fn bench_main(bencher: Bencher, source: &str, recording: bool) {
    let mut db = ProjectDatabase::new();
    let package = db.workspace(Path::new("."));
    db.file("bench.baml", source);
    let program =
        compile_program(&db, package, OptLevel::Two).expect("benchmark compilation failed");
    let sys_ops = Arc::new(sys_native::SysOps::native());
    let dir = tempfile::tempdir().expect("recording directory");
    let engine = if recording {
        BexEngine::new_with_telemetry_recording(
            program,
            sys_ops,
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files(dir.path(), btel_recorder::RecordingConfig::default()),
        )
    } else {
        BexEngine::new(program, sys_ops, vec![])
    }
    .expect("benchmark engine creation failed");
    let engine = Arc::new(engine);
    let rt = tokio::runtime::Runtime::new().expect("failed to build tokio runtime");
    bencher.bench(|| {
        let context = FunctionCallContextBuilder::new(CallId::next()).build();
        black_box(
            rt.block_on(engine.call_function("main", vec![], context, true))
                .expect("benchmark execution failed"),
        )
    });
    rt.block_on(engine.shutdown());
}

/// One scenario: a `main` making `CALLS` calls, benched off and on.
macro_rules! scenario {
    ($name:ident, $source:expr) => {
        mod $name {
            use super::*;
            #[divan::bench]
            fn off(bencher: Bencher) {
                bench_main(bencher, $source, false);
            }
            #[divan::bench]
            fn on(bencher: Bencher) {
                bench_main(bencher, $source, true);
            }
        }
    };
}

/// Calls per `main`: 10,000 for the cheap scenarios, fewer where each call
/// also builds a class.
const TYPES: &str = r#"
class Resume { name: string, age: int? }
class Box<T> { value: T, items: T[] }
class A { x: int }
class B { y: string }
class C { z: bool }
class D { w: float }
enum Status { Active, Inactive }
function Add(a: int, b: int) -> int { a + b }
function Pick<T>(x: T) -> T { x }
function None<T>() -> int { 1 }
"#;

// 1. Timing only: no span, no capture. The handoff's 10 ns target. Neither
// design may change it.
scenario!(
    timing_only,
    &format!(
        "{TYPES}
function main() -> int {{
    let t = 0; let i = 0;
    while (i < 10000) {{ t = Add(t, i); i = i + 1; }}
    t
}}"
    )
);

// 2. A generic call that is a span but captures no inputs: no type work
// (#5145), and no definition work in either design.
scenario!(
    span_without_capture,
    &format!(
        "{TYPES}
function main() -> int {{
    let r = Resume {{ name: \"ann\", age: 3 }};
    let i = 0;
    while (i < 10000) {{ let x = Pick(r, $trace = trace.span()); i = i + 1; }}
    i
}}"
    )
);

// 3. Type arguments only, one declared type, captured: the steady state.
scenario!(type_args_repeated, &format!("{TYPES}
function main() -> int {{
    let t = 0; let i = 0;
    while (i < 10000) {{ t = t + None<Box<Resume>>($trace = trace.span(inputs = true)); i = i + 1; }}
    t
}}"));

// 4. A captured instance argument naming a declared class and an enum.
scenario!(instance_inputs_repeated, &format!("{TYPES}
function main() -> int {{
    let r = Resume {{ name: \"ann\", age: 3 }};
    let i = 0;
    while (i < 10000) {{ let x = Pick(r, $trace = trace.span(inputs = true)); let s = Pick(Status.Active, $trace = trace.span(inputs = true)); i = i + 1; }}
    i
}}"));

// 5. Alternating among several declared types: a cache with more than one
// entry, every call a hit after the first round.
scenario!(
    type_args_alternating,
    &format!(
        "{TYPES}
function main() -> int {{
    let t = 0; let i = 0;
    while (i < 2500) {{
        t = t + None<A>($trace = trace.span(inputs = true));
        t = t + None<B[]>($trace = trace.span(inputs = true));
        t = t + None<map<string, C>>($trace = trace.span(inputs = true));
        t = t + None<D?>($trace = trace.span(inputs = true));
        i = i + 1;
    }}
    t
}}"
    )
);

// 6. A new runtime class on every call: every call is a first sighting.
// B's worst case. Building the class costs the same with telemetry off.
scenario!(
    new_runtime_class_each_call,
    &format!(
        "{TYPES}
function MakeAndUse(i: int) -> int {{
    let b = reflect.class.builder(\"Temp\");
    b.field(\"i\", reflect.Type.of<int>());
    b.field(\"name\", reflect.Type.of<string>());
    let t = b.build();
    type Temp = unreflect(t.as_type())
    None<Temp[]>($trace = trace.span(inputs = true))
}}
function main() -> int {{
    let t = 0; let i = 0;
    while (i < 1000) {{ t = t + MakeAndUse(i); i = i + 1; }}
    t
}}"
    )
);

// 7. Runtime classes collected soon after use, under collection pressure:
// A's worst case (definitions pending while their classes are collected).
scenario!(
    runtime_classes_under_collection,
    &format!(
        "{TYPES}
function MakeAndUse(i: int) -> int {{
    let b = reflect.class.builder(\"Temp\");
    b.field(\"i\", reflect.Type.of<int>());
    let t = b.build();
    type Temp = unreflect(t.as_type())
    None<Temp>($trace = trace.span(inputs = true))
}}
function main() -> int {{
    let t = 0; let i = 0;
    while (i < 500) {{
        t = t + MakeAndUse(i);
        if (i % 10 == 0) {{ baml.sys.collect_garbage(); }}
        i = i + 1;
    }}
    t
}}"
    )
);
