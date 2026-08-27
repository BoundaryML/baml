//! Cache & instruction profiling for BEX VM using hardware performance counters.
//!
//! - **macOS**: Uses darwin-kperf (requires sudo)
//! - **Linux**: Use `perf stat` CLI wrapper (no code dep needed)
//!
//! Usage:
//!   cargo build --bench cache_profile --profile profiling
//!   sudo ./target/profiling/cache_profile [--output path.json]

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("cache_profile bench requires macOS with Apple Silicon.");
    eprintln!(
        "On Linux, use: perf stat -e instructions,cycles,L1-icache-load-misses,L1-dcache-load-misses,branch-misses ./target/profiling/cache_profile_workload"
    );
}

#[cfg(target_os = "macos")]
fn main() {
    macos_main::run();
}

#[cfg(target_os = "macos")]
mod macos_main {

    use std::{collections::BTreeMap, path::Path, sync::Arc};

    use baml_compiler2_emit::generate_project_bytecode;
    use baml_db::ProjectDatabase;
    use baml_tests::engine::TestDbExt;
    use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
    use darwin_kperf::Sampler;
    use darwin_kperf_events::Event;
    use sys_native::{CallId, SysOpsExt};

    // Split into 3 groups to avoid counter slot conflicts on M2.
    const EVENTS_A: [Event; 4] = [
        Event::FixedCycles,
        Event::FixedInstructions,
        Event::L1ICacheMissDemand,
        Event::L1DCacheMissLdNonspec,
    ];

    const EVENTS_B: [Event; 4] = [
        Event::FixedCycles,
        Event::FixedInstructions,
        Event::L1DCacheMissStNonspec,
        Event::L1DTlbMissNonspec,
    ];

    const EVENTS_C: [Event; 4] = [
        Event::FixedCycles,
        Event::FixedInstructions,
        Event::InstBranch,
        Event::BranchMispredNonspec,
    ];

    // ---- workload definitions ----

    struct Workload {
        name: &'static str,
        source: &'static str,
    }

    const WORKLOADS: &[Workload] = &[
        Workload {
            name: "loop_50m",
            source: r#"
function main() -> int {
  let sum = 0;
  for (let i = 0; i < 50000000; i += 1) { sum += i; };
  return sum;
}
"#,
        },
        Workload {
            name: "fib_35",
            source: r#"
function fib(n: int) -> int {
  if n <= 1 { n } else { fib(n - 1) + fib(n - 2) }
}
function main() -> int { fib(35) }
"#,
        },
        Workload {
            name: "class_create_5m",
            source: r#"
class Point { x: int; y: int; }
function main() -> int {
  let sum = 0;
  for (let i = 0; i < 5000000; i += 1) {
    let p = Point { x: i, y: i + 1 };
    sum += p.x + p.y;
  };
  return sum;
}
"#,
        },
        Workload {
            name: "closure_call_50k",
            source: r#"
function main() -> int {
  let add = (a: int, b: int) -> int { a + b };
  let sum = 0;
  for (let i = 0; i < 50000; i += 1) { sum += add(i, i + 1); };
  return sum;
}
"#,
        },
    ];

    // ---- counters ----

    #[derive(Clone)]
    struct Counters {
        cycles: u64,
        instructions: u64,
        l1i_miss: u64,
        l1d_miss_ld: u64,
        l1d_miss_st: u64,
        branches: u64,
        branch_mispred: u64,
        l1d_tlb_miss: u64,
    }

    impl Counters {
        fn print(&self, label: &str) {
            let ipc = self.instructions as f64 / self.cycles as f64;
            let l1d_total = self.l1d_miss_ld + self.l1d_miss_st;
            let instr = self.instructions as f64;

            println!();
            println!("╔══════════════════════════════════════════════════════════╗");
            println!("║  {:<54} ║", label);
            println!("╠══════════════════════════════════════════════════════════╣");
            println!("║  {:.<40} {:>12} ║", "Cycles", fmt(self.cycles));
            println!(
                "║  {:.<40} {:>12} ║",
                "Instructions",
                fmt(self.instructions)
            );
            println!("║  {:.<40} {:>12} ║", "IPC", format!("{:.3}", ipc));
            println!("╟──────────────────────────────────────────────────────────╢");
            println!(
                "║  {:.<40} {:>12} ║",
                "L1I cache misses",
                fmt(self.l1i_miss)
            );
            println!(
                "║  {:.<40} {:>12} ║",
                "  per 1k instr",
                format!("{:.3}", self.l1i_miss as f64 / instr * 1000.0)
            );
            println!("╟──────────────────────────────────────────────────────────╢");
            println!(
                "║  {:.<40} {:>12} ║",
                "L1D miss (load)",
                fmt(self.l1d_miss_ld)
            );
            println!(
                "║  {:.<40} {:>12} ║",
                "L1D miss (store)",
                fmt(self.l1d_miss_st)
            );
            println!("║  {:.<40} {:>12} ║", "L1D total", fmt(l1d_total));
            println!(
                "║  {:.<40} {:>12} ║",
                "  per 1k instr",
                format!("{:.3}", l1d_total as f64 / instr * 1000.0)
            );
            println!("╟──────────────────────────────────────────────────────────╢");
            println!("║  {:.<40} {:>12} ║", "Branches", fmt(self.branches));
            println!(
                "║  {:.<40} {:>12} ║",
                "Branch mispredictions",
                fmt(self.branch_mispred)
            );
            if self.branches > 0 {
                println!(
                    "║  {:.<40} {:>12} ║",
                    "  mispredict rate",
                    format!(
                        "{:.2}%",
                        self.branch_mispred as f64 / self.branches as f64 * 100.0
                    )
                );
            }
            println!("╚══════════════════════════════════════════════════════════╝");
        }

        fn to_json_fields(&self) -> String {
            let l1d_total = self.l1d_miss_ld + self.l1d_miss_st;
            format!(
                r#"    "cycles": {},
    "instructions": {},
    "ipc": {:.3},
    "l1i_miss": {},
    "l1d_miss_ld": {},
    "l1d_miss_st": {},
    "l1d_miss_total": {},
    "branches": {},
    "branch_mispred": {},
    "l1d_tlb_miss": {}"#,
                self.cycles,
                self.instructions,
                self.instructions as f64 / self.cycles as f64,
                self.l1i_miss,
                self.l1d_miss_ld,
                self.l1d_miss_st,
                l1d_total,
                self.branches,
                self.branch_mispred,
                self.l1d_tlb_miss,
            )
        }
    }

    fn fmt(n: u64) -> String {
        let s = n.to_string();
        let mut r = String::new();
        for (i, c) in s.chars().rev().enumerate() {
            if i > 0 && i % 3 == 0 {
                r.push(',');
            }
            r.push(c);
        }
        r.chars().rev().collect()
    }

    // ---- engine helpers ----

    fn compile_source(source: &str) -> (ProjectDatabase, BexEngine) {
        let mut db = ProjectDatabase::new();
        db.workspace(Path::new("."));
        db.file("bench.baml", source);
        let bytecode = generate_project_bytecode(&db).expect("compilation failed");
        let engine = BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), vec![])
            .expect("engine creation failed");
        (db, engine)
    }

    fn call_main(rt: &tokio::runtime::Runtime, engine: &Arc<BexEngine>) -> BexExternalValue {
        rt.block_on(engine.call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        ))
        .expect("execution failed")
    }

    // ---- profiling ----

    fn run_pass<const N: usize>(
        sampler: &Sampler,
        events: [Event; N],
        rt: &tokio::runtime::Runtime,
        engine: &Arc<BexEngine>,
        repeats: usize,
        label: &str,
    ) -> Vec<[u64; N]> {
        let mut thread = sampler.thread(events).unwrap_or_else(|e| {
            panic!("thread sampler failed for {label}: {e:?}");
        });
        thread.start().expect("start");
        let mut runs = Vec::new();
        for _ in 0..repeats {
            let before = thread.sample().expect("sample");
            let _ = call_main(rt, engine);
            let after = thread.sample().expect("sample");
            let mut deltas = [0u64; N];
            for i in 0..N {
                deltas[i] = after[i].wrapping_sub(before[i]);
            }
            runs.push(deltas);
        }
        thread.stop().expect("stop");
        runs.sort_by_key(|r| r[0]); // sort by cycles
        runs
    }

    fn profile_workload(sampler: &Sampler, workload: &Workload, repeats: usize) -> Counters {
        let (_db, engine) = compile_source(workload.source);
        let engine = Arc::new(engine);
        let rt = tokio::runtime::Runtime::new().expect("runtime creation failed");

        // Warmup
        let _ = call_main(&rt, &engine);

        let a = run_pass(sampler, EVENTS_A, &rt, &engine, repeats, "A");
        let b = run_pass(sampler, EVENTS_B, &rt, &engine, repeats, "B");
        let c = run_pass(sampler, EVENTS_C, &rt, &engine, repeats, "C");

        let a = &a[a.len() / 2];
        let b = &b[b.len() / 2];
        let c = &c[c.len() / 2];

        Counters {
            cycles: a[0],
            instructions: a[1],
            l1i_miss: a[2],
            l1d_miss_ld: a[3],
            l1d_miss_st: b[2],
            l1d_tlb_miss: b[3],
            branches: c[2],
            branch_mispred: c[3],
        }
    }

    // ---- main ----

    pub fn run() {
        let sampler = match Sampler::new() {
            Ok(sampler) => sampler,
            Err(e) => {
                eprintln!("Failed to create sampler: {e}");
                eprintln!();
                eprintln!("Run with: sudo ./target/profiling/cache_profile");
                if cfg!(debug_assertions) {
                    eprintln!("Skipping cache_profile in debug/test profile.");
                    return;
                }
                std::process::exit(1);
            }
        };

        let args: Vec<String> = std::env::args().collect();
        let output_path = args
            .windows(2)
            .find(|w| w[0] == "--output")
            .map(|w| w[1].clone());
        let stability_mode = args.iter().any(|a| a == "--stability");

        let commit = std::env::var("BEX_GIT_COMMIT").unwrap_or_else(|_| {
            std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".into())
        });

        let branch = std::env::var("BEX_GIT_BRANCH").unwrap_or_else(|_| {
            std::process::Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "unknown".into())
        });

        eprintln!(
            "[kperf] CPU: {:?}, db: {}",
            sampler.cpu(),
            sampler.database().name()
        );
        eprintln!("[kperf] git: {} ({})", commit, branch);
        println!("\n=== BEX VM Cache Profile ===");
        println!("=== commit: {} ({}) ===", commit, branch);

        if stability_mode {
            eprintln!("[kperf] stability check: running each workload 5 times");
            println!("\n=== Stability Check ===\n");
            println!(
                "  {:<25} {:>16} {:>16} {:>16} {:>8}",
                "Workload", "Min instr", "Max instr", "Median instr", "CV%"
            );
            println!(
                "  {} {} {} {} {}",
                "-".repeat(25),
                "-".repeat(16),
                "-".repeat(16),
                "-".repeat(16),
                "-".repeat(8)
            );

            for workload in WORKLOADS {
                let mut instruction_counts: Vec<u64> = Vec::new();
                for _ in 0..5 {
                    let c = profile_workload(&sampler, workload, 3);
                    instruction_counts.push(c.instructions);
                }
                instruction_counts.sort();
                let min = instruction_counts[0];
                let max = instruction_counts[4];
                let median = instruction_counts[2];
                let mean = instruction_counts.iter().sum::<u64>() as f64 / 5.0;
                let variance = instruction_counts
                    .iter()
                    .map(|&x| (x as f64 - mean).powi(2))
                    .sum::<f64>()
                    / 5.0;
                let cv = (variance.sqrt() / mean) * 100.0;

                println!(
                    "  {:<25} {:>16} {:>16} {:>16} {:>7.4}%",
                    workload.name,
                    fmt(min),
                    fmt(max),
                    fmt(median),
                    cv
                );
            }
            println!();
            unsafe {
                sampler.release().ok();
            }
            return;
        }

        let repeats = 5;
        let mut results: BTreeMap<&str, Counters> = BTreeMap::new();

        for workload in WORKLOADS {
            let counters = profile_workload(&sampler, workload, repeats);
            counters.print(workload.name);
            results.insert(workload.name, counters);
        }

        unsafe {
            sampler.release().ok();
        }

        // Write JSON
        let json_path = output_path.unwrap_or_else(|| format!("/tmp/bex-perf-{}.json", commit));

        let mut json = String::from("{\n");
        json.push_str(&format!("  \"commit\": \"{}\",\n", commit));
        json.push_str(&format!("  \"branch\": \"{}\",\n", branch));
        json.push_str("  \"workloads\": {\n");

        let entries: Vec<_> = results.iter().collect();
        for (i, (name, counters)) in entries.iter().enumerate() {
            json.push_str(&format!("    \"{}\": {{\n", name));
            json.push_str(&counters.to_json_fields().replace("\n    ", "\n      "));
            json.push('\n');
            if i + 1 < entries.len() {
                json.push_str("    },\n");
            } else {
                json.push_str("    }\n");
            }
        }

        json.push_str("  }\n}\n");

        std::fs::write(&json_path, &json).expect("failed to write JSON");
        eprintln!("\n[kperf] Results saved to {}", json_path);
        println!();
    }
} // mod macos_main
