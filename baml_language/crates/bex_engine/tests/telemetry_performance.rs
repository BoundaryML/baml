//! Opt-in release-mode diagnostics, not timing assertions in CI.
//! Each scenario runs in a fresh process so telemetry environment settings never
//! change underneath VM, processor, or HTTP threads.
#![cfg(not(target_arch = "wasm32"))]
#![expect(clippy::print_stdout, reason = "machine-readable benchmark results")]

use std::{
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use bex_engine::{BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_bcs::delivery::DeliveryConfig;
use btel_publisher::RecordingConfig;
use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;
#[path = "support/telemetry.rs"]
mod telemetry;

const SOURCE: &str =
    include_str!("../../baml_tests/baml_src/ns_telemetry_performance/telemetry_performance.baml");
const PREFIX: &str = "TELEMETRY_PERF ";
const WARMUP: usize = 20;

fn setting(name: &str, default: usize) -> usize {
    std::env::var(name).map_or(default, |value| value.parse().expect("numeric setting"))
}

#[expect(
    clippy::assertions_on_constants,
    reason = "reject accidental debug measurements at runtime"
)]
fn require_release() {
    assert!(!cfg!(debug_assertions), "performance requires --release");
}

#[test]
#[ignore = "run explicitly with --release --ignored --exact telemetry_performance --nocapture"]
fn telemetry_performance() {
    require_release();
    let trials = setting("BTEL_PERF_TRIALS", 5);
    assert!(trials > 0);
    let modes = ["off", "local", "cloud", "cloud-slow", "cloud-fail"];
    for workload in ["cpu", "capture-repeat", "capture-unique"] {
        for pace_ms in [0, 2] {
            for trial in 0..trials {
                for index in 0..modes.len() {
                    let mode = modes[(index + trial) % modes.len()];
                    let output = Command::new(std::env::current_exe().unwrap())
                        .args([
                            "--ignored",
                            "--exact",
                            "telemetry_performance_child",
                            "--nocapture",
                        ])
                        .env("BAML_TELEMETRY", if mode == "off" { "off" } else { "auto" })
                        .env("BTEL_PERF_MODE", mode)
                        .env("BTEL_PERF_WORKLOAD", workload)
                        .env("BTEL_PERF_PACE_MS", pace_ms.to_string())
                        .env("BTEL_PERF_TRIAL", trial.to_string())
                        .output()
                        .unwrap();
                    assert!(
                        output.status.success(),
                        "{mode}/{workload}: {}\n{}",
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr)
                    );
                    let stdout = String::from_utf8(output.stdout).unwrap();
                    let row = stdout
                        .lines()
                        .find(|line| line.starts_with(PREFIX))
                        .unwrap();
                    println!("{row}");
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "subprocess entry point for telemetry_performance"]
async fn telemetry_performance_child() {
    require_release();
    let mode = std::env::var("BTEL_PERF_MODE").expect("run the parent performance test");
    let workload = std::env::var("BTEL_PERF_WORKLOAD").unwrap();
    let iterations = setting("BTEL_PERF_ITERATIONS", 500);
    assert!((100..=1_000_000).contains(&iterations));
    let pace = Duration::from_millis(u64::try_from(setting("BTEL_PERF_PACE_MS", 0)).unwrap());
    let warmup_pace = pace.max(Duration::from_millis(2));
    let server = MockServer::builder()
        .disable_request_recording()
        .start()
        .await;
    let prepares = Arc::new(AtomicUsize::new(0));
    let candidates = Arc::new(AtomicUsize::new(0));
    let puts = Arc::new(AtomicUsize::new(0));
    let upload_bytes = Arc::new(AtomicUsize::new(0));
    let prepare_count = Arc::clone(&prepares);
    let candidate_count = Arc::clone(&candidates);
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            prepare_count.fetch_add(1, Ordering::Relaxed);
            let response = cloud_protocol::response(request, &base, &[]);
            candidate_count.fetch_add(response.cas.len(), Ordering::Relaxed);
            ResponseTemplate::new(200).set_body_json(response)
        })
        .mount(&server)
        .await;
    let put_count = Arc::clone(&puts);
    let byte_count = Arc::clone(&upload_bytes);
    let delay = if mode == "cloud-slow" {
        Duration::from_millis(50)
    } else {
        Duration::ZERO
    };
    let status = if mode == "cloud-fail" { 503 } else { 200 };
    Mock::given(method("PUT"))
        .respond_with(move |request: &Request| {
            put_count.fetch_add(1, Ordering::Relaxed);
            byte_count.fetch_add(request.body.len(), Ordering::Relaxed);
            ResponseTemplate::new(status).set_delay(delay)
        })
        .mount(&server)
        .await;

    let root = tempfile::tempdir().unwrap();
    let recording = if mode.starts_with("cloud") {
        TelemetryRecording::cloud(
            RecordingConfig::default(),
            btel_bcs::PublisherConfig::default(),
            DeliveryConfig {
                prepare_base_url: server.uri(),
                allow_http: true,
                ..DeliveryConfig::default()
            },
        )
    } else {
        TelemetryRecording::local_files_in(root.path(), RecordingConfig::default())
    };
    let engine = telemetry::recording_engine(SOURCE, &["capture"], recording);
    assert_eq!(engine.telemetry_recording_id().is_none(), mode == "off");
    let payload = BexExternalValue::String("x".repeat(64 * 1024).into());
    let mut samples = Vec::with_capacity(iterations);
    let mut active_samples = Vec::with_capacity(iterations);
    let mut first_error_call = None;
    let mut measurement_start = None;
    let mut delivery_at_start = Value::Null;
    let mut scheduled = tokio::time::Instant::now();
    for index in 0..WARMUP + iterations {
        if index < WARMUP || !pace.is_zero() {
            tokio::time::sleep_until(scheduled).await;
        }
        if index == WARMUP {
            delivery_at_start = counters(&prepares, &candidates, &puts, &upload_bytes);
            measurement_start = Some(Instant::now());
        }
        let seed = if workload == "capture-repeat" {
            1
        } else {
            i64::try_from(index).unwrap()
        };
        let (function, args, expected) = if workload == "cpu" {
            ("cpu", vec![BexExternalValue::Int(seed)], seed + 1000)
        } else {
            (
                "capture",
                vec![BexExternalValue::Int(seed), payload.clone()],
                seed,
            )
        };
        let context = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let active_before = !matches!(engine.telemetry_result(), Some(Err(_)));
        let start = Instant::now();
        let result = engine
            .call_function(function, args, context, true)
            .await
            .unwrap();
        let elapsed = start.elapsed();
        assert_eq!(result, BexExternalValue::Int(expected));
        let active_after = !matches!(engine.telemetry_result(), Some(Err(_)));
        if !active_after && first_error_call.is_none() {
            first_error_call = Some(index + 1);
        }
        if index >= WARMUP {
            samples.push(elapsed);
            if active_before && active_after {
                active_samples.push(elapsed);
            }
        }
        // Do not catch up a missed deadline with an artificial burst.
        let interval = if index + 1 < WARMUP {
            warmup_pace
        } else {
            pace
        };
        scheduled = (scheduled + interval).max(tokio::time::Instant::now());
    }
    let wall = measurement_start.unwrap().elapsed();
    let delivery_at_end = counters(&prepares, &candidates, &puts, &upload_bytes);
    let error_before_shutdown = engine
        .telemetry_result()
        .and_then(Result::err)
        .map(|error| error.to_string());
    let shutdown_start = Instant::now();
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .unwrap();
    let shutdown = shutdown_start.elapsed();
    let result = engine.telemetry_result();
    let status = match result {
        None if mode == "off" => "off".to_owned(),
        Some(Ok(())) => "ok".to_owned(),
        Some(Err(error)) => error.to_string(),
        None => panic!("shutdown must settle telemetry"),
    };
    samples.sort_unstable();
    active_samples.sort_unstable();
    println!(
        "{PREFIX}{}",
        json!({
            "mode": mode,
            "workload": workload,
            "trial": setting("BTEL_PERF_TRIAL", 0),
            "pace_ms": pace.as_millis(),
            "warmup": WARMUP,
            "warmup_pace_ms": warmup_pace.as_millis(),
            "iterations": iterations,
            "latency_us": summary(&samples),
            "pre_error_latency_us": summary(&active_samples),
            "pre_error_samples": active_samples.len(),
            "calls_per_second": f64::from(u32::try_from(iterations).unwrap()) / wall.as_secs_f64(),
            "first_error_call": first_error_call,
            "error_before_shutdown": error_before_shutdown,
            "status": status,
            "shutdown_ms": shutdown.as_secs_f64() * 1000.0,
            "delivery_at_start": delivery_at_start,
            "delivery_at_end": delivery_at_end,
            "delivery_after_shutdown": counters(&prepares, &candidates, &puts, &upload_bytes),
        })
    );
}

fn counters(
    prepares: &AtomicUsize,
    candidates: &AtomicUsize,
    puts: &AtomicUsize,
    bytes: &AtomicUsize,
) -> Value {
    json!({
        "prepare_requests": prepares.load(Ordering::Relaxed),
        "offered_candidates": candidates.load(Ordering::Relaxed),
        "put_requests": puts.load(Ordering::Relaxed),
        "uploaded_bytes": bytes.load(Ordering::Relaxed),
    })
}

fn summary(sorted: &[Duration]) -> Value {
    if sorted.is_empty() {
        return Value::Null;
    }
    let percentile = |percent: usize| {
        sorted[(sorted.len() * percent).div_ceil(100).saturating_sub(1)].as_secs_f64() * 1_000_000.0
    };
    json!({
        "p50": percentile(50),
        "p95": percentile(95),
        "p99": percentile(99),
        "max": sorted.last().unwrap().as_secs_f64() * 1_000_000.0,
    })
}
