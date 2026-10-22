//! Packs the probe programs once with compiled bodies, then runs each as
//! bytecode, compiled, compiled under GC stress, and compiled with a VM frame
//! for every call (`BAML_COMPILED_FRAMES=1`, as before frame records), at the
//! default telemetry level, with telemetry off and at `high`. Each must agree
//! with bytecode at the same telemetry level.
//! Run with `cargo test -p baml_tests --test rust_differential -- --ignored`;
//! `BAML_RUST_DIFF_PROBES=A,B` selects probes and `BAML_RUST_DIFF_DUMP=1`
//! prints every run's output.
//! `BAML_RUST_DIFF_OPT_LEVEL=0|1|2` selects the emitted optimization level (default 2).
use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

#[path = "../rust_diff/collect_tests.rs"]
mod collect_tests;

const PROBES: &[&str] = &[
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
    "RecursiveCancellation",
    "CatchAllocation",
    "Recursion",
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
    "CatchWildcardThenNamed",
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

fn is_test_initializer_probe(probe: &str) -> bool {
    matches!(probe, "InitializerTests" | "InitializerTestErrors")
}

fn build_test_initializer_probe(build: &Path, workspace: &Path) -> std::path::PathBuf {
    // The included module is also compiled as part of this test target.
    let _ = collect_tests::main;
    std::fs::write(
        build.join("src/collect_tests.rs"),
        include_str!("../rust_diff/collect_tests.rs"),
    )
    .unwrap();
    std::fs::write(
        build.join("src/initializer_tests.rs"),
        r#"mod collect_tests;

fn main() -> std::process::ExitCode {
    collect_tests::main(
        baml_rust_program::PROGRAM_ARTIFACT,
        &baml_rust_program::COMPILED_BODIES,
    )
}

"#,
    )
    .unwrap();
    let manifest_path = build.join("Cargo.toml");
    let mut manifest = std::fs::read_to_string(&manifest_path).unwrap();
    for dependency in ["baml_artifact", "baml_exec", "bex_engine", "sys_native"] {
        manifest.push_str(&format!(
            "\n[dependencies.{dependency}]\npath = {:?}\n",
            workspace
                .join("crates")
                .join(dependency)
                .display()
                .to_string()
        ));
    }
    manifest.push_str(
        "\n[dependencies.tokio]\nversion = \"1.36\"\nfeatures = [\"rt-multi-thread\"]\n\
         \n[[bin]]\nname = \"initializer-tests\"\npath = \"src/initializer_tests.rs\"\n",
    );
    std::fs::write(&manifest_path, manifest).unwrap();
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", "initializer-tests"])
        .arg("--manifest-path")
        .arg(manifest_path)
        .arg("--target-dir")
        .arg(build.join("target"))
        .current_dir(workspace)
        .status()
        .expect("build test-initializer probe");
    assert!(status.success(), "test-initializer probe must compile");
    build
        .join("target/release")
        .join(format!("initializer-tests{}", std::env::consts::EXE_SUFFIX))
}

fn build_probe_variant(
    build: &Path,
    workspace: &Path,
    manifest: &Path,
    template: &baml_exec::PackEnvelope,
    mode: &str,
    opt: baml_compiler2_emit::OptLevel,
) -> std::path::PathBuf {
    // `baml pack` uses O2 and creates an engine for signature resolution.
    // Other levels and failing startups need direct emission instead.
    let mut sources: Vec<_> = std::fs::read_dir(manifest.join("rust_diff/baml_src"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                std::fs::read_to_string(entry.path()).unwrap().replace(
                    "const INIT_FAILURE = \"none\";",
                    &format!("const INIT_FAILURE = {mode:?};"),
                ),
            )
        })
        .collect();
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    let files: Vec<_> = sources
        .iter()
        .map(|(name, source)| (name.as_str(), source.as_str()))
        .collect();
    let db = baml_test_support::setup_multi_file_db(&files);
    let root = db.workspace_root().unwrap();
    let mut envelope = template.clone();
    envelope.program = baml_db::compile_program(&db, root, opt).unwrap();
    let package = &envelope.program.packages[envelope.program.root as usize].name;
    for target in &mut envelope.targets {
        let name = target.qualified_name.rsplit('.').next().unwrap();
        target.qualified_name = format!("{package}.{name}");
    }
    let callables = envelope.program.rendered_callables();
    let entries: Vec<_> = envelope
        .targets
        .iter()
        .map(|target| callables[&target.qualified_name].object.raw())
        .collect();
    let (artifact, fingerprint) =
        baml_artifact::encode_with_digest(baml_artifact::ArtifactKind::PackedProgram, &envelope)
            .unwrap();
    let module = baml_db::rust_backend::compile_program(
        &db,
        root,
        &envelope.program,
        opt,
        "program.bexa",
        fingerprint,
        Some(&entries),
    )
    .unwrap();
    for name in ["GlobalCalls", "GlobalCallPending"] {
        let object = callables[&format!("{package}.{name}")].object.raw();
        assert!(
            module.functions.iter().any(|f| f.object == object),
            "{name} at {opt:?}: {:?}",
            module.rejected
        );
    }
    std::fs::write(build.join("src/program.bexa"), artifact).unwrap();
    std::fs::write(build.join("src/lib.rs"), module.source).unwrap();
    let status = Command::new("cargo")
        .args(["build", "--release", "--bin", "baml_rust_program"])
        .arg("--manifest-path")
        .arg(build.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(build.join("target"))
        .current_dir(workspace)
        .status()
        .expect("build probe variant");
    assert!(status.success(), "probe variant must compile");
    let executable = build.join(format!(
        "probes-{mode}-{opt:?}{}",
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::copy(
        build
            .join("target/release")
            .join(format!("baml_rust_program{}", std::env::consts::EXE_SUFFIX)),
        &executable,
    )
    .unwrap();
    executable
}

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
    use baml_compiler2_emit::OptLevel;

    #[allow(clippy::disallowed_methods, reason = "test harness configuration")]
    let opt = match std::env::var("BAML_RUST_DIFF_OPT_LEVEL").as_deref() {
        Err(std::env::VarError::NotPresent) | Ok("2") => OptLevel::Two,
        Ok("0") => OptLevel::Zero,
        Ok("1") => OptLevel::One,
        _ => panic!("BAML_RUST_DIFF_OPT_LEVEL must be 0, 1, or 2"),
    };
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
        .filter(|probe| !is_test_initializer_probe(probe))
    {
        argv.extend(["--function".into(), (*probe).into()]);
    }
    argv.extend(["--function".into(), "InitializerTestSummary".into()]);
    let started = Instant::now();
    let result = baml_cli::run_cli(argv).expect("compiled pack must succeed");
    assert!(matches!(result, baml_cli::ExitCode::Success), "{result:?}");
    let template = std::fs::read(build.join("src/program.bexa")).unwrap();
    let template: baml_exec::PackEnvelope =
        baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, &template).unwrap();
    let executable = if opt == OptLevel::Two {
        executable
    } else {
        build_probe_variant(&build, workspace, manifest, &template, "none", opt)
    };
    let test_initializer_executable = build_test_initializer_probe(&build, workspace);
    let mut startup_executables = std::collections::BTreeMap::new();
    for (probe, mode) in [
        ("InitializerUncaught", "throw"),
        ("InitializerPanic", "panic"),
        ("InitializerPending", "pending"),
        ("GlobalCallPending", "pending-call"),
    ] {
        if only
            .as_deref()
            .is_some_and(|only| !only.split(',').any(|o| o == probe))
        {
            continue;
        }
        let executable = build_probe_variant(&build, workspace, manifest, &template, mode, opt);
        startup_executables.insert(probe, executable);
    }
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
            let mut command = if is_test_initializer_probe(probe) {
                Command::new(&test_initializer_executable)
            } else {
                let mut command =
                    Command::new(startup_executables.get(probe).unwrap_or(&executable));
                command.arg(probe);
                command
            };
            command
                .env("BAML_COMPILED_BODIES", bodies)
                .env("BAML_GC_STRESS", stress)
                .env("BAML_COMPILED_FRAMES", frames)
                .env("NO_COLOR", "1");
            match *probe {
                "InitializerTestErrors" => {
                    command.env("BAML_RUST_DIFF_INIT_FAILURE", "registration")
                }
                _ => command.env_remove("BAML_RUST_DIFF_INIT_FAILURE"),
            };
            match telemetry {
                Some(level) => command.env("BAML_TELEMETRY", level),
                None => command.env_remove("BAML_TELEMETRY"),
            };
            let output = run_probe(&mut command, timeout);
            match &output {
                Ok(output) => {
                    eprintln!("{probe}/{mode}: {} ({:?})", output.status, start.elapsed())
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
        if *probe == "GlobalCalls" {
            for output in outputs.iter().flatten() {
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let stdout = String::from_utf8_lossy(&output.stdout);
                for expected in ["6:9", "8:10:15", "global-call:9"] {
                    assert!(stdout.contains(expected), "{probe}: {stdout}");
                }
            }
        }
        if matches!(
            *probe,
            "InitializerUncaught"
                | "InitializerPanic"
                | "InitializerPending"
                | "InitializerTestErrors"
                | "GlobalCallPending"
        ) {
            for output in outputs.iter().flatten() {
                assert!(
                    !output.status.success(),
                    "{probe} must fail during initialization"
                );
                let error = String::from_utf8_lossy(&output.stderr);
                let evidence = match *probe {
                    "InitializerPending" => "INIT_LATE",
                    "GlobalCallPending" => "INIT_LATE_CALLBACK",
                    "InitializerPanic" => "INIT_GATE",
                    _ => "initializer-error",
                };
                assert!(error.contains(evidence), "{probe}: {error}");
                assert!(!String::from_utf8_lossy(&output.stdout).contains("unreachable:"));
            }
        }
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
