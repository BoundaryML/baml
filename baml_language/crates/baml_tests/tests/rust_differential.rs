//! Packs the probe programs once with compiled bodies, then runs each as
//! bytecode, compiled, compiled under GC stress, and compiled with a VM frame
//! for every call (`BAML_COMPILED_FRAMES=1`, as before frame records), at the
//! default telemetry level and at `off`, `low`, `medium` and `high`. Each must
//! agree with bytecode at the same telemetry level.
//! Run with `cargo test -p baml_tests --test rust_differential -- --ignored`;
//! `BAML_RUST_DIFF_PROBES=A,B` selects probes and `BAML_RUST_DIFF_DUMP=1`
//! prints every run's output.
use std::{
    io,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

#[path = "../rust_diff/harness.rs"]
mod harness;
#[path = "../rust_diff/initializer_probes.rs"]
mod initializer_probes;

use harness::{MODES, OUTPUT_LIMIT, Settings, capture, compare_modes, run_modes, run_probe};

const PROBES: &[&str] = &[
    "IteratorInterfaceIdentity",
    "EscapeCaptureAlias",
    "EscapeOrderedReturn",
    "EscapeSessionHandoff",
    "EscapeOrderedSpawn",
    "EscapeAwaitResult",
    "EscapeAwaitShared",
    "EscapeThrowCaught",
    "EscapeGlobal",
    "EscapePublishedField",
    "EscapePushContents",
    "EscapeMapSetContents",
    "EscapeUnshiftContents",
    "EscapeInsertContents",
    "EscapeFinalizer",
    "EscapeSpawn",
    "EscapeSpawnInCallee",
    "EscapeClosure",
    "EscapeField",
    "EscapeStoredDeep",
    "EscapeMapValue",
    "EscapeMap",
    "EscapeInsert",
    "EscapeMaskMixed",
    "EscapeMaskClosure",
    "EscapeReturnedAlias",
    "EscapeRecursive",
    "EscapeIterator",
    "ArrayWalk",
    "ThreadLocalParallel",
    "Scalars",
    "Overflow",
    "Containers",
    "Shapes",
    "Closures",
    "Interfaces",
    "Contexts",
    "Uncaught",
    "Async",
    "Cancellation",
    "TimeoutCancellation",
    "RecursiveCancellation",
    "CatchAllocation",
    "Allocation",
    "Logs",
    "Depth",
    "Iterators",
    "BoundTypes",
    "RuntimeClass",
    "Hidden",
    "Hooked",
    "Spawns",
    "Equality",
    "DeepErrors",
    "DeepUncaught",
    "FrameLimit",
    "AwaitChain",
    "CancelDeep",
    "Shielded",
    "GenericClosures",
    "SpawnPanicCaught",
    "SpawnPanicUncaught",
    "SpawnThrowUncaught",
    "SpawnPanicDetached",
    "SpawnChains",
    "AwaitErrors",
    "DeferUnwind",
    "DeferUnwindUncaught",
    "NoLeaks",
    "CallbackDepth",
    "InterfaceErrors",
    "GenericChains",
    "HookedDeep",
    "LogsDeep",
    "CancelDuringUnwind",
    "TaskOverflow",
    "CauseChains",
    "AnyWithFailure",
    "SpawnedClosures",
    "ManualSpan",
    "CatchWildcardDivision",
    "CatchWildcardIndex",
    "CatchWildcardReassigned",
    "CatchBindJumpTable",
    "CatchBindChain",
    "CatchMixedUnion",
    "CatchWildcardThrownPanic",
    "CatchAllThrownPanic",
    "CatchUnmatchedChain",
    "CatchWildcardThenBoundary",
    "CatchOrPanics",
    "CatchNonstreamCancellation",
    "CatchStreamingCancellation",
    "Initializers",
    "InitializerClosures",
    "InitializerErrors",
    "InitializerUncaught",
    "InitializerPanic",
    "InitializerPending",
    "InitializerTests",
    "InitializerTestErrors",
    "GlobalCalls",
    "GlobalCallPending",
];

#[test]
fn capture_limits_output_and_drains_stream() {
    let mut stream = io::Cursor::new(vec![b'x'; OUTPUT_LIMIT + 8192]);
    let (bytes, exceeded) = capture(&mut stream).unwrap();
    assert_eq!(bytes, vec![b'x'; OUTPUT_LIMIT]);
    assert!(exceeded);
    assert_eq!(stream.position(), (OUTPUT_LIMIT + 8192) as u64);
}

#[test]
#[cfg(unix)]
fn probe_timeout_kills_and_reaps() {
    let error = run_probe(
        Command::new("sh").args(["-c", "exec sleep 60"]),
        Duration::from_millis(20),
    )
    .unwrap_err();
    assert!(error.starts_with("timed out after"), "{error}");
    assert!(error.contains("kill: Ok(()), reap: Ok("), "{error}");
}

#[test]
#[ignore = "builds a generated Rust crate with cargo"]
fn rust_differential() {
    let settings = Settings::from_env();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let build = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rust-differential");
    std::fs::create_dir_all(&build).unwrap();
    let executable = build.join(format!("probes{}", std::env::consts::EXE_SUFFIX));
    let mut args = vec![
        "--from".to_string(),
        manifest.join("rust_diff").display().to_string(),
    ];
    for probe in PROBES
        .iter()
        .filter(|probe| !initializer_probes::collects_tests(probe))
    {
        args.extend(["--function".into(), (*probe).into()]);
    }
    args.extend(["--function".into(), "InitializerTestSummary".into()]);
    let started = Instant::now();
    harness::pack(workspace, &build, &executable, &args);
    let initializer_binaries =
        initializer_probes::build(&build, workspace, manifest, settings.only.as_deref());
    eprintln!("compiled pack wall time: {:?}", started.elapsed());
    let started = Instant::now();
    let mut failures = Vec::new();
    for probe in PROBES.iter().filter(|probe| settings.selects(probe)) {
        let make = || {
            let mut command = Command::new(initializer_binaries.get(probe).unwrap_or(&executable));
            if !initializer_probes::collects_tests(probe) {
                command.arg(probe);
            }
            command
        };
        let outputs = run_modes(probe, MODES, &settings, make, &mut failures);
        initializer_probes::assert_outputs(probe, &outputs);
        compare_modes(probe, MODES, &outputs, &mut failures);
    }
    eprintln!("all probe runs wall time: {:?}", started.elapsed());
    assert!(failures.is_empty(), "{} probe failures", failures.len());
}
