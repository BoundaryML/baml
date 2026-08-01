use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::dataset::{self, BenchRow};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Direction {
    Lower,
    Higher,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct MetricBaseline {
    pub(crate) bench_id: String,
    pub(crate) metric: String,
    pub(crate) direction: Direction,
    pub(crate) baseline: f64,
    #[serde(default = "default_delta")]
    pub(crate) max_delta_pct: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) claim: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct PlatformBaseline {
    pub(crate) schema_version: u32,
    pub(crate) platform: String,
    pub(crate) generated_unix_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) git_sha: Option<String>,
    #[serde(rename = "metric")]
    pub(crate) metrics: Vec<MetricBaseline>,
}

#[derive(Debug, Serialize)]
pub(crate) struct GateResult {
    bench_id: String,
    metric: String,
    claim: Option<String>,
    direction: Direction,
    current: Option<f64>,
    baseline: f64,
    allowed: f64,
    passed: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct CheckReport {
    pub(crate) schema_version: u32,
    pub(crate) platform: String,
    pub(crate) baseline_path: String,
    pub(crate) passed: bool,
    pub(crate) gates: Vec<GateResult>,
}

impl PlatformBaseline {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let source =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let baseline =
            toml::from_str::<Self>(&source).with_context(|| format!("parse {}", path.display()))?;
        if baseline.schema_version != 1 {
            bail!(
                "{} uses unsupported baseline schema {}",
                path.display(),
                baseline.schema_version
            );
        }
        if baseline.metrics.is_empty() {
            bail!("{} contains no metric baselines", path.display());
        }
        for metric in &baseline.metrics {
            if metric.bench_id.is_empty()
                || metric.metric.is_empty()
                || !metric.baseline.is_finite()
                || !metric.max_delta_pct.is_finite()
                || metric.max_delta_pct < 0.0
                || metric.min.is_some_and(|value| !value.is_finite())
                || metric.max.is_some_and(|value| !value.is_finite())
            {
                bail!("{} contains an invalid metric baseline", path.display());
            }
        }
        Ok(baseline)
    }

    pub(crate) fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let source = toml::to_string_pretty(self)?;
        fs::write(path, source).with_context(|| format!("write {}", path.display()))
    }
}

pub(crate) fn check(
    input_paths: &[PathBuf],
    baseline_path: &Path,
    require_measured: bool,
) -> Result<CheckReport> {
    dataset::validate(input_paths)?;
    let rows = dataset::load(input_paths)?;
    let baseline = PlatformBaseline::load(baseline_path)?;
    let mut gates = Vec::new();
    for metric in &baseline.metrics {
        let row = rows
            .iter()
            .rev()
            .find(|row| row.bench_id() == Some(metric.bench_id.as_str()));
        let current = row.and_then(|row| row.metric(&metric.metric));
        let evidence_ok =
            !require_measured || row.is_some_and(|row| row.evidence() == Some("measured"));
        let delta = metric.max_delta_pct.max(0.0) / 100.0;
        let relative_allowed = match metric.direction {
            Direction::Lower => metric.baseline * (1.0 + delta),
            Direction::Higher => metric.baseline * (1.0 - delta),
        };
        let allowed = match metric.direction {
            Direction::Lower => metric
                .max
                .map_or(relative_allowed, |max| max.min(relative_allowed)),
            Direction::Higher => metric
                .min
                .map_or(relative_allowed, |min| min.max(relative_allowed)),
        };
        let passed_value = current.is_some_and(|value| match metric.direction {
            Direction::Lower => value <= allowed,
            Direction::Higher => value >= allowed,
        });
        let passed = passed_value && evidence_ok;
        let detail = if row.is_none() {
            "benchmark row missing".to_owned()
        } else if current.is_none() {
            "metric missing or non-numeric".to_owned()
        } else if !evidence_ok {
            "gate requires evidence=\"measured\"".to_owned()
        } else if passed {
            "within committed baseline policy".to_owned()
        } else {
            "outside committed baseline policy".to_owned()
        };
        gates.push(GateResult {
            bench_id: metric.bench_id.clone(),
            metric: metric.metric.clone(),
            claim: metric.claim.clone(),
            direction: metric.direction,
            current,
            baseline: metric.baseline,
            allowed,
            passed,
            detail,
        });
    }
    Ok(CheckReport {
        schema_version: 1,
        platform: baseline.platform,
        baseline_path: baseline_path.display().to_string(),
        passed: gates.iter().all(|gate| gate.passed),
        gates,
    })
}

pub(crate) fn refresh(
    input_paths: &[PathBuf],
    baseline_path: &Path,
    platform: String,
    max_delta_pct: f64,
) -> Result<PlatformBaseline> {
    if !max_delta_pct.is_finite() || max_delta_pct < 0.0 {
        bail!("max_delta_pct must be a finite non-negative number");
    }
    dataset::validate(input_paths)?;
    let rows = dataset::load(input_paths)?;
    let existing = if baseline_path.try_exists()? {
        Some(PlatformBaseline::load(baseline_path)?)
    } else {
        None
    };
    let mut metrics = if let Some(existing) = existing {
        existing.metrics
    } else {
        default_metric_selection(&rows, max_delta_pct)
    };
    let by_id = rows_by_id(&rows);
    for metric in &mut metrics {
        let row = by_id.get(metric.bench_id.as_str()).ok_or_else(|| {
            anyhow::anyhow!(
                "cannot refresh {}.{}: benchmark row is missing",
                metric.bench_id,
                metric.metric
            )
        })?;
        if row.evidence() != Some("measured") {
            bail!(
                "cannot refresh {}.{} from non-measured evidence",
                metric.bench_id,
                metric.metric
            );
        }
        metric.baseline = row.metric(&metric.metric).ok_or_else(|| {
            anyhow::anyhow!(
                "cannot refresh {}.{}: metric is missing or non-numeric",
                metric.bench_id,
                metric.metric
            )
        })?;
        metric.max_delta_pct = max_delta_pct;
    }
    metrics.sort_by(|left, right| {
        (&left.bench_id, &left.metric).cmp(&(&right.bench_id, &right.metric))
    });
    let baseline = PlatformBaseline {
        schema_version: 1,
        platform,
        generated_unix_seconds: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs()),
        git_sha: command_line("git", &["rev-parse", "HEAD"]),
        metrics,
    };
    baseline.write(baseline_path)?;
    Ok(baseline)
}

fn rows_by_id(rows: &[BenchRow]) -> BTreeMap<&str, &BenchRow> {
    rows.iter()
        .filter_map(|row| row.bench_id().map(|id| (id, row)))
        .collect()
}

fn default_metric_selection(rows: &[BenchRow], max_delta_pct: f64) -> Vec<MetricBaseline> {
    let preferred = [
        "median_ns_per_call_pair",
        "elapsed_ns",
        "throughput_mb_s",
        "reduction_x",
        "growth_exponent_16_to_64",
        "frame_bytes",
        "encoded_bytes",
    ];
    let higher = BTreeSet::from(["throughput_mb_s", "reduction_x"]);
    let mut output = Vec::new();
    for row in rows.iter().filter(|row| row.evidence() == Some("measured")) {
        let Some(bench_id) = row.bench_id() else {
            continue;
        };
        let Some(metric) = preferred.iter().find(|metric| row.metric(metric).is_some()) else {
            continue;
        };
        output.push(MetricBaseline {
            bench_id: bench_id.to_owned(),
            metric: (*metric).to_owned(),
            direction: if higher.contains(*metric) {
                Direction::Higher
            } else {
                Direction::Lower
            },
            baseline: row.metric(metric).unwrap_or_default(),
            max_delta_pct,
            min: None,
            max: None,
            claim: None,
        });
    }
    output
}

fn command_line(program: &str, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

const fn default_delta() -> f64 {
    15.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_and_higher_gates_are_directional() {
        let root = std::env::temp_dir().join(format!("obs-bench-baseline-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let rows_path = root.join("rows.ndjson");
        fs::write(
            &rows_path,
            concat!(
                "{\"schema_version\":1,\"bench_id\":\"latency\",\"evidence\":\"measured\",\"ns\":10}\n",
                "{\"schema_version\":1,\"bench_id\":\"rate\",\"evidence\":\"measured\",\"mb_s\":300}\n"
            ),
        )
        .unwrap();
        let baseline_path = root.join("baseline.toml");
        PlatformBaseline {
            schema_version: 1,
            platform: "test".to_owned(),
            generated_unix_seconds: 0,
            git_sha: None,
            metrics: vec![
                MetricBaseline {
                    bench_id: "latency".to_owned(),
                    metric: "ns".to_owned(),
                    direction: Direction::Lower,
                    baseline: 10.0,
                    max_delta_pct: 10.0,
                    min: None,
                    max: None,
                    claim: None,
                },
                MetricBaseline {
                    bench_id: "rate".to_owned(),
                    metric: "mb_s".to_owned(),
                    direction: Direction::Higher,
                    baseline: 300.0,
                    max_delta_pct: 10.0,
                    min: None,
                    max: None,
                    claim: None,
                },
            ],
        }
        .write(&baseline_path)
        .unwrap();
        let report = check(&[rows_path], &baseline_path, true).unwrap();
        assert!(report.passed);
        fs::remove_dir_all(root).unwrap();
    }
}
