//! Closed-world admission and the rt coverage report.
use std::{collections::BTreeMap, path::Path};

use super::{Artifacts, PROBES, kpis, pack_result};

// Enable only when all telemetry matrix cells pass, including non-empty
// recordings and exact normalized equality.
const TELEMETRY_SUPPORTED: bool = false;
// The standalone probes exercise closures without the shared project's global
// initializer dependency. Their admission is a required coverage baseline.
const REQUIRED_PROBES: &[&str] = &["RtClosures", "RtEmptyClosures"];

pub fn supports_level(probe: &str, level: &str) -> bool {
    level == "off" || TELEMETRY_SUPPORTED || REQUIRED_PROBES.contains(&probe)
}

#[derive(Debug, Clone)]
pub enum Rejection {
    Unsupported(String),
    Fail(String),
}

fn classify(error: String) -> Rejection {
    // Only the compiler's explicit admission diagnostic is skippable. Build
    // failures, missing executables and other CLI errors remain failures.
    for line in error.lines() {
        if let Some((_, construct)) = line.split_once("unsupported in baml_rt: ") {
            return Rejection::Unsupported(construct.trim().to_owned());
        }
    }
    Rejection::Fail(error)
}

pub fn build(build: &Path, workspace: &Path, manifest: &Path, only: Option<&str>) -> Artifacts {
    std::fs::create_dir_all(build).unwrap();
    let mut artifacts = Artifacts::new();
    for probe in PROBES {
        if only.is_some_and(|only| !only.split(',').any(|name| name == *probe)) {
            continue;
        }
        let executable = build.join(probe);
        let result = pack_result(
            build,
            workspace,
            &super::standalone_closure_source(manifest, probe)
                .unwrap_or_else(|| manifest.join("rust_diff")),
            &executable,
            [if super::initializer_probes::collects_tests(probe) {
                "InitializerTestSummary"
            } else {
                probe
            }],
            true,
        )
        .map_err(classify)
        .and_then(|()| {
            // These fixtures mutate the packed initializer or use a test-registry
            // host. Do not silently run their unmodified source if admission grows.
            if matches!(
                *probe,
                "InitializerTests"
                    | "InitializerTestErrors"
                    | "InitializerUncaught"
                    | "InitializerPanic"
                    | "InitializerPending"
                    | "GlobalCallPending"
            ) {
                Err(Rejection::Fail(
                    "admitted initializer fixture needs the rt variant/registry host".into(),
                ))
            } else {
                Ok(executable)
            }
        });
        artifacts.insert((*probe).into(), result);
    }
    for fixture in kpis::fixtures(workspace, only) {
        let dir = build.join("kpis").join(&fixture.slug);
        std::fs::create_dir_all(dir.join("source/baml_src")).unwrap();
        std::fs::write(dir.join("source/baml.toml"),
            "[package]\nname = \"differential-kpi\"\n[pack.env_var_names]\nBAML_TELEMETRY = \"BAML_TELEMETRY\"\n").unwrap();
        std::fs::copy(&fixture.source, dir.join("source/baml_src/main.baml")).unwrap();
        let executable = dir.join("workload");
        let result = pack_result(
            build,
            workspace,
            &dir.join("source"),
            &executable,
            ["main"],
            true,
        )
        .map(|()| executable)
        .map_err(classify);
        artifacts.insert(fixture.name, result);
    }
    artifacts
}

pub struct Coverage {
    rows: BTreeMap<String, Result<(), Rejection>>,
}

impl Coverage {
    pub fn new(artifacts: &Artifacts) -> Self {
        Self {
            rows: artifacts
                .iter()
                .map(|(name, result)| {
                    (
                        name.clone(),
                        match result {
                            Err(Rejection::Unsupported(reason))
                                if REQUIRED_PROBES.contains(&name.as_str()) =>
                            {
                                Err(Rejection::Fail(format!(
                                    "required rt coverage regressed: {reason}"
                                )))
                            }
                            _ => result.as_ref().map(|_| ()).map_err(Clone::clone),
                        },
                    )
                })
                .collect(),
        }
    }

    pub fn record(&mut self, probe: &str, failures: &[String]) {
        if self.rows[probe].is_err() {
            return;
        }
        // A broken oracle also prevents claiming verified rt coverage. Failures
        // confined to the old compiled variants do not change rt's status.
        let failures: Vec<_> = failures
            .iter()
            .filter(|failure| {
                failure.starts_with(&format!("{probe}/rt"))
                    || failure.starts_with(&format!("{probe}/bytecode"))
            })
            .cloned()
            .collect();
        if !failures.is_empty() {
            self.rows
                .insert(probe.into(), Err(Rejection::Fail(failures.join("\n"))));
        }
    }

    pub fn build_failures(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter_map(|(name, result)| match result {
                Err(Rejection::Fail(reason)) => Some(format!("{name}/rt: {reason}")),
                _ => None,
            })
            .collect()
    }

    fn report(&self) -> String {
        let mut counts = [0; 3];
        let mut blockers = BTreeMap::<&str, usize>::new();
        let mut rows = String::new();
        for (name, result) in &self.rows {
            let status = match result {
                Ok(()) => {
                    counts[0] += 1;
                    "pass".into()
                }
                Err(Rejection::Unsupported(reason)) => {
                    counts[1] += 1;
                    *blockers.entry(reason).or_default() += 1;
                    format!("unsupported: {reason}")
                }
                Err(Rejection::Fail(reason)) => {
                    counts[2] += 1;
                    format!("fail: {reason}")
                }
            };
            rows.push_str(&format!("{name}\t{status}\n"));
        }
        let mut blockers: Vec<_> = blockers.into_iter().collect();
        blockers.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let mut report = format!(
            "BAML runtime coverage (normal AND GC stress): {} pass, {} unsupported, {} fail\nTelemetry off: required\nTelemetry low/medium/high/default: {}\n\nFirst unsupported constructs (fixtures blocked):\n",
            counts[0],
            counts[1],
            counts[2],
            if TELEMETRY_SUPPORTED {
                "required"
            } else {
                "not yet supported generally; required for standalone closure probes"
            }
        );
        for (construct, count) in blockers {
            report.push_str(&format!("{count}\t{construct}\n"));
        }
        report.push_str(&format!("\nPer probe / KPI workload:\n{rows}"));
        report
    }

    pub fn write(&self, build: &Path) {
        let report = self.report();
        let path = build.join("rt-coverage.txt");
        std::fs::write(&path, &report).unwrap();
        eprintln!("{report}\nCoverage report: {}", path.display());
    }
}

#[test]
fn only_explicit_admission_diagnostics_are_unsupported() {
    assert!(
        matches!(classify("compile: unsupported in baml_rt: float\nsecond".into()),
        Rejection::Unsupported(reason) if reason == "float")
    );
    for error in [
        "cargo failed",
        "pack succeeded without an executable",
        "unsupported elsewhere",
    ] {
        assert!(matches!(classify(error.into()), Rejection::Fail(_)));
    }
}

#[test]
fn coverage_counts_each_fixture_once_and_preserves_failures() {
    let artifacts = BTreeMap::from([
        ("pass".into(), Ok("binary".into())),
        ("crash".into(), Ok("binary".into())),
        ("skip".into(), Err(Rejection::Unsupported("float".into()))),
        ("build".into(), Err(Rejection::Fail("cargo failed".into()))),
    ]);
    let mut coverage = Coverage::new(&artifacts);
    coverage.record("crash", &["crash/rt-gc-stress: signal".into()]);
    coverage.record("skip", &[]);
    coverage.record("pass", &["pass/compiled-telemetry-high: mismatch".into()]);
    let report = coverage.report();
    assert!(report.contains("1 pass, 1 unsupported, 2 fail"));
    assert!(report.contains("1\tfloat"));
    assert_eq!(coverage.build_failures().len(), 2);
}

#[test]
fn standalone_closure_coverage_cannot_silently_become_unsupported() {
    let artifacts = BTreeMap::from([(
        "RtClosures".into(),
        Err(Rejection::Unsupported("captured local".into())),
    )]);
    let coverage = Coverage::new(&artifacts);
    assert_eq!(coverage.build_failures().len(), 1);
    assert!(coverage.report().contains("required rt coverage regressed"));
    for level in ["off", "low", "medium", "high", "default"] {
        assert!(supports_level("RtClosures", level));
        assert!(supports_level("RtEmptyClosures", level));
    }
}
