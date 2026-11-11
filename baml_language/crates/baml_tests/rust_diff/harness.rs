//! The differential harness shared by the probe suites: run a packed probe
//! in every mode and compare each compiled mode with bytecode at the same
//! telemetry level.
#![allow(dead_code, reason = "each suite uses part of the harness")]
use std::{
    io::{self, Read},
    path::Path,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Name, `BAML_COMPILED_BODIES`, `BAML_GC_STRESS`, `BAML_COMPILED_FRAMES`,
/// and `BAML_TELEMETRY` (`None` for the default level). Bytecode at each
/// telemetry level is the reference for that level.
pub type Mode = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<&'static str>,
);
pub const MODES: &[Mode] = &[
    ("bytecode", "0", "0", "0", None),
    ("compiled", "1", "0", "0", None),
    ("gc-stress", "1", "1", "0", None),
    ("vm-frames", "1", "0", "1", None),
    ("vm-frames-gc-stress", "1", "1", "1", None),
    ("bytecode-telemetry-off", "0", "0", "0", Some("off")),
    ("compiled-telemetry-off", "1", "0", "0", Some("off")),
    ("gc-stress-telemetry-off", "1", "1", "0", Some("off")),
    ("vm-frames-telemetry-off", "1", "0", "1", Some("off")),
    ("bytecode-telemetry-low", "0", "0", "0", Some("low")),
    ("compiled-telemetry-low", "1", "0", "0", Some("low")),
    ("gc-stress-telemetry-low", "1", "1", "0", Some("low")),
    ("bytecode-telemetry-medium", "0", "0", "0", Some("medium")),
    ("compiled-telemetry-medium", "1", "0", "0", Some("medium")),
    ("gc-stress-telemetry-medium", "1", "1", "0", Some("medium")),
    ("bytecode-telemetry-high", "0", "0", "0", Some("high")),
    ("compiled-telemetry-high", "1", "0", "0", Some("high")),
    ("gc-stress-telemetry-high", "1", "1", "0", Some("high")),
    ("vm-frames-telemetry-high", "1", "0", "1", Some("high")),
];

pub const OUTPUT_LIMIT: usize = 1024 * 1024;

pub fn capture(mut stream: impl Read) -> io::Result<(Vec<u8>, bool)> {
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

pub fn run_probe(command: &mut Command, timeout: Duration) -> Result<Output, String> {
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

/// The modes at the default level and `off`: bytecode, compiled, and
/// compiled under GC stress, for probes too slow for every level.
pub const NORMAL_AND_STRESS: &[Mode] = &[
    ("bytecode", "0", "0", "0", None),
    ("compiled", "1", "0", "0", None),
    ("gc-stress", "1", "1", "0", None),
    ("bytecode-telemetry-off", "0", "0", "0", Some("off")),
    ("compiled-telemetry-off", "1", "0", "0", Some("off")),
    ("gc-stress-telemetry-off", "1", "1", "0", Some("off")),
];

/// The suites' environment controls.
pub struct Settings {
    pub timeout: Duration,
    /// `BAML_RUST_DIFF_PROBES=A,B`: run only these probes.
    pub only: Option<String>,
    /// `BAML_RUST_DIFF_DUMP=1`: print every run's output.
    pub dump: bool,
}

impl Settings {
    #[allow(clippy::disallowed_methods, reason = "test harness configuration")]
    pub fn from_env() -> Self {
        let timeout = std::env::var("BAML_RUST_DIFF_TIMEOUT_SECS")
            .map(|value| {
                value
                    .parse::<u64>()
                    .expect("BAML_RUST_DIFF_TIMEOUT_SECS must be seconds")
            })
            .unwrap_or(120);
        assert!(timeout > 0, "BAML_RUST_DIFF_TIMEOUT_SECS must be positive");
        Self {
            timeout: Duration::from_secs(timeout),
            only: std::env::var("BAML_RUST_DIFF_PROBES").ok(),
            dump: std::env::var("BAML_RUST_DIFF_DUMP").is_ok_and(|value| value == "1"),
        }
    }

    pub fn selects(&self, probe: &str) -> bool {
        self.only
            .as_deref()
            .is_none_or(|only| only.split(',').any(|o| o == probe))
    }
}

/// Pack with compiled bodies (`--rust-profile fast`); `args` choose the
/// project and entries.
pub fn pack(workspace: &Path, build: &Path, executable: &Path, args: &[String]) {
    let mut argv: Vec<String> = vec![
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
        "-o".into(),
        executable.display().to_string(),
    ];
    argv.extend(args.iter().cloned());
    let result = baml_cli::run_cli(argv).expect("compiled pack must succeed");
    assert!(matches!(result, baml_cli::ExitCode::Success), "{result:?}");
}

/// Run `probe` once per mode with the command `make` builds; a failure to
/// run, or a Rust panic on stderr, is a failure.
pub fn run_modes(
    probe: &str,
    modes: &[Mode],
    settings: &Settings,
    make: impl Fn() -> Command,
    failures: &mut Vec<String>,
) -> Vec<Option<Output>> {
    let mut outputs = Vec::new();
    for (mode, bodies, stress, frames, telemetry) in modes {
        let start = Instant::now();
        let mut command = make();
        command
            .env("BAML_COMPILED_BODIES", bodies)
            .env("BAML_GC_STRESS", stress)
            .env("BAML_COMPILED_FRAMES", frames)
            .env("NO_COLOR", "1");
        match telemetry {
            Some(level) => command.env("BAML_TELEMETRY", level),
            None => command.env_remove("BAML_TELEMETRY"),
        };
        let output = run_probe(&mut command, settings.timeout);
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
        if settings.dump
            && let Ok(output) = &output
        {
            eprintln!(
                "--- {probe}/{mode} stdout:\n{}--- stderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        outputs.push(output.ok());
    }
    outputs
}

/// Compare each compiled mode's output with bytecode's at its own telemetry
/// level: hooks and spans legitimately differ between levels.
pub fn compare_modes(
    probe: &str,
    modes: &[Mode],
    outputs: &[Option<Output>],
    failures: &mut Vec<String>,
) {
    for ((mode, bodies, _, _, telemetry), actual) in modes.iter().zip(outputs) {
        if *bodies == "0" {
            continue;
        }
        let reference = modes
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
            let diff = diff(probe, mode, expected, actual);
            eprintln!("{diff}");
            failures.push(diff);
        }
    }
}

fn diff(probe: &str, mode: &str, expected: &Output, actual: &Output) -> String {
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
            similar::TextDiff::from_lines(&String::from_utf8_lossy(a), &String::from_utf8_lossy(b))
                .unified_diff()
                .header("bytecode", mode)
        ));
    }
    diff
}
