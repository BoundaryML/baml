//! Backend-pluggable process and semantic BTEL differential gate.
//! Bytecode is the reference at each telemetry level (off/low/medium/high).
//! See `rust_diff/README.md` for the matrix, subsets and known divergences.
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
    "StackAllocation",
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

/// A backend owns its artifact builder and run environment.
/// Shared artifact keys build once; rejected artifacts retain their diagnostics.
struct Backend {
    name: &'static str,
    build: fn(&Path, &Path, &Path, Option<&str>) -> Artifacts,
    artifact_key: &'static str,
    env: &'static [(&'static str, &'static str)],
}
type Artifacts = std::collections::BTreeMap<String, Result<std::path::PathBuf, rt::Rejection>>;

#[path = "../rust_diff/rt.rs"]
mod rt;
const BACKENDS: &[Backend] = &[
    Backend {
        name: "bytecode",
        build: build_rust,
        artifact_key: "rust",
        env: &[("BAML_COMPILED_BODIES", "0")],
    },
    Backend {
        name: "compiled",
        build: build_rust,
        artifact_key: "rust",
        env: &[("BAML_COMPILED_BODIES", "1")],
    },
    Backend {
        name: "gc-stress",
        build: build_rust,
        artifact_key: "rust",
        env: &[("BAML_COMPILED_BODIES", "1"), ("BAML_GC_STRESS", "1")],
    },
    Backend {
        name: "vm-frames",
        build: build_rust,
        artifact_key: "rust",
        env: &[("BAML_COMPILED_BODIES", "1"), ("BAML_COMPILED_FRAMES", "1")],
    },
    Backend {
        name: "vm-frames-gc-stress",
        build: build_rust,
        artifact_key: "rust",
        env: &[
            ("BAML_COMPILED_BODIES", "1"),
            ("BAML_GC_STRESS", "1"),
            ("BAML_COMPILED_FRAMES", "1"),
        ],
    },
    Backend {
        name: "rt",
        build: rt::build,
        artifact_key: "rt",
        env: &[],
    },
    Backend {
        name: "rt-gc-stress",
        build: rt::build,
        artifact_key: "rt",
        env: &[("BAML_GC_STRESS", "1")],
    },
];
const LEVELS: &[&str] = &["off", "low", "medium", "high", "default"];

#[path = "../rust_diff/known_telemetry_divergences.rs"]
mod known_telemetry_divergences;
#[path = "../rust_diff/kpis.rs"]
mod kpis;
#[path = "../rust_diff/telemetry.rs"]
mod telemetry;
#[path = "../rust_diff/telemetry_worker.rs"]
mod telemetry_worker;

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

fn build_rust(build: &Path, workspace: &Path, manifest: &Path, only: Option<&str>) -> Artifacts {
    std::fs::create_dir_all(build).unwrap();
    let mut artifacts = std::collections::BTreeMap::new();
    if PROBES
        .iter()
        .any(|p| only.is_none_or(|o| o.split(',').any(|name| name == *p)))
    {
        let executable = build.join(format!("probes{}", std::env::consts::EXE_SUFFIX));
        pack(
            build,
            workspace,
            &manifest.join("rust_diff"),
            &executable,
            PROBES
                .iter()
                .copied()
                .filter(|p| !initializer_probes::collects_tests(p))
                .filter(|p| only.is_none_or(|o| o.split(',').any(|name| name == *p)))
                .chain(["GlobalCalls", "GlobalCallPending", "InitializerTestSummary"]),
        );
        // Initializer variants need only these roots. Keep the full probe
        // executable above, but avoid recompiling every unrelated native body
        // for each synthetic failing initializer artifact.
        let initializers: Vec<_> = [
            "InitializerTests",
            "InitializerTestErrors",
            "InitializerUncaught",
            "InitializerPanic",
            "InitializerPending",
            "GlobalCallPending",
        ]
        .into_iter()
        .filter(|p| only.is_none_or(|o| o.split(',').any(|name| name == *p)))
        .collect();
        if !initializers.is_empty() {
            pack(
                build,
                workspace,
                &manifest.join("rust_diff"),
                &build.join(format!(
                    "initializer-template{}",
                    std::env::consts::EXE_SUFFIX
                )),
                initializers
                    .into_iter()
                    .filter(|p| !initializer_probes::collects_tests(p))
                    .chain(["GlobalCalls", "GlobalCallPending", "InitializerTestSummary"]),
            );
        }
        artifacts.extend(
            initializer_probes::build(build, workspace, manifest, only)
                .into_iter()
                .map(|(name, path)| (name.to_owned(), path)),
        );
        for probe in PROBES {
            artifacts
                .entry((*probe).into())
                .or_insert_with(|| executable.clone());
        }
    }
    for fixture in kpis::fixtures(workspace, only) {
        let dir = build.join("kpis").join(&fixture.slug);
        std::fs::create_dir_all(dir.join("source/baml_src")).unwrap();
        std::fs::write(
            dir.join("source/baml.toml"),
            "[package]\nname = \"differential-kpi\"\n[pack.env_var_names]\nBAML_TELEMETRY = \"BAML_TELEMETRY\"\n",
        )
        .unwrap();
        std::fs::copy(&fixture.source, dir.join("source/baml_src/main.baml")).unwrap();
        let executable = dir.join("workload");
        pack(build, workspace, &dir.join("source"), &executable, ["main"]);
        artifacts.insert(fixture.name, executable);
    }
    artifacts
        .into_iter()
        .map(|(name, path)| (name, Ok(path)))
        .collect()
}

fn pack<'a>(
    build: &Path,
    workspace: &Path,
    source: &Path,
    executable: &Path,
    functions: impl IntoIterator<Item = &'a str>,
) {
    pack_result(build, workspace, source, executable, functions, false).expect("pack must succeed");
}

fn pack_result<'a>(
    build: &Path,
    workspace: &Path,
    source: &Path,
    executable: &Path,
    functions: impl IntoIterator<Item = &'a str>,
    rt: bool,
) -> Result<(), String> {
    pack_result_options(build, workspace, source, executable, functions, rt, false)
}

fn pack_result_options<'a>(
    build: &Path,
    workspace: &Path,
    source: &Path,
    executable: &Path,
    functions: impl IntoIterator<Item = &'a str>,
    rt: bool,
    benchmark: bool,
) -> Result<(), String> {
    // Never accept an executable left over from a previous successful pack.
    if executable.exists() {
        std::fs::remove_file(executable).map_err(|e| e.to_string())?;
    }
    let mut argv: Vec<String> = [
        "baml",
        "--agent-skill-check",
        "off",
        "pack",
        "--rust",
        "--rust-profile",
        "fast",
        "--rust-workspace",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    argv.extend([
        workspace.display().to_string(),
        "--rust-build-dir".into(),
        build.display().to_string(),
        "--from".into(),
        source.display().to_string(),
        "-o".into(),
        executable.display().to_string(),
    ]);
    let mut seen = std::collections::BTreeSet::new();
    for function in functions {
        if !seen.insert(function) {
            continue;
        }
        if rt {
            argv.push(function.into());
        } else {
            argv.extend(["--function".into(), function.into()]);
        }
    }
    if rt {
        argv.push("--rt".into());
    }
    if benchmark {
        argv.push("--rt-benchmark".into());
    }
    let result = baml_cli::run_cli(argv).map_err(|error| format!("{error:#}"))?;
    if !matches!(result, baml_cli::ExitCode::Success) {
        return Err(format!("pack exited {result:?}"));
    }
    if !executable.is_file() {
        return Err("pack succeeded without an executable".into());
    }
    Ok(())
}

#[test]
#[ignore = "builds generated Rust crates with cargo"]
fn rt_call_cost_semantics_at_every_telemetry_level() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let build = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rt-call-cost-parity");
    std::fs::create_dir_all(&build).unwrap();
    let source = manifest.join("rust_diff/call_cost");
    let bytecode = build.join("bytecode");
    pack_result(
        &build.join("vm-build"),
        workspace,
        &source,
        &bytecode,
        ["main", "uncaught", "stack_calls"],
        false,
    )
    .unwrap();
    let run = |program: &Path, entry: &str, is_rt: bool, stress: bool, level: &str| {
        let home = tempfile::tempdir_in(&build).unwrap();
        let mut command = Command::new(program);
        if !is_rt {
            command.arg(entry);
        }
        command
            .env("BAML_HOME", home.path())
            .env("BAML_TELEMETRY", level)
            .env("BAML_COMPILED_BODIES", "0")
            .env("BAML_COMPILED_FRAMES", "0")
            .env("BAML_GC_STRESS", if stress { "1" } else { "0" })
            .env_remove("BOUNDARY_PROJECT")
            .env_remove("BOUNDARY_API_URL")
            .env("NO_COLOR", "1");
        if level == "default" {
            command.env_remove("BAML_TELEMETRY");
        }
        let output = run_probe(&mut command, Duration::from_secs(120)).unwrap();
        assert!(
            output.status.success() == (entry != "uncaught"),
            "{level}/{is_rt}/{stress}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let trace = telemetry::read(home.path());
        assert_eq!(
            trace.is_empty(),
            level == "off",
            "{level}: telemetry presence"
        );
        (
            output,
            known_telemetry_divergences::without_gc(&trace, "rt"),
        )
    };
    for entry in ["main", "uncaught", "stack_calls"] {
        let compiled = build.join(format!("rt-{entry}"));
        pack_result(
            &build.join("rt-build"),
            workspace,
            &source,
            &compiled,
            [entry],
            true,
        )
        .unwrap();
        for level in ["off", "low", "medium", "high", "default"] {
            let (expected, trace) = run(&bytecode, entry, false, false, level);
            if entry == "main" {
                assert_eq!(expected.stdout, b"511\n");
            } else if entry == "stack_calls" {
                assert_eq!(expected.stdout, b"28\n");
            }
            for stress in [false, true] {
                let (actual, observed) = run(&compiled, entry, true, stress, level);
                assert_eq!(
                    actual.status.code(),
                    expected.status.code(),
                    "{entry}/{level}/{stress}: status"
                );
                assert_eq!(
                    actual.stdout, expected.stdout,
                    "{entry}/{level}/{stress}: stdout"
                );
                assert_eq!(
                    actual.stderr, expected.stderr,
                    "{entry}/{level}/{stress}: stderr"
                );
                assert_eq!(
                    observed, trace,
                    "{entry}/{level}/{stress}: semantic telemetry"
                );
            }
        }
    }
}

#[test]
#[ignore = "builds generated Rust crates with cargo"]
#[allow(clippy::disallowed_methods, reason = "test harness configuration")]
fn rust_differential() {
    let _pack_guard = PACK_TESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let timeout = std::env::var("BAML_RUST_DIFF_TIMEOUT_SECS")
        .map(|v| v.parse::<u64>().expect("timeout must be seconds"))
        .unwrap_or(120);
    assert!(timeout > 0);
    let timeout = Duration::from_secs(timeout);
    let only = std::env::var("BAML_RUST_DIFF_PROBES").ok();
    let selected = std::env::var("BAML_RUST_DIFF_BACKENDS").ok().or_else(|| {
        std::env::var("BAML_RUST_DIFF_RT_ONLY")
            .is_ok_and(|v| v == "1")
            .then(|| "rt,rt-gc-stress".to_owned())
    });
    if let Some(selected) = &selected {
        for name in selected.split(',') {
            assert!(
                BACKENDS.iter().any(|b| b.name == name),
                "unknown backend: {name}"
            );
        }
    }
    // The bytecode oracle is mandatory even for a scoped backend run.
    let backends: Vec<_> = BACKENDS
        .iter()
        .filter(|backend| {
            backend.name == "bytecode"
                || selected
                    .as_ref()
                    .is_none_or(|names| names.split(',').any(|name| name == backend.name))
        })
        .collect();
    let rt_only = backends
        .iter()
        .all(|b| b.name == "bytecode" || b.artifact_key == "rt");
    let levels = LEVELS;
    let dump = std::env::var("BAML_RUST_DIFF_DUMP").is_ok_and(|v| v == "1");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest.parent().unwrap().parent().unwrap();
    let build = std::env::var_os("BAML_RUST_DIFF_BUILD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_TARGET_TMPDIR")).join("rust-differential"));
    let available: Vec<String> = PROBES
        .iter()
        .map(|s| (*s).into())
        .chain(kpis::fixtures(workspace, None).into_iter().map(|f| f.name))
        .collect();
    if let Some(only) = &only {
        for name in only.split(',') {
            assert!(
                available.iter().any(|p| p == name),
                "unknown fixture: {name}"
            );
        }
    }
    let reader = telemetry_worker::build(&build, workspace);
    std::fs::write(build.join("known-telemetry-divergences.txt"), "").unwrap();
    let mut artifacts = std::collections::BTreeMap::new();
    let oracle_only = if rt_only {
        let rt_artifacts = rt::build(&build.join("rt"), workspace, manifest, only.as_deref());
        let admitted = rt_artifacts
            .iter()
            .filter(|(_, result)| result.is_ok())
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(",");
        artifacts.insert("rt", rt_artifacts);
        Some(admitted)
    } else {
        None
    };
    for backend in &backends {
        if rt_only && backend.artifact_key == "rust" && oracle_only.as_deref() == Some("") {
            artifacts.insert("rust", Artifacts::new());
            continue;
        }
        artifacts.entry(backend.artifact_key).or_insert_with(|| {
            (backend.build)(
                &build.join(backend.artifact_key),
                workspace,
                manifest,
                oracle_only.as_deref().or(only.as_deref()),
            )
        });
    }
    let reference = backends
        .iter()
        .position(|b| b.name == "bytecode")
        .expect("bytecode reference");
    let mut probes: Vec<String> = PROBES.iter().map(|s| (*s).into()).collect();
    probes.extend(
        kpis::fixtures(workspace, only.as_deref())
            .into_iter()
            .map(|f| f.name),
    );
    probes.retain(|probe| {
        only.as_deref()
            .is_none_or(|s| s.split(',').any(|p| p == probe))
    });
    assert!(!probes.is_empty(), "no probes selected");
    let mut coverage = artifacts.get("rt").map(rt::Coverage::new);
    let mut failures = coverage
        .as_ref()
        .map_or_else(Vec::new, rt::Coverage::build_failures);
    for probe in probes {
        if rt_only && artifacts["rt"][&probe].is_err() {
            continue;
        }
        let failures_before = failures.len();
        for level in levels {
            if rt_only && !rt::supports_level(&probe, level) {
                continue;
            }
            if !probe.starts_with("strings::")
                && ((probe.contains("::") && matches!(*level, "default" | "low"))
                    || (*level == "default"
                        && !matches!(probe.as_str(), "Scalars" | "Hooked" | "ManualSpan")))
            {
                continue;
            }
            let mut outputs = Vec::new();
            let mut traces = Vec::new();
            for backend in &backends {
                if backend.artifact_key == "rt"
                    && (!rt::supports_level(&probe, level) || artifacts["rt"][&probe].is_err())
                {
                    outputs.push(None);
                    traces.push(String::new());
                    continue;
                }
                // Stress switches target the old compiled implementation. KPI
                // programs run each actual backend, with one full job per level.
                if ((probe.contains("::") || *level == "low")
                    && matches!(
                        backend.name,
                        "gc-stress" | "vm-frames" | "vm-frames-gc-stress"
                    ))
                    || (backend.name == "vm-frames-gc-stress"
                        && !matches!(*level, "medium" | "default"))
                {
                    outputs.push(None);
                    traces.push(String::new());
                    continue;
                }
                let mode = format!("{}-telemetry-{level}", backend.name);
                let home = tempfile::tempdir_in(&build).unwrap();
                let mut command =
                    Command::new(artifacts[backend.artifact_key][&probe].as_ref().unwrap());
                if backend.artifact_key == "rt" {
                    // RT packs have one entry, not a subcommand dispatcher.
                } else if probe.contains("::") {
                    command.arg("main");
                } else if !initializer_probes::collects_tests(&probe) {
                    command.arg(&probe);
                }
                command
                    .env("BAML_HOME", home.path())
                    .env("BOUNDARY_API_KEY", "local")
                    .env_remove("BOUNDARY_PROJECT")
                    .env_remove("BOUNDARY_API_URL")
                    .env("BAML_COMPILED_BODIES", "0")
                    .env("BAML_GC_STRESS", "0")
                    .env("BAML_COMPILED_FRAMES", "0")
                    .env("BAML_TELEMETRY", level)
                    .env("NO_COLOR", "1")
                    .envs(backend.env.iter().copied());
                if *level == "default" {
                    command.env_remove("BAML_TELEMETRY");
                }
                let start = Instant::now();
                let output = run_probe(&mut command, timeout);
                match &output {
                    Ok(output) => {
                        eprintln!("{probe}/{mode}: {} ({:?})", output.status, start.elapsed());
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        if backend.artifact_key == "rt" && output.status.code().is_none() {
                            failures.push(format!("{probe}/{mode}: terminated by signal"));
                        }
                        if stderr.contains("panicked at") {
                            failures.push(format!("{probe}/{mode}: Rust panic: {stderr}"));
                        }
                        if dump {
                            eprintln!(
                                "stdout: {}\nstderr: {stderr}",
                                String::from_utf8_lossy(&output.stdout)
                            );
                        }
                    }
                    Err(error) => failures.push(format!("{probe}/{mode}: {error}")),
                }
                if output.is_err() {
                    outputs.push(None);
                    traces.push(String::new());
                    continue;
                }
                let normalized = home.path().join("normalized.jsonl");
                let reader_output = run_probe(
                    Command::new(&reader).arg(home.path()).arg(&normalized),
                    timeout * 3,
                );
                let trace = match reader_output {
                    Ok(reader_output) if reader_output.status.success() => {
                        std::fs::read_to_string(normalized).map_err(|error| error.to_string())
                    }
                    Ok(reader_output) => Err(format!(
                        "{}: {}",
                        reader_output.status,
                        String::from_utf8_lossy(&reader_output.stderr)
                    )),
                    Err(error) => Err(error),
                };
                let trace = match trace {
                    Ok(trace) => trace,
                    Err(error) => {
                        failures.push(format!("{probe}/{mode}: telemetry reader: {error}"));
                        outputs.push(output.ok());
                        traces.push(String::new());
                        continue;
                    }
                };
                if *level == "off" && !trace.is_empty() {
                    failures.push(format!(
                        "{probe}/{mode}: off unexpectedly produced telemetry"
                    ));
                }
                if *level != "off" && trace.is_empty() {
                    failures.push(format!("{probe}/{mode}: no telemetry recording produced"));
                }
                if probe.contains("::")
                    && kpis::expects_success(&probe)
                    && output.as_ref().is_ok_and(|o| !o.status.success())
                {
                    failures.push(format!("{probe}/{mode}: KPI must succeed"));
                }
                std::fs::create_dir_all(build.join("telemetry")).unwrap();
                std::fs::write(
                    build
                        .join("telemetry")
                        .join(format!("{probe}-{mode}.jsonl")),
                    &trace,
                )
                .unwrap();
                traces.push(trace);
                outputs.push(output.ok());
            }
            // Assert the oracle contract; every other backend must match it exactly.
            initializer_probes::assert_outputs(&probe, std::slice::from_ref(&outputs[reference]));
            if let Some(expected) = kpis::string_probe_output(&probe) {
                assert_eq!(
                    outputs[reference].as_ref().unwrap().stdout,
                    expected,
                    "{probe}: oracle semantic assertions"
                );
            }
            for (index, backend) in backends.iter().enumerate() {
                if index == reference || outputs[index].is_none() {
                    continue;
                }
                let mode = format!("{}-telemetry-{level}", backend.name);
                if let (Some(expected), Some(actual)) = (&outputs[reference], &outputs[index])
                    && (expected.status != actual.status
                        || expected.stdout != actual.stdout
                        || expected.stderr != actual.stderr)
                {
                    failures.push(format!(
                        "{probe}/{mode}: process mismatch: bytecode {} vs {}\n{}\n{}",
                        expected.status,
                        actual.status,
                        diff(
                            &String::from_utf8_lossy(&expected.stdout),
                            &String::from_utf8_lossy(&actual.stdout),
                            &mode
                        ),
                        diff(
                            &String::from_utf8_lossy(&expected.stderr),
                            &String::from_utf8_lossy(&actual.stderr),
                            &mode
                        )
                    ));
                }
                if traces[reference] != traces[index] {
                    if backend.artifact_key == "rt" {
                        // C5 is the runtime's sole telemetry exception. Both
                        // modes share the rt artifact and its normalizer; the
                        // old backend's other exceptions never apply here.
                        let expected =
                            known_telemetry_divergences::without_gc(&traces[reference], "rt");
                        let actual = known_telemetry_divergences::without_gc(&traces[index], "rt");
                        if expected == actual {
                            report_known(
                                &build,
                                &probe,
                                &mode,
                                known_telemetry_divergences::GC_REASON,
                                &diff(&traces[reference], &traces[index], &mode),
                            );
                        } else {
                            failures.push(format!(
                                "{probe}/{mode}: telemetry mismatch\n{}",
                                diff(&expected, &actual, &mode)
                            ));
                        }
                        continue;
                    }
                    if let Some(reason) =
                        known_telemetry_divergences::fixture_exception(&probe, level, backend.name)
                    {
                        report_known(
                            &build,
                            &probe,
                            &mode,
                            reason,
                            &diff(&traces[reference], &traces[index], &mode),
                        );
                        continue;
                    }
                    let expected =
                        known_telemetry_divergences::without_gc(&traces[reference], backend.name);
                    let actual =
                        known_telemetry_divergences::without_gc(&traces[index], backend.name);
                    let detail = diff(&traces[reference], &traces[index], &mode);
                    if expected == actual {
                        report_known(
                            &build,
                            &probe,
                            &mode,
                            known_telemetry_divergences::GC_REASON,
                            &detail,
                        );
                    } else if known_telemetry_divergences::without_readiness_poll(
                        &expected,
                        &probe,
                        backend.name,
                    ) == known_telemetry_divergences::without_readiness_poll(
                        &actual,
                        &probe,
                        backend.name,
                    ) {
                        report_known(
                            &build,
                            &probe,
                            &mode,
                            known_telemetry_divergences::POLLING_REASON,
                            &diff(&expected, &actual, &mode),
                        );
                    } else if known_telemetry_divergences::without_log_locations(
                        &expected,
                        &probe,
                        backend.name,
                    ) == known_telemetry_divergences::without_log_locations(
                        &actual,
                        &probe,
                        backend.name,
                    ) {
                        report_known(
                            &build,
                            &probe,
                            &mode,
                            known_telemetry_divergences::LOG_REASON,
                            &diff(&expected, &actual, &mode),
                        );
                    } else if known_telemetry_divergences::without_timeout_wait(
                        &expected,
                        &probe,
                        backend.name,
                    ) == known_telemetry_divergences::without_timeout_wait(
                        &actual,
                        &probe,
                        backend.name,
                    ) {
                        report_known(
                            &build,
                            &probe,
                            &mode,
                            known_telemetry_divergences::TIMEOUT_REASON,
                            &diff(&expected, &actual, &mode),
                        );
                    } else {
                        failures.push(format!(
                            "{probe}/{mode}: telemetry mismatch\n{}",
                            diff(&expected, &actual, &mode)
                        ));
                    }
                }
            }
        }
        if let Some(coverage) = &mut coverage {
            coverage.record(&probe, &failures[failures_before..]);
        }
    }
    if let Some(coverage) = coverage {
        coverage.write(&build);
    }
    for failure in &failures {
        eprintln!("{failure}");
    }
    assert!(
        failures.is_empty(),
        "{} differential failures",
        failures.len()
    );
}

fn report_known(build: &Path, probe: &str, mode: &str, reason: &str, detail: &str) {
    use std::io::Write;
    let report = format!("KNOWN TELEMETRY DIVERGENCE {probe}/{mode}: {reason}\n{detail}\n");
    eprintln!("{report}");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(build.join("known-telemetry-divergences.txt"))
        .unwrap();
    file.write_all(report.as_bytes()).unwrap();
}

fn diff(expected: &str, actual: &str, mode: &str) -> String {
    // Telemetry is sorted by its JSON row, with the multiplicity prefixed.
    // Avoid an edit-distance matrix or constructing an unbounded unified diff
    // for stack-limit recordings with hundreds of thousands of differing rows.
    if expected.len() + actual.len() > 256 * 1024 {
        let mut a = expected.lines().peekable();
        let mut b = actual.lines().peekable();
        let key = |line: &str| line.split_once(' ').map_or(line, |(_, row)| row).to_owned();
        let mut lines = vec![format!("--- bytecode\n+++ {mode}")];
        while lines.len() < 17 && (a.peek().is_some() || b.peek().is_some()) {
            let ordering = match (a.peek(), b.peek()) {
                (Some(a), Some(b)) => key(a).cmp(&key(b)),
                (Some(_), None) => std::cmp::Ordering::Less,
                _ => std::cmp::Ordering::Greater,
            };
            match ordering {
                std::cmp::Ordering::Equal => {
                    let a = a.next().unwrap();
                    let b = b.next().unwrap();
                    if a != b {
                        lines.push(format!("-{}", a.chars().take(800).collect::<String>()));
                        lines.push(format!("+{}", b.chars().take(800).collect::<String>()));
                    }
                }
                std::cmp::Ordering::Less => lines.push(format!(
                    "-{}",
                    a.next().unwrap().chars().take(800).collect::<String>()
                )),
                std::cmp::Ordering::Greater => lines.push(format!(
                    "+{}",
                    b.next().unwrap().chars().take(800).collect::<String>()
                )),
            }
        }
        return lines.join("\n");
    }
    similar::TextDiff::from_lines(expected, actual)
        .unified_diff()
        .header("bytecode", mode)
        .to_string()
        .lines()
        .take(24)
        .map(|line| line.chars().take(800).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[path = "../rust_diff/type_dispatch.rs"]
mod type_dispatch;
static PACK_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
#[ignore = "builds generated Rust crates with cargo"]
fn rt_type_dispatch_semantics() {
    let _pack_guard = PACK_TESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    type_dispatch::run();
}

#[path = "../rust_diff/setup_state.rs"]
mod setup_state;

#[test]
#[ignore = "builds generated Rust crates with cargo"]
fn rt_setup_state_benchmark() {
    let _pack_guard = PACK_TESTS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    setup_state::run();
}
