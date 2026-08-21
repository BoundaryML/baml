#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};

const BOOTSTRAP: &str = include_str!("../../../../skill/bootstrap.md");

struct TestHome {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl TestHome {
    fn with_cli(script: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let bin = root.join("toolchains/0.11.0/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(root.join("toolchains/0.11.0/VERSION"), "0.11.0\n").unwrap();
        let cli = bin.join("baml-cli");
        fs::write(&cli, script).unwrap();
        fs::set_permissions(&cli, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            root.join("config.toml"),
            "[default]\nselector = \"0.11.0\"\n\n[update]\nauto_check = false\n",
        )
        .unwrap();
        Self { _dir: dir, root }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_baml"));
        command
            .env("BAML_HOME", &self.root)
            .env_remove("BAML_VERSION");
        command
    }
}

fn output_text(output: &Output) -> (&str, &str) {
    (
        std::str::from_utf8(&output.stdout).unwrap(),
        std::str::from_utf8(&output.stderr).unwrap(),
    )
}

#[test]
fn wrapper_checks_bootstrap_version_and_forwards_only_guide_arguments() {
    let home = TestHome::with_cli(
        "#!/bin/sh\nprintf 'forwarded:'\nfor arg in \"$@\"; do printf ' <%s>' \"$arg\"; done\nprintf '\\nGUIDE\\n'\n",
    );

    let outdated = home
        .command()
        .args(["agent", "guide", "--bootstrap-version", "0"])
        .output()
        .unwrap();
    assert!(outdated.status.success());
    let (stdout, stderr) = output_text(&outdated);
    assert!(stderr.is_empty(), "{stderr}");
    assert!(
        stdout.starts_with("WARNING: This BAML bootstrap skill is outdated."),
        "{stdout}"
    );
    assert!(stdout.contains("forwarded: <agent> <guide>"), "{stdout}");
    assert!(!stdout.contains("<--bootstrap-version>"), "{stdout}");

    let current = home
        .command()
        .args(["agent", "guide", "--bootstrap-version=1"])
        .output()
        .unwrap();
    assert!(current.status.success());
    let (stdout, stderr) = output_text(&current);
    assert!(stderr.is_empty(), "{stderr}");
    assert_eq!(stdout, "forwarded: <agent> <guide>\nGUIDE\n");

    let newer = home
        .command()
        .args(["agent", "guide", "--bootstrap-version", "2"])
        .output()
        .unwrap();
    assert!(newer.status.success());
    let (stdout, stderr) = output_text(&newer);
    assert!(stderr.is_empty(), "{stderr}");
    assert!(
        stdout.starts_with(
            "NOTE: This project's BAML bootstrap skill is newer than the active BAML wrapper."
        ),
        "{stdout}"
    );
    assert!(stdout.ends_with("forwarded: <agent> <guide>\nGUIDE\n"));
}

#[test]
fn wrapper_installs_bootstrap_without_resolving_a_toolchain() {
    let project = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .arg("--project")
        .arg(project.path())
        .args(["agent", "install"])
        .env("BAML_HOME", home.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert_bootstrap_installed(project.path());
    assert!(!home.path().join("toolchains").exists());
}

#[test]
fn wrapper_owns_agent_install_help() {
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["agent", "install", "--help"])
        .env("BAML_HOME", home.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();

    assert!(output.status.success());
    let (stdout, stderr) = output_text(&output);
    assert!(stderr.is_empty(), "{stderr}");
    assert!(stdout.contains("Usage:\n  baml agent install"), "{stdout}");
    assert!(!home.path().join("toolchains").exists());
}

#[test]
fn wrapper_install_detects_project_from_global_directory() {
    let project = tempfile::tempdir().unwrap();
    let nested = project.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(
        project.path().join("baml.toml"),
        "[package]\nname = \"test\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["--directory"])
        .arg(&nested)
        .args(["agent", "install"])
        .env("BAML_HOME", project.path().join("home"))
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_bootstrap_installed(project.path());
}

#[test]
fn wrapper_installs_bootstrap_after_successful_init_and_new() {
    let home = TestHome::with_cli(
        "#!/bin/sh\ncase \"$1\" in\n  init) root=\"${2:-.}\" ;;\n  new) root=\"$2\"; mkdir \"$root\" ;;\n  *) exit 2 ;;\nesac\nprintf '[package]\\nname = \"test\"\\n' > \"$root/baml.toml\"\n",
    );
    let workspace = tempfile::tempdir().unwrap();
    let init_project = workspace.path().join("initialized");
    fs::create_dir(&init_project).unwrap();

    let init = home
        .command()
        .args(["init", "initialized"])
        .current_dir(workspace.path())
        .output()
        .unwrap();
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    assert_bootstrap_installed(&init_project);

    let new = home
        .command()
        .args(["new", "created"])
        .current_dir(workspace.path())
        .output()
        .unwrap();
    assert!(
        new.status.success(),
        "{}",
        String::from_utf8_lossy(&new.stderr)
    );
    assert_bootstrap_installed(&workspace.path().join("created"));
}

#[test]
fn wrapper_does_not_install_bootstrap_after_failed_scaffolding() {
    let home = TestHome::with_cli("#!/bin/sh\nmkdir -p \"$2\"\nexit 1\n");
    let workspace = tempfile::tempdir().unwrap();

    let output = home
        .command()
        .args(["new", "failed"])
        .current_dir(workspace.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!workspace.path().join("failed/.agents").exists());
    assert!(!workspace.path().join("failed/.claude").exists());
}

fn assert_bootstrap_installed(project: &Path) {
    for skills_dir in [".agents/skills", ".claude/skills"] {
        let installed = project.join(skills_dir).join("baml/SKILL.md");
        assert_eq!(fs::read_to_string(installed).unwrap(), BOOTSTRAP);
    }
}
