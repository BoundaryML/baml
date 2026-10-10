// Measurement glue for a natively compiled BAML program. baml_native.rs writes
// this file as src/main.rs of the emitted Cargo project; it is the only place
// that names the generated entry points. Standard library only.
//
// Required library API (crate `baml_native`, emitted by `baml-cli __emit-rust`):
//   pub fn user_prepare(raw: S) -> Result<State, E>   where S: From<String>
//   pub fn user_run(state: State) -> Result<R, E>     where State: Clone,
//                                                           R: std::fmt::Display,
//                                                           E: std::fmt::Debug
// Protocol: run ARTIFACT INPUT JOBS WARMUP SINK OUTPUT (machinery/protocol.txt).
use std::{io::Write, time::Instant};

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
    if a.len() != 8 || a[1] != "run" {
        fail("usage: run ARTIFACT INPUT JOBS WARMUP SINK OUTPUT".into());
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
    // Generated code has no telemetry subsystem: any policy other than off is
    // an unsupported feature and must fail explicitly rather than report success.
    let telemetry_ok = telemetry == "off";

    let origin = Instant::now();
    let raw = std::fs::read_to_string(&a[3]).unwrap_or_else(|e| fail(format!("INPUT: {e}")));
    let state = match baml_native::user_prepare(raw.into()) {
        Ok(state) => state,
        Err(thrown) => fail(format!("prepare threw: {thrown:?}")),
    };
    // The executable image is the program; the OS loaded it before main.
    emit(format!(
        "{{\"event\":\"ready\",\"load_ns\":null,\"load_reason\":\"native image, loaded before main\",\"setup_ns\":{}}}",
        origin.elapsed().as_nanos()
    ));

    for _ in 0..warmup {
        match baml_native::user_run(std::hint::black_box(state.clone())) {
            Ok(value) => drop(std::hint::black_box(value)),
            Err(thrown) => fail(format!("run threw during warmup: {thrown:?}")),
        }
    }
    let start = Instant::now();
    let cpu = cpu_ns();
    let mut last = None;
    for _ in 0..jobs {
        match baml_native::user_run(std::hint::black_box(state.clone())) {
            Ok(value) => last = Some(std::hint::black_box(value)),
            Err(thrown) => fail(format!("run threw: {thrown:?}")),
        }
    }
    let execution_cpu_ns = cpu_ns() - cpu;
    let execution_wall_ns = start.elapsed().as_nanos();
    let result = last.expect("jobs is positive").to_string();
    emit(format!(
        "{{\"event\":\"result\",\"jobs\":{jobs},\"execution_wall_ns\":{execution_wall_ns},\"execution_cpu_ns\":{execution_cpu_ns},\"result\":{},\"gc_execution\":null,\"gc_warmup\":null,\"gc_counter\":\"unavailable\",\"gc_reason\":\"reference counting, no collector\",\"workers\":{workers},\"execution_threads\":1}}",
        quote(&result)
    ));

    // Drain is state reclamation; nothing else is pending in a native program.
    let start = Instant::now();
    drop(state);
    let drain_ns = start.elapsed().as_nanos();
    emit(format!(
        "{{\"event\":\"drained\",\"drain_ns\":{drain_ns},\"gc_drain\":null,\"telemetry_ok\":{telemetry_ok},\"delivery_loss\":0,\"telemetry_reason\":\"no telemetry subsystem in generated code; off policy only\"}}"
    ));
    if !telemetry_ok {
        fail(format!(
            "BAML_TELEMETRY={telemetry:?} is unsupported by native code; only off is implemented"
        ));
    }
}
