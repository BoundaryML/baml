//! Fixed-work recording benchmark, run by `tools/btel_query_bench.py`.
//!
//! `MODE WORKLOAD ROOTS PROJECT [PAYLOAD_BYTES] [TARGET_BYTES]`. Records into
//! `PROJECT/.baml/btel` exactly like `baml run`, so `baml query --from PROJECT`
//! reads the result. Uses only engine APIs that also exist on the base commit,
//! so the same file measures the recorder before and after this branch.
//! Adapted from the archived SQLite-recorder experiment's harness.
#![allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    unsafe_code
)]
use std::{fmt::Write as _, num::NonZeroUsize, path::Path, sync::Arc, time::Instant};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_recorder::RecordingConfig;
use sys_native::SysOpsExt;

fn files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(files(&path));
            } else {
                result.push(path);
            }
        }
    }
    result
}

/// Process CPU seconds and peak RSS bytes (Linux `getrusage`).
#[cfg(unix)]
fn usage() -> (f64, u64) {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage fills the provided struct for RUSAGE_SELF.
    assert_eq!(
        unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) },
        0
    );
    // SAFETY: initialized by the successful call above.
    let usage = unsafe { usage.assume_init() };
    let cpu = usage.ru_utime.tv_sec as f64
        + usage.ru_stime.tv_sec as f64
        + (usage.ru_utime.tv_usec + usage.ru_stime.tv_usec) as f64 / 1e6;
    (cpu, usage.ru_maxrss as u64 * 1024)
}

/// Not measured without `getrusage`.
#[cfg(not(unix))]
fn usage() -> (f64, u64) {
    (0.0, 0)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        (5..=7).contains(&args.len()),
        "MODE WORKLOAD ROOTS PROJECT [PAYLOAD_BYTES] [TARGET_BYTES]"
    );
    let (mode, workload) = (args[1].as_str(), args[2].as_str());
    let roots: usize = args[3].parse().unwrap();
    let project = Path::new(&args[4]);
    let payload_bytes: usize = args.get(5).map_or(64 * 1024, |a| a.parse().unwrap());
    let mut config = RecordingConfig::default();
    if let Some(target) = args.get(6) {
        config.target_bytes = NonZeroUsize::new(target.parse().unwrap()).unwrap();
    }
    assert_eq!(
        std::env::var("BAML_TELEMETRY").unwrap_or_default(),
        if mode == "off" { "off" } else { "medium" }
    );
    let body = match workload {
        "tiny" => "n".to_owned(),
        "dense" => {
            "let i = 0; let sum = 0; while (i < 1000) { sum = sum + leaf(i); i = i + 1; } sum"
                .into()
        }
        "spawn" => {
            let mut body = String::new();
            for i in 0..32 {
                write!(body, "let t{i} = spawn {{ leaf(n) }}; ").unwrap();
            }
            body.push_str("let sum = 0; ");
            for i in 0..32 {
                write!(body, "sum = sum + (await t{i}); ").unwrap();
            }
            body.push_str("sum");
            body
        }
        "capture-repeat" | "capture-unique" => "capture(n, payload)".into(),
        // 100 throws per root, each unwinding three frames before a catch.
        "throw" => {
            "let i = 0; let sum = 0; while (i < 100) { sum = sum + (t1(i) catch (e) { _ => 1 }); i = i + 1; } sum"
                .into()
        }
        // Startup with a sizable function dictionary; main calls a few.
        "dictionary" => "d0(n) + d1(n) + d2(n)".into(),
        _ => panic!("unknown workload"),
    };
    // Only the new workloads add functions; the others compile exactly the
    // program earlier versions of this harness did.
    let mut extra = String::new();
    if workload == "throw" {
        extra.push_str(
            "function t3(n: int) -> int throws string { if (n >= 0) { throw \"boom\" } n } \
             function t2(n: int) -> int throws string { t3(n) + 1 } \
             function t1(n: int) -> int throws string { t2(n) + 1 } ",
        );
    }
    if workload == "dictionary" {
        for i in 0..2000 {
            write!(
                extra,
                "function d{i}(n: int) -> int {{ let a = n + {i}; let b = a * 2; if (b > 10) {{ b - 1 }} else {{ a + 1 }} }} "
            )
            .unwrap();
        }
    }
    let source = format!(
        "{extra}function leaf(n: int) -> int {{ n }} function capture(n: int, payload: string) -> int {{ n }} function main(n: int, payload: string) -> int {{ {body} }}"
    );
    let mut program = baml_db::testing::compile_source(&source);
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(function) = object
            && function.name.rsplit('.').next() == Some("capture")
        {
            // Real VM input/output capture without network/LLM noise.
            function.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                client: "benchmark".into(),
            });
        }
    }
    let startup = Instant::now();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            // Monotonic by default so results compare with earlier reports;
            // `BTEL_BENCH_CLOCK=auto` measures the production default.
            if std::env::var("BTEL_BENCH_CLOCK").as_deref() == Ok("auto") {
                btel_clock::ClockMode::Auto
            } else {
                btel_clock::ClockMode::Monotonic
            },
            TelemetryRecording::local_files(project, config),
        )
        .unwrap(),
    );
    let startup_ms = startup.elapsed().as_secs_f64() * 1000.0;
    let payload = BexExternalValue::String(
        if workload.starts_with("capture") {
            "x".repeat(payload_bytes)
        } else {
            String::new()
        }
        .into(),
    );
    let mut latency = Vec::with_capacity(roots);
    let (cpu_start, rss_start) = usage();
    let start = Instant::now();
    for i in 0..roots {
        let n = if workload == "capture-unique" {
            i64::try_from(i).unwrap()
        } else {
            7
        };
        let call = Instant::now();
        let result = engine
            .call_function(
                "main",
                vec![BexExternalValue::Int(n), payload.clone()],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .unwrap();
        latency.push(call.elapsed().as_secs_f64() * 1e6);
        let expected = match workload {
            "dense" => 499_500,
            "spawn" => 32 * n,
            "throw" => 100,
            // d_i(7) = 2 * (7 + i) - 1 for these small i.
            "dictionary" => 13 + 15 + 17,
            _ => n,
        };
        assert_eq!(result, BexExternalValue::Int(expected));
        if (i + 1) % 8 == 0 {
            engine
                .collect_garbage(bex_heap::CollectionLevel::Major)
                .await;
        }
    }
    let execution_s = start.elapsed().as_secs_f64();
    let (cpu_end, _) = usage();
    let drain = Instant::now();
    engine.shutdown().await;
    let drain_ms = drain.elapsed().as_secs_f64() * 1000.0;
    let (cpu_after_drain, peak_rss_bytes) = usage();
    assert_eq!(
        engine.telemetry_result(),
        if mode == "off" { None } else { Some(Ok(())) }
    );
    let btel = project.join(".baml/btel");
    let recorded = files(&btel.join("recordings"));
    let blobs = files(&btel.join("cas"));
    let size = |paths: &[std::path::PathBuf]| -> u64 {
        paths
            .iter()
            .map(|p| p.metadata().map_or(0, |m| m.len()))
            .sum()
    };
    latency.sort_by(f64::total_cmp);
    println!(
        "{}",
        serde_json::json!({
            "mode": mode, "workload": workload, "roots": roots, "payload_bytes": payload_bytes,
            "startup_ms": startup_ms, "execution_s": execution_s,
            "roots_per_s": roots as f64 / execution_s,
            "call_p50_us": latency[roots / 2], "call_p95_us": latency[(roots - 1) * 95 / 100],
            "call_p99_us": latency[(roots - 1) * 99 / 100],
            "cpu_s": cpu_end - cpu_start, "cpu_including_drain_s": cpu_after_drain - cpu_start,
            "rss_before_execution_bytes": rss_start, "peak_rss_bytes": peak_rss_bytes,
            "drain_ms": drain_ms,
            "recording_files": recorded.iter().filter(|p| p.extension().is_some_and(|e| e == "btel")).count(),
            "recording_bytes": size(&recorded),
            "cas_blobs": blobs.len(), "cas_bytes": size(&blobs),
        })
    );
}
