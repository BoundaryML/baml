//! `obs-bench crashfuzz` (design §10.3, C8): SIGKILL workloads mid-run
//! under the v2 layout, then assert every surviving artifact honors the
//! committed-prefix recovery contract via [`crate::validate`].
//!
//! Pass categories (§6.3/§6.4):
//! - `killed_before_begin` — no sessions and no legacy profiles yet.
//! - `recovered` — artifacts scan; torn tails/truncated meta are readable
//!   crash evidence, never errors.
//! - `completed` — the child finished before the kill fired (long delays).
//!
//! Kill delays sweep deterministically (with seeded jitter) from
//! `min_delay_ms` to `max_delay_ms`, plus one uninterrupted canary leg
//! that must validate clean AND fully sealed.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::Context as _;

use crate::validate;

pub struct FuzzConfig {
    pub workload: PathBuf,
    pub args: Vec<String>,
    pub baml_cli: PathBuf,
    pub scratch: PathBuf,
    pub pipeline: String,
    pub iters: u32,
    pub min_delay_ms: u64,
    pub max_delay_ms: u64,
    pub seed: u64,
}

#[derive(Debug, Default)]
pub struct FuzzReport {
    pub killed_before_begin: u32,
    pub recovered: u32,
    pub completed: u32,
    pub torn_files: usize,
    pub failures: Vec<String>,
    pub summary: String,
}

/// Tiny deterministic LCG (no rand dependency; same constants as MMIX).
fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

pub fn crashfuzz(cfg: &FuzzConfig) -> anyhow::Result<FuzzReport> {
    use std::fmt::Write as _;
    let mut report = FuzzReport::default();
    let mut rng = cfg.seed | 1;

    for i in 0..cfg.iters {
        let span = cfg.max_delay_ms.saturating_sub(cfg.min_delay_ms).max(1);
        let base = if cfg.iters > 1 {
            cfg.min_delay_ms + span * u64::from(i) / u64::from(cfg.iters - 1)
        } else {
            cfg.min_delay_ms
        };
        // ±25% seeded jitter so repeated sweeps don't sample identical
        // instants while staying reproducible for a given seed.
        let jitter = (lcg(&mut rng) % (span / 2 + 1)).saturating_sub(span / 4);
        let delay = Duration::from_millis(base.saturating_add(jitter).max(cfg.min_delay_ms));

        let leg_dir = cfg.scratch.join(format!("iter-{i:03}"));
        let _ = std::fs::remove_dir_all(&leg_dir);
        std::fs::create_dir_all(&leg_dir)?;
        let outcome = run_one(cfg, &leg_dir, Some(delay))
            .with_context(|| format!("crashfuzz iteration {i}"))?;

        let root = leg_dir.join("baml");
        let has_v2 = root.join("sessions").is_dir();
        let has_legacy = root.join("profiles").is_dir();
        match outcome {
            Outcome::Completed => report.completed += 1,
            Outcome::Killed if !has_v2 && !has_legacy => report.killed_before_begin += 1,
            Outcome::Killed => {
                let v = validate::validate_root(&root);
                report.torn_files += v.torn;
                if v.invalid > 0 {
                    let _ = writeln!(
                        report.summary,
                        "iter {i:03} (kill @{}ms): INVALID\n{}",
                        delay.as_millis(),
                        v.render()
                    );
                    for f in v.findings.iter().filter(|f| f.status == "invalid") {
                        report
                            .failures
                            .push(format!("iter {i:03}: {} — {}", f.path, f.detail));
                    }
                } else {
                    report.recovered += 1;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&leg_dir);
    }

    // Canary: an uninterrupted run must validate clean and fully sealed —
    // proves validate itself passes on healthy output (no vacuous fuzz).
    let leg_dir = cfg.scratch.join("canary");
    let _ = std::fs::remove_dir_all(&leg_dir);
    std::fs::create_dir_all(&leg_dir)?;
    let outcome = run_one(cfg, &leg_dir, None).context("crashfuzz canary leg")?;
    anyhow::ensure!(
        matches!(outcome, Outcome::Completed),
        "canary leg must complete"
    );
    let v = validate::validate_root(&leg_dir.join("baml"));
    if v.invalid > 0 || v.torn > 0 {
        report
            .failures
            .push(format!("canary leg invalid/torn:\n{}", v.render()));
    }
    if !v.findings.iter().any(|f| f.kind == "bamlseg") {
        report
            .failures
            .push("canary leg produced no session segment".to_string());
    }
    let _ = std::fs::remove_dir_all(&leg_dir);

    let _ = writeln!(
        report.summary,
        "crashfuzz: {} iters — {} killed-before-begin, {} recovered, {} completed, \
         {} torn files accepted, {} failures",
        cfg.iters,
        report.killed_before_begin,
        report.recovered,
        report.completed,
        report.torn_files,
        report.failures.len()
    );
    Ok(report)
}

enum Outcome {
    Killed,
    Completed,
}

/// Spawn one profiled workload leg under `<leg_dir>/baml/profiles` and
/// either SIGKILL it after `delay` or let it run to completion.
fn run_one(
    cfg: &FuzzConfig,
    leg_dir: &std::path::Path,
    delay: Option<Duration>,
) -> anyhow::Result<Outcome> {
    let profile_dir = leg_dir.join("baml/profiles");
    let mut child = Command::new(&cfg.baml_cli)
        .arg("run")
        .arg("--file")
        .arg(&cfg.workload)
        .arg("--")
        .args(&cfg.args)
        .env("BAML_PROFILE", "1")
        .env("BAML_PROFILE_DIR", &profile_dir)
        .env("BAML_PROFILE_PIPELINE", &cfg.pipeline)
        .env("BAML_PROFILE_RAW", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning {}", cfg.baml_cli.display()))?;

    let Some(delay) = delay else {
        let status = child.wait()?;
        anyhow::ensure!(status.success(), "uninterrupted leg failed: {status}");
        return Ok(Outcome::Completed);
    };

    let deadline = Instant::now() + delay;
    loop {
        if let Some(_status) = child.try_wait()? {
            return Ok(Outcome::Completed);
        }
        if Instant::now() >= deadline {
            // SIGKILL — no cleanup handlers run; exactly the crash model.
            child.kill().ok();
            child.wait().ok();
            return Ok(Outcome::Killed);
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
