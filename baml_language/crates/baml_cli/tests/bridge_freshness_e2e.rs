//! End-to-end tests for bridge freshness: run the real `baml-cli` binary
//! against a temp project and assert on the passive warning, `--check`'s exit
//! code, and the printed install command.
//!
//! These exercise the seam the unit tests cannot: that the passive warning is
//! actually wired to the right commands, that `--check` really writes nothing,
//! and that the exit code reaches the shell.

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

/// `ExitCode::BridgeStale`.
const STALE: i32 = 6;

const STALE_WARNING: &str = "generated bridge `client1` is out of date";

/// A temp `BAML_HOME` with the network auto-check disabled, so nothing here
/// touches the network.
struct TestHome {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl TestHome {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::write(root.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
        Self { _dir: dir, root }
    }

    fn run(&self, args: &[&str], cwd: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_baml-cli"))
            .args(args)
            .current_dir(cwd)
            .env("BAML_HOME", &self.root)
            .env("HOME", cwd.parent().unwrap_or(cwd))
            .env("BAML_CACHE_DIR", common::shared_cache_dir())
            .env("BAML_WRAPPER_EXEC", "1")
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("DO_NOT_TRACK", "1")
            // The agent preset suppresses reporter lines, so pin the human
            // preset rather than inheriting whatever runs the test suite.
            .env_remove("AI_AGENT")
            .env_remove("CLAUDECODE")
            .env_remove("BAML_INTERNAL")
            .output()
            .unwrap()
    }
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// A project with one Python bridge writing into `<project>/out`.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("baml.toml"),
        "[package]\nname = \"demo\"\n\n[generator.client1]\noutput_type = \"python/pydantic\"\n\
         output_dir = \"out\"\nnaming_convention = \"preserve-case\"\n",
    )
    .unwrap();
    let source_root = dir.path().join("baml_src");
    fs::create_dir_all(&source_root).unwrap();
    fs::write(
        source_root.join("main.baml"),
        "class Person {\n  name string\n}\n",
    )
    .unwrap();
    dir
}

fn generated_dir(project: &Path) -> PathBuf {
    project.join("out").join("baml_sdk")
}

#[test]
fn check_reports_stale_then_goes_quiet_after_regenerating() {
    let home = TestHome::new();
    let dir = project();

    // Never generated: reported, and distinctly from a compile failure.
    let output = home.run(&["generate", "--check"], dir.path());
    assert_eq!(output.status.code(), Some(STALE), "{}", stderr_of(&output));
    assert!(
        stderr_of(&output).contains("has never been generated"),
        "{}",
        stderr_of(&output)
    );
    assert!(!generated_dir(dir.path()).exists(), "--check wrote output");

    let output = home.run(&["generate"], dir.path());
    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(generated_dir(dir.path()).is_dir());

    let output = home.run(&["generate", "--check"], dir.path());
    assert!(output.status.success(), "{}", stderr_of(&output));

    // Editing a source invalidates it without touching the generated tree.
    fs::write(
        dir.path().join("baml_src/main.baml"),
        "class Person {\n  name string\n  age int\n}\n",
    )
    .unwrap();
    let output = home.run(&["generate", "--check"], dir.path());
    assert_eq!(output.status.code(), Some(STALE), "{}", stderr_of(&output));
    assert!(
        stderr_of(&output).contains(STALE_WARNING),
        "{}",
        stderr_of(&output)
    );

    let output = home.run(&["generate"], dir.path());
    assert!(output.status.success(), "{}", stderr_of(&output));
    let output = home.run(&["generate", "--check"], dir.path());
    assert!(output.status.success(), "{}", stderr_of(&output));
}

/// The whole point of the passive check: an ordinary command surfaces the
/// staleness, so nobody has to remember to ask.
#[test]
fn ordinary_commands_warn_about_a_stale_bridge() {
    let home = TestHome::new();
    let dir = project();
    assert!(home.run(&["generate"], dir.path()).status.success());
    fs::write(
        dir.path().join("baml_src/main.baml"),
        "class Person {\n  name string\n  age int\n}\n",
    )
    .unwrap();

    for command in [["check"], ["fmt"]] {
        let stderr = stderr_of(&home.run(&command, dir.path()));
        assert!(
            stderr.contains(STALE_WARNING),
            "`baml {}` did not warn:\n{stderr}",
            command[0]
        );
    }
}

/// `bridge` fixes or precisely reports staleness itself, and the long-running
/// editor surfaces would just be noise.
#[test]
fn excluded_commands_never_warn() {
    let home = TestHome::new();
    let dir = project();
    assert!(home.run(&["generate"], dir.path()).status.success());
    fs::write(
        dir.path().join("baml_src/main.baml"),
        "class Person {\n  name string\n  age int\n}\n",
    )
    .unwrap();

    let stderr = stderr_of(&home.run(&["bridge", "list"], dir.path()));
    assert!(!stderr.contains(STALE_WARNING), "{stderr}");
}

/// A freshly configured project must not be nagged before it has opted in.
#[test]
fn a_never_generated_bridge_does_not_trigger_the_passive_warning() {
    let home = TestHome::new();
    let dir = project();

    let stderr = stderr_of(&home.run(&["check"], dir.path()));

    assert!(!stderr.contains("out of date"), "{stderr}");
    assert!(!stderr.contains("never been generated"), "{stderr}");
}

/// The runtime version must match this toolchain exactly, and generated
/// Python imports pydantic, which the runtime cannot depend on.
#[test]
fn install_prints_a_pinned_command_carrying_the_pydantic_peer() {
    let home = TestHome::new();
    let dir = project();

    let output = home.run(&["bridge", "install"], dir.path());

    assert!(output.status.success(), "{}", stderr_of(&output));
    let stdout = stdout_of(&output);
    assert!(stdout.contains("uv add"), "{stdout}");
    assert!(
        stdout.contains(&format!("baml_bridge=={}", baml_version::CANONICAL_VERSION)),
        "{stdout}"
    );
    assert!(stdout.contains("pydantic>=2"), "{stdout}");
    // Print-only: nothing was installed and no host manifest was created.
    assert!(!dir.path().join("pyproject.toml").exists());
    assert!(!dir.path().join("uv.lock").exists());
}

/// A lockfile already in the tree picks the tool, and the evidence is shown.
#[test]
fn install_detects_the_host_package_manager() {
    let home = TestHome::new();
    let dir = project();
    fs::create_dir_all(dir.path().join("out")).unwrap();
    fs::write(dir.path().join("out/poetry.lock"), "").unwrap();

    let stdout = stdout_of(&home.run(&["bridge", "install"], dir.path()));

    assert!(stdout.contains("poetry add"), "{stdout}");
    assert!(stdout.contains("recommended: found"), "{stdout}");
}

/// `baml generate` remains the primary generation entry point.
#[test]
fn generate_writes_clients_and_generate_add_remains_available() {
    let home = TestHome::new();
    let dir = project();

    let output = home.run(&["generate"], dir.path());

    assert!(output.status.success(), "{}", stderr_of(&output));
    assert!(
        generated_dir(dir.path()).exists(),
        "generate wrote no output"
    );

    let output = home.run(&["generate", "add", "python"], dir.path());
    assert!(output.status.success(), "{}", stderr_of(&output));
    let manifest = fs::read_to_string(dir.path().join("baml.toml")).unwrap();
    assert!(manifest.contains("[generator.client2]"), "{manifest}");
}

/// A team that commits its bridge needs the tree and the manifest tracked,
/// or `--check` cannot run from a clean checkout.
#[test]
fn the_commit_vcs_policy_writes_a_gitignore_that_ignores_nothing() {
    let home = TestHome::new();
    let dir = project();
    let manifest = dir.path().join("baml.toml");
    let content = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        format!("[bridge]\nvcs = \"commit\"\n\n{content}"),
    )
    .unwrap();

    assert!(home.run(&["generate"], dir.path()).status.success());

    let gitignore = fs::read_to_string(generated_dir(dir.path()).join(".gitignore")).unwrap();
    assert!(!gitignore.contains("\n*\n"), "{gitignore}");
    assert!(gitignore.contains("Generated by BAML"), "{gitignore}");
}
