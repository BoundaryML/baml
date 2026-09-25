//! Real CLI roots and packed home roots must not accidentally follow cwd or
//! serialized build-machine source paths. Keep all output inside temp fixtures.
mod common;

use std::{path::Path, process::Command};

const RECORDINGS: &str = ".baml/btel/recordings";

fn cli(cwd: &Path, home: &Path) -> Command {
    let mut command = Command::new(common::baml_cli());
    command
        .current_dir(cwd)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("BAML_HOME", home.join(".baml"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_AGENT_SKILL_CHECK", "off")
        .env("BAML_CACHE_DIR", home.join("cache"))
        .env("BAML_TELEMETRY", "medium");
    command
}

fn run(command: &mut Command, success: bool) {
    let output = command.output().unwrap();
    assert_eq!(
        output.status.success(),
        success,
        "{command:?}:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("telemetry recording failed"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_recordings(root: &Path, count: usize) {
    let directories: Vec<_> = std::fs::read_dir(root.join(RECORDINGS))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(directories.len(), count);
    for directory in directories {
        let recording = btel_file::read_directory(&directory).unwrap();
        assert!(!recording.files.is_empty(), "{}", directory.display());
        assert!(recording.issues.is_empty(), "{:?}", recording.issues);
        assert!(recording.files.iter().any(|file| {
            file.aggregates
                .as_ref()
                .is_some_and(|batch| !batch.entries.is_empty())
        }));
    }
}

fn home(root: &Path) -> std::path::PathBuf {
    let home = root.join("home");
    std::fs::create_dir_all(home.join(".baml")).unwrap();
    std::fs::write(
        home.join(".baml/config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();
    home
}

#[test]
fn cli_uses_project_root_for_cold_cached_nested_expression_and_test_execution() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    common::write_project(
        &project,
        r#"
function main() -> int { 7 }
function fail() -> int { assert.is_true(false); 0 }
testset "smoke" { test "ok" { assert.is_true(true) } }
"#,
    );
    let home = home(temp.path());
    let nested = project.join("baml_src/nested");
    std::fs::create_dir(&nested).unwrap();
    // --from must win over both cwd and the baml_src subdirectory. Repeat to
    // cover a bytecode-cache hit as well as compilation.
    for count in 1..=2 {
        run(
            cli(temp.path(), &home)
                .args(["run", "main", "--from"])
                .arg(&project),
            true,
        );
        assert_recordings(&project, count);
    }
    run(cli(&nested, &home).args(["run", "main"]), true);
    assert_recordings(&project, 3);
    run(
        cli(temp.path(), &home)
            .args(["run", "-e", "1 + 2", "--from"])
            .arg(&project),
        true,
    );
    assert_recordings(&project, 4);
    run(
        cli(temp.path(), &home)
            .args(["test", "--from"])
            .arg(&project),
        true,
    );
    assert_recordings(&project, 5);
    run(
        cli(temp.path(), &home)
            .args(["run", "fail", "--from"])
            .arg(&project),
        false,
    );
    assert_recordings(&project, 6);
    run(
        cli(temp.path(), &home)
            .args(["run", "main", "--from"])
            .arg(&project)
            .env("BAML_TELEMETRY", "off"),
        true,
    );
    assert_recordings(&project, 6);
    assert!(!temp.path().join(RECORDINGS).exists());
    assert!(!nested.join(RECORDINGS).exists());
    assert!(!home.join(RECORDINGS).exists());
}

#[test]
fn packed_modes_write_to_user_home_even_when_launched_in_another_project() {
    let _built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let launch = temp.path().join("launch");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&launch).unwrap();
    common::write_project(&source, "function main() -> int { 9 }\n");
    common::write_project(&launch, "function main() -> int { 42 }\n");
    let home = home(temp.path());
    for (index, target) in [vec!["main"], vec!["-f", "main"]].iter().enumerate() {
        let binary = temp.path().join(format!("packed-{index}"));
        run(
            cli(temp.path(), &home)
                .args(["pack", "--from"])
                .arg(&source)
                .arg("-o")
                .arg(&binary)
                .args(target),
            true,
        );
        let mut command = Command::new(&binary);
        command
            .current_dir(&launch)
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("BAML_TELEMETRY", "medium");
        if index == 1 {
            command.arg("main");
        }
        run(&mut command, true);
        assert_recordings(&home, index + 1);
        run(command.env("BAML_TELEMETRY", "off"), true);
        assert_recordings(&home, index + 1);
    }
    assert!(!source.join(RECORDINGS).exists());
    assert!(!launch.join(RECORDINGS).exists());
    assert!(!temp.path().join(RECORDINGS).exists());
}
