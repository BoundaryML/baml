//! Opt-in policy experiment. No production GC policy changes.
//! GC checkpoints are before host calls, identical for all candidate policies.
//! Run via `GC_POLICY` / `GC_WORKLOAD` and
//! `cargo test --test gc_policy_experiment --profile fasttest -- --ignored --nocapture`.
#![expect(
    clippy::print_stdout,
    reason = "Manual experiments emit machine-readable results to the runner"
)]
#![expect(
    clippy::cast_precision_loss,
    reason = "Diagnostic rates and MiB quantities are approximate floating-point measurements"
)]

use std::{collections::VecDeque, sync::Arc, time::Instant};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_heap::CollectionLevel;
use sys_native::SysOpsExt;

const MIB: usize = 1024 * 1024;
const SOURCE: &str = r#"
class BenchNode { value int }
function Identity(value: int) -> int { value }
function Make(value: int) -> BenchNode { BenchNode { value: value } }
function Churn(n: int) -> int {
    let i = 0;
    let total = 0;
    while (i < n) {
        let node = BenchNode { value: i };
        total = total + node.value;
        i = i + 1;
    }
    total
}
function Batch(n: int) -> BenchNode[] {
    let result: BenchNode[] = [];
    let i = 0;
    while (i < n) {
        result.push(BenchNode { value: i });
        i = i + 1;
    }
    result
}
function CheckBatch(nodes: BenchNode[]) -> int {
    let sum = 0;
    let i = 0;
    while (i < nodes.length()) { sum = sum + nodes[i].value; i = i + 1; }
    sum
}
function Payload(text: string) -> string { text }
function CheckPayload(text: string) -> int { text.length() }
function AsyncBatch(n: int) -> int {
    let nodes = Batch(n);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1n));
    CheckBatch(nodes)
}
"#;

struct Policy {
    name: String,
    budget: usize,
    major_goal: usize,
}

impl Policy {
    fn new(name: String, initial_old_bytes: usize) -> Self {
        let budget = match name.as_str() {
            "current" => usize::MAX,
            "fixed8" => 8 * MIB,
            "fixed32" | "adaptive" | "full32" | "full_live" => 32 * MIB,
            "fixed128" => 128 * MIB,
            _ => panic!("unknown policy: {name}"),
        };
        Self {
            name,
            budget,
            major_goal: initial_old_bytes + initial_old_bytes.max(32 * MIB),
        }
    }

    fn decide(&self, nursery: usize, older: usize) -> Option<CollectionLevel> {
        if self.name == "current" {
            return None;
        }
        if self.name == "full32" || self.name == "full_live" {
            return (nursery >= self.budget).then_some(CollectionLevel::Major);
        }
        if older >= self.major_goal {
            return Some(CollectionLevel::Major);
        }
        (nursery >= self.budget).then_some(CollectionLevel::Minor)
    }

    fn after_gc(
        &mut self,
        level: CollectionLevel,
        nursery_before: usize,
        young_survivors: usize,
        older_after: usize,
    ) {
        if level == CollectionLevel::Major {
            self.major_goal = older_after + older_after.max(32 * MIB);
            if self.name == "full_live" {
                self.budget = older_after.max(32 * MIB);
            }
        } else if self.name == "adaptive" && nursery_before > 0 {
            // Deliberately experimental, easy-to-change survival policy.
            // Compare live Gen0 survivors with all reserved Gen0 storage.
            if young_survivors * 2 >= nursery_before {
                self.budget = (self.budget * 2).min(128 * MIB);
            } else if young_survivors * 10 < nursery_before {
                self.budget = (self.budget / 2).max(8 * MIB);
            }
        }
    }
}

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next())
        .suppress_internal_profile()
        .build()
}

#[cfg(target_os = "macos")]
#[allow(deprecated)]
#[expect(
    unsafe_code,
    reason = "Mach task statistics require an FFI call with initialized output storage"
)]
fn rss_bytes() -> Option<usize> {
    // Read-only task statistics; no allocation sampling subprocess in timings.
    unsafe {
        let mut info: libc::mach_task_basic_info = std::mem::zeroed();
        let mut count = libc::MACH_TASK_BASIC_INFO_COUNT;
        let result = libc::task_info(
            libc::mach_task_self(),
            libc::MACH_TASK_BASIC_INFO,
            (&raw mut info).cast(),
            &raw mut count,
        );
        if result == 0 {
            usize::try_from(info.resident_size).ok()
        } else {
            None
        }
    }
}

#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<usize> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    line.split_whitespace()
        .nth(1)?
        .parse::<usize>()
        .ok()?
        .checked_mul(1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn rss_bytes() -> Option<usize> {
    None
}

fn rss_mib(value: Option<usize>) -> Option<f64> {
    value.map(|bytes| bytes as f64 / MIB as f64)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "The rounded rank is nonnegative and in bounds after validating the fraction"
)]
fn percentile(values: &[f64], fraction: f64) -> f64 {
    assert!((0.0..=1.0).contains(&fraction));
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

#[test]
#[ignore = "manual performance experiment, not a CI assertion"]
fn compare_gc_policy() {
    let policy_name = std::env::var("GC_POLICY").unwrap_or_else(|_| "fixed32".into());
    let workload = std::env::var("GC_WORKLOAD").unwrap_or_else(|_| "tiny".into());
    let program = baml_db::testing::compile_source(SOURCE);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let engine = Arc::new(BexEngine::new(program, Arc::new(sys_native::SysOps::native()), vec![]).unwrap());
        let slot_size = std::mem::size_of::<bex_vm_types::Object>();
        let initial_gc = engine.collect_garbage(CollectionLevel::Major).await;
        let mut policy = Policy::new(policy_name, initial_gc.live_count * slot_size);
        let (default_calls, default_n, function, copy_result): (usize, usize, &str, bool) = match workload.as_str() {
            "scalar" => (12_000, 123, "Identity", true),
            "tiny" => (12_000, 123, "Make", true),
            "churn" | "cache" => (1_024, 4_096, "Churn", true),
            "retained" | "burst" | "burst_idle" => (1_024, 2_048, "Batch", false),
            "payload" => (2_048, 65_536, "Payload", false),
            _ => panic!("unknown workload: {workload}"),
        };
        let calls = setting("GC_CALLS", default_calls);
        let n_size = setting("GC_N", default_n);
        let n = i64::try_from(n_size).expect("GC_N must fit in a BAML int");
        let retain = setting("GC_RETAIN", 256);
        let warmup = setting("GC_WARMUP", 32);
        assert!(calls > 0, "GC_CALLS must be positive");
        let cache_n = i64::try_from(setting("GC_CACHE_N", 262_144)).expect("GC_CACHE_N must fit in a BAML int");
        let make_args = || if workload == "payload" {
            vec![BexExternalValue::String("x".repeat(n_size).into())]
        } else { vec![BexExternalValue::Int(n)] };
        for _ in 0..warmup {
            engine.call_function(function, make_args(), context(), copy_result).await.unwrap();
        }
        let cache = if workload == "cache" {
            Some(engine.call_function("Batch", vec![BexExternalValue::Int(cache_n)], context(), false).await.unwrap())
        } else { None };
        let warmup_gc = engine.collect_garbage(CollectionLevel::Major).await;
        // A permanent cache is part of the survivor baseline before timings.
        policy.after_gc(CollectionLevel::Major, 0, 0, warmup_gc.live_count * slot_size);
        let mut cycles = Vec::new();
        let mut kept = VecDeque::new();
        let mut gc_ms = vec![];
        let mut call_ms = vec![];
        let mut minor_count = 0;
        let mut major_count = 0;
        let initial_rss = rss_bytes();
        let mut peak_rss = initial_rss;
        let mut peak_slots = engine.heap_stats().runtime_objects;
        let mut snapshots = vec![];
        let start = Instant::now();
        for call in 0..calls {
            if workload == "burst" && call == calls / 2 { kept.clear(); }
            let stats = engine.heap_stats();
            let nursery_slots = stats.tlab_chunks * engine.heap().tlab_size();
            let nursery_bytes = nursery_slots * slot_size;
            let older_bytes = stats.runtime_objects.saturating_sub(nursery_slots) * slot_size;
            let call_start = Instant::now();
            if let Some(level) = policy.decide(nursery_bytes, older_bytes) {
                let gc_start = Instant::now();
                let result = engine.collect_garbage(level).await;
                gc_ms.push(gc_start.elapsed().as_secs_f64() * 1000.0);
                cycles.push((call, start.elapsed().as_secs_f64(), result.clone(), nursery_bytes, policy.budget));
                match level {
                    CollectionLevel::Minor => minor_count += 1,
                    CollectionLevel::Major => major_count += 1,
                }
                policy.after_gc(level, nursery_bytes, result.promoted_to_gen1 * slot_size,
                    engine.heap_stats().runtime_objects * slot_size);
            }
            let result = engine.call_function(function, make_args(), context(), copy_result).await.unwrap();
            match workload.as_str() {
                "scalar" => assert_eq!(result, BexExternalValue::Int(n)),
                "tiny" => match result {
                    BexExternalValue::Instance { fields, .. } => assert_eq!(fields["value"], BexExternalValue::Int(n)),
                    _ => panic!("wrong return shape"),
                },
                "churn" | "cache" => assert_eq!(result, BexExternalValue::Int(n * (n-1) / 2)),
                _ => {
                    assert!(matches!(result, BexExternalValue::Handle(_)));
                    if workload == "retained" || workload == "payload" || workload == "burst_idle" || call < calls / 2 {
                        kept.push_back(result);
                        if kept.len() > retain { kept.pop_front(); }
                    }
                }
            }
            call_ms.push(call_start.elapsed().as_secs_f64() * 1000.0);
            peak_slots = peak_slots.max(engine.heap_stats().runtime_objects);
            if call % 32 == 0 || call == calls-1 {
                peak_rss = peak_rss.max(rss_bytes());
            }
            if (call+1) % (calls/10).max(1) == 0 || call+1 == calls {
                snapshots.push(serde_json::json!({"calls":call+1,
                    "elapsed_seconds":start.elapsed().as_secs_f64(),
                    "slot_mib":engine.heap_stats().runtime_objects as f64*slot_size as f64/MIB as f64,
                    "rss_mib":rss_mib(rss_bytes()), "budget_mib":policy.budget/MIB}));
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        let end_slots = engine.heap_stats().runtime_objects;
        let end_rss = rss_bytes();
        let idle_observation = if workload == "burst_idle" {
            kept.clear();
            let before = engine.heap_stats().runtime_objects;
            let before_rss = rss_bytes();
            let idle_ms = setting("GC_IDLE_MS", 1000);
            // Keep the executor alive, but make no engine call or forced GC.
            tokio::time::sleep(std::time::Duration::from_millis(idle_ms as u64)).await;
            Some(serde_json::json!({"idle_ms":idle_ms,
                "before_slots":before,"after_slots":engine.heap_stats().runtime_objects,
                "before_rss_mib":rss_mib(before_rss),"after_rss_mib":rss_mib(rss_bytes())}))
        } else { None };
        // Verify retained native handles after a moving collection, outside timings.
        let validation_gc = engine.collect_garbage(CollectionLevel::Major).await;
        if let Some(value) = &cache {
            let checked = engine.call_function("CheckBatch", vec![value.clone()], context(), true).await.unwrap();
            assert_eq!(checked, BexExternalValue::Int(cache_n * (cache_n-1) / 2));
        }
        for value in &kept {
            let checked = engine.call_function(if workload == "payload" {"CheckPayload"} else {"CheckBatch"}, vec![value.clone()], context(), true).await.unwrap();
            assert_eq!(checked, BexExternalValue::Int(if workload == "payload" { n } else {n*(n-1)/2}));
        }
        if let Ok(path) = std::env::var("GC_TRACE") {
            let events: Vec<_> = cycles.iter().map(|(call, at, stats, nursery, budget)| serde_json::json!({
                "call":call, "at_seconds":at, "trigger":"policy_before_host_call", "level":format!("{:?}",stats.level),
                "nursery_reserved_bytes":nursery, "budget_bytes":budget, "profile":profile_json(stats),
                "copied_objects":stats.live_count, "reclaimed_slots":stats.collected_count,
                "promoted_gen1":stats.promoted_to_gen1, "promoted_gen2":stats.promoted_to_gen2,
            })).collect();
            std::fs::write(path, serde_json::to_vec_pretty(&events).unwrap()).unwrap();
        }
        println!("GC_POLICY_RESULT {}", serde_json::json!({
            "policy":policy.name, "workload":workload, "calls":calls, "n":n, "retain":retain, "warmup":warmup, "slot_bytes":slot_size,
            "elapsed_seconds":elapsed, "calls_per_second":calls as f64/elapsed,
            "gc_ms_total":gc_ms.iter().sum::<f64>(), "gc_ms_p50":percentile(&gc_ms,0.5),
            "gc_ms_p95":percentile(&gc_ms,0.95), "gc_ms_max":percentile(&gc_ms,1.0),
            "call_ms_p50":percentile(&call_ms,0.5), "call_ms_p95":percentile(&call_ms,0.95),
            "call_ms_p99":percentile(&call_ms,0.99), "minor_count":minor_count, "major_count":major_count,
            "call_ms_max":percentile(&call_ms,1.0),
            "peak_slot_mib":peak_slots as f64*slot_size as f64/MIB as f64,
            "end_slot_mib":end_slots as f64*slot_size as f64/MIB as f64,
            "initial_rss_mib":rss_mib(initial_rss), "peak_sampled_rss_mib":rss_mib(peak_rss),
            "end_rss_mib":rss_mib(end_rss), "retained_handles_verified":kept.len(), "snapshots":snapshots,
            "cache_objects":if cache.is_some() {cache_n} else {0},
            "cache_verified":cache.is_some(), "idle_observation":idle_observation,
            "validation_gc_live_objects":validation_gc.live_count,
            "cycle_coverage":"harness-requested collections only; automatic cycles require engine tracing",
        }));
        kept.clear();
        drop(cache);
        engine.shutdown().await;
    });
}

fn setting(key: &str, default: usize) -> usize {
    std::env::var(key)
        .map(|s| s.parse().expect("invalid integer setting"))
        .unwrap_or(default)
}

#[cfg(not(feature = "gc_profiling"))]
fn profile_json(_: &bex_heap::GcStats) -> serde_json::Value {
    serde_json::Value::Null
}

#[cfg(feature = "gc_profiling")]
fn profile_json(stats: &bex_heap::GcStats) -> serde_json::Value {
    let p = &stats.profile;
    serde_json::json!({
        "before_generations":p.before.generation_slots, "after_generations":p.after.generation_slots,
        "before_capacity":p.before.capacity_slots, "after_capacity":p.after.capacity_slots,
        "actual_new_objects":p.before.new_objects, "roots":p.roots,
        "prepare_ms":p.prepare.as_secs_f64()*1000.0, "trace_ms":p.trace.as_secs_f64()*1000.0,
        "keepalive_ms":p.keepalive.as_secs_f64()*1000.0, "fixup_ms":p.fixup.as_secs_f64()*1000.0,
        "reclaim_ms":p.reclaim.as_secs_f64()*1000.0, "bookkeeping_ms":p.bookkeeping.as_secs_f64()*1000.0,
        "heap_total_ms":p.heap_total.as_secs_f64()*1000.0,
        "park_wait_ms":p.park_wait.as_secs_f64()*1000.0, "root_scan_ms":p.root_scan.as_secs_f64()*1000.0,
        "holder_fixup_ms":p.holder_fixup.as_secs_f64()*1000.0, "pause_ms":p.pause.as_secs_f64()*1000.0,
        "post_gc_ms":p.post_gc.as_secs_f64()*1000.0, "total_ms":p.total.as_secs_f64()*1000.0,
    })
}

#[test]
#[ignore = "manual concurrent GC profile, not a CI performance assertion"]
fn profile_concurrent_gc() {
    let program = baml_db::testing::compile_source(SOURCE);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let engine = Arc::new(
            BexEngine::new(program, Arc::new(sys_native::SysOps::native()), vec![]).unwrap(),
        );
        let workers = setting("GC_WORKERS", 8);
        let calls = setting("GC_CALLS", 200);
        let n = i64::try_from(setting("GC_N", 512)).expect("GC_N must fit in a BAML int");
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let gc_engine = engine.clone();
        let gc_done = done.clone();
        let start = Instant::now();
        let collector = tokio::spawn(async move {
            let mut events = Vec::new();
            while !gc_done.load(std::sync::atomic::Ordering::Relaxed) {
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                let stats = gc_engine.collect_garbage(CollectionLevel::Major).await;
                events.push(
                    serde_json::json!({"at_seconds":start.elapsed().as_secs_f64(),
                    "trigger":"concurrent_periodic_request", "profile":profile_json(&stats)}),
                );
            }
            events
        });
        let mut tasks = Vec::new();
        for _ in 0..workers {
            let engine = engine.clone();
            tasks.push(tokio::spawn(async move {
                let mut latencies = Vec::new();
                for _ in 0..calls {
                    let start = Instant::now();
                    let value = engine
                        .call_function(
                            "AsyncBatch",
                            vec![BexExternalValue::Int(n)],
                            context(),
                            true,
                        )
                        .await
                        .unwrap();
                    latencies.push(start.elapsed().as_secs_f64() * 1000.0);
                    assert_eq!(value, BexExternalValue::Int(n * (n - 1) / 2));
                }
                latencies
            }));
        }
        let mut latencies = Vec::new();
        tokio::time::timeout(std::time::Duration::from_secs(120), async {
            for task in tasks {
                latencies.extend(task.await.unwrap());
            }
        })
        .await
        .expect("concurrent GC timed out");
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        let events = collector.await.unwrap();
        if let Ok(path) = std::env::var("GC_TRACE") {
            std::fs::write(path, serde_json::to_vec_pretty(&events).unwrap()).unwrap();
        }
        println!(
            "GC_CONCURRENT_RESULT {}",
            serde_json::json!({"workers":workers,"calls":workers*calls,
            "elapsed_seconds":start.elapsed().as_secs_f64(),"explicit_collections":events.len(),
            "call_p50_ms":percentile(&latencies,0.5),"call_p99_ms":percentile(&latencies,0.99)})
        );
        engine.shutdown().await;
    });
}
