// Measurement glue for a `baml pack --native` program. bench/stage.py copies
// this file to src/main.rs of the generated crate in place of the pack host
// shim; the crate's library is the compiler's output, unchanged.
//
// Required library API (crate `baml_native_program`, written by `baml pack --native`):
//   pub static PROGRAM_ARTIFACT: &[u8]                 the packed program
//   pub static NATIVE_BODIES: &bex_vm::NativeBodyTable  the bodies compiled from it
// The program defines `prepare(raw: string) -> State` and `run(state: State) -> string`.
//
// One executable measures both backends of the same build, so the only
// difference between the rows is whether the compiled bodies are attached:
//   run          ARTIFACT INPUT JOBS WARMUP SINK OUTPUT   compiled bodies attached
//   run-bytecode ARTIFACT INPUT JOBS WARMUP SINK OUTPUT   the same program as bytecode
// ARTIFACT is ignored (the program is embedded). Protocol: README.md, "What is measured".
use std::{io::Write, sync::Arc, time::Instant};

use bex_engine::{
    BexEngine, BexExternalValue, EngineConfig, FunctionCallContextBuilder, TelemetryRecording,
};
use sys_native::SysOpsExt;

#[repr(C)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}
unsafe extern "C" {
    fn clock_gettime(clock: i32, ts: *mut Timespec) -> i32;
}
const CLOCK_PROCESS_CPUTIME_ID: i32 = 2;

fn cpu_ns() -> u128 {
    let mut ts = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    assert_eq!(unsafe { clock_gettime(CLOCK_PROCESS_CPUTIME_ID, &mut ts) }, 0);
    ts.tv_sec as u128 * 1_000_000_000 + ts.tv_nsec as u128
}

fn quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn emit(line: String) {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(line.as_bytes()).unwrap();
    stdout.write_all(b"\n").unwrap();
    stdout.flush().unwrap();
}

fn fail(message: String) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let native = match a.get(1).map(String::as_str) {
        Some("run") => true,
        Some("run-bytecode") => false,
        _ => fail("usage: run|run-bytecode ARTIFACT INPUT JOBS WARMUP SINK OUTPUT".into()),
    };
    if a.len() != 8 {
        fail("usage: run|run-bytecode ARTIFACT INPUT JOBS WARMUP SINK OUTPUT".into());
    }
    let jobs: u64 = a[4].parse().unwrap_or_else(|e| fail(format!("JOBS: {e}")));
    let warmup: u64 = a[5].parse().unwrap_or_else(|e| fail(format!("WARMUP: {e}")));
    if jobs == 0 {
        fail("jobs must be positive".into());
    }
    let workers: u64 = std::env::var("BENCH_WORKERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or_else(|| fail("BENCH_WORKERS must be set".into()));
    let telemetry = std::env::var("BAML_TELEMETRY").unwrap_or_default();
    // A `local` sink records into OUTPUT at the level `BAML_TELEMETRY` names;
    // `off` records nothing. A recording that cannot start fails the trial
    // rather than measuring an unrecorded run.
    let recording = if telemetry == "off" {
        None
    } else {
        if a[6] != "local" {
            fail(format!("BAML_TELEMETRY={telemetry:?} needs the local sink, not {:?}", a[6]));
        }
        std::fs::create_dir_all(&a[7]).unwrap_or_else(|e| fail(format!("OUTPUT: {e}")));
        Some(TelemetryRecording::local_files_in(&a[7], Default::default()).with_host("bench"))
    };

    let origin = Instant::now();
    let rt = tokio::runtime::Runtime::new().unwrap_or_else(|e| fail(format!("runtime: {e}")));
    let envelope: baml_exec::PackEnvelope = baml_artifact::decode(
        baml_artifact::ArtifactKind::PackedProgram,
        baml_native_program::PROGRAM_ARTIFACT,
    )
    .unwrap_or_else(|e| fail(format!("packed program: {e}")));
    let engine = {
        let _guard = rt.enter();
        BexEngine::new_with_config(
            envelope.program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            EngineConfig {
                recording,
                native_bodies: native.then_some(baml_native_program::NATIVE_BODIES),
                ..Default::default()
            },
        )
        .map(Arc::new)
        .unwrap_or_else(|e| fail(format!("engine: {e}")))
    };
    let load_ns = origin.elapsed().as_nanos();
    // Every call is a root, as every engine call is.
    let call = |name: &str, args: Vec<BexExternalValue>, copy: bool| {
        rt.block_on(engine.call_function(
            name,
            args,
            FunctionCallContextBuilder::new(sys_native::CallId::next()).build(),
            copy,
        ))
    };

    let raw = std::fs::read_to_string(&a[3]).unwrap_or_else(|e| fail(format!("INPUT: {e}")));
    // The state stays on the heap; each job receives a handle to it.
    let state = call("prepare", vec![BexExternalValue::String(raw.into())], false)
        .unwrap_or_else(|thrown| fail(format!("prepare threw: {thrown}")));
    emit(format!(
        "{{\"event\":\"ready\",\"load_ns\":{load_ns},\"load_reason\":\"embedded program decoded, engine built\",\"setup_ns\":{}}}",
        origin.elapsed().as_nanos()
    ));

    for _ in 0..warmup {
        match call("run", vec![state.clone()], true) {
            Ok(value) => drop(std::hint::black_box(value)),
            Err(thrown) => fail(format!("run threw during warmup: {thrown}")),
        }
    }
    let start = Instant::now();
    let cpu = cpu_ns();
    let mut last = None;
    for _ in 0..jobs {
        match call("run", vec![state.clone()], true) {
            Ok(value) => last = Some(std::hint::black_box(value)),
            Err(thrown) => fail(format!("run threw: {thrown}")),
        }
    }
    let execution_cpu_ns = cpu_ns() - cpu;
    let execution_wall_ns = start.elapsed().as_nanos();
    let result = match last.expect("jobs is positive") {
        BexExternalValue::String(s) => s.as_str().to_owned(),
        other => format!("{other:?}"),
    };
    emit(format!(
        "{{\"event\":\"result\",\"jobs\":{jobs},\"execution_wall_ns\":{execution_wall_ns},\"execution_cpu_ns\":{execution_cpu_ns},\"result\":{},\"gc_execution\":null,\"gc_warmup\":null,\"gc_counter\":\"unavailable\",\"gc_reason\":\"bex_engine exposes no collection counter\",\"workers\":{workers},\"execution_threads\":1}}",
        quote(&result)
    ));

    // Drain is state release and engine shutdown, which awaits the
    // recording's writes when one is running.
    let start = Instant::now();
    drop(state);
    engine.record_process_exit(bex_engine::ProcessStatus::Success);
    rt.block_on(engine.shutdown());
    let finished = engine.telemetry_result();
    let drain_ns = start.elapsed().as_nanos();
    let error = match &finished {
        Some(Err(error)) => Some(error.to_string()),
        _ => None,
    };
    emit(format!(
        "{{\"event\":\"drained\",\"drain_ns\":{drain_ns},\"gc_drain\":null,\"telemetry_ok\":{},\"delivery_loss\":0,\"telemetry_reason\":{}}}",
        error.is_none(),
        quote(error.as_deref().unwrap_or("bex_engine local recording"))
    ));
    if let Some(error) = error {
        fail(format!("telemetry recording failed: {error}"));
    }
}
