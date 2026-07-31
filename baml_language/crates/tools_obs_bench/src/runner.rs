//! `obs-bench run` — execute one committed workload under a chosen pipeline
//! mode and collect the paired measurements (design §10.3/§10.4).
//!
//! What one invocation produces:
//! - child wall time, and child user+sys CPU (`getrusage(RUSAGE_CHILDREN)`
//!   delta — user+sys, never just wall, per §10.4);
//! - the consumer's own stats line (`BAML_OBS_STATS`): thread CPU, records,
//!   bytes drained — the §10.3 self-report;
//! - bytes on disk under the run's isolated `BAML_PROFILE_DIR`;
//! - derived per-call costs when the workload's call count is known.
//!
//! `--paired` runs the workload twice — `BAML_PROFILE=0` then `=1`, same
//! binary, same session (§10.4 rules) — and adds delta rows (`c1.*`): the
//! profiled-vs-unprofiled wall difference per call.

use std::{path::PathBuf, process::Command, time::Instant};

use anyhow::Context as _;

use crate::rows::{Basis, BenchRow};

pub struct RunConfig {
    /// Workload `.baml` path.
    pub workload: PathBuf,
    /// Args after `--` for the workload's generated CLI.
    pub args: Vec<String>,
    /// `BAML_PROFILE_PIPELINE` for the child.
    pub pipeline: String,
    /// The `baml-cli` binary to drive.
    pub baml_cli: PathBuf,
    /// Scratch dir (profiles + stats land here); kept when `keep` is set.
    pub scratch: PathBuf,
    /// Also run a `BAML_PROFILE=0` leg and emit paired delta rows.
    pub paired: bool,
    /// Known profiled calls per run (for per-call derived rows); pass the
    /// workload's call count (e.g. hotloop: 2 × iters).
    pub calls: Option<u64>,
    /// Keep scratch artifacts (default: removed on success).
    pub keep: bool,
}

pub struct RunReport {
    pub rows: Vec<BenchRow>,
    pub summary: String,
}

struct LegMeasure {
    wall_s: f64,
    child_user_s: f64,
    child_sys_s: f64,
    disk_bytes: u64,
    /// §6.1 v2 session stream bytes (`<leg>/sessions`), the C3 gate's
    /// numerator (dict/ is per-revision one-time cost, counted separately).
    session_bytes: u64,
    dict_bytes: u64,
    consumer: Option<serde_json::Value>,
    exit_ok: bool,
}

pub fn run(cfg: &RunConfig) -> anyhow::Result<RunReport> {
    let workload_name = cfg
        .workload
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "workload".to_string());

    std::fs::create_dir_all(&cfg.scratch)
        .with_context(|| format!("creating scratch dir {}", cfg.scratch.display()))?;

    let mut rows = Vec::new();
    let mut summary = String::new();
    use std::fmt::Write as _;

    // Paired baseline leg first (BAML_PROFILE=0): same binary, same session.
    let off = if cfg.paired {
        let leg = run_leg(cfg, false, "off")?;
        anyhow::ensure!(leg.exit_ok, "unprofiled leg failed");
        let _ = writeln!(
            summary,
            "off : wall {:.3}s user {:.3}s sys {:.3}s",
            leg.wall_s, leg.child_user_s, leg.child_sys_s
        );
        Some(leg)
    } else {
        None
    };

    let on = run_leg(cfg, true, "on")?;
    anyhow::ensure!(on.exit_ok, "profiled leg failed");
    let _ = writeln!(
        summary,
        "on  : wall {:.3}s user {:.3}s sys {:.3}s disk {} B session {} B dict {} B",
        on.wall_s, on.child_user_s, on.child_sys_s, on.disk_bytes, on.session_bytes, on.dict_bytes
    );

    let p = cfg.pipeline.as_str();
    rows.push(BenchRow::new(
        "smoke.run.wall_s",
        &workload_name,
        p,
        "wall_s",
        on.wall_s,
        "s",
        Basis::Measured,
    ));
    rows.push(BenchRow::new(
        "smoke.run.child_cpu_s",
        &workload_name,
        p,
        "child_cpu_s",
        on.child_user_s + on.child_sys_s,
        "s",
        Basis::Measured,
    ));
    rows.push(BenchRow::new(
        "c3.run.disk_bytes",
        &workload_name,
        p,
        "disk_bytes",
        on.disk_bytes as f64,
        "bytes",
        Basis::Measured,
    ));
    if on.wall_s > 0.0 {
        rows.push(BenchRow::new(
            "c3.run.disk_bytes_per_s",
            &workload_name,
            p,
            "disk_bytes_per_s",
            on.disk_bytes as f64 / on.wall_s,
            "B/s",
            Basis::Measured,
        ));
    }
    // C3 proper (design §10.5): the v2 session stream is the always-on
    // cost; the hot-loop gate is ≤6 KB/s of session CCT bytes. Emitted
    // whenever the leg produced a sessions/ dir (dual or cct pipelines).
    if on.session_bytes > 0 {
        rows.push(BenchRow::new(
            "c3.run.session_bytes",
            &workload_name,
            p,
            "session_bytes",
            on.session_bytes as f64,
            "bytes",
            Basis::Measured,
        ));
        if on.wall_s > 0.0 {
            rows.push(BenchRow::new(
                "c3.run.session_bytes_per_s",
                &workload_name,
                p,
                "session_bytes_per_s",
                on.session_bytes as f64 / on.wall_s,
                "B/s",
                Basis::Measured,
            ));
        }
        if on.disk_bytes > 0 {
            rows.push(BenchRow::new(
                "c3.run.legacy_to_session_ratio",
                &workload_name,
                p,
                "legacy_to_session_ratio",
                on.disk_bytes as f64 / on.session_bytes as f64,
                "x",
                Basis::Measured,
            ));
        }
    }
    if on.dict_bytes > 0 {
        rows.push(BenchRow::new(
            "c3.run.dict_bytes",
            &workload_name,
            p,
            "dict_bytes",
            on.dict_bytes as f64,
            "bytes",
            Basis::Measured,
        ));
    }

    // Consumer self-report (the §10.3 instrumentation), when present.
    if let Some(stats) = &on.consumer {
        let cpu_ns = stats["cpu_ns"].as_f64().unwrap_or(0.0);
        let records = stats["records"].as_f64().unwrap_or(0.0);
        let bytes_drained = stats["bytes_drained"].as_f64().unwrap_or(0.0);
        let _ = writeln!(
            summary,
            "consumer: cpu {:.1} ms, {} records, {} bytes drained",
            cpu_ns / 1e6,
            records,
            bytes_drained
        );
        rows.push(BenchRow::new(
            "c2.run.consumer_cpu_ms",
            &workload_name,
            p,
            "consumer_cpu_ms",
            cpu_ns / 1e6,
            "ms",
            Basis::Measured,
        ));
        rows.push(BenchRow::new(
            "smoke.run.consumer_records",
            &workload_name,
            p,
            "consumer_records",
            records,
            "count",
            Basis::Measured,
        ));
        if records > 0.0 {
            rows.push(BenchRow::new(
                "c2.run.consumer_ns_per_record",
                &workload_name,
                p,
                "consumer_ns_per_record",
                cpu_ns / records,
                "ns",
                Basis::Measured,
            ));
        }
        // Cross-check row (§10.4): consumer CPU must also be visible in the
        // child's sys+user; the report flags >25% disagreement.
        if let Some(calls) = cfg.calls
            && calls > 0
        {
            rows.push(BenchRow::new(
                "c2.run.consumer_cpu_ms_per_mcall",
                &workload_name,
                p,
                "consumer_cpu_ms_per_mcall",
                (cpu_ns / 1e6) / (calls as f64 / 1e6),
                "ms",
                Basis::Measured,
            ));
        }
    } else {
        let _ = writeln!(
            summary,
            "consumer: no stats line (BAML_OBS_STATS not written)"
        );
    }

    if let Some(calls) = cfg.calls {
        if calls > 0 && on.disk_bytes > 0 {
            rows.push(BenchRow::new(
                "c3.run.disk_bytes_per_call",
                &workload_name,
                p,
                "disk_bytes_per_call",
                on.disk_bytes as f64 / calls as f64,
                "bytes",
                Basis::Measured,
            ));
        }
        if let Some(off) = &off
            && calls > 0
        {
            // The quotable §10.4 number: paired wall-per-call delta. The
            // trivial-callee slope is an upper bound on producer cost.
            let delta_ns = (on.wall_s - off.wall_s) * 1e9 / calls as f64;
            let _ = writeln!(
                summary,
                "paired: {delta_ns:.1} ns/call wall delta over {calls} calls"
            );
            rows.push(
                BenchRow::new(
                    "c1.run.paired_wall_ns_per_call",
                    &workload_name,
                    p,
                    "paired_wall_ns_per_call",
                    delta_ns,
                    "ns",
                    Basis::Measured,
                )
                .with_notes(format!(
                    "on={:.4}s off={:.4}s calls={calls}",
                    on.wall_s, off.wall_s
                )),
            );
            let cpu_delta_ns =
                ((on.child_user_s + on.child_sys_s) - (off.child_user_s + off.child_sys_s)) * 1e9
                    / calls as f64;
            rows.push(BenchRow::new(
                "c1.run.paired_cpu_ns_per_call",
                &workload_name,
                p,
                "paired_cpu_ns_per_call",
                cpu_delta_ns,
                "ns",
                Basis::Measured,
            ));
        }
    }

    if !cfg.keep {
        let _ = std::fs::remove_dir_all(&cfg.scratch);
    }

    Ok(RunReport { rows, summary })
}

fn run_leg(cfg: &RunConfig, profile_on: bool, leg: &str) -> anyhow::Result<LegMeasure> {
    let leg_dir = cfg.scratch.join(leg);
    let profile_dir = leg_dir.join("profiles");
    let stats_path = leg_dir.join("consumer-stats.ndjson");
    std::fs::create_dir_all(&leg_dir)?;

    let before = child_rusage();
    let start = Instant::now();
    let status = Command::new(&cfg.baml_cli)
        .arg("run")
        .arg("--file")
        .arg(&cfg.workload)
        .arg("--")
        .args(&cfg.args)
        .env("BAML_PROFILE", if profile_on { "1" } else { "0" })
        .env("BAML_PROFILE_DIR", &profile_dir)
        .env("BAML_OBS_STATS", &stats_path)
        .env("BAML_PROFILE_PIPELINE", &cfg.pipeline)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()
        .with_context(|| format!("spawning {}", cfg.baml_cli.display()))?;
    let wall_s = start.elapsed().as_secs_f64();
    let after = child_rusage();

    let disk_bytes = dir_bytes(&profile_dir);
    let session_bytes = dir_bytes(&leg_dir.join("sessions"));
    let dict_bytes = dir_bytes(&leg_dir.join("dict"));
    let consumer = read_last_stats_line(&stats_path);

    Ok(LegMeasure {
        wall_s,
        child_user_s: after.0 - before.0,
        child_sys_s: after.1 - before.1,
        disk_bytes,
        session_bytes,
        dict_bytes,
        consumer,
        exit_ok: status.success(),
    })
}

/// Cumulative (user_s, sys_s) of reaped children.
#[cfg(unix)]
#[expect(
    unsafe_code,
    reason = "libc getrusage FFI; no aliasing, out-param only"
)]
fn child_rusage() -> (f64, f64) {
    // SAFETY: getrusage with a zeroed out-param is the documented usage.
    unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_CHILDREN, &raw mut ru) == 0 {
            (tv_to_s(ru.ru_utime), tv_to_s(ru.ru_stime))
        } else {
            (0.0, 0.0)
        }
    }
}

#[cfg(unix)]
fn tv_to_s(tv: libc::timeval) -> f64 {
    tv.tv_sec as f64 + tv.tv_usec as f64 / 1e6
}

#[cfg(not(unix))]
fn child_rusage() -> (f64, f64) {
    (0.0, 0.0)
}

fn dir_bytes(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            total += dir_bytes(&path);
        } else if let Ok(meta) = entry.metadata() {
            total += meta.len();
        }
    }
    total
}

/// The consumer appends cumulative lines; the last one is the run's total.
fn read_last_stats_line(path: &std::path::Path) -> Option<serde_json::Value> {
    let contents = std::fs::read_to_string(path).ok()?;
    let last = contents.lines().rev().find(|l| !l.trim().is_empty())?;
    serde_json::from_str(last).ok()
}

/// Locate the workspace's release `baml-cli` (the default driver binary).
pub fn default_baml_cli() -> anyhow::Result<PathBuf> {
    // obs-bench lives in <ws>/crates/tools_obs_bench; the binary in
    // <ws>/target/release. CARGO_MANIFEST_DIR is compile-time, stable for a
    // tool that always runs from the repo.
    let ws = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_path_buf)
        .context("resolving workspace root")?;
    for profile in ["release", "debug"] {
        let candidate = ws.join("target").join(profile).join("baml-cli");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "no baml-cli found under {}/target/{{release,debug}}; build with `cargo build -p baml_cli --release`",
        ws.display()
    )
}
