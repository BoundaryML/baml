//! End-to-end tests for project-local toolchain pinning.

use std::{fs, path::Path, process::Command};

fn install_fake_toolchain(home: &Path, version: &str) {
    let bin = home.join("toolchains").join(version).join("bin");
    fs::create_dir_all(&bin).unwrap();
    fs::write(
        home.join("toolchains").join(version).join("VERSION"),
        format!("{version}\n"),
    )
    .unwrap();
    fs::write(
        bin.join(if cfg!(windows) {
            "baml-cli.exe"
        } else {
            "baml-cli"
        }),
        "",
    )
    .unwrap();
}

#[test]
fn pin_updates_nearest_manifest_and_replaces_channel() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let project = temp.path().join("project");
    let nested = project.join("baml_src/nested");
    fs::create_dir_all(&nested).unwrap();
    fs::create_dir_all(&home).unwrap();
    install_fake_toolchain(&home, "0.16.0");
    fs::write(
        project.join("baml.toml"),
        "[package]\nname = \"demo\"\n\n[toolchain]\n# selected for CI\nchannel = \"nightly\"\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["toolchain", "pin", "0.16.0"])
        .current_dir(&nested)
        .env("BAML_HOME", &home)
        .env("HOME", temp.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("pinned BAML toolchain 0.16.0"), "{stdout}");
    assert!(
        stdout.contains(&project.join("baml.toml").display().to_string()),
        "{stdout}"
    );

    let manifest = fs::read_to_string(project.join("baml.toml")).unwrap();
    assert!(manifest.contains("# selected for CI"), "{manifest}");
    assert!(manifest.contains("version = \"0.16.0\""), "{manifest}");
    assert!(!manifest.contains("channel ="), "{manifest}");
}

#[test]
fn pin_rejects_channels_without_modifying_manifest() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&home).unwrap();
    let original = "[package]\nname = \"demo\"\n";
    fs::write(project.join("baml.toml"), original).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_baml"))
        .args(["toolchain", "pin", "nightly"])
        .current_dir(&project)
        .env("BAML_HOME", &home)
        .env("HOME", temp.path())
        .env_remove("BAML_VERSION")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("requires an exact BAML SemVer"));
    assert_eq!(
        fs::read_to_string(project.join("baml.toml")).unwrap(),
        original
    );
}
