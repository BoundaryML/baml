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
    level == "off"
        || TELEMETRY_SUPPORTED
        || REQUIRED_PROBES.contains(&probe)
        || probe.starts_with("strings::")
        || probe.starts_with("RuntimeListJoin")
}

// List coverage baseline: each admitted fixture must keep passing both runtime
// variants. An admission regression is a failure, never an unsupported skip.
const REQUIRED_LIST_COVERAGE: &[&str] = &[
    "RuntimeListJoinOverrides",
    "RuntimeListJoinMutation",
    "RuntimeListJoinThrow",
    "RuntimeListRopes",
    "RuntimeListCallbacks",
    "RuntimeListPublicSort",
    "RuntimeListSortClasses",
    "RuntimeListCallbackThrow",
    "RuntimeListSortThrow",
    "RuntimeListTaggedSort",
    "RuntimeListSortMixedError",
    "RuntimeListSortNullError",
    "RuntimeListSortBoolError",
    "RuntimeListPointerCallbacks",
    "RuntimeListJsonMaps",
    "RuntimeListJsonMapNonFinite",
    "RuntimeListJsonMapError",
    "RuntimeListJsonMapKeyError",
    "RuntimeListJsonAliases",
    "RuntimeListJsonMapCycle",
    "RuntimeListGenericJson",
    "RuntimeListNarrowedFields",
    "RuntimeListSortSingletons",
    "RuntimeListSortMutation",
    "RuntimeListEmptyCallbacks",
    "RuntimeListSavedTypeTest",
    "RuntimeListValidationThrow",
    "RuntimeListScalarBoundaries",
    "RuntimeListInvalidCodePoint",
    "RuntimeListFloatTruncationError",
    "RuntimeListStringEntry",
    "RuntimeListFloatConstants",
    "RuntimeLists",
    "RuntimeListBounds",
    "RuntimeListStoreBounds",
    "RuntimeListJson",
    "RuntimeListJsonField",
    "RuntimeListJsonError",
    "RuntimeListJsonOptional",
    "RuntimeListJsonOptionalError",
    "RuntimeListBuiltins",
    "RuntimeListInsertError",
    "RuntimeListMaps",
    "RuntimeListRecursiveJson",
    "RuntimeListLiteralJson",
    "RuntimeListLiteralJsonError",
    "RuntimeListJsonFieldError",
    "RuntimeListReductions",
    "RuntimeListReductionOverflow",
    "RuntimeListMedianError",
    "RuntimeListStructuralJoin",
    "RuntimeListFloatSpecials",
    "RuntimeListJsonNonFinite",
    "compute::array build sum 100k",
    "compute::array traversal 8k",
    "compute::merge sort 20k",
    "compute::quick sort 3-way 1k",
    "memory::live walk",
    "memory::splay 2k",
];
fn require_list_coverage<T>(name: &str, result: Result<T, Rejection>) -> Result<T, Rejection> {
    match result {
        Err(Rejection::Unsupported(reason)) if REQUIRED_LIST_COVERAGE.contains(&name) => Err(
            Rejection::Fail(format!("list coverage regression: {reason}")),
        ),
        other => other,
    }
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
    let mut initializer_rejection: Option<Rejection> = None;
    for probe in PROBES {
        if only.is_some_and(|only| !only.split(',').any(|name| name == *probe)) {
            continue;
        }
        // Initializer admission is checked for the whole source project before
        // entry reachability. Rechecking every entry cannot change this result.
        // Standalone fixtures use a different project and must still be packed.
        if *probe != "StackAllocation"
            && !super::is_list_probe(probe)
            && super::standalone_closure_source(manifest, probe).is_none()
            && let Some(rejection) = &initializer_rejection
        {
            artifacts.insert((*probe).into(), Err(rejection.clone()));
            continue;
        }
        let executable = build.join(probe);
        let standalone = build.join("stack-allocation-source");
        let source = if *probe == "StackAllocation" {
            std::fs::create_dir_all(standalone.join("baml_src")).unwrap();
            std::fs::copy(
                manifest.join("rust_diff/baml.toml"),
                standalone.join("baml.toml"),
            )
            .unwrap();
            std::fs::copy(
                manifest.join("rust_diff/baml_src/stack_allocation.baml"),
                standalone.join("baml_src/stack_allocation.baml"),
            )
            .unwrap();
            standalone
        } else if let Some(source) = super::standalone_closure_source(manifest, probe) {
            source
        } else if super::is_list_probe(probe) {
            manifest.join("rust_diff/list_probes")
        } else {
            manifest.join("rust_diff")
        };
        let result = pack_result(
            build,
            workspace,
            &source,
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
        if *probe != "StackAllocation"
            && !super::is_list_probe(probe)
            && super::standalone_closure_source(manifest, probe).is_none()
            && matches!(&result, Err(Rejection::Unsupported(reason)) if reason == "top-level initializers")
        {
            initializer_rejection = result.as_ref().err().cloned();
        }
        artifacts.insert((*probe).into(), require_list_coverage(probe, result));
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
        let result = require_list_coverage(&fixture.name, result);
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
                Err(Rejection::Unsupported(reason))
                    if name.starts_with("strings::") || name.starts_with("string::") =>
                {
                    Some(format!(
                        "{name}/rt: required string fixture unsupported: {reason}"
                    ))
                }
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
            "BAML runtime coverage (normal AND GC stress): {} pass, {} unsupported, {} fail\nTelemetry off: required\nString probes: off/low/medium/high/default required\nTelemetry low/medium/high/default for other probes: {}\n\nFirst unsupported constructs (fixtures blocked):\n",
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
fn list_coverage_baseline_rejects_admission_regressions() {
    assert!(matches!(
        require_list_coverage::<()>(
            "RuntimeLists",
            Err(Rejection::Unsupported("list element".into()))
        ),
        Err(Rejection::Fail(_))
    ));
    assert!(matches!(
        require_list_coverage::<()>("Closures", Err(Rejection::Unsupported("closures".into()))),
        Err(Rejection::Unsupported(_))
    ));
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

#[test]
fn string_admission_regressions_fail_the_gate() {
    let artifacts = BTreeMap::from([
        (
            "strings::semantics".into(),
            Err(Rejection::Unsupported("builtin".into())),
        ),
        (
            "string::concat loop 10k".into(),
            Err(Rejection::Unsupported("loop".into())),
        ),
        (
            "other".into(),
            Err(Rejection::Unsupported("other job".into())),
        ),
    ]);
    assert_eq!(Coverage::new(&artifacts).build_failures().len(), 2);
}
