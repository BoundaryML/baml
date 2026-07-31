//! `obs-bench calibrate` — pick per-leg `iters` for the fixed-wall
//! `bench_rate` sweep (design §10.2: the C4 rate sweep holds wall time
//! constant while the `work` knob varies the profiled-call rate ~100x).
//!
//! Method: for each `work` value, probe with a small `iters`, then scale
//! linearly to the target wall time and re-measure once. Loop cost is
//! near-linear in `iters`, so one correction lands within a few percent —
//! good enough for a sweep whose gate is ±25% flatness.

use std::path::PathBuf;

use serde::Serialize;

use crate::runner::{RunConfig, default_baml_cli};

#[derive(Debug, Serialize)]
pub struct CalibratedLeg {
    pub work: u64,
    pub iters: u64,
    pub measured_wall_s: f64,
}

pub struct CalibrateConfig {
    pub workload: PathBuf,
    pub target_wall_s: f64,
    pub work_values: Vec<u64>,
    pub scratch: PathBuf,
    pub baml_cli: Option<PathBuf>,
}

pub fn run(cfg: &CalibrateConfig) -> anyhow::Result<Vec<CalibratedLeg>> {
    let baml_cli = match &cfg.baml_cli {
        Some(p) => p.clone(),
        None => default_baml_cli()?,
    };
    let mut legs = Vec::new();
    for &work in &cfg.work_values {
        let probe_iters: u64 = 100_000;
        let probe_s = leg_wall(cfg, &baml_cli, probe_iters, work)?;
        let per_iter = probe_s / probe_iters as f64;
        let mut iters = ((cfg.target_wall_s / per_iter) as u64).max(1);
        let measured = leg_wall(cfg, &baml_cli, iters, work)?;
        // One linear correction.
        if measured > 0.0 {
            iters = ((iters as f64) * cfg.target_wall_s / measured) as u64;
            iters = iters.max(1);
        }
        let final_s = leg_wall(cfg, &baml_cli, iters, work)?;
        legs.push(CalibratedLeg {
            work,
            iters,
            measured_wall_s: final_s,
        });
    }
    Ok(legs)
}

fn leg_wall(
    cfg: &CalibrateConfig,
    baml_cli: &std::path::Path,
    iters: u64,
    work: u64,
) -> anyhow::Result<f64> {
    // Calibration runs unprofiled (BAML_PROFILE=0 leg only): the sweep's
    // profiled legs are launched by `run` with these iters.
    let leg = crate::runner::run(&RunConfig {
        workload: cfg.workload.clone(),
        args: vec![
            "--iters".to_string(),
            iters.to_string(),
            "--work".to_string(),
            work.to_string(),
        ],
        pipeline: "cct".to_string(),
        baml_cli: baml_cli.to_path_buf(),
        scratch: cfg.scratch.join(format!("cal-w{work}-i{iters}")),
        paired: false,
        calls: None,
        keep: false,
    })?;
    let wall = leg
        .rows
        .iter()
        .find(|r| r.metric == "wall_s")
        .map(|r| r.value)
        .unwrap_or(0.0);
    anyhow::ensure!(wall > 0.0, "calibration leg produced no wall time");
    Ok(wall)
}
