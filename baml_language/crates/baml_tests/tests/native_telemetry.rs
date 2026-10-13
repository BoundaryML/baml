//! Telemetry parity between the VM and natively compiled code.
//!
//! Each case runs one fixture function on the VM and natively, both
//! recording at `medium` and at `high`, decodes both recordings, and
//! compares what they say happened: per call path, named by its chain of
//! callees, how many calls completed and how many errored, were cancelled
//! or panicked.
//!
//! The native program observes only some functions (`bex_aot::telemetry`):
//! the others are folded and their time is their nearest observed
//! ancestor's. So the VM's tree, which records every BAML function, is
//! projected onto the native program's observed set first: a path whose
//! callee is folded is dropped, and a folded function vanishes from the
//! chains below it. What remains must match exactly.
//!
//! A recording starts once per process, so the native side of every case
//! runs in a child process of this test binary
//! (`native_recording_child`).
//!
//! `NATIVE_TELEMETRY_SHOW=1` prints each native summary (with
//! `--nocapture`).

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use baml_test_support::compile_source_with_opt;
use baml_tests::engine::OptLevel;
use bex_aot::{Int63, Thrown};
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, EngineConfig, FunctionCallContextBuilder,
    TelemetryRecording,
};
use btel_recorder::proto;
use btel_settings::artifact::{ArtifactTelemetry, RecordingLevel};
use sys_native::SysOpsExt;

macro_rules! generated_module {
    ($name:ident, $file:literal) => {
        #[allow(unreachable_pub, dead_code, clippy::all)]
        #[path = $file]
        mod $name;
    };
}

generated_module!(calls, "native/generated/calls.rs");
generated_module!(catches, "native/generated/catches.rs");
generated_module!(lambdas, "native/generated/lambdas.rs");
generated_module!(recursion, "native/generated/recursion.rs");
generated_module!(telemetry, "native/generated/telemetry.rs");
generated_module!(telemetry_all, "native/generated/telemetry_all.rs");
generated_module!(throws, "native/generated/throws.rs");

/// One call: the native module (a fixture's golden), the function and its
/// `int` argument. `telemetry_all` is the `telemetry` fixture with every
/// function observed.
const CASES: &[(&str, &str, i64)] = &[
    // Nested observed calls from two sites, through a folded function and
    // a callback, with errors caught; then a panic; then reentry.
    ("telemetry", "run", 4),
    ("telemetry", "run_panics", 5),
    ("telemetry", "recurse", 3),
    // The same with every function observed: the VM's tree, unprojected
    // but for the standard library.
    ("telemetry_all", "run", 4),
    ("telemetry_all", "run_panics", 5),
    ("telemetry_all", "recurse", 3),
    // An observed loop calling a folded leaf.
    ("calls", "sum_of_squares", 5),
    ("calls", "count_even", 6),
    // A panic through folded frames.
    ("calls", "step_twice", -3),
    // Folded recursion: only the root's thread span.
    ("recursion", "fib", 10),
    // Observed loops over recursive helpers.
    ("recursion", "deep_for_in", 5),
    ("recursion", "deep_map", 5),
    // Errors and a panic out of observed and folded frames.
    ("throws", "throw_in_loop", 2),
    ("throws", "throw_deep", -1),
    ("throws", "panic_deep", 5),
    // Errors caught inside observed loops.
    ("catches", "catch_in_loop", 4),
    ("catches", "catch_returns", 2),
    // Recursion through a function value.
    ("lambdas", "recursion_through_value", 5),
];

/// The native call for a case, as the child runs it.
fn native_call(fixture: &str, function: &str, arg: i64) -> Result<(), Thrown> {
    let n = Int63::new(arg).expect("an int");
    let unit = |result: Result<_, Thrown>| result.map(drop);
    match (fixture, function) {
        ("telemetry", "run") => unit(telemetry::user_run(n)),
        ("telemetry", "run_panics") => unit(telemetry::user_run_panics(n)),
        ("telemetry", "recurse") => unit(telemetry::user_recurse(n)),
        ("telemetry_all", "run") => unit(telemetry_all::user_run(n)),
        ("telemetry_all", "run_panics") => unit(telemetry_all::user_run_panics(n)),
        ("telemetry_all", "recurse") => unit(telemetry_all::user_recurse(n)),
        ("calls", "sum_of_squares") => unit(calls::user_sum_of_squares(n)),
        ("calls", "count_even") => unit(calls::user_count_even(n)),
        ("calls", "step_twice") => unit(calls::user_step_twice(n)),
        ("recursion", "fib") => unit(recursion::user_fib(n)),
        ("recursion", "deep_for_in") => unit(recursion::user_deep_for_in(n)),
        ("recursion", "deep_map") => unit(recursion::user_deep_map(n)),
        ("throws", "throw_in_loop") => unit(throws::user_throw_in_loop(n)),
        ("throws", "throw_deep") => unit(throws::user_throw_deep(n)),
        ("throws", "panic_deep") => unit(throws::user_panic_deep(n)),
        ("catches", "catch_in_loop") => unit(catches::user_catch_in_loop(n)),
        ("catches", "catch_returns") => unit(catches::user_catch_returns(n)),
        ("lambdas", "recursion_through_value") => unit(lambdas::user_recursion_through_value(n)),
        _ => panic!("no native call for {fixture}.{function}"),
    }
}

fn native_table(fixture: &str) -> &'static [bex_aot::telemetry::Function] {
    match fixture {
        "calls" => calls::BAML_TELEMETRY,
        "catches" => catches::BAML_TELEMETRY,
        "lambdas" => lambdas::BAML_TELEMETRY,
        "recursion" => recursion::BAML_TELEMETRY,
        "throws" => throws::BAML_TELEMETRY,
        "telemetry" => telemetry::BAML_TELEMETRY,
        "telemetry_all" => telemetry_all::BAML_TELEMETRY,
        _ => panic!("no native table for {fixture}"),
    }
}

/// The call paths with one chain: how many distinct paths (one per call
/// site and visible caller), and their completions: count, errored,
/// cancelled, panicked.
type Outcomes = [u64; 5];

/// What a recording says, per call path: the chain of observed callees
/// from the root (with the reentry flag of the path's own node) to its
/// outcomes. Paths whose callee is not observed are left out.
type Summary = BTreeMap<(Vec<String>, bool), Outcomes>;

/// Decode the one recording under `dir` and summarize it over `observed`.
fn summarize(dir: &Path, observed: &HashSet<String>) -> Summary {
    let recording = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| path.is_dir() && path.file_name().is_some_and(|name| name != "cas"))
        .unwrap_or_else(|| panic!("no recording under {}", dir.display()));
    let read = btel_file::read_directory(&recording).expect("readable recording");
    assert!(
        read.issues.is_empty(),
        "recording issues: {:?}",
        read.issues
    );
    assert!(read.has_recording_end, "the recording ended");
    let mut names: HashMap<u64, String> = HashMap::new();
    let mut paths: HashMap<u32, (u32, u64)> = HashMap::new();
    let mut deltas: Vec<proto::AggregateDelta> = Vec::new();
    for file in &read.files {
        if let Some(definitions) = &file.definitions {
            for function in &definitions.functions {
                if let Some(proto::function_definition::Resolution::Metadata(metadata)) =
                    &function.resolution
                {
                    names.insert(function.function_id, metadata.fqn.clone());
                }
            }
            for path in &definitions.call_paths {
                paths.insert(
                    path.call_path_id,
                    (path.parent_call_path_id, path.callee_function_id),
                );
            }
        }
        if let Some(aggregates) = &file.aggregates {
            deltas.extend(aggregates.entries.iter().cloned());
        }
    }
    let chain = |mut path: u32| -> Vec<String> {
        let mut chain = Vec::new();
        while path != 0 {
            let (parent, callee) = paths[&path];
            let name = names.get(&callee).cloned().unwrap_or_default();
            if observed.contains(&name) {
                chain.push(name);
            }
            path = parent;
        }
        chain.reverse();
        chain
    };
    let mut summary = Summary::new();
    let mut counted: HashSet<u32> = HashSet::new();
    for delta in deltas {
        let path = u32::try_from(delta.node >> 1).expect("a 32-bit call path");
        let reentry = delta.node & 1 == 1;
        let callee = names.get(&paths[&path].1).cloned().unwrap_or_default();
        if !observed.contains(&callee) {
            continue;
        }
        let outcomes = delta.outcomes.unwrap_or_default();
        let entry = summary.entry((chain(path), reentry)).or_default();
        if counted.insert(path << 1 | u32::from(reentry)) {
            entry[0] += 1;
        }
        entry[1] += delta.count;
        entry[2] += outcomes.errored;
        entry[3] += outcomes.cancelled;
        entry[4] += outcomes.panicked.unwrap_or(0);
    }
    summary
}

fn fixture_source(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/native/fixtures")
        .join(format!("{name}.baml"));
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// Run a case on the VM, recording at `level` into `dir`.
async fn vm_recording(fixture: &str, function: &str, arg: i64, level: RecordingLevel, dir: &Path) {
    let program = compile_source_with_opt(&fixture_source(fixture), OptLevel::One);
    let engine = Arc::new(
        BexEngine::new_with_config(
            program,
            Arc::new(sys_ops::SysOps::native()),
            Vec::new(),
            EngineConfig {
                artifact_telemetry: Some(ArtifactTelemetry {
                    recording_level: level,
                    ..Default::default()
                }),
                recording: Some(TelemetryRecording::local_files_in(
                    dir,
                    btel_recorder::RecordingConfig::default(),
                )),
                runtime_compiler: Some(bex_project::runtime_compiler()),
                ..Default::default()
            },
        )
        .expect("engine"),
    );
    // Twice, as the native side makes two root calls on one recording.
    for _ in 0..2 {
        let _ = engine
            .call_function_bound_args(
                &format!("user.{function}"),
                vec![BexCallArg::Provided(Box::new(BexExternalValue::Int(arg)))],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await;
    }
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .expect("engine shutdown");
    assert_eq!(engine.telemetry_result(), Some(Ok(())), "the VM recorded");
}

/// Run a case natively in a child process, recording at `level` into `dir`.
fn native_recording(fixture: &str, function: &str, arg: i64, level: &str, dir: &Path) {
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .env("BAML_TELEMETRY", level)
        .env(
            "NATIVE_TELEMETRY_CASE",
            format!("{fixture} {function} {arg}"),
        )
        .env("NATIVE_TELEMETRY_DIR", dir)
        .args([
            "--exact",
            "native_recording_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .output()
        .expect("child test");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("1 passed"),
        "the native child failed:\n{}\n{stdout}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The native side of one case: start a recording, make the call twice as
/// the host would, finish the recording.
#[test]
#[ignore = "run by `native_recording_matches_the_vm` in a child process"]
fn native_recording_child() {
    let case = baml_env::os_var("NATIVE_TELEMETRY_CASE")
        .and_then(|case| case.into_string().ok())
        .expect("a case");
    let mut parts = case.split(' ');
    let (fixture, function, arg) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap().parse::<i64>().unwrap(),
    );
    let dir = PathBuf::from(baml_env::os_var("NATIVE_TELEMETRY_DIR").expect("a directory"));
    bex_aot::telemetry::start(
        native_table(fixture),
        bex_aot::telemetry::Destination::Directory(dir),
    )
    .expect("telemetry starts");
    // Two root calls: the producer keeps one execution scope across them.
    for _ in 0..2 {
        let _ = bex_aot::telemetry::root(|| native_call(fixture, function, arg));
    }
    bex_aot::telemetry::finish().expect("telemetry finishes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_recording_matches_the_vm() {
    let mut failures = Vec::new();
    for &(fixture, function, arg) in CASES {
        let observed: HashSet<String> = native_table(fixture)
            .iter()
            .filter(|function| function.observed)
            .map(|function| function.fqn.to_owned())
            .collect();
        for (level, name) in [
            (RecordingLevel::Medium, "medium"),
            (RecordingLevel::High, "high"),
        ] {
            let vm_dir = tempfile::tempdir().unwrap();
            let native_dir = tempfile::tempdir().unwrap();
            // `telemetry_all` is the `telemetry` fixture's source.
            let source = fixture.strip_suffix("_all").unwrap_or(fixture);
            vm_recording(source, function, arg, level, vm_dir.path()).await;
            native_recording(fixture, function, arg, name, native_dir.path());
            let vm = summarize(vm_dir.path(), &observed);
            let native = summarize(native_dir.path(), &observed);
            if baml_env::os_var("NATIVE_TELEMETRY_SHOW").is_some() {
                eprintln!("{fixture}.{function}({arg}) at {name}: {native:?}");
            }
            if vm != native {
                failures.push(format!(
                    "{fixture}.{function}({arg}) at {name}:\n  VM:     {vm:?}\n  native: {native:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The observed set is what the design says: functions that loop or are
/// large; a small straight-line function is folded and marked inlined.
#[test]
fn small_functions_fold_and_loops_are_observed() {
    let observed = |name: &str| {
        calls::BAML_TELEMETRY
            .iter()
            .find(|function| function.fqn == name)
            .unwrap_or_else(|| panic!("{name} in the table"))
            .observed
    };
    assert!(observed("user.sum_of_squares"), "a loop is observed");
    assert!(!observed("user.square"), "a one-line leaf is folded");
}
