// End-to-end tests for `baml __emit-rust`, the ahead-of-time Rust backend's
// developer command.
//
// The report and argument checks run on every build. Building the emitted
// project needs `cargo` and the workspace's dependency sources, so that test
// is ignored and run by hand:
//
//     cargo test -p baml_cli --test emit_rust_e2e -- --ignored --nocapture

mod common;

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

const PROJECT: &str = r#"
function collatz_steps(n: int) -> int {
    let steps = 0;
    while (n != 1) {
        if (n % 2 == 0) { n = n / 2; } else { n = 3 * n + 1; }
        steps = steps + 1;
    }
    steps
}

function triple(n: int) -> int { 3 * n }

class Pair { a: int, b: int }

function sum(p: Pair) -> int { p.a + p.b }

function lambda_user(xs: int[]) -> int[] { xs.map((x) => { x + 1 }) }
"#;

fn write_project(dir: &Path) {
    std::fs::create_dir_all(dir.join("baml_src")).unwrap();
    std::fs::write(
        dir.join("baml.toml"),
        "[package]\nname = \"emit_rust_e2e\"\n",
    )
    .unwrap();
    std::fs::write(dir.join("baml_src/main.baml"), PROJECT).unwrap();
}

fn emit_rust(dir: &Path, args: &[&str]) -> Output {
    let home = dir.join(".baml-home");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    common::share_build_cache(&home);
    let mut cmd = Command::new(common::baml_cli());
    cmd.args(["--agent-skill-check", "off", "--no-progress", "__emit-rust"]);
    cmd.args(args);
    cmd.current_dir(dir);
    cmd.env("HOME", dir);
    cmd.env("BAML_HOME", &home);
    cmd.env("BAML_CLI_ALLOW_DIRECT", "1");
    cmd.env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1");
    cmd.env_remove("BAML_LOG");
    cmd.output().expect("spawn baml-cli")
}

fn runtime_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../bex_aot")
        .canonicalize()
        .expect("bex_aot next to baml_cli")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// The report counts what compiles, callees included, and marks a function
/// the shim cannot call from the command line.
#[test]
fn report_lists_admitted_and_rejected_functions() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let output = emit_rust(dir.path(), &["--report"]);
    let stdout = text(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        text(&output.stderr)
    );
    assert!(
        stdout.contains("native      user.collatz_steps"),
        "{stdout}"
    );
    assert!(stdout.contains("native      user.triple"), "{stdout}");
    assert!(
        stdout.contains("native (library only) user.sum"),
        "{stdout}"
    );
    assert!(
        stdout.contains("unsupported user.lambda_user: lambda"),
        "{stdout}"
    );
    assert!(stdout.contains("3 of 4 function(s) admitted"), "{stdout}");
}

#[test]
fn rejects_an_invalid_crate_name_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let out = dir.path().join("native");
    let output = emit_rust(
        dir.path(),
        &[
            "--function",
            "triple",
            "--crate-name",
            "9x",
            "--out",
            out.to_str().unwrap(),
            "--runtime-path",
            runtime_path().to_str().unwrap(),
        ],
    );
    assert!(!output.status.success());
    assert!(
        text(&output.stderr).contains("is not a crate name"),
        "{}",
        text(&output.stderr)
    );
    assert!(!out.exists(), "nothing is written for a bad name");
}

#[test]
fn emits_a_project_with_the_entry_shim() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let out = dir.path().join("native");
    let output = emit_rust(
        dir.path(),
        &[
            "--function",
            "collatz_steps",
            "--function",
            "triple",
            "--out",
            out.to_str().unwrap(),
            "--runtime-path",
            runtime_path().to_str().unwrap(),
        ],
    );
    let stdout = text(&output.stdout);
    assert!(
        output.status.success(),
        "{stdout}\n{}",
        text(&output.stderr)
    );
    assert!(stdout.contains("native user.collatz_steps"), "{stdout}");
    assert!(stdout.contains("native user.triple"), "{stdout}");
    let manifest = std::fs::read_to_string(out.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("bex_aot = { path = "), "{manifest}");
    assert!(manifest.contains("panic = \"abort\""), "{manifest}");
    let lib = std::fs::read_to_string(out.join("src/lib.rs")).unwrap();
    assert!(
        lib.contains("pub fn user_collatz_steps(mut _1: Int63)"),
        "{lib}"
    );
    let main = std::fs::read_to_string(out.join("src/main.rs")).unwrap();
    assert!(main.contains("user_collatz_steps("), "{main}");
    assert!(out.join("mir.txt").exists());
}

/// Build the emitted project and run its binary: the result prints through
/// `to_string`, a bad argument and an uncaught throw are reported on stderr
/// with the exit code the pack host uses.
#[test]
#[ignore = "builds the emitted project with cargo (about a minute cold)"]
fn built_binary_runs_the_function() {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path());
    let out = dir.path().join("native");
    let output = emit_rust(
        dir.path(),
        &[
            "--function",
            "triple",
            "--out",
            out.to_str().unwrap(),
            "--runtime-path",
            runtime_path().to_str().unwrap(),
        ],
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    let build = Command::new(env!("CARGO"))
        .args(["build", "--release", "--manifest-path"])
        .arg(out.join("Cargo.toml"))
        .output()
        .expect("spawn cargo");
    assert!(build.status.success(), "{}", text(&build.stderr));
    let binary = out.join("target/release/baml_native");

    let run = Command::new(&binary).args(["--n", "14"]).output().unwrap();
    assert!(run.status.success());
    assert_eq!(text(&run.stdout), "42\n");

    let overflow = Command::new(&binary)
        .args(["--n", "4611686018427387903"])
        .output()
        .unwrap();
    assert_eq!(overflow.status.code(), Some(1));
    assert!(
        text(&overflow.stderr).starts_with("error: uncaught throw: baml.panics.IntegerOverflow {"),
        "{}",
        text(&overflow.stderr)
    );

    let bad = Command::new(&binary).args(["--m", "1"]).output().unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert_eq!(text(&bad.stderr), "error: unknown argument `--m`\n");
}
