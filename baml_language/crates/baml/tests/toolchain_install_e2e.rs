use std::{fs, process::Command};

fn baml_command(project: &tempfile::TempDir) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_baml"));
    command
        .arg("toolchain")
        .arg("install")
        .current_dir(project.path())
        .env("BAML_HOME", project.path().join(".baml-home"))
        .env("HOME", project.path())
        .env_remove("BAML_VERSION");
    command
}

#[test]
fn bare_install_uses_project_toolchain_pin() {
    let project = tempfile::tempdir().unwrap();
    fs::write(
        project.path().join("baml.toml"),
        "[package]\nname = \"demo\"\n\n[toolchain]\nversion = \"0.15.0\"\n",
    )
    .unwrap();

    let output = baml_command(&project)
        .args(["--manifest-base-url", "http://127.0.0.1:1/manifest"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("http://127.0.0.1:1/manifest/version/0.15.0.json"),
        "{stderr}"
    );
    assert!(!stderr.contains("usage:"), "{stderr}");
}

#[test]
fn install_help_is_not_parsed_as_a_version() {
    let project = tempfile::tempdir().unwrap();
    let output = baml_command(&project)
        .args([
            "--help",
            "--manifest-base-url",
            "http://127.0.0.1:1/manifest",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("baml toolchain install [canary|nightly|version] [--force]"),
        "{stdout}"
    );
    assert!(!stdout.contains("127.0.0.1"), "{stdout}");
}

#[test]
fn bare_install_without_a_project_pin_is_actionable() {
    let project = tempfile::tempdir().unwrap();
    let output = baml_command(&project).output().unwrap();

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "no BAML toolchain is pinned in baml.toml\nRun: baml toolchain install <canary|nightly|version>\n"
    );
}
