//! Packs the probe programs once with compiled bodies, then runs each as
//! bytecode, compiled, and compiled under GC stress. All three must agree.
//! Run with `cargo test -p baml_tests --test rust_differential -- --ignored`.
use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

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
    for probe in PROBES {
        argv.extend(["--function".into(), (*probe).into()]);
    }
    let started = Instant::now();
    let result = baml_cli::run_cli(argv).expect("compiled pack must succeed");
    assert!(matches!(result, baml_cli::ExitCode::Success), "{result:?}");
    eprintln!("compiled pack wall time: {:?}", started.elapsed());
    let started = Instant::now();
    let mut failures = Vec::new();
    for probe in PROBES {
        let mut outputs = Vec::new();
        for (mode, bodies, stress) in [
            ("bytecode", "0", "0"),
            ("compiled", "1", "0"),
            ("gc-stress", "1", "1"),
        ] {
            let start = Instant::now();
            let output = run_probe(
                Command::new(&executable)
                    .arg(probe)
                    .env("BAML_COMPILED_BODIES", bodies)
                    .env("BAML_GC_STRESS", stress)
                    .env("NO_COLOR", "1"),
                timeout,
            );
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
            outputs.push(output.ok());
        }
        for (mode, actual) in ["compiled", "gc-stress"].into_iter().zip(&outputs[1..]) {
            let (Some(expected), Some(actual)) = (&outputs[0], actual) else {
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
