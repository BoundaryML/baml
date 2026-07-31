//! Per-platform TOML baselines + the `check` / `refresh-baseline`
//! subcommands (design §10.3 — the size-gate architecture, cloned).
//!
//! Baselines live in `.ci/obs-bench/<platform>.toml`, one `[rows.<bench_id>]`
//! table per gated row. `check` compares fresh NDJSON rows against the same
//! platform's baseline:
//! - only `basis = "measured"` rows gate (§10.1);
//! - rows from `debug` builds never gate;
//! - cross-platform comparison is structurally impossible (file-per-platform).
//!
//! Direction matters: for most metrics smaller is better (`max_regress_pct`
//! bounds growth). Rows where bigger is better set `direction = "higher"`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

use crate::rows::{Basis, BenchRow};

pub const DEFAULT_BASELINE_DIR: &str = ".ci/obs-bench";
/// Default allowed regression vs baseline, in percent. Deliberately loose
/// while the suite is young; tighten per-row via `max_regress_pct`.
pub const DEFAULT_MAX_REGRESS_PCT: f64 = 20.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineRow {
    pub value: f64,
    pub unit: String,
    /// `lower` (default): value growing past baseline+pct fails.
    /// `higher`: value shrinking past baseline-pct fails.
    #[serde(default)]
    pub direction: Option<String>,
    /// Override of [`DEFAULT_MAX_REGRESS_PCT`].
    #[serde(default)]
    pub max_regress_pct: Option<f64>,
    /// Absolute ceiling (for `lower` rows) / floor (for `higher` rows) that
    /// fails regardless of the baseline — the size-gate "hard ceiling".
    #[serde(default)]
    pub hard_limit: Option<f64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct PlatformBaseline {
    /// Platform key, echoed for self-description.
    pub platform: String,
    /// Git sha the baseline was recorded from.
    #[serde(default)]
    pub recorded_from: Option<String>,
    /// bench_id -> baseline row.
    #[serde(default)]
    pub rows: BTreeMap<String, BaselineRow>,
}

pub fn baseline_path(dir: &Path, platform: &str) -> PathBuf {
    dir.join(format!("{platform}.toml"))
}

pub fn load(dir: &Path, platform: &str) -> anyhow::Result<Option<PlatformBaseline>> {
    let path = baseline_path(dir, platform);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("reading baseline {}", path.display()))?;
    let baseline =
        toml::from_str(&text).with_context(|| format!("parsing baseline {}", path.display()))?;
    Ok(Some(baseline))
}

/// `refresh-baseline`: (re)write the platform baseline from measured rows.
/// Existing per-row policy fields (direction/limits) are preserved.
pub fn refresh(dir: &Path, platform: &str, rows: &[BenchRow]) -> anyhow::Result<PathBuf> {
    let mut baseline = load(dir, platform)?.unwrap_or_default();
    baseline.platform = platform.to_string();
    baseline.recorded_from = crate::machine::git_sha();
    for row in rows {
        if row.basis != Basis::Measured || row.machine.build_profile != "release" {
            continue;
        }
        if row.machine.platform != platform {
            continue;
        }
        let entry = baseline
            .rows
            .entry(row.bench_id.clone())
            .or_insert_with(|| BaselineRow {
                value: row.value,
                unit: row.unit.clone(),
                direction: None,
                max_regress_pct: None,
                hard_limit: None,
            });
        entry.value = row.value;
        entry.unit = row.unit.clone();
    }
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = baseline_path(dir, platform);
    let text = toml::to_string_pretty(&baseline)?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

#[derive(Debug)]
pub struct Violation {
    pub bench_id: String,
    pub message: String,
}

/// `check`: compare measured release rows against the platform baseline.
/// Returns (checked, violations, skipped-reasons).
pub fn check(
    baseline: &PlatformBaseline,
    rows: &[BenchRow],
) -> (usize, Vec<Violation>, Vec<String>) {
    let mut checked = 0;
    let mut violations = Vec::new();
    let mut skipped = Vec::new();
    for row in rows {
        let Some(base) = baseline.rows.get(&row.bench_id) else {
            continue; // ungated row: context only
        };
        if row.basis != Basis::Measured {
            skipped.push(format!(
                "{}: basis {:?} never gates (only measured rows may)",
                row.bench_id, row.basis
            ));
            continue;
        }
        if row.machine.build_profile != "release" {
            skipped.push(format!("{}: debug build never gates", row.bench_id));
            continue;
        }
        if row.machine.platform != baseline.platform {
            skipped.push(format!(
                "{}: platform {} != baseline {} (cross-machine comparison refused)",
                row.bench_id, row.machine.platform, baseline.platform
            ));
            continue;
        }
        checked += 1;
        let pct = base.max_regress_pct.unwrap_or(DEFAULT_MAX_REGRESS_PCT);
        let higher_is_better = base.direction.as_deref() == Some("higher");
        if higher_is_better {
            let floor = base.value * (1.0 - pct / 100.0);
            if row.value < floor {
                violations.push(Violation {
                    bench_id: row.bench_id.clone(),
                    message: format!(
                        "{} {} < floor {} ({}% below baseline {})",
                        row.value, row.unit, floor, pct, base.value
                    ),
                });
            }
            if let Some(limit) = base.hard_limit
                && row.value < limit
            {
                violations.push(Violation {
                    bench_id: row.bench_id.clone(),
                    message: format!("{} {} < hard floor {}", row.value, row.unit, limit),
                });
            }
        } else {
            let ceiling = base.value * (1.0 + pct / 100.0);
            if row.value > ceiling {
                violations.push(Violation {
                    bench_id: row.bench_id.clone(),
                    message: format!(
                        "{} {} > ceiling {} ({}% above baseline {})",
                        row.value, row.unit, ceiling, pct, base.value
                    ),
                });
            }
            if let Some(limit) = base.hard_limit
                && row.value > limit
            {
                violations.push(Violation {
                    bench_id: row.bench_id.clone(),
                    message: format!("{} {} > hard ceiling {}", row.value, row.unit, limit),
                });
            }
        }
    }
    (checked, violations, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rows::BenchRow;

    fn measured(bench_id: &str, value: f64) -> BenchRow {
        let mut row = BenchRow::new(bench_id, "w", "legacy", "m", value, "ns", Basis::Measured);
        // Force release so gating logic is exercised regardless of how the
        // test binary was built.
        row.machine.build_profile = "release".to_string();
        row
    }

    fn baseline_with(bench_id: &str, value: f64) -> PlatformBaseline {
        let mut baseline = PlatformBaseline {
            platform: crate::machine::platform_key(),
            recorded_from: None,
            rows: BTreeMap::new(),
        };
        baseline.rows.insert(
            bench_id.to_string(),
            BaselineRow {
                value,
                unit: "ns".to_string(),
                direction: None,
                max_regress_pct: Some(10.0),
                hard_limit: None,
            },
        );
        baseline
    }

    #[test]
    fn within_tolerance_passes_and_regression_fails() {
        let baseline = baseline_with("c1.w.m", 100.0);
        let (checked, violations, _) = check(&baseline, &[measured("c1.w.m", 105.0)]);
        assert_eq!(checked, 1);
        assert!(violations.is_empty(), "{violations:?}");
        let (_, violations, _) = check(&baseline, &[measured("c1.w.m", 150.0)]);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn non_measured_rows_never_gate() {
        let baseline = baseline_with("c1.w.m", 100.0);
        let mut row = measured("c1.w.m", 500.0);
        row.basis = Basis::Extrapolated;
        let (checked, violations, skipped) = check(&baseline, &[row]);
        assert_eq!(checked, 0);
        assert!(violations.is_empty());
        assert_eq!(skipped.len(), 1);
    }

    #[test]
    fn refresh_then_check_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let platform = crate::machine::platform_key();
        let mut row = measured("c2.w.m", 42.0);
        row.machine.platform.clone_from(&platform);
        let path = refresh(dir.path(), &platform, &[row.clone()]).unwrap();
        assert!(path.exists());
        let baseline = load(dir.path(), &platform).unwrap().unwrap();
        assert_eq!(baseline.rows["c2.w.m"].value, 42.0);
        let (checked, violations, _) = check(&baseline, &[row]);
        assert_eq!((checked, violations.len()), (1, 0));
    }
}
