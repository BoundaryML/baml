//! Packs the probe programs once with compiled bodies, then runs each as
//! bytecode, compiled, compiled under GC stress, and compiled with a VM frame
//! for every call (`BAML_COMPILED_FRAMES=1`, as before frame records), at the
//! default telemetry level, with telemetry off and at `high`. Each must agree
//! with bytecode at the same telemetry level.
//! Run with `cargo test -p baml_tests --test rust_differential -- --ignored`;
//! `BAML_RUST_DIFF_PROBES=A,B` selects probes and `BAML_RUST_DIFF_DUMP=1`
//! prints every run's output.
use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

#[path = "../rust_diff/initializer_probes.rs"]
mod initializer_probes;

const PROBES: &[&str] = &[
    "EscapeSpawn",
    "EscapeSpawnInCallee",
    "EscapeClosure",
    "EscapeField",
    "EscapeStoredDeep",
    "EscapeMapValue",
    "EscapeMap",
    "EscapeReturnedAlias",
    "EscapeRecursive",
    "EscapeIterator",
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

/// Name, `BAML_COMPILED_BODIES`, `BAML_GC_STRESS`, `BAML_COMPILED_FRAMES`,
/// and `BAML_TELEMETRY` (`None` for the default level). Bytecode at each
/// telemetry level is the reference for that level.
type Mode = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);
const MODES: &[Mode] = &[
    ("bytecode", "0", "0", "0", None),
    ("compiled", "1", "0", "0", None),
    ("gc-stress", "1", "1", "0", None),
    ("vm-frames", "1", "0", "1", None),
    ("vm-frames-gc-stress", "1", "1", "1", None),
    ("bytecode-telemetry-off", "0", "0", "0", Some("off")),
    ("compiled-telemetry-off", "1", "0", "0", Some("off")),
    ("gc-stress-telemetry-off", "1", "1", "0", Some("off")),
    ("vm-frames-telemetry-off", "1", "0", "1", Some("off")),
    ("bytecode-telemetry-high", "0", "0", "0", Some("high")),
    ("compiled-telemetry-high", "1", "0", "0", Some("high")),
    ("gc-stress-telemetry-high", "1", "1", "0", Some("high")),
    ("vm-frames-telemetry-high", "1", "0", "1", Some("high")),
];

const OUTPUT_LIMIT: usize = 1024 * 1024;

fn capture(mut stream: impl Read) -> io::Result<(Vec<u8>, bool)> {
    let mut bytes = Vec::new();
    let mut exceeded = false;
    let mut buffer = [0; 8192];
    loop {
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Ok((bytes, exceeded));
        }
        let keep = count.min(OUTPUT_LIMIT - bytes.len());
        bytes.extend_from_slice(&buffer[..keep]);
        exceeded |= keep < count;
        // Keep draining after the limit so a full pipe cannot block the child.
    }
}

fn run_probe(command: &mut Command, timeout: Duration) -> Result<Output, String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn failed: {error}"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stdout = thread::spawn(move || capture(stdout));
    let stderr = thread::spawn(move || capture(stderr));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(10)),
            result => {
                let reason = match result {
                    Ok(None) => format!("timed out after {timeout:?}"),
                    Err(error) => format!("wait failed: {error}"),
                    Ok(Some(_)) => unreachable!(),
                };
                let killed = child.kill();
                let reaped = child.wait();
                break Err(format!("{reason} (kill: {killed:?}, reap: {reaped:?})"));
            }
        }
    };
    let stdout = stdout.join().map_err(|_| "stdout reader panicked")?;
    let stderr = stderr.join().map_err(|_| "stderr reader panicked")?;
    let status = status?;
    let (stdout, stdout_exceeded) = stdout.map_err(|error| format!("stdout: {error}"))?;
    let (stderr, stderr_exceeded) = stderr.map_err(|error| format!("stderr: {error}"))?;
    if stdout_exceeded || stderr_exceeded {
        return Err(format!(
            "output exceeded {OUTPUT_LIMIT} bytes (stdout: {stdout_exceeded}, stderr: {stderr_exceeded})"
        ));
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

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
    #[allow(clippy::disallowed_methods, reason = "test harness configuration")]
    let timeout = std::env::var("BAML_RUST_DIFF_TIMEOUT_SECS")
        .map(|value| {
            value
                .parse::<u64>()
                .expect("BAML_RUST_DIFF_TIMEOUT_SECS must be seconds")
        })
        .unwrap_or(120);
    assert!(timeout > 0, "BAML_RUST_DIFF_TIMEOUT_SECS must be positive");
    let timeout = Duration::from_secs(timeout);
    #[allow(clippy::disallowed_methods, reason = "test harness configuration")]
    let only = std::env::var("BAML_RUST_DIFF_PROBES").ok();
    #[allow(clippy::disallowed_methods, reason = "test harness configuration")]
    let dump = std::env::var("BAML_RUST_DIFF_DUMP").is_ok_and(|value| value == "1");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let build = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rust-differential");
    std::fs::create_dir_all(&build).unwrap();
    let executable = build.join(format!("probes{}", std::env::consts::EXE_SUFFIX));
    let mut argv = vec![
        "baml".into(),
        "--agent-skill-check".into(),
        "off".into(),
        "pack".into(),
        "--rust".into(),
        "--rust-profile".into(),
        "fast".into(),
        "--rust-workspace".into(),
        workspace.display().to_string(),
        "--rust-build-dir".into(),
        build.display().to_string(),
        "--from".into(),
        manifest.join("rust_diff").display().to_string(),
        "-o".into(),
        executable.display().to_string(),
    ];
    for probe in PROBES
        .iter()
        .filter(|probe| !initializer_probes::collects_tests(probe))
    {
        argv.extend(["--function".into(), (*probe).into()]);
    }
    argv.extend(["--function".into(), "InitializerTestSummary".into()]);
    let started = Instant::now();
    let result = baml_cli::run_cli(argv).expect("compiled pack must succeed");
    assert!(matches!(result, baml_cli::ExitCode::Success), "{result:?}");
    let initializer_binaries =
        initializer_probes::build(&build, workspace, manifest, only.as_deref());
    eprintln!("compiled pack wall time: {:?}", started.elapsed());
    let started = Instant::now();
    let mut failures = Vec::new();
    let selected = PROBES.iter().filter(|probe| {
        only.as_deref()
            .is_none_or(|only| only.split(',').any(|o| o == **probe))
    });
    for probe in selected {
        let mut outputs = Vec::new();
        for (mode, bodies, stress, frames, telemetry) in MODES {
            let start = Instant::now();
            let mut command = Command::new(initializer_binaries.get(probe).unwrap_or(&executable));
            if !initializer_probes::collects_tests(probe) {
                command.arg(probe);
            }
            command
                .env("BAML_COMPILED_BODIES", bodies)
                .env("BAML_GC_STRESS", stress)
                .env("BAML_COMPILED_FRAMES", frames)
                .env("NO_COLOR", "1");
            match telemetry {
                Some(level) => command.env("BAML_TELEMETRY", level),
                None => command.env_remove("BAML_TELEMETRY"),
            };
            let output = run_probe(&mut command, timeout);
            match &output {
                Ok(output) => {
                    eprintln!("{probe}/{mode}: {} ({:?})", output.status, start.elapsed());
                    // Tokio worker panics need not change the process exit status.
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if stderr.contains("panicked at") {
                        let failure = format!("{probe}/{mode}: Rust panic on stderr:\n{stderr}");
                        eprintln!("{failure}");
                        failures.push(failure);
                    }
                }
                Err(error) => {
                    let failure = format!("{probe}/{mode}: {error}");
                    eprintln!("{failure}");
                    failures.push(failure);
                }
            }
            if dump && let Ok(output) = &output {
                eprintln!(
                    "--- {probe}/{mode} stdout:\n{}--- stderr:\n{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            outputs.push(output.ok());
        }
        initializer_probes::assert_outputs(probe, &outputs);
        for ((mode, bodies, _, _, telemetry), actual) in MODES.iter().zip(&outputs) {
            // Each mode against bytecode at its own telemetry level: hooks and
            // spans legitimately differ between levels.
            if *bodies == "0" {
                continue;
            }
            let reference = MODES
                .iter()
                .position(|(_, bodies, _, _, level)| *bodies == "0" && level == telemetry)
                .expect("each telemetry level has a bytecode reference");
            let (Some(expected), Some(actual)) = (&outputs[reference], actual) else {
                continue;
            };
            if expected.status != actual.status
                || expected.stdout != actual.stdout
                || expected.stderr != actual.stderr
            {
                let mut diff = format!(
                    "{probe}/{mode}: bytecode {} vs {}\n",
                    expected.status, actual.status
                );
                for (stream, a, b) in [
                    ("stdout", &expected.stdout, &actual.stdout),
                    ("stderr", &expected.stderr, &actual.stderr),
                ] {
                    diff.push_str(&format!(
                        "{stream}:\n{}",
                        similar::TextDiff::from_lines(
                            &String::from_utf8_lossy(a),
                            &String::from_utf8_lossy(b)
                        )
                        .unified_diff()
                        .header("bytecode", mode)
                    ));
                }
                eprintln!("{diff}");
                failures.push(diff);
            }
        }
    }
    eprintln!("all probe runs wall time: {:?}", started.elapsed());
    assert!(failures.is_empty(), "{} probe failures", failures.len());
}
