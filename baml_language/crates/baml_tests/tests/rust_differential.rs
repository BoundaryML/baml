//! Packs the probe programs once with compiled bodies, then runs each as
//! bytecode, compiled, and compiled under GC stress. All three must agree.
//! Run with `cargo test -p baml_tests --test rust_differential -- --ignored`.
use std::{path::Path, process::Command, time::Instant};

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
    "Recursion",
    "Allocation",
    "Logs",
    "Depth",
    "Hidden",
    "Hooked",
];

#[test]
#[ignore = "builds a generated Rust crate with cargo"]
fn rust_differential() {
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
            let output = Command::new(&executable)
                .arg(probe)
                .env("BAML_COMPILED_BODIES", bodies)
                .env("BAML_GC_STRESS", stress)
                .env("NO_COLOR", "1")
                .output()
                .expect("run packed probe");
            eprintln!("{probe}/{mode}: {} ({:?})", output.status, start.elapsed());
            outputs.push(output);
        }
        for (mode, actual) in ["compiled", "gc-stress"].into_iter().zip(&outputs[1..]) {
            let expected = &outputs[0];
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
    assert!(failures.is_empty(), "{} mode divergences", failures.len());
}
