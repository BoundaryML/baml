//! The compiled-code fixtures lifted from the old runtime branch's
//! differential suite (`rust_diff/fixtures`): closures, tasks, natives,
//! operators, reflection, lists, exceptions, the testing stdlib, strings,
//! JSON, type dispatch, setup state and stack shapes, and the speedtest KPI
//! workloads. Each is packed with compiled bodies and every probe run in
//! every mode against bytecode at the same telemetry level, as
//! `rust_differential` runs its probes.
//! Run with `cargo test -p baml_tests --test rust_differential_fixtures --
//! --ignored`; `BAML_RUST_DIFF_PROBES` and `BAML_RUST_DIFF_DUMP` as there.
use std::{
    path::{Path, PathBuf},
    process::Command,
};

#[path = "../rust_diff/harness.rs"]
mod harness;

use harness::{MODES, Mode, NORMAL_AND_STRESS, Settings, compare_modes, run_modes};

fn manifest() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn workspace() -> &'static Path {
    manifest().parent().unwrap().parent().unwrap()
}

fn fixtures() -> PathBuf {
    manifest().join("rust_diff").join("fixtures")
}

fn build_dir(name: &str) -> PathBuf {
    let build = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("rust-differential-fixtures")
        .join(name);
    std::fs::create_dir_all(&build).unwrap();
    build
}

/// A new empty directory for one probe run.
fn fresh_dir() -> PathBuf {
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let run = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("rust-differential-fixtures")
        .join("runs")
        .join(format!("{}-{run}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The zero-argument functions of `source` whose names `keep` accepts.
fn zero_arg_functions(source: &str, keep: impl Fn(&str) -> bool) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| line.strip_prefix("function "))
        .filter_map(|rest| rest.split_once('('))
        .filter(|(_, rest)| rest.starts_with(')'))
        .map(|(name, _)| name.trim().to_string())
        .filter(|name| keep(name))
        .collect()
}

/// Run each of `probes` (an argument of `executable`, or none for a binary
/// with one entry) in `modes` and compare.
fn run_all(
    executable: &Path,
    probes: &[(String, Option<String>)],
    modes: &[Mode],
    settings: &Settings,
    failures: &mut Vec<String>,
) {
    for (name, arg) in probes.iter().filter(|(name, _)| settings.selects(name)) {
        // Each run starts in a fresh directory: file probes leave files.
        let make = || {
            let mut command = Command::new(executable);
            command.args(arg).current_dir(fresh_dir());
            command
        };
        let outputs = run_modes(name, modes, settings, make, failures);
        compare_modes(name, modes, &outputs, failures);
    }
}

fn project_sources(project: &Path) -> String {
    std::fs::read_dir(project.join("baml_src"))
        .unwrap()
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect()
}

/// Each project's `Rt*` and `RuntimeList*` probes, and every zero-argument
/// function of the exceptions project, as a subcommand binary.
#[test]
#[ignore = "builds generated Rust crates with cargo"]
fn rt_fixture_projects() {
    let settings = Settings::from_env();
    let mut failures = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(fixtures().join("projects"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.file_name().is_some_and(|name| name != "testing"))
        .collect();
    dirs.sort();
    for dir in dirs {
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let all = name == "exceptions";
        let probes = zero_arg_functions(&project_sources(&dir), |function| {
            all || function.starts_with("Rt") || function.starts_with("RuntimeList")
        });
        let build = build_dir(&name);
        let executable = build.join("probes");
        let mut args = vec!["--from".to_string(), dir.display().to_string()];
        for probe in &probes {
            args.extend(["--function".into(), probe.clone()]);
        }
        harness::pack(workspace(), &build, &executable, &args);
        let probes: Vec<_> = probes
            .into_iter()
            .map(|probe| (format!("{name}::{probe}"), Some(probe)))
            .collect();
        run_all(&executable, &probes, MODES, &settings, &mut failures);
    }
    assert!(failures.is_empty(), "{} probe failures", failures.len());
}

/// The single-file programs: `main`, or each uppercase zero-argument
/// function of a file without one.
#[test]
#[ignore = "builds generated Rust crates with cargo"]
fn rt_fixture_programs() {
    let settings = Settings::from_env();
    let mut failures = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(fixtures().join("programs"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    files.sort();
    for file in files {
        let name = file.file_stem().unwrap().to_string_lossy().into_owned();
        let source = std::fs::read_to_string(&file).unwrap();
        let mut probes = zero_arg_functions(&source, |function| function == "main");
        if probes.is_empty() {
            probes = zero_arg_functions(&source, |function| {
                function.starts_with(|c: char| c.is_ascii_uppercase())
            });
        }
        let build = build_dir(&name);
        let executable = build.join("probes");
        let mut args = vec!["--file".to_string(), file.display().to_string()];
        for probe in &probes {
            args.extend(["--function".into(), probe.clone()]);
        }
        harness::pack(workspace(), &build, &executable, &args);
        let probes: Vec<_> = probes
            .into_iter()
            .map(|probe| (format!("{name}::{probe}"), Some(probe)))
            .collect();
        run_all(&executable, &probes, MODES, &settings, &mut failures);
    }
    assert!(failures.is_empty(), "{} probe failures", failures.len());
}

/// The testing project's tests, packed with `--tests`: the test report is
/// the output.
#[test]
#[ignore = "builds a generated Rust crate with cargo"]
fn rt_fixture_testing() {
    let settings = Settings::from_env();
    let mut failures = Vec::new();
    let build = build_dir("testing");
    let executable = build.join("tests");
    let project = fixtures().join("projects").join("testing");
    let args = vec![
        "--tests".to_string(),
        "--from".to_string(),
        project.display().to_string(),
    ];
    harness::pack(workspace(), &build, &executable, &args);
    let probes = [("testing::tests".to_string(), None)];
    run_all(&executable, &probes, MODES, &settings, &mut failures);
    assert!(failures.is_empty(), "{} probe failures", failures.len());
}

/// The workloads the old branch's coverage ran as probes: one job each, by
/// its own `main`, normal and under GC stress at the default level and `off`.
const KPI_WORKLOADS: &[&str] = &[
    "classes/method-call-100k",
    "compute/fib32-recursive",
    "compute/generate-sort-8k",
    "compute/json-aggregate-2k",
    "compute/json-hello",
    "compute/json-ndjson-events",
    "compute/json-request-body",
    "compute/json-tree-path",
    "compute/k-nucleotide-25k",
    "compute/nested-loops-500x500",
    "compute/pure-call-1m",
    "concurrency/spawn-await-x10k",
    "interfaces/polymorphic-dispatch-100k",
    "memory/binary-trees-14",
    "memory/heap-calls",
    "process/startup",
    "programs/agent-loop-300",
    "programs/richards-50",
    "string/concat-loop-10k",
    "string/split-medium-literal-10k",
];

#[test]
#[ignore = "builds generated Rust crates with cargo"]
fn rt_kpi_workloads() {
    let settings = Settings::from_env();
    let mut failures = Vec::new();
    let workloads = workspace()
        .join("tools")
        .join("speedtest")
        .join("workloads");
    for workload in KPI_WORKLOADS {
        let source = workloads.join(workload).join("main.baml");
        assert!(source.is_file(), "missing workload {workload}");
        let name = workload.replace('/', "__");
        let build = build_dir(&name);
        let executable = build.join("workload");
        let args = vec![
            "--file".to_string(),
            source.display().to_string(),
            "--function".to_string(),
            "main".to_string(),
        ];
        harness::pack(workspace(), &build, &executable, &args);
        let probes = [(format!("kpi::{workload}"), Some("main".to_string()))];
        run_all(
            &executable,
            &probes,
            NORMAL_AND_STRESS,
            &settings,
            &mut failures,
        );
    }
    assert!(failures.is_empty(), "{} probe failures", failures.len());
}
